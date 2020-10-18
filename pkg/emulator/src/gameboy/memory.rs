use crate::gameboy::joypad::Joypad;
use crate::gameboy::sound::SoundControllerState;
use crate::gameboy::timer::Timer;
use crate::gameboy::video::VideoController;
use common::bits::{bitget, bitset};
use common::errors::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

// External memory controller assigned the following ranges:
// - 0000-3FFF
// - 4000-7FFF
// - A000-BFFF

const ROM_BANK_SIZE: usize = 0x4000; // 16KB
const RAM_BANK_SIZE: usize = 0x2000; // 8KB

pub trait MemoryInterface {
    fn store8(&mut self, addr: u16, value: u8) -> Result<()>;
    // NOTE: Loading is mutable as only one byte can be read over the memory bus
    // at one time.
    fn load8(&mut self, addr: u16) -> Result<u8>;

    fn store16(&mut self, addr: u16, value: u16) -> Result<()> {
        if addr > 0xffff - 1 {
            return Err(err_msg("Storing 16bits out of range"));
        }

        let buf = value.to_le_bytes();
        self.store8(addr, buf[0])?;
        self.store8(addr + 1, buf[1])?;

        // Must check not read only. Must check <= 0xffff - 2

        Ok(())
    }

    fn load16(&mut self, addr: u16) -> Result<u16> {
        if addr > 0xffff - 1 {
            return Err(err_msg("Loading 16bits out of range"));
        }

        let mut buf = [0u8; 2];
        buf[0] = self.load8(addr)?;
        buf[1] = self.load8(addr + 1)?;

        Ok(u16::from_le_bytes(*array_ref![buf, 0, 2]))
    }
}

impl<T: MemoryInterface + std::marker::Sync> MemoryInterface for Arc<Mutex<T>> {
    fn store8(&mut self, addr: u16, value: u8) -> Result<()> {
        let mut guard = self.lock().unwrap();
        guard.store8(addr, value)
    }

    fn load8(&mut self, addr: u16) -> Result<u8> {
        let mut guard = self.lock().unwrap();
        guard.load8(addr)
    }
}

//impl MemoryInterface {
//
//}

pub struct MBC1 {}

pub struct MBC3 {
    rom: Vec<u8>,
    /// In the range 0x01 - 0x7F. Defaults to 0x01.
    rom_number: u8,

    ram: Vec<u8>,
    /// Whether or not reads/writes to the RAM and RTC are enabled.
    ram_enabled: bool,
    /// 0x00 - 0x03 select a RAM bank. 0x08 - 0x0C selects RTC registers.
    ram_number: u8,

    clock_latch: bool,

    /// 0x08 - 0x0C
    /// TODO: Ensure that we support writing these.
    clock_registers: [u8; 5],
}

// TODO: Properly implement the real time clock.

impl MBC3 {
    pub fn new(rom: Vec<u8>) -> Result<Self> {
        if (rom.len() % ROM_BANK_SIZE) != 0 || rom.len() / ROM_BANK_SIZE < 2 {
            return Err(err_msg("Invalid ROM size"));
        }

        // RAM is always 32KB (4 banks)
        let mut ram = vec![0u8; 4 * RAM_BANK_SIZE];

        Ok(Self {
            rom,
            rom_number: 1,
            ram,
            ram_enabled: false,
            ram_number: 0,
            clock_latch: false,
            clock_registers: [0u8; 5],
        })
    }
}

impl MemoryInterface for MBC3 {
    fn store8(&mut self, addr: u16, value: u8) -> Result<()> {
        if addr < 0x2000 {
            // TODO: What happens if trying to store a 2 byte value
            self.ram_enabled = value & 0x0A == 0x0A;
        } else if addr < 0x4000 {
            let mut num = value;
            if num == 0 {
                num = 1;
            }

            if num >= 0x80 {
                return Err(err_msg("ROM number out of range."));
            }

            // TODO: Verify in range here?
            self.rom_number = num;
        } else if addr < 0x6000 {
            //			println!("CHANGE BANKING");

            if !(value <= 0x03 || (value >= 0x08 && value <= 0x0C)) {
                return Err(err_msg("Invalid RAM/RTC bank number"));
            }

            self.ram_number = value;
        } else if addr < 0x8000 {
            //			println!("LATCH");

            let v = match value {
                0 => false,
                1 => true,
                _ => {
                    return Err(err_msg(format!("Unexpected latch value {}", value)));
                }
            };

            if !self.clock_latch && v {
                // Newly latched to current time.
            }

            self.clock_latch = v;
        } else if addr >= 0xA000 && addr < 0xC000 {
            // TODO: Redundant with load8
            let off = (addr - 0xA000) as usize;
            if self.ram_number <= 0x03 {
                self.ram[RAM_BANK_SIZE * (self.ram_number as usize) + off] = value;
            } else {
                self.clock_registers[(self.ram_number - 0x8) as usize] = value;
            }
        }

        Ok(())
    }

    fn load8(&mut self, addr: u16) -> Result<u8> {
        // ROM Bank 0
        if addr < 0x4000 {
            return Ok(self.rom[addr as usize]);
        }
        // ROM Bank N
        else if addr < 0x8000 {
            let off = (addr - 0x4000) as usize;
            return Ok(self.rom[ROM_BANK_SIZE * (self.rom_number as usize) + off]);
        } else if addr >= 0xA000 && addr < 0xC000 {
            if !self.ram_enabled {
                return Err(err_msg("Reading from disabled RAM/RTC"));
            }

            let off = (addr - 0xA000) as usize;
            if self.ram_number <= 0x03 {
                return Ok(self.ram[RAM_BANK_SIZE * (self.ram_number as usize) + off]);
            } else {
                return Ok(self.clock_registers[(self.ram_number - 0x8) as usize]);
            }
        }

        Err(err_msg("Invalid offset"))
    }
}

const WORK_RAM_BANK_SIZE: usize = 0x10000;

/// TODO: Move to a separate file as this causes a cyclic reference between
/// memory and video files. TODO: Verify that bits 5, 6, 7 are not set.
#[derive(Default)]
pub struct InterruptState {
    /// FFFF (R/W)
    enabled: u8,

    /// FF0F (R/W)
    flag: u8,
}

impl InterruptState {
    pub fn some_requested(&self) -> bool {
        self.enabled & self.flag != 0
    }

    pub fn next(&mut self) -> Option<u16> {
        const INTERRUPT_ADDRS: &[u16] = &[0x40, 0x48, 0x50, 0x58, 0x60];

        let masked = self.enabled & self.flag;
        // TODO: Implement with a trailing zeros instruction
        for i in 0..INTERRUPT_ADDRS.len() {
            if bitget(masked, i as u8) {
                bitset(&mut self.flag, false, i as u8);
                return Some(INTERRUPT_ADDRS[i]);
            }
        }

        None
    }

    pub fn trigger(&mut self, typ: InterruptType) {
        bitset(&mut self.flag, true, typ as u8);
    }
}

pub enum InterruptType {
    VBlank = 0,
    LCDStat = 1,
    Timer = 2,
    Serial = 3,
    Joypad = 4,
}

pub struct Memory {
    // TODO: Deprecate this
    buffer: [u8; 0x10000],

    boot_rom: [u8; 256],

    external_controller: MBC3,

    video_controller: Rc<RefCell<VideoController>>,

    sound: Arc<Mutex<SoundControllerState>>,

    joypad: Rc<RefCell<Joypad>>,

    work_ram: Vec<u8>,

    high_ram: [u8; 127],

    interrupts: Rc<RefCell<InterruptState>>,

    timer: Rc<RefCell<Timer>>,

    dma_pending: u64,
}

impl Memory {
    pub fn new(
        boot_rom: &[u8],
        external_controller: MBC3,
        video_controller: Rc<RefCell<VideoController>>,
        sound: Arc<Mutex<SoundControllerState>>,
        joypad: Rc<RefCell<Joypad>>,
        interrupts: Rc<RefCell<InterruptState>>,
        timer: Rc<RefCell<Timer>>,
    ) -> Self {
        let mut boot_rom_owned = [0u8; 256];
        boot_rom_owned.copy_from_slice(boot_rom);

        let mut work_ram = vec![0u8; 2 * WORK_RAM_BANK_SIZE];

        Self {
            buffer: [0u8; 0x10000],
            boot_rom: boot_rom_owned,
            external_controller,
            video_controller,
            sound,
            joypad,
            work_ram,
            high_ram: [0u8; 127],
            interrupts,
            timer,
            dma_pending: 0,
        }
    }

    fn boot_rom_enabled(&self) -> bool {
        self.buffer[0xff50] == 0
    }

    pub fn step(&mut self) -> Result<()> {
        if self.dma_pending > 0 {
            self.dma_pending -= 1;
        }

        Ok(())
    }
}

impl MemoryInterface for Memory {
    fn store8(&mut self, addr: u16, value: u8) -> Result<()> {
        // TODO: Must ensure not read only.

        if addr > 0xffff {
            return Err(err_msg("Out of range"));
        }

        if self.dma_pending > 0 {
            if addr < 0xff80 || addr > 0xfffe {
                return Err(err_msg(format!(
                    "Can only access HRAM during DMA transfer: {:04X}",
                    addr
                )));
            }
        }

        // Serial
        if addr == 0xff01 || addr == 0xff02 {
            //			println!("Serial write {:x} {}", value, value as char);

            return Ok(());
        }

        if addr >= 0xE000 && addr < 0xfe00 {
            // TODO: Echo
            panic!("ECHO STORE");
        }

        if addr < 0x8000 || (addr >= 0xA000 && addr < 0xC000) {
            return self.external_controller.store8(addr, value);
        }

        if addr == 0xff00 {
            return self.joypad.borrow_mut().store8(addr, value);
        }

        if SoundControllerState::addr_mapped(addr) {
            return self.sound.store8(addr, value);
        }

        // Video
        match addr {
            0xFF46 => {
                if self.dma_pending > 0 {
                    return Err(err_msg("Triggered DMA while already running"));
                }

                // Transfer the memory and wait 160 microseconds.
                // TODO: Apply the memory copy after the time is up?
                if value > 0xf1 {
                    return Err(err_msg("Out of range DMA source bits"));
                }

                for i in 0..0xA0 {
                    let src = (((value as u16) << 8) + i);
                    let dst = 0xfe00 + i;
                    // TODO: What if the video controller is currently reading
                    // this.
                    let v = self.load8(src)?;
                    self.store8(dst, v)?;
                }

                // TODO: The timing seems to be off as this should be 671 but
                // games are not waitign for it to finish.
                self.dma_pending = 600; // 671

                return Ok(());
            }
            0x8000..=0x9FFF | 0xFE00..=0xFE9F | 0xFF40..=0xFF4B => {
                return self.video_controller.borrow_mut().store8(addr, value);
            }
            _ => {}
        }

        // Sound
        match addr {
            0xFF10..=0xFF14
            | 0xFF16..=0xFF19
            | 0xFF1A..=0xFF1E
            | 0xFF20..=0xFF23
            | 0xFF24..=0xFF26 => {
                return Ok(());
            }
            _ => {}
        }

        if addr >= 0xC000 && addr <= 0xDFFF {
            self.work_ram[(addr - 0xC000) as usize] = value;
            return Ok(());
        }

        if addr >= 0xff04 && addr <= 0xff07 {
            return self.timer.borrow_mut().store8(addr, value);
        }

        if addr == 0xff50 {
            if !self.boot_rom_enabled() {
                return Err(err_msg("Attempting to toggle boot ROM after disabled."));
            }

            self.buffer[addr as usize] = value;
        } else if addr >= 0xff80 && addr < 0xffff {
            self.high_ram[(addr - 0xff80) as usize] = value;
        } else if addr == 0xff0f {
            // TODO: Check bit mask
            self.interrupts.borrow_mut().flag = value & 0b11111;
        } else if addr == 0xffff {
            if value & 0b11111 != value {
                return Err(err_msg(
                    "Trying to set unknown upper bits of interrupt enable register",
                ));
            }

            self.interrupts.borrow_mut().enabled = value;
        } else {
            return Err(err_msg(format!(
                "Storing into unsupported memory range {:04X}",
                addr
            )));
        }

        Ok(())
    }

    fn load8(&mut self, addr: u16) -> Result<u8> {
        if addr > 0xffff {
            return Err(err_msg("Out of range"));
        }

        // Serial
        if addr == 0xff01 || addr == 0xff02 {
            return Ok(0);
        }

        // TODO: Dedup with above
        if self.dma_pending > 0 {
            if addr < 0xff80 || addr > 0xfffe {
                return Err(err_msg(format!(
                    "Can only access HRAM during DMA transfer: {:04X}",
                    addr
                )));
            }
        }

        if addr >= 0xE000 && addr < 0xfe00 {
            // TODO: Echo
            panic!("ECHO LOAD");
        }

        if addr == 0xff00 {
            return self.joypad.borrow_mut().load8(addr);
        }

        if addr <= 0xff && self.boot_rom_enabled() {
            return Ok(self.boot_rom[addr as usize]);
        }

        // Video
        match addr {
            0xFF46 => {
                // Do DMA
                return Err(err_msg("Trying to read DMA register"));
            }
            0x8000..=0x9FFF | 0xFE00..=0xFE9F | 0xFF40..=0xFF4B => {
                return self.video_controller.borrow_mut().load8(addr);
            }
            _ => {}
        }

        if SoundControllerState::addr_mapped(addr) {
            return self.sound.load8(addr);
        }

        if addr >= 0xC000 && addr <= 0xDFFF {
            return Ok(self.work_ram[(addr - 0xC000) as usize]);
        }

        if addr < 0x8000 || (addr >= 0xA000 && addr < 0xC000) {
            return self.external_controller.load8(addr);
        }

        if addr >= 0xff04 && addr <= 0xff07 {
            return self.timer.borrow_mut().load8(addr);
        }

        if addr == 0xff46 {
            return Err(err_msg("LCD OAM DMA register is write only."));
        }

        if addr >= 0xff80 && addr < 0xffff {
            return Ok(self.high_ram[(addr - 0xff80) as usize]);
        }

        if addr == 0xff0f {
            return Ok(self.interrupts.borrow_mut().flag);
        }

        if addr == 0xffff {
            return Ok(self.interrupts.borrow_mut().enabled);
        }

        Err(err_msg(format!(
            "Loading from unsupported memory range: {:04X}",
            addr
        )))
    }
}
