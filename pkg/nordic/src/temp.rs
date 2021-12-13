use peripherals::Interrupt;
use peripherals::{RegisterRead, RegisterWrite};

pub struct Temp {
    periph: peripherals::temp::TEMP,
}

impl Temp {
    pub fn new(mut periph: peripherals::temp::TEMP) -> Self {
        periph.events_datardy.write_notgenerated();
        periph.intenset.write_with(|v| v.set_datardy());

        Self { periph }
    }

    /// Returns temperature in 0.25 degree celsius units
    pub async fn measure(&mut self) -> u32 {
        self.periph.events_datardy.write_notgenerated();
        self.periph.tasks_start.write_trigger();

        while self.periph.events_datardy.read().is_notgenerated() {
            crate::interrupts::wait_for_irq(Interrupt::TEMP).await;
        }

        self.periph.events_datardy.write_notgenerated();

        self.periph.temp.read()
    }
}
