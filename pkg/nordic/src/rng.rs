use common::register::RegisterRead;
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


pub struct Xoshiro128PlusPlus {
    state: [u32; 4],
}

impl Xoshiro128PlusPlus {
    pub const fn new(seed: [u32; 4]) -> Self {
        Self { state: seed }
    }

    pub fn next(&mut self) -> u32 {
        let result = self.state[0]
            .wrapping_add(self.state[3])
            .rotate_left(7)
            .wrapping_add(self.state[0]);

        let t = self.state[1] << 9;

        self.state[2] ^= self.state[0];
        self.state[3] ^= self.state[1];
        self.state[1] ^= self.state[2];
        self.state[0] ^= self.state[3];

        self.state[2] ^= t;
        self.state[3] = self.state[3].rotate_left(11);

        result
    }

    pub fn range(&mut self, min: u32, max: u32) -> u32 {
        if min >= max {
            return min;
        }

        let range = max - min;
        
        // Calculate the threshold to avoid modulo bias
        // This is equivalent to (2^32 % range)
        let limit = u32::MAX - (u32::MAX % range);

        loop {
            let r = self.next();
            if r < limit {
                return min + (r % range);
            }
            // If r >= limit, we reject it and loop again
        }
    }
}