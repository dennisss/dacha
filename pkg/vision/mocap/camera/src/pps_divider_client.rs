use std::time::{Duration, Instant};
use std::sync::Arc;

use common::errors::*;
use common::io::{Readable, Writeable, SharedWriteable};
use executor::sync::{AsyncMutex, AsyncVariable};
use executor::FileHandle;
use file::LocalPathBuf;
use macros::executor_main;
use peripherals::serial::{SerialPort, SerialOptions};
use peripherals::gpio::*;
use executor_multitask::{impl_resource_passthrough, TaskResource};
use executor::{lock, lock_async};
use ptp::SignedDuration;
use flasher_swd::*;
use rpi::model::Model;
use mocap_camera_core::CameraHardwareConfigContainer;

use crate::pps_divider_protocol::*;

/// The negative of this is the amount of time in cycles that a STM32G03 takes to detect a PPS pulse on a
/// timer capture channel. We will add this value to all output pulse offsets so that the output pulses
/// align with input pulses.
const HARDWARE_DELAY_OFFSET: i32 = -3; 


pub struct PPSDividerClient {
    shared: Arc<Shared>,
    task_resource: TaskResource,
}

// NOTE: The widths/offsets are currently not corrected by the PPS width.
#[derive(Default, PartialEq)]
pub struct PPSDividerConfigRequest {
    pub unlock: bool,
    pub pulse_rate: u8,
    pub frame_width: Option<Duration>,
    pub frame_offset: Option<SignedDuration>,
    pub strobe_width: Option<Duration>,
    pub strobe_offset: Option<SignedDuration>,
}

struct Shared {
    writer_state: AsyncMutex<WriterState>,
    state: AsyncVariable<State>,
}

struct WriterState {
    writer: Box<dyn SharedWriteable>,
    last_sequence: u8,
    reset_pin: GPIOPin,
    programmer: SWDProgrammer
}

#[derive(Default)]
struct State {
    last_ack: Option<AckPayload>,
    last_heartbeat: Option<(HeartbeatPayload, Instant)>,
    last_telemetry: Option<(TelemetryPayload, Instant)>,
    locked_count: usize,
}

impl PPSDividerClient {

    pub async fn create(config: Arc<CameraHardwareConfigContainer>) -> Result<Self> {
        let gpio = GPIOChip::default_chip()?;

        // Perform an initial reset of the MCU.
        let mut reset_pin = gpio.pin(config.mcu_reset_pin())?;
        reset_pin.configure(GPIOLineFlags::OUTPUT)?;
        reset_pin.write(false)?;
        executor::sleep(Duration::from_millis(1)).await?;
        reset_pin.write(true)?;

        let clk_pin = gpio.pin(config.mcu_swd_clk_pin())?;
        let io_pin = gpio.pin(config.mcu_swd_io_pin())?;
        let programmer = SWDProgrammer::create(clk_pin, io_pin)?;

        // NOTE: This is intentionally fairly slow since the MCU gets interrupted on every single byte
        // so we don't want to disrupt the timing critical pulse work or get serial overruns if we
        // are currently running the pulse logic.
        let mut serial = SerialPort::open(&config.mcu_serial_device(), 115_200)?;

        let (reader, writer) = serial.split();

        let shared = Arc::new(Shared {
            state: AsyncVariable::default(),
            writer_state: AsyncMutex::new(WriterState {
                writer,
                last_sequence: 0,
                reset_pin,
                programmer,
            })
        });

        let task_resource = TaskResource::spawn_interruptable(
            "PPSDividerClient::serial_reader()",
            Self::serial_reader(shared.clone(), reader),
        );
    
        Ok(Self {
            shared,
            task_resource
        })
    }

    /// Note that config changes only sync on the MCU at the start of each PPS pulse.
    pub async fn configure(&self, req: &PPSDividerConfigRequest) -> Result<()> {

        lock_async!(writer_state <= self.shared.writer_state.lock().await?, {
            let mut sequence = writer_state.last_sequence.wrapping_add(1);
            if sequence == 0 {
                sequence = 1;
            }

            writer_state.last_sequence = sequence;

            let packet = Packet::Config(ConfigPayload {
                sequence,
                unlock: if req.unlock { 1 } else { 0 },
                pulse_rate: req.pulse_rate,
                frame_width_ticks: self.duration_to_ticks(req.frame_width).await?,
                frame_offset_ticks: self.signed_duration_to_ticks(req.frame_offset).await? + HARDWARE_DELAY_OFFSET,
                strobe_offset_ticks: self.signed_duration_to_ticks(req.strobe_offset).await? + HARDWARE_DELAY_OFFSET,
                strobe_width_ticks: self.duration_to_ticks(req.strobe_width).await?,
            });

            println!("Sending config: {:?}", packet);

            let data = packet.to_bytes();

            // Retries
            for _ in 0..3 {
                writer_state.writer.write_all(&data).await?;

                let res = executor::timeout(
                    Duration::from_millis(500),
                    Self::wait_for_ack(&self.shared, sequence)
                ).await;

                match res {
                    Ok(v) => {
                        return v;
                    }
                    Err(_) => {
                        // Timeout. Retry.
                        continue;
                    }
                }
            }

            Result::<(), Error>::Err(err_msg("Ran out of retries while attempting to configure PPS divider MCU."))            
        })?;

        if req.unlock {
            lock!(state <= self.shared.state.lock().await?, {
                state.locked_count = 0;
            });
        }

        Ok(())
    }

    async fn wait_for_ack(shared: &Shared, sequence: u8) -> Result<()> {
        loop {
            let state = shared.state.lock().await?.read_exclusive();

            if state.last_ack.as_ref().map(|p| p.sequence == sequence).unwrap_or(false) {
                return Ok(());
            }

            state.wait().await;
        }
    }

    pub async fn flash(&self, data: &[u8]) -> Result<()> {
        lock_async!(writer_state <= self.shared.writer_state.lock().await?, {
            let swd = &mut writer_state.programmer;
            
            println!("Code: {}", swd.probe()?);

            println!("Init debug...");
            swd.init_debug()?;
            println!("=> Done!");

            println!("Halting core...");
            swd.halt_core()?;

            // TODO: Clear all the health state.

            let s = Instant::now();

            swd.flash_chip(McuTarget::STM32G031, &data)?;

            let e = Instant::now();

            println!("Flash {} bytes in {:?}", data.len(), e - s);

            println!("Verifying...");

            let s_verify = Instant::now();
            swd.verify_flash(&data)?;
            let e_verify = Instant::now();

            println!("Verified {} bytes in {:?}", data.len(), e_verify - s_verify);

            println!("Resetting...");

            swd.reset_core()?;

            Ok(())
        })
    }

    async fn signed_duration_to_ticks(&self, dur: Option<SignedDuration>) -> Result<i32> {
        let dur = match dur {
            Some(v) => v,
            None => return Ok(0)
        };

        let v = self.duration_to_ticks(Some(dur.duration)).await?;

        Ok((v as i32) * dur.sign)
    }

    async fn duration_to_ticks(&self, dur: Option<Duration>) -> Result<u32> {
        let dur = match dur {
            Some(v) => v,
            None => return Ok(0)
        };

        // TODO: Probably just store an atomic variable with this value to avoid needing locks.
        lock!(state <= self.shared.state.lock().await?, {

            let frequency_mhz = state.last_telemetry.as_ref()
                .ok_or_else(|| err_msg("No telemetry received yet"))?
                .0
                .frequency_mhz;

            let v = (dur.as_secs_f64() * (frequency_mhz as f64) * 1_000_000.0).round() as u32;

            Ok(v)
        })
    }

    pub async fn last_heartbeat(&self) -> Result<Option<(HeartbeatPayload, Instant)>> {
        lock!(state <= self.shared.state.lock().await?, {
            Ok(state.last_heartbeat.clone())
        })
    }

    pub async fn last_telemetry(&self) -> Result<Option<(TelemetryPayload, Instant)>> {
        lock!(state <= self.shared.state.lock().await?, {
            Ok(state.last_telemetry.clone())
        })
    }

    pub async fn wait_until_healthy(&self) -> Result<()> {
        loop {
            let healthy = lock!(state <= self.shared.state.lock().await?, {
                state.last_heartbeat.is_some()
            });

            if healthy {
                return Ok(());
            }

            executor::sleep(Duration::from_millis(100)).await?;
        }
    }

    // TODO: Also require that the samples were all recently acquired
    pub async fn is_locked(&self) -> Result<bool> {
        lock!(state <= self.shared.state.lock().await?, {
            Ok(state.locked_count >= 2)
        })        
    }

    pub async fn wait_until_locked(&self) -> Result<()> {
        loop {
            let healthy = self.is_locked().await?;

            if healthy {
                return Ok(());
            }

            executor::sleep(Duration::from_millis(100)).await?;
        }
    }

    // TODO: Want to be complaining if we don't get a Telemetry or Heartbeat packet in a while.
    async fn serial_reader(shared: Arc<Shared>, mut reader: Box<dyn Readable + Sync>) -> Result<()> {
        let mut parser = PacketParser::new();
        let mut buf = [0u8; 64];

        loop {
            let n = reader.read(&mut buf[..]).await?;
            let time = Instant::now();
            // if n > 0 {
            //     println!("{}: {:?}", n, &buf[..n] /*common::format::format_bytes(&buf[..n])*/);
            // }

            let mut remaining = &buf[..n];

            while !remaining.is_empty() {
                let (pkt, n) = parser.parse(remaining);
                assert!(n > 0);
                remaining = &remaining[n..];
                if let Some(pkt) = pkt {
                    // println!("{:#?}", pkt);

                    match pkt {
                        Packet::Config(_) => {}
                        Packet::Ack(p) => {
                            lock!(state <= shared.state.lock().await?, {
                                state.last_ack = Some(p);
                                state.notify_all();
                            });
                        }
                        Packet::Telemetry(p) => {
                            // TODO: Need monitoring for errors.
                            
                            // TODO: Need to check the config_sequence

                            lock!(state <= shared.state.lock().await?, {
                                if p.prediction_error.abs() < 10 {
                                    state.locked_count += 1;
                                } else {
                                    state.locked_count = 0;
                                }

                                state.last_telemetry = Some((p, time));
                                state.notify_all();
                            });
                        }
                        Packet::Heartbeat(p) => {
                            lock!(state <= shared.state.lock().await?, {
                                if state.last_heartbeat.is_none() {
                                    eprintln!("[Got first pps_divider heartbeat]");
                                }

                                state.last_heartbeat = Some((p, time));
                                state.notify_all();
                            });
                        }
                    }
                }
            }
        }
    }


}
