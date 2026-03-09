use std::{collections::HashMap, sync::Arc, time::Instant};
use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};

use base_error::*;
use executor_multitask::RootResource;
use file::LocalPathBuf;
use peripherals_service::config::*;
use peripherals_service::device::*;
use peripherals_proto::peripherals::PeripheralRequest;
use peripherals_proto::peripherals::{StepperMotorMotion, StepperMotorMotion_Direction, StepperMotorStatus, StepperMotorStatus_StoppedReason};
use peripherals_service::utilization_tracker::*;

/*
cargo run --bin builder -- build //pkg/nordic:nordic_radio_dongle --config=//pkg/nordic:nrf52840

cargo run --bin flasher -- built/pkg/nordic/nordic_radio_dongle uf2-dfu --usb_device_id=8888:

cargo run --bin peripheral_tester
*/



#[derive(Args)]
pub struct BenchmarkCommand {
    #[arg(positional)]
    mode: Mode
}

#[derive(Args)]
pub enum Mode {
    /// Loadtests the MCU by sending many no-op requests in parallel.
    /// (use to measure peak USB command rate throughput)
    #[arg(name = "noop")]
    Noop,

    #[arg(name = "one-step-motion")]
    OneStepMotion
}

struct BinarySearch {
    min: u32,
    max: u32,
    current: u32,
}

impl BinarySearch {
    pub fn new(min: u32, max: u32) -> Self {
        Self {
            min,
            max,
            current: (min + max) / 2
        }
    }

    pub fn done(&self) -> bool {
        self.min == self.max
    }

    pub fn current(&self) -> u32 {
        self.current
    }

    pub fn greater_eq_current(&mut self) {
        self.min = self.current;
        self.current = (self.min + self.max) / 2;
    }

    pub fn greater_than_current(&mut self) {
        self.min = self.current + 1;
        self.current = (self.min + self.max) / 2;
    }

    pub fn less_than_current(&mut self) {
        self.max = self.current - 1;
        self.current = (self.min + self.max) / 2;
    }

    pub fn less_eq_current(&mut self) {
        self.max = self.current;
        self.current = (self.min + self.max) / 2;
    }
}



impl BenchmarkCommand {
    pub async fn run(self) -> Result<()> {
        match self.mode {
            Mode::Noop => Self::run_noop_test().await,
            Mode::OneStepMotion => Self::run_step_motion_test().await,
        }
    }

    /*
    Best so far is 615.
    After optimizations best is 238
    Move to dedicated interrupt and FixedVec : 126
    More tuning: 111
    */

    async fn run_step_motion_test() -> Result<()> {
        let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let config = configs.remove(&"benchmark")
            .ok_or_else(|| err_msg("No config with the given name"))?;
        let (mut device, _) = PeripheralsDevice::create(&config).await?;

        let device = Arc::new(device);

        let util_tracker = RemoteUtilizationTracker::create();
        util_tracker.add_device("mcu", device.clone()).await?;


        let mut search = BinarySearch::new(10, 10_000);

        while !search.done() {
            println!("########");
            println!("Try {}", search.current());
            let pass = Self::run_single_step_motion_round(&device, search.current()).await?;
            println!("=> Pass: {}", pass);

            if pass {
                search.less_eq_current();
            } else {
                search.greater_than_current();
            }
        }

        println!("########");
        println!("Min Stable Step Duration: {}", search.current());


        Ok(())
    }

    /*
    # Steppers configured for random unconnected pins on the board.
    peripherals {
        name: "stepper1_step"
        stepper {
            step_pin_name: "0.13"
            dir_pin_name: "0.15"
        }
    }
    peripherals {
        name: "stepper2_step"
        stepper {
            step_pin_name: "0.17"
            dir_pin_name: "0.20"
        }
    }

    */

    /// Returns whether or not the test passed.
    async fn run_single_step_motion_round(device: &PeripheralsDevice, step_duration: u32) -> Result<bool> {
        device.reset_stepper_motor_queue("stepper1_step").await?;

        let time = device.get_clock_time().await?.remote_time;

        // 250ms
        let mut start_time = time + 4_000_000;

        let target_time = 5 * 16_000_000; // 5 seconds.

        let num_steps = target_time / step_duration;

        {
            let mut m = StepperMotorMotion::default();
            m.set_direction(StepperMotorMotion_Direction::FORWARD);
            m.set_next_step_time(start_time);
            m.set_num_steps_minus_one(num_steps - 1);
            m.set_next_step_duration(step_duration);
            device.enqueue_stepper_motion("stepper1_step", m.clone()).await?;

            println!("{:?}", m);
        }

        loop {
            let status = device.get_stepper_motor_status("stepper1_step").await?;
            
            if !status.active() {
                println!("Final status: {:?}", status.stopped());
                return Ok(status.stopped() == StepperMotorStatus_StoppedReason::NONE);
            }

            executor::sleep(Duration::from_millis(200)).await;
        }
    }

    async fn run_noop_test() -> Result<()> {
        let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let config = configs.remove(&"benchmark")
            .ok_or_else(|| err_msg("No config with the given name"))?;
        let (mut device, _) = PeripheralsDevice::create(&config).await?;

        let device = Arc::new(device);

        let util_tracker = RemoteUtilizationTracker::create();
        util_tracker.add_device("mcu", device.clone()).await?;

        let mut counter = Arc::new(AtomicU64::new(0));

        for i in 0..32 {
            let device2 = device.clone();
            let counter = counter.clone();
            executor::spawn(async move {
                // TODO: Ideally try to use buffer requests here.
                let mut req = PeripheralRequest::default();
                req.set_noop(true);

                loop {
                    device2.send_request(&req).await.unwrap();
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            });
        }

        loop {
            let t1 = Instant::now();
            let c1 = counter.fetch_add(0, Ordering::SeqCst);
            let b1 = device.raw().transfered_bytes().await?;

            executor::sleep(Duration::from_millis(1000)).await?;
            
            let t2 = Instant::now();
            let c2 = counter.fetch_add(0, Ordering::SeqCst);
            let b2 = device.raw().transfered_bytes().await?;

            println!("noop rate: {:.1} ; usb data rate: {:.1} Mbps",
                ((c2 - c1) as f64) / (t2 - t1).as_secs_f64(),
                8.0 * ((b2 - b1) as f64) / (t2 - t1).as_secs_f64() / 1_000_000.0,
            );
        }

        Ok(())
    }
}