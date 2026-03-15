use std::{collections::HashMap, sync::Arc, time::Instant};
use std::collections::VecDeque;
use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};

use base_args::define_arg_command;
use base_error::*;
use executor::lock;
use executor::sync::AsyncMutex;
use executor_multitask::{RootResource, TaskResource};
use file::{LocalPathBuf, project_path, LocalPath};
use peripherals_service::config::*;
use peripherals_service::device::*;
use peripherals_proto::peripherals::PeripheralRequest;
use peripherals_proto::peripherals::{StepperMotorMotion, StepperMotorMotion_Direction, StepperMotorStatus, StepperMotorStatus_StoppedReason};
use peripherals_service::utilization_tracker::*;
use cnc_controller_proto::cnc::*;
use sstable::record_log::*;
use protobuf::{Message, StaticMessage};
use math::vecxd;
use cnc::quadratic_stepper_motion::*;
use cnc::linear_motion::LinearMotion;
use cnc_controller::proto_utils::*;
use math::matrix::{VectorXd, MatrixXd};
use cnc_controller::stats::*;
use crypto::random::RngExt;
use cnc_controller::ma732::MA732;

use crate::remote::*;
use crate::regression::*;
use crate::motion_log::*;
use crate::revolution_tracker::*;

/*
## Breadboard Motor Testing

cargo run --bin cnc_tools -- motion-analysis test-motor-encoder

cargo run --bin cnc_controller -- service     --config_name=breadboard_motor     --port=8000

cargo run --bin cnc_tools -- motion-analysis record-single-motion \
    --motion_config=breadboard-motor \
    --output_path=data/motion_analysis/breadboard_motor/angle_calibration.log

cargo run --bin cnc_tools -- motion-analysis process-saved-log \
    --motion_config=breadboard-motor \
    --input_path=data/motion_analysis/breadboard_motor/angle_calibration.log

cargo run --bin cnc_tools -- motion-analysis sweep \
    --motion_config=breadboard-motor \
    --speed_min=5 --speed_max=100 --speed_step=5 \
    --correction_log=data/motion_analysis/breadboard_motor/angle_calibration.log \
    --output_dir=data/motion_analysis/breadboard_motor/sweep_velocity_5v

======================

## Running Voron 0 Motor Encoder Testing

cargo run --bin cnc_controller -- service     --config_name=voron0     --port=8000

cargo run --bin cnc_tools -- execute --proto="commands: [{ home {} }]"

cargo run --bin cnc_tools -- motion-analysis record-single-motion \
    --motion_config=voron0-motor-left \
    --output_path=data/motion_analysis/voron0/angle_calibration.log

cargo run --bin cnc_tools -- motion-analysis process-saved-log \
    --motion_config=voron0-motor-left \
    --input_path=data/motion_analysis/voron0/angle_calibration.log

cargo run --bin cnc_tools -- motion-analysis sweep \
    --motion_config=voron0-motor-left \
    --speed_min=25 --speed_max=600 --speed_step=25 \
    --accel=4000 \
    --correction_log=data/motion_analysis/voron0/angle_calibration.log \
    --output_dir=data/motion_analysis/voron0/sweep_velocity_spreadcycle

TODO: Enable stealth for this.
    cargo run --bin cnc_tools -- motion-analysis sweep \
        --motion_config=voron0-motor-left \
        --speed_min=25 --speed_max=600 --speed_step=25 \
        --accel=4000 \
        --correction_log=data/motion_analysis/voron0/angle_calibration.log \
        --output_dir=data/motion_analysis/voron0/sweep_velocity_stealth

cargo run --bin cnc_tools -- motion-analysis sweep \
    --motion_config=voron0-motor-left \
    --speed=600 \
    --accel_min=1000 --accel_max=20000 --accel_step=500 \
    --correction_log=data/motion_analysis/voron0/angle_calibration.log \
    --output_dir=data/motion_analysis/voron0/sweep_accel_spreadcycle


cargo run --bin cnc_tools -- motion-analysis sweep \
    --motion_config=voron0-motor-left \
    --speed=600 \
    --accel_min=10000 --accel_max=20000 --accel_step=1000 \
    --correction_log=data/motion_analysis/voron0/angle_calibration.log \
    --output_dir=data/motion_analysis/voron0/sweep_velocity15

cargo run --bin cnc_tools -- motion-analysis sweep \
    --motion_config=voron0-motor-left \
    --speed_min=600 --speed_max=1000 --speed_step=100 \
    --accel=10000 \
    --correction_log=data/motion_analysis/voron0/angle_calibration.log \
    --output_dir=data/motion_analysis/voron0/sweep_velocity16

Max speed is >600mm/s

Max acceleration is something like 15000 (16000 skips a step).


================

cargo run --bin cnc_tools -- motion-analysis test-filament-sensor 

cargo run --bin cnc_tools -- execute --proto="commands: [{ set_temp { target: 215 } }]"
cargo run --bin cnc_tools -- execute --proto="commands: [{ set_temp { target: 0 } }]"

cargo run --bin cnc_tools -- execute --rel_z=50

cargo run --bin cnc_tools -- execute --extrude=10


cargo run --bin cnc_tools -- motion-analysis sweep-extruder \
    --output_dir=data/motion_analysis/voron0/sweep_extruder3

*/


// 16 * 200
const STEPS_PER_REVOLUTION: f64 = 3200.0;

const STEPS_PER_FULL_STEP: f64 = 16.0;


define_arg_command!(MotionAnalysisCommand {
    TestPatternMode = "test-pattern",

    TestMotorEncoderMode = "test-motor-encoder",
    
    TestFilamentSensor = "test-filament-sensor",

    /// Performs a single linear motion and records all the collected data to a log file.
    RecordSingleMotionMode = "record-single-motion",

    /// This does one-off analysis and stats dumping for a single log file collected by
    /// one of the other commands.
    ProcessSavedLogMode = "process-saved-log",

    /// Sweeps across different velocities 
    SweepMode = "sweep",

    /// Tries to find the max feed rate of the extruder by progressively
    /// increasing 
    ExtruderSweepMode = "sweep-extruder",
});

#[derive(Args)]
enum MotionConfig {
    #[arg(name = "breadboard-motor")]
    BreadboardMotor,

    /// Encoder on stepper2 (index 1)
    /// Motions between [5, 5] and [115, 115]
    #[arg(name = "voron0-motor-left")]
    Voron0MotorLeft
}

impl MotionConfig {
    pub fn motor_index(&self) -> usize {
        match self {
            Self::BreadboardMotor => 0,
            Self::Voron0MotorLeft => 1
        }
    }

    pub fn inverted(&self) -> bool {
        match self {
            Self::BreadboardMotor => false,
            Self::Voron0MotorLeft => false
        }
    }

    // Note that this should be called before logging starts.
    pub async fn setup(&self, machine: &mut RemoteMachineController) -> Result<()> {
        match self {
            Self::BreadboardMotor => {}
            Self::Voron0MotorLeft => {
                machine.home().await?;

                // Move to initial point.
                let mut pos = machine.last_position().await?;
                pos[0] = 5.0;
                pos[1] = 5.0;
                machine.move_to(&pos, 10.0).await?;
                machine.wait_until_idle().await?;

                // Ensure that any future log entries don't look at the homing movements.
                executor::sleep(Duration::from_secs(1)).await?;
            }
        }    

        Ok(())
    }

    pub async fn next_position(&self, machine: &mut RemoteMachineController) -> Result<VectorXd> {
        match self {
            Self::BreadboardMotor => {
                let mut pos = machine.last_position().await?;
                pos[0] += 160.0;
                Ok(pos)
            }
            Self::Voron0MotorLeft => {
                let mut endpoints = vec![
                    vecxd!(5.0, 5.0),
                    vecxd!(115.0, 115.0),
                ];

                let pos = machine.last_position().await?;

                // Augment endpoints for extra axes
                for pt in &mut endpoints {
                    let mut new_pt = pos.clone();
                    for i in 0..pt.len() {
                        new_pt[i] = pt[i];
                    }

                    *pt = new_pt;
                }


                let mut next_endpoint_idx = 0;
                let mut next_endpoint_distance = 0.0;

                for (i, pt) in endpoints.iter().enumerate() {
                    let dist = (pt - &pos).norm();
                    if dist > next_endpoint_distance {
                        next_endpoint_idx = i;
                        next_endpoint_distance = dist;
                    }
                }

                Ok(endpoints[next_endpoint_idx].clone())
            }
        }
    }
}







async fn spi_transfer_oneshot(
    device: &PeripheralsDevice,
    periph_name: &str,
    buf_name: &str,
    data: &[u8]
) -> Result<Vec<u8>> {

    /*
    let mut out = vec![];
    out.resize(data.len(), 0);

    device.spi_transfer(
        periph_name,
        data,
        &mut out
    ).await?;
    */

    let now = device.get_clock_time().await?.remote_time;
    let start_time = now.wrapping_add(16_000_000 / 100); // 10ms in the future.

    let transfer_count = 1;

    let out = device.spi_transfer_timed(
        periph_name,
        data,
        buf_name,
        start_time,
        transfer_count,
        16_000_000 / 100, // TODO: Need to verify that the transfer finished.
    ).await?;

    Ok(out)
}

async fn read_as5047_reg(device: &PeripheralsDevice, addr: u16) -> Result<u16> {

    let req = cnc_controller::as5047p::create_as5047p_command(addr, true);

    spi_transfer_oneshot(&device, "encoder_spi", "buf1", &req).await?;
    let out = spi_transfer_oneshot(&device, "encoder_spi", "buf1", &[0, 0]).await?;

    let value = cnc_controller::as5047p::parse_as5047p_data(array_ref!(out, 0, 2))?;

    Ok(value)
}

async fn read_as5047_angle(device: &PeripheralsDevice) -> Result<Vec<f64>> {
    let addr = cnc_controller::as5047p::ANGLECOM;
    let periph_name = "encoder_spi";
    let buf_name = "buf1";

    let req = cnc_controller::as5047p::create_as5047p_command(addr, true);

    let data = {
        let now = device.get_clock_time().await?.remote_time;
        let start_time = now.wrapping_add(16_000_000 / 100); // 10ms in the future.

        let sample_rate = 8000;
        let transfer_count = 8000;
        let transfer_interval = 16_000_000 / (sample_rate as u32);

        device.spi_transfer_timed(
            periph_name,
            &req,
            buf_name,
            start_time,
            transfer_count,
            transfer_interval,
        ).await?
    };

    let mut processed_data = vec![];

    for i in 1..(data.len() / 2) {
        let angle = cnc_controller::as5047p::parse_as5047p_data(array_ref!(data, 2*i, 2))?;

        processed_data.push(
            ((angle << 2) as f64) / ((u16::max_value() as f64) + 1.0)
        );
    }

    Ok(processed_data)
}

#[derive(Args)]
pub struct TestFilamentSensor {


}

impl TestFilamentSensor {

    async fn run(self) -> Result<()> {
        let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let config = configs.remove("filament_sensor")
            .ok_or_else(|| err_msg("No config with the given name"))?;

        let (mut device, _) = PeripheralsDevice::create(&config).await?;

        let device = Arc::new(device);

        loop {
            let mut out = [0, 0];

            let res = device.i2c_transfer(
                "i2c",
                0x36, // i2c address
                &[0x0C], // send data ('RAW_ANGLE' register address)
                &mut out
            ).await?;

            println!("Value: {:?}", out);

            executor::sleep(Duration::from_millis(1000)).await?;
        }


        /*
        // let out = spi_transfer_oneshot(&device, "encoder_spi", "buf1", &[0,0,0,0]).await?;
        // println!("Test no-op: {:?}", out);

        let diag = read_as5047_reg(&device, cnc_controller::as5047p::DIAAGC).await?;

        println!("Gain: {}", diag & 0xff);
        println!("Magnet too weak: {}", (diag >> 11) & 1);
        println!("Magnet too strong: {}", (diag >> 10) & 1);
        println!("CORDIC overflow: {}", (diag >> 9) & 1);

        let mut angle_stats = NumericalMetricsTracker::default();
        for _ in 0..2 {
            let angles = read_as5047_angle(&device).await?;
            for angle in angles {
                angle_stats.add(angle);
            }
        }


        println!("Angle Stddev: {}", angle_stats.stddev() * STEPS_PER_REVOLUTION);


        loop {
            let angle = read_as5047_reg(&device, cnc_controller::as5047p::ANGLECOM).await?;
            println!("Angle: {}", angle);

            executor::sleep(Duration::from_millis(1000)).await?;

        }

        Ok(())
        */
    }
}


#[derive(Args)]
pub struct TestMotorEncoderMode {

}

impl TestMotorEncoderMode {

    async fn run(self) -> Result<()> {
        Self::run_test_motor_encoder_as5047p().await
    }

    async fn run_test_motor_encoder_as5047p() -> Result<()> {
        let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let config = configs.remove("motor_encoder_as5047p")
            .ok_or_else(|| err_msg("No config with the given name"))?;

        let (mut device, _) = PeripheralsDevice::create(&config).await?;

        let device = Arc::new(device);


        // let out = spi_transfer_oneshot(&device, "encoder_spi", "buf1", &[0,0,0,0]).await?;
        // println!("Test no-op: {:?}", out);

        let diag = read_as5047_reg(&device, cnc_controller::as5047p::DIAAGC).await?;

        println!("Gain: {}", diag & 0xff);
        println!("Magnet too weak: {}", (diag >> 11) & 1);
        println!("Magnet too strong: {}", (diag >> 10) & 1);
        println!("CORDIC overflow: {}", (diag >> 9) & 1);

        let mut angle_stats = NumericalMetricsTracker::default();
        for _ in 0..2 {
            let angles = read_as5047_angle(&device).await?;
            for angle in angles {
                angle_stats.add(angle);
            }
        }


        println!("Angle Stddev: {}", angle_stats.stddev() * STEPS_PER_REVOLUTION);


        loop {
            let angle = read_as5047_reg(&device, cnc_controller::as5047p::ANGLECOM).await?;
            println!("Angle: {}", angle);

            executor::sleep(Duration::from_millis(1000)).await?;

        }

        Ok(())
    }

    async fn run_test_motor_encoder_ma732() -> Result<()> {
        let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let config = configs.remove("motor_encoder")
            .ok_or_else(|| err_msg("No config with the given name"))?;

        let (mut device, _) = PeripheralsDevice::create(&config).await?;

        let device = Arc::new(device);

        let mag = MA732::new(device.clone());

        loop {
            println!("{:?}", mag.get_angle().await?);

            executor::sleep(Duration::from_secs(1)).await?;
        }

        Ok(())
    }

}


#[derive(Args)]
pub struct TestPatternMode {

}

impl TestPatternMode {
    pub async fn run(self) -> Result<()> {

        let mut machine = RemoteMachineController::create().await?;

        machine.home().await?;

        let mut pos = machine.last_position().await?;

        let pts = [
            (5., 5.),
            (115., 115.),
            (115., 5.),
            (5., 115.,),
            (115., 115.),
            (5., 5.)
        ];

        let speed = 600.0;
        let accel = 4000.0;

        for _ in 0..20 {
            for (x, y) in pts.iter().cloned() {
                pos[0] = x;
                pos[1] = y;
                {
                    let mut request = ExecuteRequest::default();

                    let cmd = request.new_commands();
                    let m = cmd.move_to_mut();
                    m.set_position(pos.to_proto());
                    m.options_mut().set_feed_rate(speed);
                    m.options_mut().set_acceleration(accel);
                    m.options_mut().set_force(true);

                    machine.execute(&request).await?;
                }
            }
        }

        Ok(())






    }
}

#[derive(Args)]
struct RecordSingleMotionMode {
    #[arg(default = 5.0)]
    feed_rate: f64,

    motion_config: MotionConfig,

    output_path: LocalPathBuf
}


impl RecordSingleMotionMode {
    pub async fn run(self) -> Result<()> {

        let mut machine = RemoteMachineController::create().await?;

        self.motion_config.setup(&mut machine).await?;

        let log_collector = machine.read_log().await?;

        // Collect some idle data.
        executor::sleep(Duration::from_millis(4000)).await?;

        println!("Moving...");

        let mut pos = self.motion_config.next_position(&mut machine).await?;

        machine.move_to(&pos, self.feed_rate).await?;

        machine.wait_until_idle().await?;

        // Wait for most recent log entries to come in.
        executor::sleep(Duration::from_millis(1000)).await?;

        let mut all_entries = log_collector.drain().await?;

        write_log_file(&self.output_path, &all_entries).await?;

        println!("Done!");

        Ok(())
    }
}

#[derive(Args)]
struct SpeedAccelArgs {
    speed: Option<f64>,
    speed_min: Option<f64>,
    speed_max: Option<f64>,
    speed_step: Option<f64>,

    accel: Option<f64>,
    accel_min: Option<f64>,
    accel_max: Option<f64>,
    accel_step: Option<f64>,
}

#[derive(Debug)]
struct SpeedAccelSetting {
    speed: f64,
    accel: Option<f64>
} 

impl SpeedAccelArgs {
    pub fn enumerate(&self, default_speed: f64) -> Result<Vec<SpeedAccelSetting>> {

        let mut speeds = vec![];

        if let Some(v) = &self.speed {
            if self.speed_min.is_some() || self.speed_max.is_some() || self.speed_step.is_some() {
                return Err(err_msg("Both speed range and value specified"));
            }

            speeds.push(*v);
        } else if self.speed_min.is_some() || self.speed_max.is_some() || self.speed_step.is_some() {
            if self.speed.is_some() || self.speed_min.is_none() || self.speed_max.is_none() || self.speed_step.is_none() {
                return Err(err_msg("Invalid speed range args"));
            }

            let min = self.speed_min.unwrap();
            let max = self.speed_max.unwrap();
            let step = self.speed_step.unwrap();

            let mut cur = min;
            while cur < max + 0.1 {
                speeds.push(cur);
                cur += step;
            }

            if speeds.is_empty() {
                return Err(err_msg("No speed(s) selected"));
            }
        } else {
            speeds.push(default_speed);
        }

        let mut accels = vec![];

        if let Some(v) = &self.accel {
            if self.accel_min.is_some() || self.accel_max.is_some() || self.accel_step.is_some() {
                return Err(err_msg("Both accel range and value specified"));
            }

            accels.push(Some(*v));
        } else if self.accel_min.is_some() || self.accel_max.is_some() || self.accel_step.is_some() {
            if self.accel.is_some() || self.accel_min.is_none() || self.accel_max.is_none() || self.accel_step.is_none() {
                return Err(err_msg("Invalid accel range args"));
            }

            let min = self.accel_min.unwrap();
            let max = self.accel_max.unwrap();
            let step = self.accel_step.unwrap();

            let mut cur = min;
            while cur < max + 0.1 {
                accels.push(Some(cur));
                cur += step;
            }

            if accels.is_empty() {
                return Err(err_msg("No accel(s) selected"));
            }
        } else {
            accels.push(None);
        }

        if accels.len() > 1 && speeds.len() > 1 {
            return Err(err_msg("Tools don't currently handle both accel and speed sweeping"));
        }

        let mut out = vec![];
        for speed in speeds {
            for accel in accels.iter().cloned() {
                out.push(SpeedAccelSetting {
                    speed,
                    accel
                });
            }
        }

        Ok(out)
    }

}




#[derive(Args)]
pub struct SweepMode {
    motion_config: MotionConfig,
    correction_log: LocalPathBuf,
    speeds: SpeedAccelArgs,
    output_dir: LocalPathBuf,
}

impl SweepMode {
    pub async fn run(self) -> Result<()> {

        let test_cases = self.speeds.enumerate(5.0)?;

        if file::exists(&self.output_dir).await? {
            return Err(err_msg("Output directory already exists"));
        }

        file::create_dir_all(&self.output_dir).await?;

        let mut analysis_options = MotorEncoderAnalysisOptions {
            motor_index: self.motion_config.motor_index(),
            correction_model: None,
            inverted: self.motion_config.inverted(),
        };

        {
            let mut entries = read_log_file(&self.correction_log).await?;
            let inst = MotorEncoderAnalysis::create(analysis_options.clone(), &entries)?;
            analysis_options.correction_model = Some(inst.compute_correction_model());
        }

        let mut machine = RemoteMachineController::create().await?;

        self.motion_config.setup(&mut machine).await?;

        let log_collector = machine.read_log().await?;
        
        println!("Moving...");

        let mut results = "speed,accel,mean,stddev,end_error\n".to_string();

        for test_case in test_cases {
            println!("Test: {:?}", test_case);

            // Wait for pre-motion data.
            executor::sleep(Duration::from_millis(1500)).await?;

            let pos = self.motion_config.next_position(&mut machine).await?;

            // machine.move_to(&pos, speed).await?;
            {
                let mut request = ExecuteRequest::default();

                let cmd = request.new_commands();
                let m = cmd.move_to_mut();
                m.set_position(pos.to_proto());
                m.options_mut().set_feed_rate(test_case.speed);
                if let Some(v) = test_case.accel {
                    m.options_mut().set_acceleration(v);
                }
                m.options_mut().set_force(true);

                machine.execute(&request).await?;
            }

            machine.wait_until_idle().await?;

            // Wait for most recent log entries to come in (and collect post-motion data).
            executor::sleep(Duration::from_millis(1500)).await?;

            let entries = log_collector.drain().await?;

            println!("=> Analysis");

            let analysis = MotorEncoderAnalysis::create(analysis_options.clone(), &entries)?;

            // TODO: Also check the achieved acceleration.
            // analysis.motion_log.check_hit_speed(test_case.speed as f64)?;

            let scale = STEPS_PER_REVOLUTION;

            println!("- Lag: {}", analysis.error_stats.mean() * scale);
            // TODO: replace with using 99th percentile entirel
            println!("- Error StdDev: {}", analysis.error_stats.stddev() * scale);
            
            let end_error = analysis.end_angle_error() * scale;
            println!("- End error: {}", end_error);

            println!("- Accel Error: {}", analysis.error_stats_accelerating.mean() * scale);
            println!("- Accel Error Range: {}", analysis.error_stats_accelerating.range().print_scaled(scale));
            println!("- Accel Count: {}", analysis.error_stats_accelerating.count());

            let output_prefix = format!("speed-{}-accel-{}", test_case.speed, test_case.accel.unwrap_or(0.0));

            write_log_file(&self.output_dir.join(format!("{}.log", output_prefix)), &entries).await?;
            
            file::write(&self.output_dir.join(format!("{}.csv", output_prefix)), analysis.dump_error_csv()).await?;

            // let mut results = "speed,accel,mean,stddev,end_error\n";
            results.push_str(&format!(
                "{},{},{},{},{}\n",
                test_case.speed,
                test_case.accel.unwrap_or(0.0),
                analysis.error_stats.mean() * scale,
                analysis.error_stats.stddev() * scale,
                end_error
            ));
            file::write(&self.output_dir.join("results.csv"), results.as_bytes()).await?;

            // 2 full steps skipped.
            if end_error.abs() > 32.0 {
                println!("Done. End error too high.");
                break;
            }
        }

        /*
        TODO: Also want to perform an acceleration test:
        - End error is most important for this (ideally collect separate metrics on the acceleration stage.)


        let mut request = ExecuteRequest::default();
        let cmd = request.new_commands();
        let m = cmd.move_to_mut();
        m.set_position(pos.to_proto());
        m.options_mut().set_feed_rate(feed_rate);
        */


        println!("Done!");


        Ok(())

    }


}


#[derive(Args)]
struct ProcessSavedLogMode {
    input_path: LocalPathBuf,
    motion_config: MotionConfig,
    correction_log: Option<LocalPathBuf>,
}

impl ProcessSavedLogMode {
    pub async fn run(self) -> Result<()> {
        let mut analysis_options = MotorEncoderAnalysisOptions {
            motor_index: self.motion_config.motor_index(),
            correction_model: None,
            inverted: self.motion_config.inverted(),
        };

        if let Some(path) = self.correction_log {
            let mut entries = read_log_file(&path).await?;
            let inst = MotorEncoderAnalysis::create(analysis_options.clone(), &entries)?;
            analysis_options.correction_model = Some(inst.compute_correction_model());
        }

        let mut entries = read_log_file(&self.input_path).await?;

        println!("====================");

        {
            let inst = MotorEncoderAnalysis::create(analysis_options, &entries)?;

            inst.print_stats();

            file::write(project_path!("encoder.csv"), inst.dump_error_csv().as_bytes()).await?;
        }

        Ok(())

    }
}




#[derive(Args)]
pub struct ExtruderSweepMode {
    output_dir: LocalPathBuf,
}

impl ExtruderSweepMode {

    pub async fn run(self) -> Result<()> {

        if file::exists(&self.output_dir).await? {
            return Err(err_msg("Output directory already exists"));
        }

        file::create_dir_all(&self.output_dir).await?;

        let mut machine = RemoteMachineController::create().await?;

        let log_collector = machine.read_log().await?;

        // NOTE: We assume we are already heated up.

        let extrude_amount = 100.0;

        let mut speeds = vec![];
        
        let mut x = 2.5;
        while x <= 40.1 {
            speeds.push(x);
            x += 2.5;
        }
   
        let mut csv = "speed,revolutions\n".to_string();

        for speed in speeds {

            println!("Test: {}", speed);

            // Collect pre-motion angle.
            executor::sleep(Duration::from_millis(1000)).await?;

            // Extrude
            println!("Extruding...");
            let mut pos = machine.last_position().await?;
            pos[3] += extrude_amount;
            // machine.move_to(&pos, speed).await?;
            {
                let mut request = ExecuteRequest::default();

                let cmd = request.new_commands();
                let m = cmd.move_to_mut();
                m.set_position(pos.to_proto());
                m.options_mut().set_feed_rate(speed);
                m.options_mut().set_force(true);

                machine.execute(&request).await?;
            }

            machine.wait_until_idle().await?;

            println!("=> Extruded");

            // Collect post-motion angle.
            executor::sleep(Duration::from_millis(1000)).await?;

            let entries = log_collector.drain().await?;

            let revs = Self::analyze_log(&entries);

            println!("=> revs: {}", revs);
            csv.push_str(&format!("{},{}\n", speed, revs));

            write_log_file(&self.output_dir.join(format!("speed-{}.log", speed)), &entries).await?;

            // TODO: Fix this.
            // {
            //     let motion_log = MotionLog::create(&entries)?;
            //     motion_log.check_hit_speed(speed)?;
            // }
        }

        file::write(&self.output_dir.join("results.csv"), csv.as_bytes()).await?;

        println!("Done!");

        Ok(())
    }


    fn analyze_log(entries: &[LogEntry]) -> f64 {
        let mut rev_tracker = None;
        let mut start_revolutions = 0.0;
        let mut end_revolutions = 0.0;

        for entry in entries {
            if !entry.has_sampled_data() {
                continue;
            }

            let mut time = entry.sampled_data().start_time();
            let mut remaining = &entry.sampled_data().data()[..];

            while !remaining.is_empty() {
                let buf = array_ref![remaining, 0, 2];
                remaining = &remaining[2..];

                // unit: [0, 1)
                let angle = (u16::from_be_bytes(*buf) as f64) / ((u16::max_value() as f64) + 1.0);

                if rev_tracker.is_none() {
                    rev_tracker = Some(RevolutionTracker::new(angle));
                    start_revolutions = angle;
                    end_revolutions = angle;
                }

                end_revolutions = rev_tracker.as_mut().unwrap().next(angle);

                time += entry.sampled_data().sample_interval();
            }
        }

        end_revolutions - start_revolutions
    }


}



pub async fn read_log_file(path: &LocalPath) -> Result<Vec<LogEntry>> {
    let mut reader = RecordReader::open(path).await?;
    
    let mut entries = vec![];
    while let Some(record) = reader.read().await? {
        entries.push(LogEntry::parse(&record)?);
    }

    Ok(entries)
}

pub async fn write_log_file(path: &LocalPath, entries: &[LogEntry]) -> Result<()> {
    let mut writer = RecordWriter::create_new(path).await?;

    for entry in entries {
        writer.append(&entry.serialize()?).await?;
    }

    writer.flush().await?;
    drop(writer);

    Ok(())
}

#[derive(Clone)]
pub struct MotorEncoderAnalysisOptions {
    /// Index of the motor which corresponds to the one being measured by the encoder.
    /// (the assumption is that the log data only contains a single encoder for now)
    motor_index: usize,

    correction_model: Option<FourierRegression>,

    inverted: bool,
}

/// This analyzes the motion of a motion which is directly measured by an angular position encoder
/// attached to one of the motors.
pub struct MotorEncoderAnalysis {
    options: MotorEncoderAnalysisOptions,

    motion_log: MotionLog,

    /// Units: [0, 1) angle.
    start_angle: Option<f64>,

    /// Units: Absolute step count.
    start_motor_pos: i32,

    /// Units: [0, 1) angle.
    angle_premotion: NumericalMetricsTracker,

    /// Units: [0, 1) angle.
    angle_postmotion: NumericalMetricsTracker,

    /// Units: [0, 1) angle.
    error_stats: NumericalMetricsTracker,

    error_stats_accelerating: NumericalMetricsTracker,

    /// Data points are mappings from measured encoder position to 
    ///
    /// Units: [0, 1) angle.
    error_data: Vec<(f64, f64)>,

    output_data: Vec<MotorEncoderDataRow>
}

struct MotorEncoderDataRow {
    time: f64,

    /// Raw angle reported by the encoder.
    angle_raw: f64,

    angle_corrected: f64,

    error: f64,

    actual_step: f64,

    /// Target step 
    target_step: f64,
}

impl MotorEncoderAnalysis {

    pub fn create(options: MotorEncoderAnalysisOptions, entries: &[LogEntry]) -> Result<Self> {
        let motion_log = MotionLog::create(&entries)?;
        let start_motor_pos = motion_log.start_motor_position[options.motor_index];

        let mut inst = Self {
            options,

            motion_log,

            start_angle: None,
            start_motor_pos,

            angle_premotion: NumericalMetricsTracker::default(),
            angle_postmotion: NumericalMetricsTracker::default(),

            error_stats: NumericalMetricsTracker::default(),
            error_stats_accelerating: NumericalMetricsTracker::default(),
            error_data: vec![],

            output_data: vec![],
        };

        let mut seen_start = false;

        let mut revolution_tracker = None;

        for entry in entries {
            if entry.has_motion_start() {
                // TODO: We can do this later since we may have some entry data right after 
                // let a = inst.angle_premotion.mean();
                // inst.start_angle = Some(a);
                seen_start = true;
            }

            if !entry.has_sampled_data() {
                continue;
            }

            let mut time = entry.sampled_data().start_time();
            let mut remaining = &entry.sampled_data().data()[..];

            let mut sample_i = 0;
            while !remaining.is_empty() {
                let buf = array_ref![remaining, 0, 2];
                remaining = &remaining[2..];

                if entry.sampled_data().bad_indexes().contains(&sample_i) {
                    time += entry.sampled_data().sample_interval();
                    sample_i += 1;
                    continue;
                }

                // unit: [0, 1)
                let mut angle_raw = (u16::from_be_bytes(*buf) as f64) / ((u16::max_value() as f64) + 1.0);

                // if inst.options.inverted {
                //     angle_raw = 1.0 - angle_raw;

                //     if angle_raw >= 1.0 {
                //         angle_raw = 0.0;
                //     }
                // }

                let mut angle_corrected = angle_raw;
                if let Some(model) = &inst.options.correction_model {
                    angle_corrected += model.compute(angle_raw);

                    while angle_corrected < 0.0 {
                        angle_corrected += 1.0;
                    }

                    while angle_corrected >= 1.0 {
                        angle_corrected -= 1.0;
                    }

                }

                if revolution_tracker.is_none() {
                    // TODO: Apply the corrections first.
                    revolution_tracker = Some(RevolutionTracker::new(angle_corrected));
                }

                // TODO: Apply the corrections first.
                let angle_revs = revolution_tracker.as_mut().unwrap().next(angle_corrected);

                if time < inst.motion_log.start_time {
                    inst.angle_premotion.add(angle_raw);
                }

                if time > inst.motion_log.end_time {
                    inst.angle_postmotion.add(angle_raw);
                }

                let pos = inst.motion_log.motor_positions_at_time(time);
                if let Some(motor_pos) = pos {

                    if inst.start_angle.is_none() {
                        let a = inst.angle_premotion.mean();
                        inst.start_angle = Some(a);
                    }

                    let error = inst.calculate_error(motor_pos[inst.options.motor_index], angle_raw);

                    inst.error_stats.add(error);

                    {
                        let m = inst.motion_log.position_derivatives_at_time(time).unwrap();
                        if m.acceleration.norm() > 0.01 {
                            inst.error_stats_accelerating.add(error);
                        }
                    }


                    inst.error_data.push((
                        angle_raw,
                        error
                    ));

                
                    let time_secs = ((time - inst.motion_log.start_time) as f64) / 16_000_000.0;
                    let start_angle = {
                        let mut start_angle = inst.start_angle.unwrap();
                        if let Some(model) = &inst.options.correction_model {
                            start_angle += model.compute(start_angle);
                        }

                        start_angle
                    };

                    inst.output_data.push(MotorEncoderDataRow {
                        time: time_secs,
                        angle_raw,
                        angle_corrected,
                        error,
                        actual_step: (angle_revs - start_angle) * STEPS_PER_REVOLUTION,
                        target_step: motor_pos[inst.options.motor_index] - (inst.start_motor_pos as f64)
                    });
                }

                time += entry.sampled_data().sample_interval();
                sample_i += 1;
            }

        }

        if inst.angle_premotion.count() < 100 {
            return Err(err_msg("Too few pre-motion angle data points"));
        }
        if inst.angle_postmotion.count() < 100 {
            return Err(format_err!("Too few post-motion angle data points. Have: {}", inst.angle_postmotion.count()));
        }

        // TODO: Should also analyze the final position after motion and verify that is good within one revolution.

        Ok(inst)
    } 

    // TODO: Think about having this take in the corrected angle and switching all metrics to use corrected angle.(e.g. self.start_angle) 
    // 
    // motor_pos: position of the motor in step units.
    // angle:     raw angle reported by the encoder in the range [0, 1)
    //
    // Returns the error in [0, 1) angle units. This is the amount that must be added to
    // the encoder angle to make it match the motor position. 
    fn calculate_error(&self, motor_pos: f64, mut angle: f64) -> f64 {
        // unit: steps
        let rel_pos = (motor_pos - (self.start_motor_pos as f64)) % STEPS_PER_REVOLUTION;

        let mut start_angle = self.start_angle.unwrap();

        if let Some(model) = &self.options.correction_model {
            angle += model.compute(angle);
        }
        if let Some(model) = &self.options.correction_model {
            start_angle += model.compute(start_angle);
        }

        // 0.1 - 0.9 = -0.8

        // unit: [0, 1)
        let mut rel_angle = angle - start_angle;

        while rel_angle < 0.0 {
            rel_angle += 1.0;
        }
        while rel_angle >= 1.0 {
            rel_angle -= 1.0;
        }

        let mut error = (rel_pos / STEPS_PER_REVOLUTION) - rel_angle;

        // Take the nearest of the two distances between the angle and position.
        error -= (error + 0.5).floor();

        error
    }

    // TODO: Use the revolution tracked metric for this.
    pub fn end_angle_error(&self) -> f64 {
        let final_angle = self.angle_postmotion.mean();
        let final_position = self.motion_log.end_motor_position[self.options.motor_index] as f64;

        self.calculate_error(final_position, final_angle)
    }

    pub fn print_stats(&self) {

        let scale = STEPS_PER_REVOLUTION;

        println!("");        
        println!("Pre-motion angle:");
        println!("- Points: {}", self.angle_premotion.count());
        println!("- Range: {}", self.angle_premotion.range().print_scaled(scale));
        println!("- Stddev: {}", self.angle_premotion.stddev() * scale);

        println!("");
        println!("Error (during motion):");
        println!("- Error points: {}", self.error_stats.count());
        println!("- Stats: {}", self.error_stats.range().print_scaled(scale));
        println!("- Mean: {}", self.error_stats.mean() * scale);
        println!("- Stddev: {}", self.error_stats.stddev() * scale);

        println!("");
        println!("Error (accelerations only):");
        println!("- Error points: {}", self.error_stats_accelerating.count());
        println!("- Stats: {}", self.error_stats_accelerating.range().print_scaled(scale));
        println!("- Mean: {}", self.error_stats_accelerating.mean() * scale);
        println!("- Stddev: {}", self.error_stats_accelerating.stddev() * scale);

        println!("");

        println!("Post motion data: {}", self.angle_postmotion.count());
        println!("End Error: {}", self.end_angle_error() * scale);
    }

    pub fn dump_error_csv(&self) -> String {
        let mut out = String::from("time,angle_raw,angle_corrected,actual_step,target_step,error\n");

        for row in &self.output_data {
            out.push_str(&format!(
                "{},{},{},{},{},{}\n",
                row.time,
                row.angle_raw,
                row.angle_corrected,
                row.actual_step,
                row.target_step,
                row.error
            ));
        }

        out
    }

    pub fn compute_correction_model(&self) -> FourierRegression {
        let mut fourier = FourierRegression::create(&self.error_data, 4);

        fourier.clear_dc_offset();

        let mut stats = NumericalMetricsTracker::default();

        for (angle, error) in self.error_data.iter().cloned() {
            let e = fourier.compute(angle);
            stats.add(e - error);
        }

        let scale = STEPS_PER_REVOLUTION;
        println!("Fourier Model Error:");
        println!("- Stats: {}", stats.range().print_scaled(scale));
        println!("- Stddev: {}", stats.stddev() * scale);

        fourier
    }

}







