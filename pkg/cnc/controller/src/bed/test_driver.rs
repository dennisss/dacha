use std::time::{Duration, Instant};

use common::errors::*;
use file::{LocalPath, LocalPathBuf, LocalFile};
use peripherals_proto::peripherals::*;
use peripherals_service::device::*;
use common::io::Writeable;
use math_compute::io::CSVReader;
use common::hash::FastHasherBuilder;
use cnc_controller_proto::cnc::BedClientConfig;

use electronics::*;

use crate::bed::client::*;
use crate::bed::thermal_model::*;
use crate::bed::training_data::*;


pub struct BedTestDriver {
    bed_client: BedClient,
    controller: PeripheralsDevice,
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

impl BedTestDriver {

    pub fn create_bed_client() -> Result<BedClient> {
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

    pub async fn create(log_path: Option<LocalPathBuf>) -> Result<Self> {
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

        let (usb_device, _) = PeripheralsDevice::create(&controller_config).await?;

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

        file.write_all(BedTrainingDataRow::csv_header().as_bytes()).await?;

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

    pub fn normalize_heater_duty_cycle(&self, mut v: f32) -> f32 {
        if v > 1.0 {
            v = 1.0;
        }
        if v < 0.0 {
            v = 0.0;
        }

        (v * 60.0).round() / 60.0
    }

    async fn set_heater_duty_cycle_inner(&mut self, v: f32) -> Result<()> {
        self.controller.pwm_write("heater", v).await?;
        Ok(())
    }


    /// NOTE: Only 0 and 1 are supported right now.
    pub async fn set_fan_duty_cycle(&mut self, mut v: f32) -> Result<()> {
        self.current_fan_duty_cycle = v;
        self.read_state().await?;
        Ok(())
    }

    pub async fn set_fan_and_heater(&mut self, fan: f32, heater: f32) -> Result<Response> {
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