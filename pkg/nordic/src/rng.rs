use peripherals::raw::register::RegisterRead;
use peripherals::raw::Interrupt;

pub struct Rng {
    periph: peripherals::raw::rng::RNG,
}

impl Rng {
    pub fn new(mut periph: peripherals::raw::rng::RNG) -> Self {
        periph
            .config
            .write_with(|v| v.set_dercen_with(|v| v.set_enabled()));
        periph.events_valrdy.write_notgenerated();
        periph.intenset.write_with(|v| v.set_valrdy());

        Self { periph }
    }

    // TODO: If we need to generate more than one byte, we might as well use a
    // shortcut.
    // TODO: Only 8 bits are generated at a time (not 32)
    pub async fn generate(&mut self, mut out: &mut [u32]) {
        self.periph.events_valrdy.write_notgenerated();
        self.periph.tasks_start.write_trigger();

        while !out.is_empty() {
            while self.periph.events_valrdy.read().is_notgenerated() {
                executor::interrupts::wait_for_irq(Interrupt::RNG).await;
            }
            self.periph.events_valrdy.write_notgenerated();

            out[0] = self.periph.value.read();
            out = &mut out[1..];
        }

        self.periph.tasks_stop.write_trigger();
    }
}
