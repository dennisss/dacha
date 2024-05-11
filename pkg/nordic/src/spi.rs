use core::mem::transmute;

use executor::interrupts::wait_for_irq;
use common::register::{RegisterRead, RegisterWrite};
use peripherals::raw::spim0::SPIM0;
use peripherals::raw::PinLevel;
use peripherals::raw::{Interrupt, InterruptState, PinDirection};

use crate::gpio::GPIOPin;
use crate::pins::{connect_pin, PeripheralPin};

// Depends on HFCLK for precise clock timing.
pub struct SPIHost {
    periph: SPIM0,
    cs: GPIOPin,
}

impl SPIHost {
    // NOTE: Chip select is not supported in most of the SPIM peripherals so instead
    // we implement it in software.
    pub fn new<MOSI: PeripheralPin, MISO: PeripheralPin, SCK: PeripheralPin>(
        mut periph: SPIM0,
        frequency: usize,
        mosi: MOSI,
        miso: MISO,
        sck: SCK,
        mut cs: GPIOPin,
        mode: SPIMode,
    ) -> Self {
        match frequency {
            125_000 => periph.frequency.write_k125(),
            250_000 => periph.frequency.write_k250(),
            500_000 => periph.frequency.write_k500(),
            1_000_000 => periph.frequency.write_m1(),
            2_000_000 => periph.frequency.write_m2(),
            4_000_000 => periph.frequency.write_m4(),
            8_000_000 => periph.frequency.write_m8(),
            16_000_000 => periph.frequency.write_m16(),
            32_000_000 => periph.frequency.write_m32(),
            _ => panic!(),
        }

        periph.intenset.write_with(|v| v.set_stopped().set_end());

        connect_pin(mosi, &mut periph.psel.mosi);
        connect_pin(miso, &mut periph.psel.miso);
        connect_pin(sck, &mut periph.psel.sck);
        // connect_pin(cs, &mut periph.psel.csn);
        // periph.csnpol.write_activelow();

        cs.set_direction(PinDirection::Output).write(PinLevel::High);

        let mut config = peripherals::raw::spim0::config::CONFIG_VALUE::new();
        config.set_order_with(|v| v.set_msbfirst());

        match mode {
            SPIMode::Mode0 | SPIMode::Mode1 => {
                config.set_cpol_with(|v| v.set_activehigh());
            }
            SPIMode::Mode2 | SPIMode::Mode3 => {
                config.set_cpol_with(|v| v.set_activelow());
            }
        }

        match mode {
            SPIMode::Mode0 | SPIMode::Mode2 => {
                config.set_cpha_with(|v| v.set_leading());
            }
            SPIMode::Mode1 | SPIMode::Mode3 => {
                config.set_cpha_with(|v| v.set_trailing());
            }
        }

        periph.config.write(config);

        // If reading more than writing, pad writes with zeros.
        periph.orc.write(0);

        periph.enable.write_enabled();

        Self { periph, cs }
    }

    // TODO: Use SHORTS to implement write_then_read.

    pub async fn transfer(&mut self, write_data: &[u8], read_data: &mut [u8]) {
        self.cs.write(PinLevel::Low);
        let mut transfer = SPIHostTransfer {
            periph: &mut self.periph,
            cs: &mut self.cs,
            running: false,
        };

        transfer
            .periph
            .txd
            .ptr
            .write(unsafe { transmute::<*const u8, u32>(write_data.as_ptr()) });
        transfer.periph.txd.maxcnt.write(write_data.len() as u32);

        transfer
            .periph
            .rxd
            .ptr
            .write(unsafe { transmute::<*const u8, u32>(read_data.as_ptr()) });
        transfer.periph.rxd.maxcnt.write(read_data.len() as u32);

        transfer.periph.tasks_start.write_trigger();
        transfer.running = true;

        while transfer.periph.events_end.read().is_notgenerated() {
            wait_for_irq(Interrupt::SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0).await;
        }

        transfer.periph.events_end.write_notgenerated();
        transfer.running = false;
    }
}

pub enum SPIMode {
    Mode0,
    Mode1,
    Mode2,
    Mode3,
}

struct SPIHostTransfer<'a> {
    periph: &'a mut SPIM0,
    cs: &'a mut GPIOPin,
    running: bool,
}

impl<'a> Drop for SPIHostTransfer<'a> {
    fn drop(&mut self) {
        self.cs.write(PinLevel::High);

        self.cancel_blocking();

        // Responsible for flushing events and ensuring there is sufficient delay
        // between transfers when flipping the chip select.
        crate::events::flush_events_clear();
    }
}

impl<'a> SPIHostTransfer<'a> {
    fn cancel_blocking(&mut self) {
        if !self.running {
            return;
        }

        self.periph.tasks_stop.write_trigger();
        while self.periph.events_stopped.read().is_notgenerated() {
            // Block
        }

        self.periph.events_stopped.write_notgenerated();
        self.periph.events_end.write_notgenerated();
        self.running = false;
    }
}
