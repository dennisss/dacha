use std::time::{Duration, Instant};
use std::collections::HashMap;

use common::errors::*;
use file::{LocalPath, LocalPathBuf, LocalFile};
use peripherals_proto::peripherals::*;
use common::io::Writeable;
use math_compute::io::CSVReader;
use common::hash::FastHasherBuilder;
use electronics::*;
use executor::child_task::ChildTask;
use peripherals_proto::peripherals::TMC2209Config;

use crate::toolhead::test_driver::*;
use crate::optimizer::*;
use crate::ptc_heater_model::*;
use crate::toolhead::training_data::*;
use crate::toolhead::thermal_model::*;
use crate::toolhead::heater_controller::*;
use crate::tmc2209::TMC2209Device;





/*
####################################3
Next steps (benchmarking strategy):

Each round:
- Turn on the middle fan and verify speed after a few seconds.
- Set power to 100% on the hotend
- Wait for temperature of 250 C on any dimension
- Turn off hotend
- Wait for cooldown to < 40C
- Set power to 50% on the hotend
- Wait for same time or until 250 C on any dimension
- Wait for cooldown to < 100C
- Set power to 25% on the hotend
- Wait for same time or until 250 C on any dimension


Test with the nozzle moving:

- For each type of filament
    - 

- Sensors:
    - Internal Thermocouple (when Dry)
    - Heater current
    -

# Round 1 (Dry Nozzle, Stock Sock, No Cooling)

- Heat up at 100% power and measure the 

# Round 2 (Dry Nozzle, Stock Sock, Cooling @ 100%)

# Round 3 (Dry Nozzle, New Sock, No Cooling)

# Round 4 (Dry Nozzle, New Sock, Cooling @ 100%)

<Stop and calibrate the step_per_mm amount>

# Round 5 (Wet Nozzle, New Sock, No Cooling)
(To observe extra heat added to filament)

# Round 5 (Wet Nozzle Extruding, New Sock, No Cooling)
(to observe heat loss due to extruding filament)
(note: I need to extrude really fast for this to be measurable).
(note: need to extrude enough filament so that pressure advance is negligable on average)

* This last part is interesting because we need to heat up in advance of a future action.





Calculating error:
- Set heater amount
- Do time step forward
- Set heater amount again
    - Assuming timesteps are small, don't need to deal with micro-adjustments to heater value.





Later

- Filament
    - Takes heat away from the nozzle
    - For now assume that this dosn't lose heat

cargo build --bin cnc_controller --release

scp target/release/media_thermal dennis@10.1.0.126:~/tests

ssh dennis@10.1.0.126

DISPLAY=:0 ./media_thermal record --output_path=toolhead_dry_ring_sock_thermal.log --min_temp=20 --max_temp=250

DISPLAY=:0 ./media_thermal record --output_path=toolhead_dry_full_sock_thermal.log --min_temp=20 --max_temp=250



cargo run --bin media_thermal --release -- \
    encode-mp4 \
    --min_temp=20 --max_temp=250 \
    --input_path=dump/toolhead_dry_full_sock_thermal.log \
    --output_path=dump/oolhead_dry_full_sock_thermal.mp4






cargo run --bin cnc_controller --release -- measure-toolhead \
    --log_path=toolhead_dry_full_sock_data.csv \
    --psu_addr=10.1.0.136 \
    --multimeter_addr=10.1.0.134


scp -r dennis@10.1.0.126:~/tests .



Results:

- Ring
    - No Fan
        - Ambient to 240: 43
        - 240 -> 35: 582.6
    - With fan
        - Ambient to 240 : 2215.45 - 2172.81 = 42.64
        - 240 to 35 : 2511.99 - 2215.45 = 296.54

- Full
    - No Fan
        - Ambient to 240: 44s
        - 240 to 35 : 594.38
    - With Fan
        - Ambient to 240 : 2259.56 - 2214.45  : 45.11
        - 240 to 35: 2591.04 - 2259.56 = 331.48 (~10% improvement)


===========================


cargo run --bin cnc_controller --release -- train-toolhead-heater-curve \
    --log_path=toolhead_dry_full_sock_data.csv 

*/


#[derive(Args)]
pub struct MeasureToolheadCommand {
    log_path: LocalPathBuf,
    psu_addr: String,
    multimeter_addr: Option<String>,
}

impl MeasureToolheadCommand {

    pub async fn run(self) -> Result<()> {
        let mut driver = ToolheadTestDriver::create(
            Some(self.log_path),
            Some(&self.psu_addr),
            self.multimeter_addr.as_ref().map(|s| s.as_str())
        ).await?;

        println!("# Cool below 35C");
        driver.set_fan_duty_cycle(1.0).await?;
        driver.wait_for_temp(|t| t <= 35.0, None).await?;

        println!("Start: [y/N]?");
        if !file::read_user_confirmation().await? {
            return Ok(());
        }

        driver.start_logging().await?;

        for fan_power in [0.0, 1.0] {
            println!("## Fan {}", fan_power);
            driver.set_fan_duty_cycle(fan_power).await?;

            println!("# 100% power ramp");
            let t1 = Instant::now();
            driver.set_heater_duty_cycle(1.0).await?;
            driver.wait_for_temp(|t| t >= 240.0, None).await?;
            let t2 = Instant::now();

            let ramp1_time = t2 - t1;

            println!("# Cool below 35C");
            driver.set_heater_duty_cycle(0.0).await?;
            driver.wait_for_temp(|t| t <= 35.0, None).await?;

            println!("# 50% power ramp");
            driver.set_heater_duty_cycle(0.5).await?;
            driver.wait_for_temp(|t| t >= 240.0, Some(2 * ramp1_time)).await?;

            println!("# 25% power ramp");
            driver.set_heater_duty_cycle(0.25).await?;
            driver.wait_for_temp(|t| t >= 260.0, Some(2 * ramp1_time)).await?;

            println!("# Cool below 100C");
            driver.set_heater_duty_cycle(0.0).await?;
            driver.wait_for_temp(|t| t <= 100.0, None).await?;

            println!("# 75% power ramp");
            driver.set_heater_duty_cycle(0.75).await?;
            driver.wait_for_temp(|t| t >= 260.0, Some(2 * ramp1_time)).await?;

            println!("# Cool below 40C");
            driver.set_heater_duty_cycle(0.0).await?;
            driver.wait_for_temp(|t| t <= 40.0, None).await?;

            println!("# 10% Duty Cycle");
            driver.set_heater_duty_cycle(0.1).await?;
            driver.wait_for_temp(|t| t >= 240.0, Some(6 * ramp1_time)).await?;

            println!("# Cool below 40C");
            driver.set_heater_duty_cycle(0.0).await?;
            driver.wait_for_temp(|t| t <= 40.0, None).await?;
        }

        driver.stop_logging().await?;

        println!("Done!");

        Ok(())
    }

}


#[derive(Args)]
pub struct TrainToolheadHeaterCurveCommand {
    log_path: LocalPathBuf,
}

impl TrainToolheadHeaterCurveCommand {

    pub async fn run(self) -> Result<()> {
        let data = ToolheadTrainingData::read_csv(&self.log_path).await?;

        let mut input = PTCHeaterOptimizerInput::create(data);
        gradient_descent(&mut input, OptimizerOptions::default()).await?;

        println!("{:?}", input.model());

        Ok(())
    }

}

#[derive(Args)]
pub struct TrainToolheadModelCommand {
    log_path: LocalPathBuf,

    // TODO: Add heater curve inputs.
}

impl TrainToolheadModelCommand {

    pub async fn run(self) -> Result<()> {
        let data = ToolheadTrainingData::read_csv(&self.log_path).await?;

        // TODO: For this stage, we should validate that all inputs have a nozzle_temp and heater_temp

        let mut input = ToolheadThermalOptimizerInput::create(data.clone());


        {
            let model = ToolheadThermalModel::create(input.weights());
            
            let mut out = ToolheadTrainingData::default();
            model.calculate_error(&data, Some(&mut out));

            let mut csv = String::new();
            out.csv_to(&mut csv);

            file::write("toolhead_sim.csv", csv.as_bytes()).await?;

        }

        let mut options = OptimizerOptions::default();
        options.min_error_improvement_fraction = None;
        gradient_descent(&mut input, options).await?;


        Ok(())
    }

}

use std::sync::Arc;

use peripherals_proto::peripherals::{StepperMotorMotion, StepperMotorStatus};
use peripherals_service::device::PeripheralsDevice;

async fn motor_active(driver: &TMC2209Device) -> Result<bool> {
    let status = driver.get_stepper_motor_status().await?;
    Ok(status.active())
}

async fn step(n: u32, device: &PeripheralsDevice, driver: &TMC2209Device) -> Result<()> {


    let time = device.get_clock_time().await?.remote_time;

    let mut start_time = time + 4_000_000;

    {
        let mut m = StepperMotorMotion::default();
        m.set_next_step_time(start_time);
        m.set_num_steps(n);
        m.set_next_step_duration(8_000u32);
        driver.enqueue_stepper_motion(m).await?;
    }

    Ok(())

}

#[derive(Args)]
pub struct ControlToolheadHeaterCommand {
    log_path: Option<LocalPathBuf>,
    target_temperature: f32,
}

impl ControlToolheadHeaterCommand {

    pub async fn run(self) -> Result<()> {
        let weights = &[
            15.831538, 0.20786208, 0.5193688, 0.67528397
        ];

        let mut stepper_config = TMC2209Config::default();
        protobuf::text::parse_text_proto(r#"
            step_peripheral: "stepper_step"
            uart_peripheral: "stepper_uart"
            diag_peripheral: "stepper_diag"
            enable_peripheral: "stepper_enable"
        "#, &mut stepper_config)?;

        /*
        {
            let data = ToolheadTrainingData::read_csv(LocalPath::new("/home/dennis/workspace/dacha/toolhead_dry_full_sock_data.csv")).await?;

            let model = ToolheadThermalModel::create(weights);
            
            let mut out = ToolheadTrainingData::default();
            model.calculate_error(&data, Some(&mut out));

            let mut csv = String::new();
            out.csv_to(&mut csv);

            file::write("toolhead_sim.csv", csv.as_bytes()).await?;

            return Ok(());
        }
        */

        /*        
        {
            let mut controller = ToolheadHeaterController::create(weights, 25.0);
            controller.set_target_nozzle_temperature(self.target_temperature);

            let heater = controller.next_control_input(25.0).await?;

            println!("{}", heater);

            return Ok(());
        }
        */


        let mut driver = ToolheadTestDriver::create(
            self.log_path.clone(), None, None).await?;

        let stepper = TMC2209Device::create(stepper_config, driver.device()).await?;
        stepper.enable().await?;

        // Self::continous_feed_inner(driver.device()).await?;

        if self.log_path.is_some() {
            driver.start_logging().await?;
        }

        let initial_state = driver.read_state().await?;



        let mut controller = ToolheadHeaterController::create(weights, initial_state.heater_temp.unwrap());
        controller.set_target_nozzle_temperature(self.target_temperature);


        let cancellation_token = executor::signals::new_shutdown_token();

        /*
        547
        : 108 -> extrude 100mm -> left with 30mm
        : 701.282051

        Then 109 -> 10
        : So 708.365708
        */

        let steps_per_mm: f32 = 708.365708;

        let mut next_control_time = Instant::now();

        while !cancellation_token.is_cancelled().await {
            let now = Instant::now();

            // TODO: Make sure this is fast.
            let state = driver.read_state().await?;

            if now >= next_control_time {                
                let heater = controller.next_control_input(state.heater_temp.unwrap()).await?;
                driver.set_heater_duty_cycle(heater).await?;

                // TODO: Coordinate this time with the controller
                next_control_time = now + Duration::from_secs(1);
            }

            let user_button_pressed = !driver.device().gpio_read("user_button").await?;
            let mut stepper_motor_active = motor_active(&stepper).await?;

            if user_button_pressed && !stepper_motor_active {
                // TODO: Why can't I run this in a parallel loop.
                step((100.0 * steps_per_mm).round() as u32, &driver.device(), &stepper).await?;
                stepper_motor_active = true;
            }

            let mut color = [0u8; 3];
            if stepper_motor_active {
                color[0] = 0xff;
            } else if state.heater_temp.unwrap() > 210.0 {
                color[1] = 0xff;
            }

            driver.device().neopixel_transfer("leds", &color).await?;



            let end_time = Instant::now();
            if end_time >= next_control_time {
                println!("[Slow control step]");
            } else {
                executor::sleep(Duration::from_millis(10)).await?;
                // executor::sleep(target_time - end_time).await?;
            }
        }

        println!("Finishing...");

        driver.set_heater_duty_cycle(0.0).await?;
        stepper.disable().await?;

        if self.log_path.is_some() {
            driver.stop_logging().await?;
        }



        /*
        some threads:

        - 


        */

        /*
        let mut stdin = Stdin::get();

        // Note that we will also consume any newline characters added.
        let mut buf = [0u8; 10];
        let n = stdin.read(&mut buf[..]).await?;

        */

        Ok(())
    }

}

use crate::pid::*;




#[derive(Args)]
pub struct ToolheadPIDCommand {
    log_path: Option<LocalPathBuf>,
}

impl ToolheadPIDCommand {

    pub async fn run(self) -> Result<()> {
        let target_temp = 215.0;
        
        let mut pid = PIDController::new();

        // For simulating how the PID controller performs.
        /*
        let weights = &[
            15.831538, 0.20786208, 0.5193688, 0.67528397
        ];

        let mut model = ToolheadThermalModel::create(weights);
        let start = Instant::now();
    
        for i in 0..100 {
           
            let current_temp = model.fem.elements[model.ring_el];
            println!("{}", current_temp);
            // println!("=> {}", pid.k_i * pid.error_integral)

            let error = target_temp - current_temp;

            let value = pid.next(error, start + Duration::from_secs(i));
            model.set_heater(value);
            model.fem.step(1.0);
        }
        */

        let mut driver = ToolheadTestDriver::create(
            self.log_path.clone(), None, None).await?;


        let cancellation_token = executor::signals::new_shutdown_token();

        while !cancellation_token.is_cancelled().await {
            let now = Instant::now();
            let state = driver.read_state().await?;

            let current_temp = state.heater_temp.as_ref().unwrap();
            let error = target_temp - current_temp;

            let input = pid.next(error, now);

            driver.set_heater_duty_cycle(input).await?;


            executor::sleep(Duration::from_millis(1000)).await?;
        }

        println!("Finishing...");

        driver.set_heater_duty_cycle(0.0).await?;

        if self.log_path.is_some() {
            driver.stop_logging().await?;
        }

        Ok(())

    }

}



