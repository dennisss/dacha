use core::mem::transmute;
use core::ops::{Deref, DerefMut};

use common::register::{RegisterRead, RegisterWrite};
use executor::interrupts::wait_for_irq;
use peripherals::raw::spim0::{SPIM0, SPIM0_REGISTERS};
use peripherals::raw::spim1::SPIM1;
use peripherals::raw::spim2::SPIM2;
use peripherals::raw::spim3::SPIM3;
use peripherals::raw::PinLevel;
use peripherals::raw::{Interrupt, InterruptState, PinDirection, TaskRegister};

use crate::gpio::GPIOPin;
use crate::pins::{connect_pin, connect_optional_pin, PeripheralPin};

// TODO: Codegen this.
pub struct SPIMx {
    base_address: u32,
    interrupt: Interrupt,
    all_features: bool,
}

impl SPIMx {
    pub fn all_features_supported(&self) -> bool {
        self.all_features
    }
}

impl Deref for SPIMx {
    type Target = SPIM0_REGISTERS;

    fn deref(&self) -> &Self::Target {
        unsafe { ::core::mem::transmute(self.base_address) }
    }
}

impl DerefMut for SPIMx {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { ::core::mem::transmute(self.base_address) }
    }
}

macro_rules! spimx_from {
    ($t:ident, $i:ident, $f:expr) => {
        impl From<$t> for SPIMx {
            fn from(mut value: $t) -> Self {
                SPIMx {
                    base_address: unsafe {
                        core::mem::transmute::<&mut SPIM0_REGISTERS, u32>(value.deref_mut())
                    },
                    interrupt: Interrupt::$i,
                    all_features: $f
                }
            }
        }
    };
}

spimx_from!(SPIM0, SPI0_SPIM0_SPIS0_TWI0_TWIM0_TWIS0, false);
spimx_from!(SPIM1, SPI1_SPIM1_SPIS1_TWI1_TWIM1_TWIS1, false);
spimx_from!(SPIM2, SPI2_SPIM2_SPIS2, false);
spimx_from!(SPIM3, SPIM3, true);

// Depends on HFCLK for precise clock timing.
pub struct SPIHost {
    periph: SPIMx,

    /// Present when doing CS toggling in software.
    cs: Option<GPIOPin>,
}

impl SPIHost {
    // NOTE: Chip select is not supported in most of the SPIM peripherals so instead
    // we implement it in software.
    //
    // TODO: All callers are expected to configure the GPIO pins in the GPIO peripheral as described in
    // https://docs.nordicsemi.com/bundle/ps_nrf52840/page/spim.html#ariaid-title4
    pub fn new<MOSI: PeripheralPin, MISO: PeripheralPin, SCK: PeripheralPin>(
        mut periph: SPIMx,
        frequency: usize,
        mosi: Option<MOSI>,
        miso: Option<MISO>,
        sck: Option<SCK>,
        mut cs: Option<GPIOPin>,
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

        connect_optional_pin(mosi, &mut periph.psel.mosi);
        connect_optional_pin(miso, &mut periph.psel.miso);
        connect_optional_pin(sck, &mut periph.psel.sck);
        if periph.all_features {
            connect_optional_pin(cs.take(), &mut periph.psel.csn);
            periph.csnpol.write_low();
        }

        if let Some(cs) = &mut cs {
            cs.set_direction(PinDirection::Output).write(PinLevel::High);
        }

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
        if let Some(cs) = &mut self.cs {
            cs.write(PinLevel::Low);
        }

        self.setup_transfer(write_data, read_data);

        let mut transfer = SPIHostTransfer {
            periph: &mut self.periph,
            cs: &mut self.cs,
            running: false,
        };

        // TODO: Reset the event initially.

        transfer.periph.tasks_start.write_trigger();
        transfer.running = true;

        // Wait for EVENTS_END
        {
            transfer.periph.intenset.write_with(|v| v.set_end());

            while transfer.periph.events_end.read().is_notgenerated() {
                wait_for_irq(transfer.periph.interrupt).await;
            }

            transfer.periph.events_end.write_notgenerated();

            // TODO: Need to ensure this is cleared on future drop.
            transfer.periph.intenclr.write_with(|v| v.set_end());
        }


        transfer.running = false;
    }

    /// Just sets up the input/output buffers for the next transfer
    /// This assumes the user separately handles calling start_task
    /// and ensuring that transfers finish before buffers are deallocated.
    pub fn setup_transfer(&mut self, write_data: &[u8], read_data: &mut [u8]) {
        self
            .periph
            .txd
            .ptr
            .write(unsafe { transmute::<*const u8, u32>(write_data.as_ptr()) });
        self.periph.txd.maxcnt.write(write_data.len() as u32);

        self
            .periph
            .rxd
            .ptr
            .write(unsafe { transmute::<*const u8, u32>(read_data.as_ptr()) });
        self.periph.rxd.maxcnt.write(read_data.len() as u32);
    }


    pub fn start_task(&mut self) -> &mut TaskRegister {
        &mut self.periph.tasks_start
    }

    pub fn into_inner(mut self) -> SPIMx {
        self.periph.enable.write_disabled();
        // TODO: Also reset the GPIO pin if we used it separately.
        self.periph
    }

    pub fn disable(&mut self) {
        self.periph.enable.write_disabled();
    }
}

// TODO: Add this back
/*
impl Drop for SPIHost {
    fn drop(&mut self) {
        self.periph.enable.write_disabled();
    }
}
*/

pub enum SPIMode {
    Mode0,
    Mode1,
    Mode2,
    Mode3,
}

struct SPIHostTransfer<'a> {
    // TODO: Instead want a reference to the full instance.
    periph: &'a mut SPIMx,
    cs: &'a mut Option<GPIOPin>,
    running: bool,
}

impl<'a> Drop for SPIHostTransfer<'a> {
    fn drop(&mut self) {
        if let Some(cs) = self.cs {
            cs.write(PinLevel::High);
        }

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
