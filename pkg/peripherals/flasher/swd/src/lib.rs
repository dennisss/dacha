#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use common::errors::*;
use peripherals::gpio::*;


// NOTE: We don't currently do any explicit sleeping since the
// delay from gpiochip syscalls is sufficient.
// const CLOCK_FREQUENCY: u64 = 1_000_000;
// const HALF_CYCLE_DURATION: Duration = Duration::from_nanos(1000000000 / CLOCK_FREQUENCY / 2);

#[derive(Args, Copy, Clone, PartialEq)]
pub enum McuTarget {
    STM32F411,
    STM32G031,
}

pub struct SWDProgrammer {
    io_pin: GPIOPin,
    clk_pin: GPIOPin,
}

impl SWDProgrammer {
    pub fn create(mut clk_pin: GPIOPin, mut io_pin: GPIOPin) -> Result<Self> {
        clk_pin.configure(GPIOLineFlags::OUTPUT)?;
        clk_pin.write(true)?;

        io_pin.configure(GPIOLineFlags::OUTPUT | GPIOLineFlags::BIAS_PULL_UP)?;
        io_pin.write(true)?;

        Ok(Self { clk_pin, io_pin })
    }

    pub fn release_pins(&mut self) -> Result<()> {
        self.clk_pin.configure(GPIOLineFlags::INPUT)?;
        self.io_pin.configure(GPIOLineFlags::INPUT)?;
        Ok(())
    }

    fn write_bit(&mut self, bit: bool) -> Result<()> {
        self.clk_pin.write(false)?;
        self.io_pin.write(bit)?;
        // sleep(HALF_CYCLE_DURATION);
        self.clk_pin.write(true)?;
        // sleep(HALF_CYCLE_DURATION);
        Ok(())
    }

    fn read_bit(&mut self) -> Result<bool> {
        self.clk_pin.write(false)?;
        // sleep(HALF_CYCLE_DURATION);
        let bit = self.io_pin.read()?;
        self.clk_pin.write(true)?;
        // sleep(HALF_CYCLE_DURATION);
        Ok(bit)
    }

    fn trn_in(&mut self) -> Result<()> {
        self.clk_pin.write(false)?;
        self.io_pin.configure(GPIOLineFlags::INPUT | GPIOLineFlags::BIAS_PULL_UP)?;
        // sleep(HALF_CYCLE_DURATION);
        self.clk_pin.write(true)?;
        // sleep(HALF_CYCLE_DURATION);
        Ok(())
    }

    fn trn_out(&mut self) -> Result<()> {
        self.clk_pin.write(false)?;
        // sleep(HALF_CYCLE_DURATION);
        self.io_pin.configure(GPIOLineFlags::OUTPUT | GPIOLineFlags::BIAS_PULL_UP)?;
        self.clk_pin.write(true)?;
        // sleep(HALF_CYCLE_DURATION);
        Ok(())
    }

    /// Tries to find a connected chip by entering SWD mode and querying for the IDCODE register.
    ///
    /// For most chips, this will return 0x2BA01477.
    /// GO31 Id is 0xBC11477
    ///
    /// TODO: Automatically check the return value.
    pub fn probe(&mut self) -> Result<u32> {
        self.io_pin.configure(GPIOLineFlags::OUTPUT | GPIOLineFlags::BIAS_PULL_UP)?;

        // 1. Line Reset
        for _ in 0..56 { self.write_bit(true)?; }

        // 2. JTAG to SWD Switch (0xE79E)
        let magic = 0xE79E_u16;
        for i in 0..16 { self.write_bit((magic >> i) & 1 == 1)?; }

        // 3. Line Reset
        for _ in 0..56 { self.write_bit(true)?; }

        // 4. Idle
        for _ in 0..4 { self.write_bit(false)?; }

        // 5. Read DP IDCODE (0xA5)
        let req = 0xA5_u8;
        for i in 0..8 { self.write_bit((req >> i) & 1 == 1)?; }

        self.trn_in()?;

        let mut ack = 0;
        for i in 0..3 { if self.read_bit()? { ack |= 1 << i; } }
        if ack != 1 { return Err(err_msg("SWD read failed: target did not ACK with OK")); }

        let mut idcode = 0_u32;
        for i in 0..32 { if self.read_bit()? { idcode |= 1 << i; } }

        let parity_bit = self.read_bit()?;
        let mut expected_parity = 0;
        for i in 0..32 { expected_parity ^= (idcode >> i) & 1; }

        if parity_bit != (expected_parity == 1) {
            return Err(err_msg("SWD read failed: Parity mismatch"));
        }

        self.trn_out()?;
        for _ in 0..8 { self.write_bit(false)?; }

        Ok(idcode)
    }

    /// Powers up the debug domain and configures the AHB-AP for memory access
    pub fn init_debug(&mut self) -> Result<()> {
        // Clear sticky errors (Write to DP ABORT register 0x00)
        self.transfer(false, false, 0x00, 0x0000001E)?;

        // Request System and Debug Power-Up (Write to DP CTRL/STAT register 0x04)
        let pwr_req = 0x50000000; // CSYSPWRUPREQ | CDBGPWRUPREQ
        self.transfer(false, false, 0x04, pwr_req)?;
        
        // Wait for power-up to complete
        let mut powered_up = false;
        for _ in 0..1000 {
            let stat = self.transfer(false, true, 0x04, 0)?;
            if (stat & 0xA0000000) == 0xA0000000 {
                powered_up = true;
                break;
            }
        }
        if !powered_up {
            return Err(err_msg("Debug power-up timed out: CSYSPWRUPACK/CDBGPWRUPACK not set"));
        }
        
        // Select AP Bank 0 (Write to DP SELECT register 0x08)
        // This ensures subsequent AP accesses go to the AHB-AP control registers
        self.transfer(false, false, 0x08, 0x00000000)?;
        
        // Configure AHB-AP CSW (Control/Status Word, register 0x00)
        // Set Size to 32-bit (0x2) and auto-increment off
        let csw_val = 0x23000052; 
        self.transfer(true, false, 0x00, csw_val)?;

        Ok(())
    }

    /// Halts the Cortex-M core so we can safely modify flash
    pub fn halt_core(&mut self) -> Result<()> {
        let dhcsr_addr = 0xE000EDF0;
        let dbgkey = 0xA05F0000;
        let c_halt = 0x00000002;
        let c_debugen = 0x00000001;
        
        self.write_mem32(dhcsr_addr, dbgkey | c_halt | c_debugen)?;
        Ok(())
    }

    /// Unlocks and flashes the chip based on the specific MCU architecture
    pub fn flash_chip(&mut self, target: McuTarget, binary: &[u8]) -> Result<()> {
        self.halt_core()?;

        let start_addr = 0x08000000;

        match target {
            McuTarget::STM32F411 => {
                let flash_keyr = 0x40023C04;
                let flash_cr   = 0x40023C10;
                let flash_sr   = 0x40023C0C;

                // 1. Unlock Flash
                self.write_mem32(flash_keyr, 0x45670123)?;
                self.write_mem32(flash_keyr, 0xCDEF89AB)?;

                // 2. Smart Erase
                self.smart_erase(McuTarget::STM32F411, binary.len())?;

                // 3. Program Mode (PG bit = 0, PSIZE 32-bit = 0b10 at bit 8)
                self.write_mem32(flash_cr, (1 << 0) | (2 << 8))?;

                // 4. Write data 32 bits at a time
                for (i, chunk) in binary.chunks(4).enumerate() {
                    let mut word = 0u32;
                    for (j, &byte) in chunk.iter().enumerate() {
                        word |= (byte as u32) << (j * 8);
                    }
                    self.write_mem32(start_addr + (i as u32 * 4), word)?;
                    self.wait_for_flash_ready(flash_sr, 16)?;
                }

                // 5. Clear PG bit and lock flash
                let cr = self.read_mem32(flash_cr)?;
                self.write_mem32(flash_cr, cr & !(1 << 0))?;
                let cr2 = self.read_mem32(flash_cr)?;
                self.write_mem32(flash_cr, cr2 | (1 << 31))?;
            }
            McuTarget::STM32G031 => {
                let flash_keyr = 0x40022008;
                let flash_cr   = 0x40022014;
                let flash_sr   = 0x40022010;
                let flash_acr  = 0x40022000;

                // 1. Unlock Flash
                self.write_mem32(flash_keyr, 0x45670123)?;
                self.write_mem32(flash_keyr, 0xCDEF89AB)?;

                // 2. Smart Erase
                self.smart_erase(McuTarget::STM32G031, binary.len())?;

                // 3. Program Mode (PG bit = 0)
                self.write_mem32(flash_cr, 1 << 0)?;

                // 4. Write data (G031 requires double-word / 64-bit programming)
                for (i, chunk) in binary.chunks(8).enumerate() {
                    let mut word1 = 0u32;
                    let mut word2 = 0u32;
                    
                    for (j, &byte) in chunk.iter().enumerate() {
                        if j < 4 { word1 |= (byte as u32) << (j * 8); } 
                        else     { word2 |= (byte as u32) << ((j - 4) * 8); }
                    }

                    let current_addr = start_addr + (i as u32 * 8);
                    self.write_mem32(current_addr, word1)?;
                    self.write_mem32(current_addr + 4, word2)?;
                    
                    self.wait_for_flash_ready(flash_sr, 16)?;
                }

                // 5. Clear PG bit and lock flash
                let cr = self.read_mem32(flash_cr)?;
                self.write_mem32(flash_cr, cr & !(1 << 0))?;
                let cr2 = self.read_mem32(flash_cr)?;
                self.write_mem32(flash_cr, cr2 | (1 << 31))?;

                // 6. Clear the EMPTY bit (bit 16) in FLASH_ACR.
                // (allows restarting a chip on its first flash after the factory)
                let acr_val = self.read_mem32(flash_acr)?;
                if (acr_val & (1 << 16)) != 0 {
                    self.write_mem32(flash_acr, acr_val & !(1 << 16))?;
                }
            }
        }

        Ok(())
    }

    /// Verifies the flashed data by reading back memory and comparing
    pub fn verify_flash(&mut self, binary: &[u8]) -> Result<()> {
        // Clear sticky errors (Write to DP ABORT register 0x00)
        self.transfer(false, false, 0x00, 0x0000001E)?;
        
        let start_addr = 0x08000000;
        
        for (i, chunk) in binary.chunks(4).enumerate() {
            let addr = start_addr + (i as u32 * 4);
            let val = self.read_mem32(addr)?;
            
            let mut expected = 0u32;
            for (j, &byte) in chunk.iter().enumerate() {
                expected |= (byte as u32) << (j * 8);
            }
            
            // Mask out padding bytes if chunk is less than 4 bytes
            let mask = match chunk.len() {
                1 => 0x000000FF,
                2 => 0x0000FFFF,
                3 => 0x00FFFFFF,
                _ => 0xFFFFFFFF,
            };
            
            if (val & mask) != (expected & mask) {
                return Err(err_msg(format!("Verification failed at 0x{:08X}: expected 0x{:08X}, got 0x{:08X}", addr, expected, val)));
            }
        }
        
        Ok(())
    }

    /// Erases only the necessary flash sectors based on binary size.
    /// Falls back to a mass erase if the binary takes up more than half the flash.
    fn smart_erase(&mut self, target: McuTarget, binary_size: usize) -> Result<()> {
        match target {
            McuTarget::STM32F411 => {
                let flash_cr = 0x40023C10;
                let flash_sr = 0x40023C0C;

                // F411 has 512KB total. Sector 6 starts at 256KB.
                // If the binary is larger than 256KB, just do a mass erase.
                if binary_size > 256 * 1024 {
                    // MER (Mass Erase) is bit 2, STRT is bit 16
                    self.write_mem32(flash_cr, 1 << 2)?; 
                    self.write_mem32(flash_cr, (1 << 2) | (1 << 16))?;
                    self.wait_for_flash_ready(flash_sr, 16)?;
                } else {
                    // Calculate the highest sector we need to touch
                    let last_sector = if binary_size <= 16 * 1024 { 0 }
                    else if binary_size <= 32 * 1024 { 1 }
                    else if binary_size <= 48 * 1024 { 2 }
                    else if binary_size <= 64 * 1024 { 3 }
                    else if binary_size <= 128 * 1024 { 4 }
                    else { 5 }; // Up to 256KB

                    for sector in 0..=last_sector {
                        // SER (Sector Erase) is bit 1, SNB (Sector Number) is bits 6:3
                        let cr_val = (1 << 1) | (sector << 3);
                        
                        self.write_mem32(flash_cr, cr_val)?;
                        self.write_mem32(flash_cr, cr_val | (1 << 16))?; // Add STRT
                        self.wait_for_flash_ready(flash_sr, 16)?;
                    }
                }
                
                // Clear any erase flags (SER, MER)
                let cr = self.read_mem32(flash_cr)?;
                self.write_mem32(flash_cr, cr & !((1 << 1) | (1 << 2)))?;
            }
            McuTarget::STM32G031 => {
                let flash_cr = 0x40022014;
                let flash_sr = 0x40022010;

                // G031 has 64KB total (32 pages of 2KB).
                // Fall back to Mass Erase if more than half (>32KB) is overwritten.
                if binary_size > 32 * 1024 {
                    // MER1 (Mass Erase) is bit 2, STRT is bit 16
                    self.write_mem32(flash_cr, 1 << 2)?;
                    self.write_mem32(flash_cr, (1 << 2) | (1 << 16))?;
                    self.wait_for_flash_ready(flash_sr, 16)?;
                } else {
                    // Calculate how many 2KB pages are needed
                    let num_pages = (binary_size + 2047) / 2048;
                    
                    for page in 0..num_pages {
                        // PER (Page Erase) is bit 1, PNB (Page Number) is bits 9:3
                        let cr_val = (1 << 1) | ((page as u32) << 3);
                        
                        self.write_mem32(flash_cr, cr_val)?;
                        self.write_mem32(flash_cr, cr_val | (1 << 16))?; // Add STRT
                        self.wait_for_flash_ready(flash_sr, 16)?;
                    }
                }

                // Clear any erase flags (PER, MER1)
                let cr = self.read_mem32(flash_cr)?;
                self.write_mem32(flash_cr, cr & !((1 << 1) | (1 << 2)))?;
            }
        }
        Ok(())
    }

    /// Tears down the debug session so that a subsequent hardware reset (NRST)
    /// will cleanly boot the new firmware. The caller is responsible for
    /// toggling the reset pin after this returns.
    pub fn reset_core(&mut self) -> Result<()> {
        // 1. Clear VC_CORERESET in DEMCR — if set, the core halts on reset
        //    vector fetch. Persists across NRST; only POR or explicit clear
        //    removes it.
        self.write_mem32(0xE000EDFC, 0x00000000)?;

        // 2. Clear C_HALT and C_DEBUGEN in DHCSR so the debug logic won't
        //    re-halt the core after reset.
        self.write_mem32(0xE000EDF0, 0xA05F0000)?;

        // 3. De-assert CDBGPWRUPREQ/CSYSPWRUPREQ — the debug power domain
        //    survives NRST, so we must explicitly shut it down.
        let _ = self.transfer(false, false, 0x04, 0x00000000);

        // 4. Drive SWCLK low — on STM32G0, PA14 doubles as BOOT0. If high
        //    during reset (and nBOOT_SEL=0), the chip enters the bootloader.
        let _ = self.clk_pin.write(false);
        let _ = self.io_pin.write(false);

        Ok(())
    }


    /// Helper to poll the Flash Status Register's BSY (Busy) bit
    fn wait_for_flash_ready(&mut self, sr_addr: u32, bsy_bit: u8) -> Result<()> {
        let timeout = 100_000;
        for _ in 0..timeout {
            let sr = self.read_mem32(sr_addr)?;
            if (sr & (1 << bsy_bit)) == 0 {
                return Ok(());
            }
        }
        Err(err_msg("Flash timeout: BSY bit did not clear"))
    }

    /// Generic SWD Transfer function to read/write AP and DP registers
    fn transfer(&mut self, is_ap: bool, is_read: bool, reg_addr: u8, mut data: u32) -> Result<u32> {
        let apndp = if is_ap { 1 } else { 0 };
        let rnw = if is_read { 1 } else { 0 };
        let a2 = (reg_addr >> 2) & 1;
        let a3 = (reg_addr >> 3) & 1;

        // Header parity: even parity of APnDP, RnW, A2, and A3
        let req_parity = apndp ^ rnw ^ a2 ^ a3;

        // Construct Request Byte (Start=1, Stop=0, Park=1)
        // LSB first: 1 | APnDP | RnW | A2 | A3 | Parity | Stop(0) | Park(1)
        let req = 1 | (apndp << 1) | (rnw << 2) | (a2 << 3) | (a3 << 4) | (req_parity << 5) | (1 << 7);

        // 1. Send Request Phase
        for i in 0..8 {
            self.write_bit((req >> i) & 1 == 1)?;
        }

        // 2. Turnaround (Host switches to Input)
        self.trn_in()?;

        // 3. Read ACK
        let mut ack = 0;
        for i in 0..3 {
            if self.read_bit()? { ack |= 1 << i; }
        }

        if ack != 1 {
            // ACK 2 = WAIT, ACK 4 = FAULT
            return Err(err_msg("SWD transfer failed: Target did not ACK with OK"));
        }

        let mut result = 0;

        // 4. Data Phase
        if is_read {
            // Target sends data
            for i in 0..32 {
                if self.read_bit()? { result |= 1 << i; }
            }
            let parity_bit = self.read_bit()?;
            
            // Turnaround (Target releases line, Host takes control)
            self.trn_out()?;

            // Verify Parity
            let expected_parity = (0..32).fold(0, |acc, i| acc ^ ((result >> i) & 1));
            if parity_bit != (expected_parity == 1) {
                return Err(err_msg("SWD data parity mismatch on read"));
            }
        } else {
            // Turnaround (Host takes control to write data)
            self.trn_out()?;
            
            // Host sends data
            let expected_parity = (0..32).fold(0, |acc, i| acc ^ ((data >> i) & 1));
            for i in 0..32 {
                self.write_bit((data >> i) & 1 == 1)?;
            }
            self.write_bit(expected_parity == 1)?;
        }

        // 5. Idle cycles to clock the transaction through
        for _ in 0..8 {
            self.write_bit(false)?;
        }

        Ok(result)
    }

    /// Writes a 32-bit value to a specific memory address via the AHB-AP
    fn write_mem32(&mut self, addr: u32, data: u32) -> Result<()> {
        // 1. Write the target address into the AP Transfer Address Register (TAR, 0x04)
        self.transfer(true, false, 0x04, addr)?;
        // 2. Write the data into the AP Data Read/Write Register (DRW, 0x0C)
        self.transfer(true, false, 0x0C, data)?;
        Ok(())
    }

    /// Reads a 32-bit value from a specific memory address via the AHB-AP
    fn read_mem32(&mut self, addr: u32) -> Result<u32> {
        // 1. Write the target address into the AP Transfer Address Register (TAR, 0x04)
        self.transfer(true, false, 0x04, addr)?;
        
        // 2. Read from the AP DRW (0x0C). 
        // IMPORTANT: In SWD, reading an AP register initiates the read but returns the 
        // result of the PREVIOUS transfer. We throw this dummy value away.
        self.transfer(true, true, 0x0C, 0)?;
        
        // 3. To get the actual data from our read, we read the DP Read Buffer (RDBUFF, 0x0C)
        let val = self.transfer(false, true, 0x0C, 0)?;
        Ok(val)
    }

}
