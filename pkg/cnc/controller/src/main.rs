#[macro_use]
extern crate macros;

use std::time::{Duration, Instant};
use std::collections::HashMap;

use common::errors::*;
use file::{LocalPath, LocalPathBuf, LocalFile};
use peripherals_proto::peripherals::*;
use nordic_tools::usb_radio::USBRadio;
use cnc_controller::bed_client::*;
use cnc_controller::thermistor::*;
use common::io::Writeable;
use math_compute::io::CSVReader;
use common::hash::FastHasherBuilder;
use cnc_controller_proto::cnc::BedClientConfig;

/*

TODO: Validate that my 2Hz PWM wave is working as expected on the hardware.

TODO: Need following safety checks:
- Min/max temperature checks
- Allow retrying failed bed client calls after a short timeout
- Allow temporary loss of valid temperature information but log number of failures per time window


Bed temp resistor is 0.9993k
sheet resistor is 0.998
aux resistor is 0.997

Both are ±100ppm/°C so variance will be around 0.35% over the 25-60degree temp range we care about


Ambient bed temperature was '195',

5.168 in VDD \ 2.729 out

Actually measuring 4336 / 8191

zero point is around 7

actually measuring 2.64680747

TODO: Get 1.2K resistors since they are the most precise for PT1000 thermistors


----

Note that the relays can only switch at zero intersections.

- My power is 120V / 60Hz => 120 flips
- Configure a 2Hz PWM so we have 60 degrees of freedom

- At 2hz, period is 60 flips.
    - Support n/60



-----


cargo run --bin cnc_controller -- --mode=measure-bed --log_path=bed_in_frame.csv

cargo run --bin media_thermal --release -- record --output_path=bed_in_frame_thermal.log


cargo run --bin cnc_controller --release -- train-bed-model \
    --log_path=data/bed/bed_in_frame.csv \
    --step_output_dir=data/bed/in_frame/training_steps \
    --weights_output_path=data/bed/in_frame/training_weights.csv

cargo run --bin cnc_controller --release -- control-bed \
    --target_temperature=100 \
    --step_output_dir=data/bed/in_frame/control_to_100_steps \
    --results_path=data/bed/in_frame/control_to_100_data.csv

cargo run --bin media_thermal --release -- record --output_path=data/bed/in_frame/control_to_100_thermal.log


-----

cargo run --bin cnc_controller --release -- control-bed \
    --target_temperature=100 \
    --results_path=data/bed/in_frame/control_to_100_data_take2.csv

cargo run --bin media_thermal --release -- record --output_path=data/bed/in_frame/control_to_100_thermal_take2.log

cargo run --bin cnc_controller --release -- control-bed \
    --target_temperature=100 \
    --results_path=data/bed/in_frame/control_to_100_data_take3.csv

--------

cargo run --bin media_thermal --release -- record --output_path=data/bed/in_frame/control_to_100_thermal_take4.log

cargo run --bin cnc_controller --release -- control-bed \
    --target_temperature=100 \
    --results_path=data/bed/in_frame/control_to_100_data_take4.csv


cargo run --bin media_thermal --release -- \
    encode-mp4 \
    --input_path=data/bed/in_frame/control_to_100_thermal_take4.log \
    --output_path=data/bed/in_frame/control_to_100_thermal_take4.mp4

----

cargo run --bin media_thermal --release -- \
    encode-mp4 \
    --input_path=data/bed/bed_in_frame_thermal_compressed.log \
    --output_path=data/bed/bed_in_frame_thermal.mp4

data/bed/bed_in_frame_thermal_compressed.log

car


*/

/*
All the animations I need to create:

- Measurement (collecting training data):
    - Single graph with more points being added over time.
    - Just need 1 CSV
- Training Bed Model
    - 1 CSV per epoch
    - 1 CSV that contains per-epoch weights.

- Controlling Training
    - 1 CSV per epoch
    - Frame per epoch
    - Don't show the weights separately.

- Control real bed (combine heat up and down)
    - Just 1 CSV
    - Frame per time step
    - Inputs will be quantized.
    - TODO: Also print out an ETA

*/


#[derive(Args)]
struct Args {
    mode: Mode,
}

#[derive(Args)]
enum Mode {
    #[arg(name = "measure-bed")]
    MeasureBed(MeasureBedCommand),

    #[arg(name = "train-bed-model")]
    TrainBedModel(TrainBedModelCommand),

    #[arg(name = "control-bed")]
    ControlBed(ControlBedCommand),

    #[arg(name = "light-show")]
    LightShow,

    #[arg(name = "fan-test")]
    FanTest,

    #[arg(name = "heater-test")]
    HeaterTest
}

#[derive(Args)]
struct MeasureBedCommand {
    log_path: LocalPathBuf,
}

#[derive(Args)]
struct TrainBedModelCommand {
    log_path: LocalPathBuf,

    step_output_dir: Option<LocalPathBuf>,

    weights_output_path: Option<LocalPathBuf>,
}

#[derive(Args)]
struct ControlBedCommand {

    initial_temperature: Option<f32>,

    target_temperature: f32,

    step_output_dir: Option<LocalPathBuf>,

    results_path: Option<LocalPathBuf>,

}


struct TestDriver {
    bed_client: BedClient,
    controller: USBRadio,
    controller_config: BoardConfig,

    log_path: Option<LocalPathBuf>,
    log_state: Option<LoggingState>,

    current_heater_duty_cycle: f32,
    current_fan_duty_cycle: f32,
}

struct LoggingState {
    file: LocalFile,
    start_time: Instant,
}

impl TestDriver {

    fn create_bed_client() -> Result<BedClient> {
        // use cnc_controller_proto::cnc::BedClientConfig;

        let mut config = BedClientConfig::default();
        protobuf::text::parse_text_proto(r#"
            bed_temp_resistor: 999.3,
            sheet_temp_resistor: 998.0,
            aux_temp_resistor: 997.0,
            calibration_a: 0.9963536962,
            calibration_b: -0.0008514803899,
            chip_temp_calibration: 0.955696203
        "#, &mut config)?;

        BedClient::create(LocalPath::new("/dev/ttyUSB0"), BedClientOptions {
            config
        })
    }

    async fn create(log_path: Option<LocalPathBuf>) -> Result<Self> {
        if let Some(path) = &log_path {
            if file::exists(path).await? {
                return Err(err_msg("Log file already exists"));
            }
        }

        let mut bed_client = Self::create_bed_client()?;

        let configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let mut controller_config = BoardConfig::default();
        protobuf::text::parse_text_proto(r#"
            name: "calibrator"
            base_config: "nrf52840_feather"

            peripherals {
                name: "heater"
                pwm {
                    pin_name: "D10"
                    config {
                        default_value: 0
                        frequency: 2
                    }
                }
            }
        "#, &mut controller_config)?;

        controller_config = configs.compile(&controller_config)?;


        let mut selector = usb::DeviceSelector::default();
        selector.vendor_id = Some(0x8888);
        selector.product_id = Some(0x0004);
        let mut usb_device = nordic_tools::usb_radio::USBRadio::find(&selector).await?;

        let (reqs, peripherals_state) = peripherals_service::config::build_configuration_requests(&controller_config)?;

        // TODO: Need to support batch sending of requests. 
        for req in reqs {
            usb_device.send_request(&req).await?;
        }

        let mut inst = Self {
            bed_client,
            controller: usb_device,
            controller_config,
            log_path,
            log_state: None,
            current_heater_duty_cycle: 0.0,
            current_fan_duty_cycle: 0.0,
        };

        Ok(inst)
    }

    pub async fn start_logging(&mut self) -> Result<()> {
        let log_path = match self.log_path.as_ref() {
            Some(v) => v,
            None => return Err(err_msg("No log path defined"))
        };

        if file::exists(log_path).await? {
            return Err(err_msg("Log file already exists"));
        }

        if self.log_state.is_some() {
            return Err(err_msg("Logging already started"));
        }

        let mut file = file::LocalFile::open_with_options(
            log_path,
            file::LocalFileOpenOptions::new().write(true).create(true),
        )?;

        file.write_all(b"time,heater,fan,bed,sheet\n").await?;

        let start_time = Instant::now();

        self.log_state = Some(LoggingState {
            file,
            start_time
        });

        self.read_state().await?;

        Ok(())
    }

    pub async fn read_state(&mut self) -> Result<Response> {
        
        let state = self.bed_client.request(self.current_fan_duty_cycle as u8, 0).await?;
        
        let mut time = None;

        if let Some(log_state) = &mut self.log_state {
            let t = Instant::now().duration_since(log_state.start_time);
            log_state.file.write_all(format!(
                "{},{},{},{},{}\n",
                t.as_secs_f32(),
                self.current_heater_duty_cycle,
                self.current_fan_duty_cycle,
                state.bed_temperature,
                state.sheet_temperature).as_bytes()
            ).await?;

            time = Some(t.as_secs());
        }

        println!(
            "[Time: {:.2?}] [Heater: {:.2}] [Fan: {:?}] [Bed: {:.2}] [Sheet: {:.2}]",
            time,
            self.current_heater_duty_cycle,
            self.current_fan_duty_cycle,
            state.bed_temperature,
            state.sheet_temperature
        );

        Ok(state)
    }

    pub async fn stop_logging(&mut self) -> Result<()> {
        let mut state = match self.log_state.take() {
            Some(v) => v,
            None => return Ok(())
        };

        state.file.flush().await?;
        Ok(())
    }

    pub async fn set_heater_duty_cycle(&mut self, mut v: f32) -> Result<Response> {
        v = self.normalize_heater_duty_cycle(v);
        self.set_heater_duty_cycle_inner(v).await?;
        self.current_heater_duty_cycle = v;
        self.read_state().await
    }

    fn normalize_heater_duty_cycle(&self, mut v: f32) -> f32 {
        if v > 1.0 {
            v = 1.0;
        }
        if v < 0.0 {
            v = 0.0;
        }

        (v * 60.0).round() / 60.0
    }

    async fn set_heater_duty_cycle_inner(&mut self, v: f32) -> Result<()> {
        let heater_periph_index = self.controller_config.peripherals().iter()
            .find(|p| p.name() == "heater")
            .unwrap()
            .index();

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(heater_periph_index);
        req.set_pwm_mut().set_value(((((1 << 16) - 1) as f32) * v) as u32);
        self.controller.send_request(&req).await?;

        Ok(())
    }


    /// NOTE: Only 0 and 1 are supported right now.
    pub async fn set_fan_duty_cycle(&mut self, mut v: f32) -> Result<()> {
        self.current_fan_duty_cycle = v;
        self.read_state().await?;
        Ok(())
    }

    pub async fn set_fan_and_header(&mut self, fan: f32, heater: f32) -> Result<Response> {
        self.current_fan_duty_cycle = fan;
        self.set_heater_duty_cycle(heater).await
    }


    pub async fn wait_for_temp<F: Fn(f32) -> bool>(&mut self, done: F, max_time: Option<Duration>) -> Result<()> {
        let mut start_time = Instant::now();
        
        loop {
            let bed_state = self.read_state().await?;
            let max_temp = bed_state.bed_temperature.max(bed_state.sheet_temperature);
            if done(max_temp) {
                println!("[Hit Target Temperature]");
                break;
            }

            if let Some(max_time) = max_time {
                if Instant::now().duration_since(start_time) > max_time {
                    println!("[Max Time Limit Hit]");
                    break;
                }
            }

            executor::sleep(Duration::from_secs(1)).await?;
        }

        Ok(())
    }

}

#[derive(Default, Clone)]
struct TrainingData {
    rows: Vec<TrainingDataRow>,
}

// NOTE: At each 'time', the heater/fan values start to be switched to that input value at that time.
#[derive(Clone)]
struct TrainingDataRow {
    time: f32,
    heater: f32,
    fan: f32,
    bed: Option<f32>,
    sheet: Option<f32>
}


async fn measure_bed(cmd: MeasureBedCommand) -> Result<()> {

    let mut driver = TestDriver::create(Some(cmd.log_path)).await?;

    // Initially set the heater to off.

    /*
    - Starting at ambient
    - Heat up with 100% power until we hit 60 degrees on max sensor
    - Wait for cooldown below 30 degrees celsius
    - Heat up with 50% power for the same amount of time
    - Heat using 25% power for same amount of time or until we exceed 80 degrees
    - Let everything cool down.
    - Repeat the same test using the fan on at 100%
    */

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
        driver.wait_for_temp(|t| t >= 60.0, None).await?;
        let t2 = Instant::now();

        let ramp1_time = t2 - t1;

        println!("# Cool below 35C");
        driver.set_heater_duty_cycle(0.0).await?;
        driver.wait_for_temp(|t| t <= 35.0, None).await?;

        println!("# 50% power ramp");
        driver.set_heater_duty_cycle(0.5).await?;
        driver.wait_for_temp(|t| t >= 60.0, Some(2 * ramp1_time)).await?;

        println!("# 25% power ramp");
        driver.set_heater_duty_cycle(0.25).await?;
        driver.wait_for_temp(|t| t >= 80.0, Some(2 * ramp1_time)).await?;

        println!("# Cool below 50C");
        driver.set_heater_duty_cycle(0.0).await?;
        driver.wait_for_temp(|t| t <= 50.0, None).await?;

        println!("# 75% power ramp");
        driver.set_heater_duty_cycle(0.75).await?;
        driver.wait_for_temp(|t| t >= 70.0, Some(2 * ramp1_time)).await?;

        println!("# Cool below 40C");
        driver.set_heater_duty_cycle(0.0).await?;
        driver.wait_for_temp(|t| t <= 40.0, None).await?;

        println!("# 10% Duty Cycle");
        driver.set_heater_duty_cycle(0.1).await?;
        driver.wait_for_temp(|t| t >= 80.0, Some(6 * ramp1_time)).await?;

        println!("# Cool below 40C");
        driver.set_heater_duty_cycle(0.0).await?;
        driver.wait_for_temp(|t| t <= 40.0, None).await?;
    }


    driver.stop_logging().await?;

    println!("Done!");

    Ok(())
}

use executor::channel;

async fn train_bed_model_logging_thread(
    step_output_dir: LocalPathBuf,
    mut step_receiver: channel::Receiver<(usize, TrainingData)>
) -> Result<()> {
    let mut buf = String::new();
    while let Ok((training_step, result)) = step_receiver.recv().await {
        buf.clear();

        buf.push_str("time,heater,fan,bed,sheet\n");

        for row in &result.rows {
            buf.push_str(&format!(
                "{:.2},{:.2},{:.2},{:.2},{:.2}\n",
                row.time,
                row.heater,
                row.fan,
                row.bed.unwrap(),
                row.sheet.unwrap())
            );
        }

        file::write(step_output_dir.join(format!("{:08}.csv", training_step)), buf.as_bytes()).await?;
    }

    Ok(())

}


async fn train_bed_model(cmd: TrainBedModelCommand) -> Result<()> {

    let mut reader = CSVReader::new(file::LocalFile::open(cmd.log_path)?);

    // Header
    let header = reader.read().await?.unwrap();

    let mut field_indexes = HashMap::<String, usize, FastHasherBuilder>::default();
    for i in 0..header.num_fields() {
        field_indexes.insert(header.field(i)?.to_string(), i);
    }


    let mut out = TrainingData::default();

    let time_idx = *field_indexes.get("time").ok_or_else(|| err_msg("Missing field"))?;
    let heater_idx = *field_indexes.get("heater").ok_or_else(|| err_msg("Missing field"))?;
    let bed_idx = *field_indexes.get("bed").ok_or_else(|| err_msg("Missing field"))?;
    let sheet_idx = *field_indexes.get("sheet").ok_or_else(|| err_msg("Missing field"))?;

    while let Some(row) = reader.read().await? {
        // assert_eq!(row.num_fields(), 4);

        let fan = match field_indexes.get("fan") {
            Some(v) => row.field(*v)?.parse()?,
            None => 0.0
        };

        out.rows.push(TrainingDataRow {
            time: row.field(time_idx)?.parse()?,
            heater: row.field(heater_idx)?.parse()?,
            fan,
            bed: Some(row.field(bed_idx)?.parse()?),
            sheet: Some(row.field(sheet_idx)?.parse()?),
        });
    }

    println!("Loaded {} measured time steps", out.rows.len());

    // NOTE: All weights need to be >= 0.

    let mut weights = vec![
        // 0.2, 0.1, 0.0, 0.0

        1.5, 0.03, 0.04, 0.15,

        // Weights for the free-air test without the fan (outside printer)
        // 1.6693797, 0.030415019, 0.0442844

        // in_frame results. (with 0.1s timesteps)
        // 1.5573804, 0.03456809, 0.045723405, 0.15496261

        // in_frame results. (with 0.05s timesteps)
        // 1.6234571, 0.03667893, 0.04765498, 0.1617854
    ];



    let (mut step_sender, step_receiver) = channel::bounded(128);
    let mut logging_bundle = executor::bundle::TaskResultBundle::new();
    
    if let Some(dir) = &cmd.step_output_dir {
        file::create_dir_all(dir).await?;

        for i in 0..8 {
            logging_bundle.add("Worker", train_bed_model_logging_thread(dir.clone(), step_receiver.clone()));
        }
    }

    let mut weights_output_file = None;
    if let Some(path) = &cmd.weights_output_path {
        file::create_dir_all(path.parent().unwrap()).await?;

        let mut f = file::LocalFile::open_with_options(
            path,
            file::LocalFileOpenOptions::new().write(true).truncate(true).create(true),
        )?;
        f.write_all(b"step,error,w0,w1,w2,w3\n").await?;

        weights_output_file = Some(f);
    }

    let mut training_step = 0;
    let mut last_error = 0.0;
    let mut last_time = Instant::now();


    loop {
        let mut result = TrainingData::default();
        result.rows.reserve_exact(out.rows.len());
        let error = calculate_error(&weights, &out, Some(&mut result));

        if let Some(f) = &mut weights_output_file {
            f.write_all(format!(
                "{},{},{},{},{},{}\n",
                training_step,
                error,
                weights[0],
                weights[1],
                weights[2],
                weights[3],
            ).as_bytes()).await?;
        }

        if cmd.step_output_dir.is_some() {
            step_sender.send((training_step, result)).await?;
        }

        if training_step % 1000 == 0 {
            let t = Instant::now();

            let time_per_step = (t - last_time).as_secs_f32() / 1000.0;

            last_time = t;

            println!("[Step: {}] [Weights: {:?}] [Error: {}] [Steps/s: {:.2?}]", training_step, weights, error, 1.0 / time_per_step);

            if training_step > 0 && ((last_error - error) / last_error).abs() < 0.0001 {
                break;
            }
            last_error = error;
        }

        let mut de_dw = vec![0.0; weights.len()];

        for i in 0..weights.len() {
            let original_weight = weights[i];

            weights[i] = original_weight + 0.001;
            let e_a = calculate_error(&weights, &out, None);

            weights[i] = original_weight - 0.001;
            let e_b = calculate_error(&weights, &out, None);

            de_dw[i] = (e_a - e_b) / 0.002;

            // Restore the value.
            weights[i] = original_weight;
        }

        // println!("Gradients: {:?}", de_dw);

        let learning_rate = 0.0000000001;

        for i in 0..weights.len() {
            weights[i] += learning_rate * -de_dw[i];

            weights[i] = weights[i].max(0.0);
        }

        training_step += 1;
    }

    drop(step_sender);
    drop(step_receiver);

    println!("Stopping...");

    if let Some(f) = &mut weights_output_file {
        f.flush().await?;
    }


    logging_bundle.join().await?;

    // println!("Erorr: {}", error);


    Ok(())
}

/*
Generally:
- Given an initial guess and an initial FEM state, do the math.

- Initially build the whole curve.
- Later, do small adjustments (copying the last output as the next guess).
*/

async fn control_bed(cmd: ControlBedCommand) -> Result<()> {
    let weights = vec![
        // 0.1s integration
        // 1.5573804, 0.03456809, 0.045723405, 0.15496261

        // 0.05s integration.
        // 1.6234571, 0.03667893, 0.04765498, 0.1617854

        // 0.025s integration
        1.6613106, 0.03765705, 0.04876574, 0.16561244
    ];

    let time_horizon = 300;
    let target_temperature = cmd.target_temperature;

    let mut inputs = TrainingData::default();

    // Initial guess of control inputs.
    for i in 0..time_horizon {
        if i % 2 != 0 {
            continue;
        }

        inputs.rows.push(TrainingDataRow {
            time: i as f32,
            heater: 0.1,
            fan: 0.0,
            bed: None,
            sheet: Some(target_temperature)
        });
    }

    let mut fem = BedThermalFEM::create(&weights);

    let mut driver = TestDriver::create(cmd.results_path.clone()).await?;

    {
        let state = driver.set_fan_and_header(0.0, 0.0).await?;
        fem.fem.elements[fem.middle_el] = state.bed_temperature;
        fem.fem.elements[fem.bottom_el] = state.bed_temperature;
        fem.fem.elements[fem.top_el] = state.sheet_temperature;
    }

    run_control_input_training(&fem, &mut inputs, 20, cmd.step_output_dir, None).await?;

    println!("Start [y/N]?");
    if !file::read_user_confirmation().await? {
        return Ok(());
    }


    if cmd.results_path.is_some() {
        driver.start_logging().await?;
    }
    
    // let mut error_integral = 0.0;

    loop {
        let t1 = Instant::now();

        println!("Raw Control: {} : {}", inputs.rows[0].heater, inputs.rows[0].fan);

        let heater = driver.normalize_heater_duty_cycle(inputs.rows[0].heater);

        // TODO: Need some hysterisis to prevent turning the fan on and off very frequently since it takes a few seconds to get up to full speed.
        let fan = if inputs.rows[0].fan > 0.2 { 1.0 } else { 0.0 };
        let state = driver.set_fan_and_header(fan, heater).await?;

        fem.fem.elements[fem.middle_el] = state.bed_temperature;
        fem.set_fan(fan);
        fem.set_heater(heater);
        fem.fem.step(2.0);

        inputs.rows.remove(0);

        let mut next_input = inputs.rows.last().unwrap().clone();
        next_input.time += 2.0;
        inputs.rows.push(next_input);

        run_control_input_training(&fem, &mut inputs, 5, None, Some(40)).await?;

        let t2 = Instant::now();
        let dur = t2 - t1;

        println!("[Control duration: {:?}]", dur);
        println!("FEM: {:?}", fem.fem.elements);

        if dur < Duration::from_secs(2) {
            executor::sleep(Duration::from_secs(2) - dur).await?;
        }
    }

    if cmd.results_path.is_some() {
        driver.stop_logging().await?;
    }


    /*
    Calculate error assuming that we must be at the target_temperature all the time.

    Constrain inputs between 0 and 1.

    Ideally enforce regularity that the inputs are continous.
    */




    Ok(())

}

async fn run_control_input_training(
    initial_fem: &BedThermalFEM,
    inputs: &mut TrainingData,
    steps_per_epoch: usize,
    step_output_dir: Option<LocalPathBuf>,
    max_steps: Option<usize>,
) -> Result<()> {
    let mut training_step = 0;
    let mut last_error = 0.0;
    let mut last_time = Instant::now();


    let (mut step_sender, step_receiver) = channel::bounded(128);
    let mut logging_bundle = executor::bundle::TaskResultBundle::new();
    
    if let Some(dir) = &step_output_dir {
        file::create_dir_all(dir).await?;

        for i in 0..8 {
            logging_bundle.add("Worker", train_bed_model_logging_thread(dir.clone(), step_receiver.clone()));
        }
    }

    loop {
        if let Some(s) = max_steps {
            if training_step > s {
                println!("[Hit Max Steps]");
                break;
            }
        }

        let mut result = TrainingData::default();
        result.rows.reserve_exact(inputs.rows.len());
        let error = calculate_error_fem(initial_fem.clone(), &inputs, Some(&mut result));

        if step_output_dir.is_some() {
            step_sender.send((training_step, result)).await?;
        }

        if training_step % steps_per_epoch == 0 {
            let t = Instant::now();

            let time_per_step = (t - last_time).as_secs_f32() / (steps_per_epoch as f32);

            last_time = t;

            // let inputs = inputs.rows.iter().map(|i| i.heater).collect::<Vec<_>>();
            println!("[Step: {}] [Error: {}] [Steps/s: {}]", training_step, error, 1.0 /time_per_step);

            /*
            {
                let mut file = file::LocalFile::open_with_options(
                    file::project_path!("bed-control.csv"),
                    file::LocalFileOpenOptions::new().write(true).truncate(true).create(true),
                )?;
                file.write_all(b"time,heater,fan,bed,sheet\n").await?;

                for row in &sim.rows {
                    file.write_all(format!(
                        "{},{},{},{},{}\n",
                        row.time,
                        row.heater,
                        row.fan,
                        row.bed.unwrap(),
                        row.sheet.unwrap()).as_bytes()
                    ).await?;
                }

                file.flush().await?;
            }
            */

            if training_step > 0 && (
                ((last_error - error) / last_error).abs() < 0.01 ||
                error < 0.1 ||
                last_error < 0.1
            ) {
                break;
            }
            last_error = error;

        }

        // TODO: Must do for the heater and fan elements.

        let mut de_dh = vec![0.0; inputs.rows.len()];

        for i in 0..inputs.rows.len() {
            let original_value = inputs.rows[i].heater;

            inputs.rows[i].heater = original_value + 0.001;
            let e_a = calculate_error_fem(initial_fem.clone(), &inputs, None);

            inputs.rows[i].heater = original_value - 0.001;
            let e_b = calculate_error_fem(initial_fem.clone(), &inputs, None);

            de_dh[i] = (e_a - e_b) / 0.002;

            // Restore the value.
            inputs.rows[i].heater = original_value;
        }

        let mut de_df = vec![0.0; inputs.rows.len()];

        for i in 0..inputs.rows.len() {
            let original_value = inputs.rows[i].fan;

            inputs.rows[i].fan = original_value + 0.001;
            let e_a = calculate_error_fem(initial_fem.clone(), &inputs, None);

            inputs.rows[i].fan = original_value - 0.001;
            let e_b = calculate_error_fem(initial_fem.clone(), &inputs, None);

            de_df[i] = (e_a - e_b) / 0.002;

            // Restore the value.
            inputs.rows[i].fan = original_value;
        }

        // .00001 : 486902.47
        // .00002 : 486236.6
        let learning_rate = -0.00002;

        for i in 0..inputs.rows.len() {
            inputs.rows[i].heater += learning_rate * de_dh[i];
            inputs.rows[i].heater = inputs.rows[i].heater.max(0.0).min(1.0);
        }

        for i in 0..inputs.rows.len() {
            inputs.rows[i].fan += learning_rate * de_df[i];
            inputs.rows[i].fan = inputs.rows[i].fan.max(0.0).min(1.0);
        }
        
        training_step += 1;
    }


    drop(step_sender);
    drop(step_receiver);
    logging_bundle.join().await?;


    Ok(())
}


const AMBIENT_TEMP: f32 = 24.0;

#[derive(Clone)]
struct BedThermalFEM {
    fem: ThermalFEM,
    bottom_el: usize,
    middle_el: usize,
    top_el: usize,
    air_el: usize,
    heater_coeff: f32,
    fan_coeff: f32,
    fan_relation_idx: usize,
}

impl BedThermalFEM {
    fn create(weights: &[f32]) -> Self {
        let mut fem = ThermalFEM::default();

        let bottom_el = fem.add_element(AMBIENT_TEMP);
        let middle_el = fem.add_element(AMBIENT_TEMP);
        let top_el = fem.add_element(AMBIENT_TEMP);
        let air_el = fem.add_element(AMBIENT_TEMP);
        // let sheet_el = fem.add_element(AMBIENT_TEMP);

        // Heater
        fem.model.sources.push((bottom_el, 0.0));

        fem.model.relations.push((bottom_el, middle_el, weights[1]));
        fem.model.relations.push((middle_el, bottom_el, weights[1]));

        fem.model.relations.push((top_el, middle_el, weights[1]));
        fem.model.relations.push((middle_el, top_el, weights[1]));

        let large_area = 0.0144;
        let small_area = 0.0016;

        let fan_relation_idx = fem.model.relations.len();
        fem.model.relations.push((air_el, bottom_el, 0.0)); // Fan initially off.
        let fan_coeff = (large_area + small_area) * weights[3];

        fem.model.relations.push((air_el, bottom_el, (large_area + small_area) * weights[2]));
        fem.model.relations.push((air_el, top_el, (large_area + small_area) * weights[2]));
        fem.model.relations.push((air_el, middle_el, (large_area + small_area) * weights[2]));

        Self {
            fem,
            bottom_el,
            middle_el,
            top_el,
            air_el,
            heater_coeff: weights[0],
            fan_coeff,
            fan_relation_idx
        }
    }

    fn set_heater(&mut self, duty_cycle: f32) {
        self.fem.model.sources[0].1 = duty_cycle * self.heater_coeff;
    }

    fn set_fan(&mut self, duty_cycle: f32) {
        self.fem.model.relations[self.fan_relation_idx].2 = duty_cycle * self.fan_coeff;
    }

}


fn squared(v: f32) -> f32 {
    v * v
}

fn calculate_error(weights: &[f32], data: &TrainingData, mut result: Option<&mut TrainingData>) -> f32 {
    calculate_error_fem(BedThermalFEM::create(weights), data, result)
}

fn calculate_error_fem(
    mut bed_fem: BedThermalFEM,
    data: &TrainingData,
    mut result: Option<&mut TrainingData>
) -> f32 {
    let mut error = 0.0;
    let mut time = data.rows[0].time;

    for row in &data.rows {
        let dt = row.time - time;
        bed_fem.fem.step(dt);

        // TODO: Use trapezoidal error.
        let mut e = 0.0;
        if let Some(t) = row.sheet {
            e += squared(bed_fem.fem.elements[bed_fem.top_el] - t);
        }
        if let Some(t) = row.bed {
            e += squared(bed_fem.fem.elements[bed_fem.middle_el] - t);
        }

        error += dt * e;

        bed_fem.set_heater(row.heater);
        bed_fem.set_fan(row.fan);

        if let Some(ref mut result) = result {
            result.rows.push(TrainingDataRow {
                heater: row.heater,
                time: row.time,
                fan: row.fan,
                sheet: Some(bed_fem.fem.elements[bed_fem.top_el]),
                bed: Some(bed_fem.fem.elements[bed_fem.middle_el]),
            });
        }

        time = row.time;
    }

    error
}


pub struct BedModel {
    /*
    0: Heater  -> Bottom
    1: Bottom <-> Middle
       Middle <-> Top
    2: Air (not scaled by surface area)
    */
    weights: Vec<f32>
}

/*
Surface Areas:
- Bed is 120mm x 120mm x ~10mm
    - 0.0144 m^2 on top/bottom
    - 0.0016 m^2 for each third on the sides
*/


/*
Model:

[Heater] -> [Bottom]

[Bottom] -> [Middle]
         -> [Air] (sides and bottom)

[Middle] -> [Bottom]
         -> [Top]
         -> [Air] (sides)

[Top]    -> [Sheet]
         -> [Middle]
         -> [Air] (sides)

[Sheet]  -> [Air] (top)
         -> [Middle]

-> Middle


*/

/// Maximum size of the step we are allowed to take in a simulation (in seconds).
const MAX_SIMULATION_TIMESTEP: f32 = 0.025;

#[derive(Default, Clone)]
struct ThermalFEM {
    /// Temperature of each element in the system.
    elements: Vec<f32>,

    next_elements: Vec<f32>,

    // deriv: Vec<f32>,

    // elements_temp: Vec<f32>,
    // k1: Vec<f32>,
    // k2: Vec<f32>,
    // k3: Vec<f32>,
    // k4: Vec<f32>,

    model: ThermalModel,
}

#[derive(Default, Clone)]
struct ThermalModel {
    // num_elements?

    /// from_element_index, to_element_index, coefficient
    relations: Vec<(usize, usize, f32)>,

    sources: Vec<(usize, f32)>,
}

impl ThermalFEM {

    /// Advances forward the state of the simulation by 'dt' amount of time.
    pub fn step(&mut self, dt: f32) {
        let mut t = 0.0;

        while t < dt {
            let next_t = (t + dt).min(t + MAX_SIMULATION_TIMESTEP);
            self.single_step(next_t - t);
            t = next_t;
        }
    }

    fn single_step(&mut self, dt: f32) {
        /*
        self.model.compute_derivatives(&self.elements, &mut self.k1);

        let dt2 = (dt / 2.0);
        for i in 0..self.elements.len() {
            self.elements_temp[i] = self.elements[i] + dt2 * self.k1[i];
        }
        self.model.compute_derivatives(&self.elements_temp, &mut self.k2);

        for i in 0..self.elements.len() {
            self.elements_temp[i] = self.elements[i] + dt2 * self.k2[i];
        }
        self.model.compute_derivatives(&self.elements_temp, &mut self.k3);

        for i in 0..self.elements.len() {
            self.elements_temp[i] = self.elements[i] + dt * self.k3[i];
        }
        self.model.compute_derivatives(&self.elements_temp, &mut self.k4);
        
        let dt6 = (dt / 6.0);
        for i in 0..self.elements.len() {
            self.elements[i] += dt6 * (self.k1[i] + 2.0 * self.k2[i] + 2.0 * self.k3[i] + self.k4[i]);
        }
        */

        for i in 0..self.elements.len() {
            self.next_elements[i] = self.elements[i];
        }

        for (from_i, to_i, coeff) in self.model.relations.iter().cloned() {
            let scale = self.elements[from_i] - self.elements[to_i];
            self.next_elements[to_i] += coeff * scale * dt;
        }

        for (source_target, coeff) in self.model.sources.iter().cloned() {
            self.next_elements[source_target] += coeff * dt; 
        }

        core::mem::swap(&mut self.elements, &mut self.next_elements);
    }

    fn add_element(&mut self, initial_temp: f32) -> usize {
        let n = self.elements.len();
        self.elements.push(initial_temp);
        self.next_elements.push(initial_temp);
        // self.deriv.push(0.0);

        // self.elements_temp.push(0.0);
        // self.k1.push(0.0);
        // self.k2.push(0.0);
        // self.k3.push(0.0);
        // self.k4.push(0.0);

        n
    }

    fn clear_sources(&mut self) {
        self.model.sources.clear();
    }

}

impl ThermalModel {
    fn compute_derivatives(&self, elements: &[f32], out: &mut [f32]) {
        for v in out.iter_mut() {
            *v = 0.0;
        }

        for (from_i, to_i, coeff) in self.relations.iter().cloned() {
            let scale = elements[from_i] - elements[to_i];
            out[to_i] += coeff * scale;
        }

        for (source_target, coeff) in self.sources.iter().cloned() {
            out[source_target] += coeff; 
        }
    }

}

use std::f32::consts::PI;

use math::matrix::Vector3f;

use cnc_controller::color::*;


async fn light_show() -> Result<()> {
        let mut bed_client = TestDriver::create_bed_client()?;

    // WRGB
    let colors = vec![
        0x00ff0000,
        0x00ffff00,
        0x0000ff00,
        0x0000ffff,
        0x000000ff,
        0xff0000ff,
    ];

    bed_client.request(0, 0x000000ff).await?;

    let duration = Duration::from_millis(1000);

    let mut color_i = 0;
    loop {
        let c1 = RGB::from_rgb(colors[color_i % colors.len()]);
        let c2 = RGB::from_rgb(colors[(color_i + 1) % colors.len()]);

        let h1 = c1.to_hsv();
        let h2 = c2.to_hsv();

        let start_time = std::time::Instant::now();

        loop {
            let now = std::time::Instant::now();

            let mut i = (now - start_time).as_secs_f32() / duration.as_secs_f32();
            i = i.clamp(0.0, 1.0);

            // Ease in/out
            i = -0.5 * ((i * std::f32::consts::PI).cos() - 1.0);


            let hx = linear_interpolate_hsx(&h1, &h2, i);
            let rgb = RGB::from_hsv(&hx);

            bed_client.request(0, rgb.to_rgb()).await?;

            if i == 1.0 {
                break;
            }

            executor::sleep(Duration::from_millis(2)).await;
        }

        color_i += 1;
    }


    Ok(())
}


fn norm_radians(v: f32) -> f32 {
    let deg360 = 2.0 * PI;

    let mut m = v % deg360;
    if m < 0.0 {
        m += deg360;
    }

    assert!(m >= 0.0 && m < deg360);

    m
}

fn linear_interpolate_hsx(a: &Vector3f, b: &Vector3f, i: f32) -> Vector3f {
    let deg180 = PI;
    let deg360 = 2.0 * PI;

    let mut hue_distance = norm_radians(b[0] - a[0]);
    if hue_distance > deg180 {
        hue_distance = -1.0 * norm_radians(a[0] - b[0]);
        // hue_distance -= deg360;
    };

    let hue = norm_radians(a[0] + i * hue_distance);

    let s = a[1] * (1.0 - i) + b[1] * i;
    let x = a[2] * (1.0 - i) + b[2] * i;

    Vector3f::from_slice(&[hue, s, x])
}

async fn heater_test() -> Result<()> {
    let mut driver = TestDriver::create(None).await?;

    driver.set_heater_duty_cycle(0.0).await?;

    // watts
    // 0
    // 26
    // 71
    // 142
    // 212
    // 282

    for v in [0.0, 0.1, 0.25, 0.5, 0.75, 1.0, 0.0] {
        println!("Proceed to {}? [y/N]", v);
        if !file::read_user_confirmation().await? {
            driver.set_heater_duty_cycle(0.0).await?;
            return Ok(());
        }

        driver.set_heater_duty_cycle(v).await?;
    }


    println!("Done!");



    Ok(())

}

async fn fan_test() -> Result<()> {
    // TODO: Also need to connect to the microcontroller to turn off the bed.

        let mut bed_client = TestDriver::create_bed_client()?;

    loop {
        let res = bed_client.request(1, 0).await?;
        println!("{:?}", res);

        executor::sleep(Duration::from_millis(1000)).await;
    }

    Ok(())
}



#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    match args.mode {
        Mode::MeasureBed(cmd) => {
            return measure_bed(cmd).await;
        }
        Mode::TrainBedModel(cmd) => {
            return train_bed_model(cmd).await;
        }
        Mode::ControlBed(cmd) => {
            return control_bed(cmd).await;
        }
        Mode::LightShow => {
            return light_show().await;
        }
        Mode::FanTest => {
            return fan_test().await;
        }
        Mode::HeaterTest => {
            return heater_test().await;
        }
    }




    /*
    let r1 = PT1000::temperature_to_resistance(25.5);

    println!("V: {}", divide_voltage(1.0, 998.0, r1));

    for r_upper in [1000.0, 1200.0, 1800.0, 4700.0] {
        let r25 = PT1000::temperature_to_resistance(25.0);
        let r26 = PT1000::temperature_to_resistance(26.0);

        let r100 = PT1000::temperature_to_resistance(110.0);
        let r101 = PT1000::temperature_to_resistance(111.0);

        let v25 = divide_voltage(5.0, r_upper, r25);
        let v26 = divide_voltage(5.0, r_upper, r26);
        let v100 = divide_voltage(5.0, r_upper, r100);
        let v101 = divide_voltage(5.0, r_upper, r101);

        println!("R_u: {} ; V_range: {} ; {} | {}", r_upper, v100 - v25, v26 - v25, v101 - v100);

    }
    */


    // loop {
    //     println!("{:?}", client.request(0, 0).await?);


    // }




    Ok(())
}