use std::sync::Arc;
use std::time::{Duration, Instant};

use common::errors::*;
use file::{LocalPath, LocalPathBuf, LocalFile};
use peripherals_proto::peripherals::*;
use peripherals_service::device::*;
use common::io::Writeable;
use math_compute::io::CSVReader;
use common::hash::FastHasherBuilder;
use cnc_controller_proto::cnc::BedClientConfig;
use scpi::*;
use electronics::*;

use crate::toolhead::training_data::*;


pub struct ToolheadTestDriver {
    device: Arc<PeripheralsDevice>,
    psu_client: Option<SCPIClient>,
    multimeter_client: Option<SCPIClient>,

    log_path: Option<LocalPathBuf>,
    log_state: Option<LoggingState>,
    
    current_heater_duty_cycle: f32,
    current_fan_duty_cycle: f32,
}

struct LoggingState {
    file: LocalFile,
    start_time: Instant,
}

impl ToolheadTestDriver {
    pub async fn create(
        log_path: Option<LocalPathBuf>,
        psu_addr: Option<&str>,
        multimeter_addr: Option<&str>,
    ) -> Result<Self> {
        if let Some(path) = &log_path {
            if file::exists(path).await? {
                return Err(err_msg("Log file already exists"));
            }
        }
        
        let psu_client = match psu_addr {
            Some(addr) => {
                let mut psu_client = SCPIClient::create(addr).await?;
                psu_client.check_instrument_type(InstrumentType::PowerSupply).await?;
                Some(psu_client)
            }
            None => None
        };


        let multimeter_client = match multimeter_addr {
            Some(addr) => {
                let mut client = SCPIClient::create(addr).await?;
                client.check_instrument_type(InstrumentType::Multimeter).await?;

                // TODO: Dedup this.
                for cmd in [
                    "CONF:TEMP THER,KITS90",
                    "TRIG:SOUR IMM",
                    "TRIG:COUN INF",
                    "INIT"
                ] {
                    client.run_command_noreply(cmd).await?;
                }

                Some(client)
            }
            None => None
        };

        let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let config = configs.remove("voron0_toolhead")
            .ok_or_else(|| err_msg("No config with the given name"))?;

        let (device, _) = PeripheralsDevice::create(&config).await?;
        let device = Arc::new(device);
        
        device.pwm_write("fan_mid_pwm", 1.0).await?;


        Ok(Self {
            device,
            psu_client,
            multimeter_client,
            log_path,
            log_state: None,
            current_heater_duty_cycle: 0.0,
            current_fan_duty_cycle: 0.0,
        })
    }

    pub fn device(&self) -> Arc<PeripheralsDevice> {
        self.device.clone()
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

        file.write_all(ToolheadTrainingDataRow::csv_header().as_bytes()).await?;

        let start_time = Instant::now();

        self.log_state = Some(LoggingState {
            file,
            start_time
        });

        self.read_state().await?;

        Ok(())
    }

    pub async fn read_state(&mut self) -> Result<ToolheadTrainingDataRow> {
        let mut now = Instant::now();
    
        let heater = self.current_heater_duty_cycle;
        let heater_temp = executor::timeout(Duration::from_secs(1),
            self.device.analog_read("thermistor_sense"))
            .await
            .map_err(|_| err_msg("Timed out thermistor_sense"))??;
        let fan = self.current_fan_duty_cycle;

        let heater_voltage = executor::timeout(Duration::from_secs(1),
            self.device.analog_read("24v_sense"))
            .await
            .map_err(|_| err_msg("Timed out 24v_sense"))??;
                
        let heater_current = executor::timeout(Duration::from_secs(2),
            self.device.analog_read("heater_csense"))
            .await
            .map_err(|_| err_msg("Timed out heater_csense"))??;

        let mut psu_current = -1.0;
        let mut psu_voltage = 0.0;
        if let Some(client) = &mut self.psu_client {
            let m = executor::timeout(Duration::from_secs(1), client.measure_psu_ch1())
                .await
                .map_err(|_| err_msg("Timed out measure_psu_ch1"))??;
            psu_current = m.current;
            psu_voltage = m.voltage;
        }

        let mut nozzle_temp = None;
        if let Some(c) = &mut self.multimeter_client {
            let raw = executor::timeout(Duration::from_secs(5), c.run_command("DATA:LAST?"))
                .await
                .map_err(|_| err_msg("Timed out measure_temp_ktype"))??;
            
            // TODO: Dedup this.
            let v = raw.strip_suffix(" C")
                .ok_or_else(|| err_msg("Invalid temp measurement format"))?
                .trim()
                .parse::<f32>()?;

            nozzle_temp = Some(v);
        }

        let mut state = ToolheadTrainingDataRow {
            time: 0.0,
            heater,
            heater_temp: Some(heater_temp),
            fan,
            nozzle_temp,
            heater_current,
            heater_voltage,
            psu_current,
            psu_voltage
        };

        if let Some(log_state) = &mut self.log_state {
            let t = Instant::now().duration_since(log_state.start_time);
            state.time = t.as_secs_f32();
            log_state.file.write_all(state.to_csv_row().as_bytes()).await?;
        }

        println!("FIL: {:?}", self.device.analog_read("filament_sense").await?);

        println!("{:?}", state);

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

    pub async fn set_heater_duty_cycle(&mut self, mut v: f32) -> Result<()> {
        v = self.normalize_heater_duty_cycle(v);
        self.device.pwm_write("heater_pwm", v).await?;
        self.current_heater_duty_cycle = v;
        self.read_state().await?;
        Ok(())
    }

    pub fn normalize_heater_duty_cycle(&self, mut v: f32) -> f32 {
        if v > 1.0 {
            v = 1.0;
        }
        if v < 0.0 {
            v = 0.0;
        }

        v
    }

    pub async fn set_fan_duty_cycle(&mut self, mut v: f32) -> Result<()> {
        self.current_fan_duty_cycle = v;
        self.device.pwm_write("fan_l_pwm", v).await?;
        self.device.pwm_write("fan_r_pwm", v).await?;

        self.read_state().await?;
        Ok(())
    }

    pub async fn set_fan_and_heater(&mut self, fan: f32, heater: f32) -> Result<()> {
        // TODO: Dedeup the state reads.
        self.set_fan_duty_cycle(fan).await?;
        self.set_heater_duty_cycle(heater).await?;
        Ok(())
    }

    pub async fn wait_for_temp<F: Fn(f32) -> bool>(&mut self, done: F, max_time: Option<Duration>) -> Result<()> {
        let mut start_time = Instant::now();
        
        loop {
            let state = self.read_state().await?;
            let max_temp = state.heater_temp.unwrap_or(0.0).max(state.nozzle_temp.unwrap_or(0.0));
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