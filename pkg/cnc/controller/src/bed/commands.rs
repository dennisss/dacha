use std::time::{Duration, Instant};
use std::collections::HashMap;

use common::errors::*;
use file::{LocalPath, LocalPathBuf, LocalFile};
use peripherals_proto::peripherals::*;
use common::io::Writeable;
use math_compute::io::CSVReader;
use common::hash::FastHasherBuilder;
use electronics::*;
use executor::channel;

use crate::bed::thermal_model::*;
use crate::bed::client::*;
use crate::bed::test_driver::*;
use crate::bed::training_data::*;
use crate::bed::error::*;

#[derive(Args)]
pub struct MeasureBedCommand {
    log_path: LocalPathBuf,
}

impl MeasureBedCommand {
    pub async fn run(self) -> Result<()> {
        let mut driver = BedTestDriver::create(Some(self.log_path)).await?;

        // TODO: Initially set the heater to off.
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
}

#[derive(Args)]
pub struct TrainBedModelCommand {
    log_path: LocalPathBuf,

    step_output_dir: Option<LocalPathBuf>,

    weights_output_path: Option<LocalPathBuf>,
}


async fn train_bed_model_logging_thread(
    step_output_dir: LocalPathBuf,
    mut step_receiver: channel::Receiver<(usize, BedTrainingData)>
) -> Result<()> {
    let mut buf = String::new();
    while let Ok((training_step, result)) = step_receiver.recv().await {
        result.csv_to(&mut buf);
        file::write(step_output_dir.join(format!("{:08}.csv", training_step)), buf.as_bytes()).await?;
    }

    Ok(())

}

impl TrainBedModelCommand {

    pub async fn run(self) -> Result<()> {

        let mut reader = CSVReader::new(file::LocalFile::open(self.log_path)?);

        // Header
        let header = reader.read().await?.unwrap();

        let mut field_indexes = HashMap::<String, usize, FastHasherBuilder>::default();
        for i in 0..header.num_fields() {
            field_indexes.insert(header.field(i)?.to_string(), i);
        }


        let mut out = BedTrainingData::default();

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

            out.rows.push(BedTrainingDataRow {
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
        
        if let Some(dir) = &self.step_output_dir {
            file::create_dir_all(dir).await?;

            for i in 0..8 {
                logging_bundle.add("Worker", train_bed_model_logging_thread(dir.clone(), step_receiver.clone()));
            }
        }

        let mut weights_output_file = None;
        if let Some(path) = &self.weights_output_path {
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
            let mut result = BedTrainingData::default();
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

            if self.step_output_dir.is_some() {
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
}

#[derive(Args)]
pub struct ControlBedCommand {
    initial_temperature: Option<f32>,

    target_temperature: f32,

    step_output_dir: Option<LocalPathBuf>,

    results_path: Option<LocalPathBuf>,
}


/*
Generally:
- Given an initial guess and an initial FEM state, do the math.

- Initially build the whole curve.
- Later, do small adjustments (copying the last output as the next guess).
*/

impl ControlBedCommand {
    pub async fn run(self) -> Result<()> {
        let weights = vec![
            // 0.1s integration
            // 1.5573804, 0.03456809, 0.045723405, 0.15496261

            // 0.05s integration.
            // 1.6234571, 0.03667893, 0.04765498, 0.1617854

            // 0.025s integration
            1.6613106, 0.03765705, 0.04876574, 0.16561244
        ];

        let time_horizon = 300;
        let target_temperature = self.target_temperature;

        let mut inputs = BedTrainingData::default();

        // Initial guess of control inputs.
        for i in 0..time_horizon {
            if i % 2 != 0 {
                continue;
            }

            inputs.rows.push(BedTrainingDataRow {
                time: i as f32,
                heater: 0.1,
                fan: 0.0,
                bed: None,
                sheet: Some(target_temperature)
            });
        }

        let mut fem = BedThermalFEM::create(&weights);

        let mut driver = BedTestDriver::create(self.results_path.clone()).await?;

        {
            let state = driver.set_fan_and_heater(0.0, 0.0).await?;
            fem.fem.elements[fem.middle_el] = state.bed_temperature;
            fem.fem.elements[fem.bottom_el] = state.bed_temperature;
            fem.fem.elements[fem.top_el] = state.sheet_temperature;
        }

        run_control_input_training(&fem, &mut inputs, 20, self.step_output_dir, None).await?;

        println!("Start [y/N]?");
        if !file::read_user_confirmation().await? {
            return Ok(());
        }


        if self.results_path.is_some() {
            driver.start_logging().await?;
        }
        
        // let mut error_integral = 0.0;

        loop {
            let t1 = Instant::now();

            println!("Raw Control: {} : {}", inputs.rows[0].heater, inputs.rows[0].fan);

            let heater = driver.normalize_heater_duty_cycle(inputs.rows[0].heater);

            // TODO: Need some hysterisis to prevent turning the fan on and off very frequently since it takes a few seconds to get up to full speed.
            let fan = if inputs.rows[0].fan > 0.2 { 1.0 } else { 0.0 };
            let state = driver.set_fan_and_heater(fan, heater).await?;

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

        if self.results_path.is_some() {
            driver.stop_logging().await?;
        }


        /*
        Calculate error assuming that we must be at the target_temperature all the time.

        Constrain inputs between 0 and 1.

        Ideally enforce regularity that the inputs are continous.
        */




        Ok(())

    }
}

async fn run_control_input_training(
    initial_fem: &BedThermalFEM,
    inputs: &mut BedTrainingData,
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

        let mut result = BedTrainingData::default();
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

        /*
        Iterate over weights and try making them bigger or smaller 
        - So basically need a 'Weights' input which has a length and can be mutated.

        */

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


use std::f32::consts::PI;

use math::matrix::Vector3f;
use color::*;

#[derive(Args)]
pub struct BedLightShowCommand {

}

impl BedLightShowCommand {
    pub async fn run(self) -> Result<()> {
        let mut bed_client = BedTestDriver::create_bed_client()?;

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
}


#[derive(Args)]
pub struct BedHeaterTestCommand {}

impl BedHeaterTestCommand {
    pub async fn run(self) -> Result<()> {
        let mut driver = BedTestDriver::create(None).await?;

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
}

#[derive(Args)]
pub struct BedFanTestCommand {}

impl BedFanTestCommand {
    pub async fn run(self) -> Result<()> {
        // TODO: Also need to connect to the microcontroller to turn off the bed.

        let mut bed_client = BedTestDriver::create_bed_client()?;

        loop {
            let res = bed_client.request(1, 0).await?;
            println!("{:?}", res);

            executor::sleep(Duration::from_millis(1000)).await;
        }

        Ok(())
    }
}

