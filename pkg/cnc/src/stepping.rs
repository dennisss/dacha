

/*
Based on the "Generate stepper-motor speed profiles in real time" paper.

TODO: Unit test this.

https://ww1.microchip.com/downloads/en/Appnotes/doc8017.pdf
https://www.ti.com/lit/an/slyt482/slyt482.pdf?ts=1763387854733
https://gemini.google.com/app/fd789ec70d51d588



*/


pub struct StepDurationSchedule {
    next_duration: u32,
    next_i: u32,
    num_steps: u32,
    accelerating: bool,
}

impl StepDurationSchedule {

    pub fn create(start_velocity: f32, acceleration: f32, num_steps: u32, clock_frequency: u32) -> Self {

        let (next_duration, next_i) = {
            if start_velocity <= 0.01 {
                // Starting from rest
                let initial_duration = 0.676 * (2.0 / acceleration.abs()).sqrt();
                let initial_i = 0;
                (initial_duration, initial_i)
            } else {
                let initial_duration = 1.0 / start_velocity;
                // TODO: Round?
                let initial_i = ((start_velocity * start_velocity) / (2.0 * acceleration.abs())) as u32;
                (initial_duration, initial_i)
            }
        };

        Self {
            // TODO: Round?
            next_duration: (next_duration * (clock_frequency as f32)) as u32,
            next_i,
            num_steps,
            accelerating: acceleration > 0.0 
        }
    }

    pub fn next(&mut self) -> Option<u32> {
        if self.num_steps == 0 {
            return None;
        }

        let t = self.next_duration;

        self.num_steps -= 1;
        self.next_i += 1;

        // TODO: Need to ensure we don't run into extremely slow motions and have this overflow.
        let delta = 2 * t / ((4 * self.next_i) + 1);

        if self.accelerating {
            self.next_duration -= delta; 
        } else {
            self.next_duration += delta;
        }

        Some(delta)
    }

}

