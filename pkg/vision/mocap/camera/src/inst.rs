use std::sync::Arc;
use std::collections::VecDeque;
use std::time::{Instant, Duration};

use common::bytes::Bytes;
use common::errors::*;
use pio_rp1::PIO;
use rpi::ws2812::WS2812SPIController;
use peripherals::pwm::*;
use peripherals::i2c::I2CHostController;
use executor::sync::AsyncMutex;
use executor::{lock, lock_async};
use executor::channel;
use executor::channel::oneshot;
use media_camera::rp1_direct::*;
use media_camera::mjpeg::*;
use executor_multitask::{impl_resource_passthrough, ServiceResource, ServiceResourceGroup, BroadcastChannel};
use mocap_proto::mocap::*;
use rpi::temp::CPUTemperatureReader;
use http::header::CONTENT_TYPE;
use ptp::MonotonicClockTimeSyncer;
use v4l2::Controllable;
use media_camera::v4l2::controls::*;
use media_camera::v4l2::capture_buffer::*;
use ptp::SignedDuration;
use rpi::model::Model;
use mocap_camera_core::*;

use crate::sensors::*;
use crate::pps_divider_client::*;
use crate::accelerometer::*;
use crate::timestamp::*;
use crate::strobe::*;
use crate::capture::*;

// TODO: Dedup this constant.
const STROBE_DIMMING_FREQUENCY: f32 = 10000.0;

const NUM_RGB_LEDS: usize = 12;


pub struct MocapCamera {
    resources: ServiceResourceGroup,

    pio_forwarder: Option<PIO>,

    shared: Arc<Shared>,
}

impl_resource_passthrough!(MocapCamera, resources);

struct Shared {
    camera_sensor: CameraSensorData,

    v4l2_controls: Vec<v4l2::ControlDefinition>,

    /// All controls that we directly expose to the user in the Configure RPC.
    user_controls: Vec<v4l2::ControlDefinition>,

    user_controls_prop: media_camera_proto::media::camera::Property,

    pps_divider_client: PPSDividerClient,

    /// This state is locked while we are reading the status of the camera.
    status_state: AsyncMutex<StatusState>,

    /// This state is locked while we are reconfiguring part of the sensor.
    configure_state: AsyncMutex<ConfigureState>,

    /// NOTE: This should never be locked for more than a short period of time to prevent blocking
    /// frame processing.
    config: Arc<AsyncMutex<Arc<MocapCameraConfigureRequest>>>,

    // state: AsyncMutex<State>,

    ptp_device: Arc<ptp::PTPDevice>,

    mono_time_sync: Arc<MonotonicClockTimeSyncer>,

    capture_processor: Arc<MocapCameraCaptureProcessor>,
}

#[derive(Clone)]
pub(crate) struct CameraSensorData {
    pub model_name: String,

    pub width: u32,

    pub height: u32,
}


struct StatusState {
    cpu_temp: CPUTemperatureReader,
    accelerometer: Option<Box<dyn Accelerometer>>,
}

struct ConfigureState {
    strobe_dimming: Option<PWMChannel>,

    last_pps_config: PPSDividerConfigRequest,

    rgb_leds: Option<WS2812SPIController>,

    /// Use this for configuring controls.
    camera_subdev: v4l2::SubDevice,
}

struct State {
    config: Arc<MocapCameraConfigureRequest>,
}

impl MocapCamera {

    pub async fn create(
        config: Arc<CameraHardwareConfigContainer>,
        ptp_device: Arc<ptp::PTPDevice>
    ) -> Result<Self> {
        // Note that will reset the MCU immediately so it will be back in sync with the default
        // config on the host side.
        let pps_divider_client = PPSDividerClient::create(config.clone()).await?;

        let mut strobe_dimming = None;
        if config.local_strobe_dimming() {
            strobe_dimming = Some(PWMChannel::open(0, 0).await?);
            strobe_dimming.as_mut().unwrap().write(STROBE_DIMMING_FREQUENCY, 0.0).await?;
        }

        let mut pio_forwarder = None;
        if config.enable_pio_trigger_forwarder() {
            pio_forwarder = Some(PIO::create_pin_forwarder(16, 22)?);
        }
        
        let mut rgb_leds = None;
        if config.local_rgb_control() {
            rgb_leds = Some(WS2812SPIController::create("/dev/spidev0.0")?);
        }

        let mut i2c_bus = I2CHostController::open(&config.accelerometer_i2c_device())?;
        let accelerometer = Some(create_accelerometer(i2c_bus.device(0x19)).await?);

        // TODO: Have all key controls like exposure and analog/digital gains exported to a config file that we can re-initialize everything on boot.
        let mut cam = RP1DirectCamera::open().await?;

        let v4l2_controls = cam.camera_subdev.list_controls().await?;

        let reserved_controls = vec![
            v4l2::V4L2_CID_VBLANK,
            v4l2::V4L2_CID_EXPOSURE,
            v4l2::V4L2_CID_HFLIP,
            v4l2::V4L2_CID_VFLIP,
            v4l2::V4L2_CID_DIGITAL_GAIN,
            v4l2::V4L2_CID_TEST_PATTERN,
            v4l2::V4L2_CID_TEST_PATTERN_RED,
            v4l2::V4L2_CID_TEST_PATTERN_GREENR,
            v4l2::V4L2_CID_TEST_PATTERN_BLUE,
            v4l2::V4L2_CID_TEST_PATTERN_GREENB,
        ];

        let user_controls = v4l2_controls.iter().cloned().filter(|control| {
            !reserved_controls.contains(&control.id()) &&
            !control.flags().contains(v4l2::ControlFlags::READ_ONLY)
        }).collect::<Vec<_>>();

        let resources = ServiceResourceGroup::new("MocapCamera");

        // TODO: Need to wait for this to be ready before doing anything.
        let mono_time_sync = Arc::new(MonotonicClockTimeSyncer::create(ptp_device.clone())?);
        resources.register_dependency(mono_time_sync.clone()).await;

        let mut user_controls_prop = media_camera_proto::media::camera::Property::default();

        let mut initial_config = MocapCameraConfigureRequest::default();
        initial_config.set_pixel_threshold(255u32);

        initial_config.set_blob_filter(FrameProcessor::default_blob_filter_config()?);
        initial_config.set_mjpeg(MJPEGEncoder::default_config()?);
        controls_to_proto(
            &user_controls, &cam.camera_subdev, "",
            &mut user_controls_prop, initial_config.camera_controls_mut()
        ).await?;
        Self::validate_config(&mut initial_config)?;

        let config = Arc::new(AsyncMutex::new(Arc::new(initial_config)));

        let camera_sensor = CameraSensorData {
            model_name: cam.model_name,
            width: cam.width,
            height: cam.height,
        };

        let capture_processor = Arc::new(MocapCameraCaptureProcessor::create(
            config.clone(),
            camera_sensor.clone(),
            cam.capture_device,
            cam.capture_stream,
            mono_time_sync.clone()
        ).await?);
        resources.register_dependency(capture_processor.clone()).await;

        let shared = Arc::new(Shared {
            pps_divider_client,
            camera_sensor,
            v4l2_controls,
            user_controls,
            user_controls_prop,
            status_state: AsyncMutex::new(StatusState {
                cpu_temp: CPUTemperatureReader::create().await?,
                accelerometer,
            }),
            configure_state: AsyncMutex::new(ConfigureState {
                strobe_dimming,
                rgb_leds,
                // TODO: Always ensure it is initialized to this just in case we need to restart the server.
                last_pps_config: PPSDividerConfigRequest::default(),
                camera_subdev: cam.camera_subdev,
            }),
            config,
            ptp_device,
            capture_processor,
            mono_time_sync,
        });

        Ok(Self {
            resources,
            pio_forwarder,
            shared
        })
    }

    pub async fn configure(&self, config: &MocapCameraConfigureRequest) -> Result<()> {
        let mut config = config.clone();
        Self::validate_config(&mut config)?;

        if self.shared.pps_divider_client.last_heartbeat().await?.is_none() {
            return Err(err_msg("PPS divider not yet healthy"));
        }

        executor::spawn(Self::configure_inner(self.shared.clone(), config)).join().await
    }

    // NOT CANCEL SAFE
    async fn configure_inner(
        shared: Arc<Shared>,
        mut config: MocapCameraConfigureRequest
    ) -> Result<()> {
        // TODO: Have a concept of twhether or not the old config is dirty (must be re-synced)
        
        // TODO: Use a try_lock?
        lock_async!(configure_state <= shared.configure_state.lock().await?, {
            let old_config: Arc<MocapCameraConfigureRequest> = lock!(config <= shared.config.lock().await?, {
                config.clone()
            });

            if config.strobe_power() != old_config.strobe_power() {
                if let Some(strobe_dimming) = &mut configure_state.strobe_dimming {
                    // TODO: Have a timeout on this.
                    strobe_dimming.write(STROBE_DIMMING_FREQUENCY, config.strobe_power()).await?;
                }
            }

            // TODO: If using local SPI, re-run this every few seconds since it is flaky
            if config.rgb_led_colors() != old_config.rgb_led_colors() {
                if let Some(rgb_leds) = &mut configure_state.rgb_leds {
                    rgb_leds.write(config.rgb_led_colors())?;
                }
            }

            // 

            let driver = AR0234Driver::default();

            // TODO: Exposure only needs to be controlled via v4l2 for some sensors.
            {
                let mut exposure_control = None;

                let controls = configure_state.camera_subdev.list_controls().await?;
                for control in controls {
                    if control.id() == v4l2::V4L2_CID_EXPOSURE {
                        exposure_control = Some(control);
                        break;
                    }
                }

                let exposure_control = exposure_control
                    .ok_or_else(|| err_msg("Couldn't find the exposure control"))?;

    
                let exposure_value = driver.exposure_time_to_value(config.exposure_micros());

                configure_state.camera_subdev.set_control_value(
                    &exposure_control,
                    exposure_value as i32
                ).await?;
            }

            let new_camera_states = set_controls_from_proto(
                &configure_state.camera_subdev,
                &shared.user_controls,
                "",
                old_config.camera_controls(),
                config.camera_controls()
            ).await?;
            config.set_camera_controls(new_camera_states);

            // Low values must desable the strobe entirely since we might accidentally enter
            // 'hybrid dimming' mode on the LED driver.
            let strobe_width = {
                if config.strobe_power() >= 0.02 {
                    Some(Duration::from_micros(config.exposure_micros() as u64))
                } else {
                    None
                }
            };

            /*

            frame_offset - exposure_

            After I send the trigger pulse, exposure ends after 8.1ms
            so exposure 
            */

            // TODO: Use better than microsecond precision for the exposure here.

            let pps_config = PPSDividerConfigRequest {
                unlock: false,
                pulse_rate: config.frame_rate() as u8,
                frame_width: Some(driver.frame_width()),
                // TODO: Based on the pulse_rate, maybe pick the negative of this.
                // frame_offset: Some(driver.frame_offset() - Duration::from_micros(config.exposure_micros() as u64)),
                frame_offset: Some(SignedDuration::from_micros(-8160 -126 + (config.exposure_micros() as i64))),

                strobe_width,
                strobe_offset: None,
                strobe_power: config.strobe_power(),

                rgb_color: config.rgb_led_colors()[0]
            };
            if pps_config != configure_state.last_pps_config {
                shared.pps_divider_client.configure(&pps_config).await?;
                configure_state.last_pps_config = pps_config;
            }

            lock!(c <= shared.config.lock().await?, {
                *c = Arc::new(config);
            });

            Ok(())
        })
    }

    fn validate_config(config: &mut MocapCameraConfigureRequest) -> Result<()> {

        if config.frame_rate() == 0 {
            // config.set_strobe_power(0.0);
        } else {
            // NOTE: The LED driver we are using will turn off completely if the LEDs are off for more than 50ms. If the driver is fully off, it won't be enable to re-enable the LEDs quickly so we have to strobe frequently enough to overcome this (so the minimum rate is 20 FPS with some buffer.)

            if config.frame_rate() < 24 {
                config.set_frame_rate(24u32);
            }

            // Must be slightly less than the max FPS of the sensor to avoid it missing a frame if the previous frame
            // is still in progress.
            if config.frame_rate() > 119 {
                config.set_frame_rate(119u32);
            }
        }

        {
            let v = config.exposure_micros();
            config.set_exposure_micros(v.min(6000).max(20));
        }

        {
            let mut v = config.strobe_power();

            /*
            let mut v = StrobeSimulationModel::settings_r1().clamp_power_percent(&StrobeSettings {
                frequency: (config.frame_rate() as f32),
                pulse_width: (config.exposure_micros() as f32) / 1000000.0,
                power_percent: config.strobe_power(),
            }, 0.05);
            */

            if v < 0.05 {
                v = 0.0;
            }

            config.set_strobe_power(v);
        }

        {
            let v = config.pixel_threshold();
            config.set_pixel_threshold(v.min(255).max(1));
        }

        config.rgb_led_colors_mut().resize(NUM_RGB_LEDS, 0);
        

        Ok(())
    }

    pub async fn status(&self) -> Result<MocapCameraStatus> {
        let mut proto = MocapCameraStatus::default();

        {
            let p = proto.sensor_mut();
            p.set_model_name(&self.shared.camera_sensor.model_name);
            p.set_frame_width(self.shared.camera_sensor.width);
            p.set_frame_height(self.shared.camera_sensor.height);
            p.set_max_frame_rate(119u32);
            p.set_min_exposure_micros(20u32);
            p.set_max_exposure_micros(6000u32);
        }

        proto.set_camera_controls(self.shared.user_controls_prop.clone());

        if let Some((pkt, time)) = self.shared.pps_divider_client.last_heartbeat().await? {
            let now = Instant::now();

            let p = proto.pps_divider_heartbeat_mut();
            p.set_mcu_temperature(pkt.temp_c);
            p.set_vcc_voltage_min(pkt.vcc_min_v);
            p.set_vcc_voltage_max(pkt.vcc_max_v);

            fn get_poe_voltage(v: f32) -> f32 {
                electronics::undivide_voltage(
                    v,
                    24.9,
                    1.0
                )
            }

            p.set_poe_voltage_min(get_poe_voltage(pkt.poe_min_v));
            p.set_poe_voltage_max(get_poe_voltage(pkt.poe_max_v));
        }

        if let Some((pkt, time)) = self.shared.pps_divider_client.last_telemetry().await? {

            let p = proto.pps_divider_telemetry_mut();
            p.set_locked(self.shared.pps_divider_client.is_locked().await?);

            let freq = 1.0 / ((pkt.frequency_mhz as f64) * 1_000_000.0);
            p.set_prediction_error((pkt.prediction_error as f64) * freq);
            p.set_pps_width((pkt.width as f64) * freq);
            p.set_output_errors(pkt.output_errors);
        }

        let shared = self.shared.clone();
        proto = executor::spawn(async move {
            lock_async!(state <= shared.status_state.lock().await?, {

                // TODO: All this needs a timeout.

                proto.set_cpu_temperature(state.cpu_temp.read().await? as f32);
                
                if let Some(accelerometer) = &mut state.accelerometer {
                    // TODO: Cache this.
                    let accel = accelerometer.read_acceleration().await?;
                    
                    let v = proto.accelerometer_mut().value_mut();
                    // TODO: Customize whether or not this needs to be flipped based on the camera model.
                    v.set_x(accel.x);
                    v.set_y(accel.y);
                    v.set_z(accel.z);
                }

                Result::<_, Error>::Ok(proto)
            })
        }).join().await?;

        lock!(config <= self.shared.config.lock().await?, {
            proto.set_config(config.as_ref().clone());
        });

        Ok(proto)
    }

    // TODO: Ensure that this has low prioritization over the network.
    pub fn live_stream(&self) -> http::Response {
        // Have the queue always overwrite any pending data in the queue if the queue is full.
        let body = MJPGCameraStreamBody::new(self.shared.capture_processor.mjpeg_subscribers().subscribe(1));

        http::ResponseBuilder::new()
            .status(http::status_code::OK)
            .header(CONTENT_TYPE, body.content_type())
            .body(Box::new(body))
            .build().unwrap()
    }



}

#[async_trait]
impl CameraService for MocapCamera {

    async fn Status(
        &self,
        request: rpc::ServerRequest<StatusRequest>,
        response: &mut rpc::ServerResponse<MocapCameraStatus>
    ) -> Result<()> {
        response.value = self.status().await?;
        Ok(())
    }

    async fn Configure(
        &self,
        request: rpc::ServerRequest<MocapCameraConfigureRequest>,
        response: &mut rpc::ServerResponse<ConfigureResponse>
    ) -> Result<()> {
        self.configure(&request.value).await?;
        Ok(())
    }

    // TODO: Ensure that this RPC is prioritized for network bandwidth relative to everything else
    // on the HTTP server (mainly need to ensure that all MJPEG carrying requests are low priority)
    async fn ReadBlobs(
        &self,
        request: rpc::ServerRequest<ReadBlobsRequest>,
        response: &mut rpc::ServerStreamResponse<ReadBlobsResponse>
    ) -> Result<()> {        
        // TODO: Need some logging if we ever drop frames
        let mut subscriber = self.shared.capture_processor.blob_subscribers().subscribe(1024);

        response.send_head().await?;

        let mut last_response = Instant::now();

        let mut min_interval = Duration::ZERO;
        if request.max_rate() != 0 {
            min_interval = Duration::from_secs_f32(1.0 / (request.max_rate() as f32));
        }

        loop {
            let res = subscriber.recv().await?;
            
            let now = Instant::now();
            if now - last_response >= min_interval {
                response.send(res).await?;
                last_response = now;
            }
        }

        Ok(())
    }

    async fn ReadFrames(
        &self,
        request: rpc::ServerRequest<ReadFramesRequest>,
        response: &mut rpc::ServerStreamResponse<ReadFramesResponse>
    ) -> Result<()> {
        let mut subscriber = self.shared.capture_processor.mjpeg_subscribers().subscribe(8);

        loop {
            let frame = subscriber.recv().await?;

            let mut res = ReadFramesResponse::default();
            // TODO: Make this zero copy
            res.set_mjpeg(&frame[..]);
            response.send(res).await?;
        }

        Ok(())
    }

    async fn FlashMCU(
        &self,
        request: rpc::ServerRequest<FlashMCURequest>,
        response: &mut rpc::ServerResponse<FlashMCUResponse>
    ) -> Result<()> {
        let shared = self.shared.clone();
        executor::spawn(async move {
            lock_async!(configure_state <= shared.configure_state.lock().await?, {
                shared.pps_divider_client.flash(request.firmware()).await
            })
        }).join().await
    }
}


#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn validate_config() {
        let mut initial_config = MocapCameraConfigureRequest::default();
        MocapCamera::validate_config(&mut initial_config).unwrap();
    }

}

