#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::time::{Duration, Instant};
use std::sync::Arc;
use std::collections::HashMap;
use std::io::Write;

use base_args::define_arg_command;
use file::LocalPathBuf;
use common::args::list::CommaSeparated;
use common::errors::*;
use common::bytes::Bytes;
use executor_multitask::RootResource;
use rpc_util::NamedPortArg;
use cluster_client::meta::*;
use cluster_client::{ClusterServer, ClusterMetaClient};
use mocap_proto::mocap::*;
use cluster_client::service::create_rpc_channel;
use file::{project_path, LocalPath};
use net::ip::SocketAddr;
use mocap_manager::calibration::*;
use mocap_manager::*;
use cluster_client::id::*;
use protobuf_json::MessageJsonSerialize;
use mocap_manager::matching::*;
use math::matrix::axis_angle::*;
use protobuf::Message;
use peripherals_service::device::PeripheralsDevice;
use scpi::*;
use image::Image;
use mocap_camera_core::FrameProcessor;
use vision::*;
use math::matrix::vec2d;


const NUM_SAMPLES: usize = 1;

#[derive(Args)]
struct Args {
    command: Command,
}


define_arg_command!(Command {
    BenchmarkFrameProcessorCommand = "benchmark_frame_processor",
    TestCameraBoardCommand = "test_camera_board",
    TestLEDBoardCommand = "test_led_board",
    TestLEDBoardFullPower = "test_led_board_full_power",
    TestLEDBoardRGB = "test_led_board_rgb",
    PowerOffCommand = "power_off",
    UpdateImageCommand = "update_image",
    GrabFramesCommand = "grab_frames",
    CalibrateExtrinsicsCommand = "calibrate_extrinsics",
    DumpMatchesCommand = "dump_matches",
    SetRGBCommand = "set_rgb",
    ConfigureSimulationCommand = "configure_simulation"
});

/*


cargo run --bin mocap_manager -- --port=8000 --data_dir=data/mocap_data_dir

cargo run --bin mocap_manager --release -- --port=8000 --data_dir=data/mocap_data_dir

cargo run --bin mocap_cli -- configure_simulation "playing: true marker_around_cube: true"

cargo run --bin mocap_cli -- configure_simulation "playing: true spinning_wand: true"

cargo run --bin mocap_cli -- configure_simulation "playing: true wanding_calibration: true"


cargo run --bin mocap_cli -- test_led_board_full_power

*/
#[derive(Args)]
pub struct ConfigureSimulationCommand {
    #[arg(positional)]
    proto: String,
}

impl ConfigureSimulationCommand {
    async fn run(self) -> Result<()> {
        let meta_client = ClusterMetaClient::create_from_environment().await?;

        let channel = create_rpc_channel(
            "localhost:8000",
            meta_client.clone()
        ).await?;

        let stub = Arc::new(ManagerStub::new(channel.clone()));

        let mut req = ExecuteRequest::default();
        protobuf::text::parse_text_proto(
            &self.proto,
            req.configure_simulation_mut()
        )?;

        let ctx = rpc::ClientRequestContext::default();

        stub.Execute(&ctx, &req).await.result?;

        Ok(())
    }

}


/*
cargo run --bin mocap_cli --release -- benchmark_frame_processor
*/
#[derive(Args)]
pub struct BenchmarkFrameProcessorCommand {

}

impl BenchmarkFrameProcessorCommand {

    async fn run(self) -> Result<()> {
        // let image = Image::<u8>::read(project_path!("pkg/vision/mocap/scripts/synthetic_human_frame.jpg")).await?.to_grayscale();
        let image = Image::<u8>::read(project_path!("pkg/vision/mocap/scripts/worst_case_image.jpg")).await?.to_grayscale();

        let mut processor = FrameProcessor::new(image.width(), image.height());

        let blob_filter = FrameProcessor::default_blob_filter_config()?;

        let threshold = 100;

        const NUM_ITERS: usize = 127;

        for _ in 0..10 {
            let mut v = 0;
            let s = Instant::now();
            for i in 0..NUM_ITERS {
                processor.reset();
                v += processor.process(&image.array.data, threshold, &blob_filter).blobs().len();
            }
            let e = Instant::now();

            println!("{:?}", v);
            println!("{:?}", (e - s) / (NUM_ITERS as u32));
        }

        Ok(())
    }


}

/*

cargo run --bin mocap_cli -- test_camera_board
*/
#[derive(Args)]
pub struct TestCameraBoardCommand {

}

impl TestCameraBoardCommand {

    pub async fn run(self) -> Result<()> {
        // TODO: Configure i2c explicitly with pull ups and not driven high so that power can't flow
        // through those pins to power the camera board.

        let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let config = configs.remove(&"mocap_camera_tester")
            .ok_or_else(|| err_msg("No config with the given name"))?;

        let (mut device, _) = PeripheralsDevice::create(&config).await?;

        let device = Arc::new(device);

        loop {
            // ENABLE pulled down to GND
            device.gpio_write("enable_drive_high", true).await?;

            println!("Plug in and flash the board");

            println!("Done? [y/N]");
            if !file::read_user_confirmation().await? {
                return Ok(());
            }

            println!("Turning on camera...");
            device.gpio_write("enable_drive_high", false).await?;

            executor::sleep(Duration::from_secs(1)).await?;

            let mut register = [0u8; 2];
            device.i2c_transfer(
                "i2c",
                0x10, // address
                &[0x30, 0x00], // chip id register
                &mut register
            ).await?;

            // Expecting 0xa, 0x56
            println!("Read Register: {:0x?}", register);

            device.gpio_write("enable_drive_high", true).await?;

            if &register[..] != &[0x0A, 0x56] {
                return Err(err_msg("Wrong chip id"));
            }
        }

        Ok(())
    }



}


/*
cargo run --bin mocap_cli -- test_led_board --psu_addr=10.1.0.152

- Channel 1/2 on the PSU should be in serial and supply 48V
- Channel 3 should supply 5V

Note that we turn off 5V when doing IR testing since the IR testing seems to cause lots of interference and noise in the RGB LEDs (probably ground bounce due to our cheap current sense setup)

TODO: I often get:
    Resource permanent failure: USBRadio: Task Receier failed: UnexpectedEndGroup
    Error: ErrorMessage { msg: "Device background thread failed" }


*/

#[derive(Args)]
pub struct TestLEDBoardFullPower {

}

impl TestLEDBoardFullPower {

    pub async fn run(self) -> Result<()> {
        let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let config = configs.remove(&"mocap_led_tester")
            .ok_or_else(|| err_msg("No config with the given name"))?;

        let (mut device, _) = PeripheralsDevice::create(&config).await?;

        let device = Arc::new(device);


        loop {
            device.gpio_write("v_poe_good", true).await?;
            device.pwm_write("strobe_dimming", 1.0).await?;
            device.pwm_write("strobe_en", 0.03).await?;
            executor::sleep(Duration::from_millis(1000)).await?;
        }


    }
}

#[derive(Args)]
pub struct TestLEDBoardRGB {

}

impl TestLEDBoardRGB {

    pub async fn run(self) -> Result<()> {
        let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let config = configs.remove(&"mocap_led_tester")
            .ok_or_else(|| err_msg("No config with the given name"))?;

        let (mut device, _) = PeripheralsDevice::create(&config).await?;

        let device = Arc::new(device);


        loop {
            for current_led in 0..12 {
                let mut buf = vec![];
                for i in 0..12 {
                    if i == current_led {
                        buf.extend_from_slice(&[0xff, 0xff, 0xff]);
                    } else {
                        buf.extend_from_slice(&[0, 0, 0]);
                    }
                }

                device.neopixel_transfer("leds", 0, &buf[..]).await?;
                device.neopixel_show("leds").await?;
                executor::sleep(Duration::from_millis(1000)).await?;

            }
        }
    }
}


#[derive(Args)]
pub struct TestLEDBoardCommand {
    psu_addr: String,
}

impl TestLEDBoardCommand {

    pub async fn run(self) -> Result<()> {
        let mut psu_client = SCPIClient::create(&self.psu_addr).await?;
        psu_client.check_instrument_type(InstrumentType::PowerSupply).await?;

        let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let config = configs.remove(&"mocap_led_tester")
            .ok_or_else(|| err_msg("No config with the given name"))?;

        let (mut device, _) = PeripheralsDevice::create(&config).await?;

        let device = Arc::new(device);

        loop {
            // Before a board is plugged in, stay in the lowest power state to avoid
            // glitching while it is being plugged in. 
            psu_client.toggle_psu_channel(1, false).await?;
            psu_client.toggle_psu_channel(3, false).await?;
            device.gpio_write("v_poe_good", false).await?;
            device.pwm_write("strobe_dimming", 0.0).await?;
            device.pwm_write("strobe_en", 0.0).await?;
            executor::sleep(Duration::from_millis(500)).await?;

            println!("");
            println!("####################");
            println!("# All Power Off");
            println!("####################");
            println!("");

            println!("Plug in the LED board. Continue: [y/N]");
            if !file::read_user_confirmation().await? {
                return Ok(());
            }

            println!("");
            println!("####################");
            println!("# RGB LED Testing");
            println!("####################");
            println!("");

            // Turn on power
            println!("Powering on 5V...");
            psu_client.toggle_psu_channel(3, true).await?;
            executor::sleep(Duration::from_millis(1000)).await?;

            // Clear LEDs initially.
            println!("Clearing LEDs...");
            Self::leds_off(&device).await?;
            executor::sleep(Duration::from_millis(100)).await?;

            println!("Measure 5V idle current...");
            {
                let v = Self::measure_average_current(&device).await?;
                println!("  => Idle: {}", v);
                
                if !Self::current_in_tolerance(v, 0.003) {
                    println!("[[[IDLE CURRENT FAILED]]]");
                    continue;
                }
            }

            if !Self::rgb_led_test(&device).await? {
                println!("[[[FAILED RGB LED TEST]]]");
                continue;
            }

            println!("");
            println!("####################");
            println!("# IR LED Testing");
            println!("####################");
            println!("");

            println!("Switch to 48V power...");
            psu_client.toggle_psu_channel(3, false).await?;
            psu_client.toggle_psu_channel(1, true).await?;
            device.gpio_write("v_poe_good", true).await?;
            executor::sleep(Duration::from_millis(1000)).await?;

            println!("Measure 48V idle current...");
            {
                let v = Self::measure_average_current(&device).await?;
                println!("  => Idle: {}", v);
                
                if !Self::current_in_tolerance(v, 0.001) {
                    println!("[[[IDLE CURRENT FAILED]]]");
                    continue;
                }
            }

            if !Self::ir_led_test(&device).await? {
                println!("[[[FAILED IR LED TEST]]]");
                continue;
            }
        }

        Ok(())
    }


    async fn rgb_led_test(device: &PeripheralsDevice) -> Result<bool> {
        println!("Turning on RGB LEDs (one by one)..");
        let mut passing = true;
        for current_led in 0..2 {

            let mut buf = vec![];
            for i in 0..2 {
                if i == current_led {
                    buf.extend_from_slice(&[0xff, 0xff, 0xff]);
                } else {
                    buf.extend_from_slice(&[0, 0, 0]);
                }
            }

            device.neopixel_transfer("leds", 0, &buf[..]).await?;
            device.neopixel_show("leds").await?;
            executor::sleep(Duration::from_millis(100)).await?;

            let v = Self::measure_average_current(device).await?;
            println!("  => RGB LED {} Current: {}", current_led, v);

            // 0.041414227 if using 12 LEDs
            passing &= Self::current_in_tolerance(v, 0.032);
        }

        Self::leds_off(device).await?;
        executor::sleep(Duration::from_millis(100)).await?;
        Ok(passing)
    }
    
    async fn ir_led_test(device: &PeripheralsDevice) -> Result<bool> {
        const MAX_CURRENT: f32 = 0.105;

        for power_i in 1..11 {
            let mut power = (power_i as f32) / 10.0;

            println!("Test IR LED power level: {}", power);

            device.pwm_write("strobe_dimming", power).await?;
            executor::sleep(Duration::from_millis(10)).await?;

            // At 120Hz, this is around 250us
            device.pwm_write("strobe_en", 0.03).await?;
            executor::sleep(Duration::from_millis(100)).await?;

            let v = Self::measure_average_current(&device).await?;
            println!("  => Current: {}", v);

            if !Self::current_in_tolerance(v, MAX_CURRENT * power) {
                return Ok(false);
            }
        }

        // If we are going over what the efuse can handle or the efuse isn't properly soldered,
        // then the efuse will probably go into thermal shutdown and stop for half a second and
        // then turn back on for half a second which we will detect over longer periods of time
        // as drops in average current draw.
        println!("");
        println!("100% IR Power Stress Test");
        device.pwm_write("strobe_dimming", 1.0).await?;
        device.pwm_write("strobe_en", 0.03).await?;
        for _ in 0..100 {
            let (v, v_peak) = Self::measure_current(&device).await?;
            println!("  => Current: {} avg, {} p95", v, v_peak);

            if !Self::current_in_tolerance(v, MAX_CURRENT) {
                return Ok(false);
            }

            // This check is to make sure that the current limiting is working (we shouldn't be
            // re-charging the capacitor too fast).
            if !Self::current_in_tolerance(v_peak, 0.2) {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn current_in_tolerance(v: f32, expected_v: f32) -> bool {
        // Up to 10% error allowed
        if ((v - expected_v).abs() / expected_v) < 0.1 || (v - expected_v).abs() < 0.01 {
            return true;
        }

        println!("FAIL: expected {}", expected_v);
        false
    }


    async fn leds_off(device: &PeripheralsDevice) -> Result<()> {
        let mut buf = vec![];
        for i in 0..12 {
            buf.extend_from_slice(&[0, 0, 0]);
        }

        device.neopixel_transfer("leds", 0, &buf[..]).await?;
        device.neopixel_show("leds").await?;
        Ok(())
    }

    // Each call to this averages over ~0.5 seconds
    // TODO: Also grab the P99 current (that should be near 0.2 for the IR LEDs)
    async fn measure_average_current(device: &PeripheralsDevice) -> Result<f32> {
        Ok(Self::measure_current(device).await?.0)
    }

    async fn measure_current(device: &PeripheralsDevice) -> Result<(f32, f32)> {
        let mut enqueued_requests = vec![];
        for buffer in ["buf1", "buf2", "buf3"] {
            enqueued_requests.push((
                buffer,
                device.enqueue_analog_read_window(
                    "current_sense", buffer
                ).await?
            ));
        }

        let mut average = 0.0f64;
        let mut count = 0usize;

        let mut all_data = vec![];

        for (buffer, req) in enqueued_requests {
            req.await?;
            let data = device.analog_fetch_window("current_sense", buffer).await?;

            for v in data.iter().cloned() {
                all_data.push(v);
                average += v as f64;
                count += 1;
            }
        }

        average /= (count as f64);
        average /= 0.2; // 200mOhm current sense.

        all_data.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let p99_current = all_data[((all_data.len() as f32) * 0.95) as usize] / 0.2;

        Ok((average as f32, p99_current))
    }


}



/*
PoE power on the switch drops to <1W when in halt mode.

cargo run --bin mocap_cli -- power_off
*/

#[derive(Args)]
struct PowerOffCommand {

}

impl PowerOffCommand {

    async fn find_camera_ips() -> Result<Vec<String>> {
        let meta_client = ClusterMetaClient::create_from_environment().await?;
        let db = meta_client.db();

        // TODO: Base this on the job workers set.

        let nodes = db.list::<NodeMetadataTable>().await?;
        let mut nodes_by_id = HashMap::new();
        for node in &nodes {
            nodes_by_id.insert(node.id(), node);
        }

        let node_scheduling = db.list::<NodeSchedulingMetadataTable>().await?;

        let mut ips = vec![];

        for node_scheduling in node_scheduling {

            let mut matched = false;
            for l in node_scheduling.labels().label() {
                if l.key() == "mocap_camera" {
                    matched = true;
                    break;
                }
            }

            if !matched {
                continue;
            }

            let node_meta = nodes_by_id.get(&node_scheduling.node_id())
                .ok_or_else(|| err_msg("No metadata for node"))?;

            let addr: SocketAddr = node_meta.address().parse()?;

            ips.push(addr.ip().to_string());
        }

        Ok(ips)
    }

    async fn run(self) -> Result<()> {
        let ips = Self::find_camera_ips().await?;
        println!("Found cameras: {:?}", ips);

        for ip in ips {
            println!("### Halting {}", ip);
            println!("{:?}", Self::run_on_ip(&ip));
        }

        Ok(())

    }

    fn run_on_ip(ip: &str) -> Result<Bytes> {
        let mut args = vec![];
        args.push(format!("cluster-user@{}", ip));
        args.push("-i".to_string());
        args.push("~/.ssh/id_cluster".to_string());
        args.push("-o".to_string());
        args.push("ConnectTimeout=2".to_string());
        args.push("sudo systemctl halt".to_string());



        let output = std::process::Command::new("ssh").args(args).output()?;
        if !output.status.success() {
            std::io::stdout().write_all(&output.stdout).unwrap();
            std::io::stderr().write_all(&output.stderr).unwrap();
            return Err(err_msg("Command failed"));
        }

        Ok(output.stdout.into())
    }

/*
/interface ethernet poe set [find] poe-out=off 

cargo run --bin mocap_cli -- update_image
*/

}

#[derive(Args)]
struct UpdateImageCommand {

}

impl UpdateImageCommand {

    async fn run(self) -> Result<()> {

        let data = file::read(project_path!("third_party/rpi/linux/build/out/boot/firmware/kernel_2712.img")).await?;

        let ips = PowerOffCommand::find_camera_ips().await?;

        let remote_path = LocalPath::new("/boot/firmware/kernel_2712.img");

        for ip in ips {
            println!("### {}", ip);
            Self::upload_impl(&data, &ip, remote_path).await?;
        }


        Ok(())
    }

    async fn upload_impl(data: &[u8], ip: &str, remote_path: &LocalPath) -> Result<()> {
        let command = format!("sudo tee {} > /dev/null", remote_path.as_str());

        let mut args = vec![];
        args.push(format!("cluster-user@{}", ip));
        args.push("-i".to_string());
        args.push("~/.ssh/id_cluster".to_string());
        args.push("-o".to_string());
        args.push("ConnectTimeout=2".to_string());

        args.push(command);

        let mut child = std::process::Command::new("ssh")
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(data)?;
        drop(stdin);

        let output = child.wait_with_output()?;
        if !output.status.success() {
            std::io::stdout().write_all(&output.stdout).unwrap();
            std::io::stderr().write_all(&output.stderr).unwrap();
            return Err(err_msg("Command failed"));
        }

        Ok(())
    }

}

#[derive(Args)]
struct GrabFramesCommand {
    camera_addr: String,
    output_dir: LocalPathBuf,
}

impl GrabFramesCommand {
    async fn run(self) -> Result<()> {
        let meta_client = ClusterMetaClient::create_from_environment().await?;
        
        file::create_dir_all(&self.output_dir).await?;

        let channel = create_rpc_channel(
            &self.camera_addr,
            meta_client.clone()
        ).await?;

        let stub = Arc::new(CameraStub ::new(channel.clone()));

        let mut snapshot_i = 0;
        loop {
            println!("Grab snapshot. Continue: [y/N]");
            if !file::read_user_confirmation().await? {
                return Ok(());
            }
            
            println!("Grabbing snapshot {}", snapshot_i);

            let req = ReadFramesRequest::default();
            let ctx = rpc::ClientRequestContext::default();

            let mut res_stream = stub.ReadFrames(&ctx, &req).await;

            let mut frames = vec![];
            while let Some(res) = res_stream.recv().await {
                frames.push(res.mjpeg().to_vec());

                if frames.len() >= NUM_SAMPLES {
                    break;
                }
            }

            if frames.len() != NUM_SAMPLES {
                res_stream.finish().await?;
            }

            for (i, frame) in frames.into_iter().enumerate() {
                let path = self.output_dir.join(&format!("{:04}_{:04}.jpg", snapshot_i, i));
                file::write(&path, frame).await?;
            }

            println!("=> Done");

            snapshot_i += 1;

        }
        
        Ok(())
    }
}

/*
cargo run --bin mocap_cli -- set_rgb --camera_addr=jg30xx5m7wcky.mocap_camera.worker.home.cluster.internal
*/

#[derive(Args)]
struct SetRGBCommand {
    camera_addr: String,
}

impl SetRGBCommand {
    async fn run(self) -> Result<()> {
        let meta_client = ClusterMetaClient::create_from_environment().await?;
        
        let channel = create_rpc_channel(
            &self.camera_addr,
            meta_client.clone()
        ).await?;

        let stub = Arc::new(CameraStub ::new(channel.clone()));

        let req = ReadFramesRequest::default();
        let ctx = rpc::ClientRequestContext::default();

        let status = stub.Status(&ctx, &StatusRequest::default()).await.result?;

        let mut config: MocapCameraConfigureRequest = status.config().clone();

        for (i, c) in config.rgb_led_colors_mut().iter_mut().enumerate() {
            if i == 4 || i == 4 + 6 {
                *c = 0xff;
            } else {
                *c = 0;
            }
        
        }

        stub.Configure(&ctx, &config).await.result?;        
        Ok(())
    }
}

use protobuf::StaticMessage;

/*
cargo run --bin mocap_cli --release -- calibrate_extrinsics --log_path=data/mocap/calibration10_wanding.log


cargo run --bin mocap_cli --release -- calibrate_extrinsics --log_path=data/mocap_data_dir/recording/1783268997955084-wanding.log


cargo run --bin mocap_cli --release -- calibrate_extrinsics --log_path=data/mocap_data_dir/recording/1785191195988207-wanding.log
*/

const CONFIG_PATCH_FILE: &'static str = "config.pb";



#[derive(Args)]
struct CalibrateExtrinsicsCommand {
    log_path: LocalPathBuf,
    // output_path: LocalPathBuf
}

impl CalibrateExtrinsicsCommand {

    async fn run(self) -> Result<()> {

        let data_dir = project_path!("data/mocap_data_dir");

        let config = {
            let mut config = MocapManagerConfig::default();
            protobuf::text::parse_text_proto(
                &file::read_to_string(project_path!("pkg/vision/mocap/config/manager.txtpb")).await?,
                &mut config
            )?;

            let mut config = ManagerConfigContainer::create(&config)?;


            let config_path = data_dir.join(CONFIG_PATCH_FILE);
            if file::exists(&config_path).await? {
                let data = file::read(&config_path).await?;
                let diff = MocapManagerConfig::parse(&data)?;
                config.merge_from(&diff)?;
            }

            config
        };


        let entries = read_log_file(&self.log_path).await?;
        println!("Num Entries: {}", entries.len());

        let initial_system_state = {
            let first_entry = &entries[0];
            if !first_entry.has_system_state() {
                return Err(err_msg("Expected first entry to have system_state"));
            }

            first_entry.system_state()
        };

        let mut intrinsics = config.camera_intrinsics().clone();
        for v in intrinsics.values_mut() {
            v.center += vec2d(0.5, 0.5);
        }

        let mut calibrator = WandingCalibrationSolver::new(
            config.value().clone(),
            intrinsics
        );
        calibrator.set_initial_status(initial_system_state.clone());

        for entry in entries {
            calibrator.add_frame(&entry)?;
        }

        let solution = calibrator.solve()?;

        let mut patch = MocapManagerConfig::default();

        for params in solution.params {
            let proto = patch.new_per_camera();
            proto.set_camera_id_str(entity_id_to_string(params.id).unwrap());
            proto.set_intrinsics(params.intrinsics.to_proto());
            proto.set_extrinsics(params.extrinsics.to_proto());
        }
        
        // println!("{:?}", patch);

        Ok(())
    }
}

use mocap_manager::skeleton::*;
use math::matrix::Vector3d;
use math_proto_util::VectorProtoExt;


use math::matrix::vec3d;

/*
cargo run --bin mocap_cli --release -- dump_matches
cargo run --bin mocap_cli -- dump_matches
*/
#[derive(Args)]
struct DumpMatchesCommand {

}



impl DumpMatchesCommand {

    async fn run(self) -> Result<()> {

        let data_dir = project_path!("data/mocap_data_dir");

        let config = {
            let mut config = MocapManagerConfig::default();
            protobuf::text::parse_text_proto(
                &file::read_to_string(project_path!("pkg/vision/mocap/config/manager.txtpb")).await?,
                &mut config
            )?;

            let mut config = ManagerConfigContainer::create(&config)?;


            let config_path = data_dir.join(CONFIG_PATCH_FILE);
            if file::exists(&config_path).await? {
                let data = file::read(&config_path).await?;
                let diff = MocapManagerConfig::parse(&data)?;
                config.merge_from(&diff)?;
            }

            config
        };


        let mut params = vec![];


        for per_cam in config.per_camera() {
            let id = per_cam.camera_id();

            let intrinsics = match config.camera_intrinsics().get(&id) {
                Some(v) => v.clone(),
                None => continue
            };

            params.push(CameraParameters {
                id,
                intrinsics,
                extrinsics: config.camera_extrinsics().get(&id).unwrap().clone(),
            });
        }
        

        // let entries = read_log_file(&project_path!("data/mocap_data_dir/recording/1785189642152706-human.log")).await?;
        // let entries = read_log_file(&project_path!("data/mocap_data_dir/recording/1785191681256771-rigid.log")).await?;

        // All the part 2 animations.
        // let entries = read_log_file(&project_path!("data/mocap_data_dir/recording/1785368493461614.log")).await?;

        let entries = read_log_file(&project_path!("data/mocap_data_dir/recording/1785384664118013.log")).await?;

        // data/mocap_data_dir/recording/1785383746472082.log

        println!("Num Entries: {}", entries.len());

        let mut matcher = BlobMatcher::new(config.matching());
        matcher.set_camera_parameters(&params);


        let num_cameras = params.len();

        let mut out = MocapTrackingLog::default();


        for cam in &params {
            let proto = out.new_cameras();
            proto.set_id(cam.id);
            for v in cam.extrinsics.translation.as_ref() {
                proto.add_translation(*v);
            }

            for v in cam.extrinsics.rotation.as_ref() {
                proto.add_rotation(*v);
            }
        }

        let mut skeleton_tracker = SkeletonTracker::default();

        {
            let mut config = config.skeleton_tracker().clone();;
            config.clear_skeletons();
            config.new_skeletons().set_id(1u32);
            skeleton_tracker.set_config(config);

            out.add_skeletons(skeleton_tracker.config_patch().skeletons()[0].as_ref().clone());
        }
        skeleton_tracker.set_skeleton_searching(1u32, true);


        let mut rigid_body_tracker = {
            let mut tracker_config = config.rigid_body_tracker().clone();
            tracker_config.clear_bodies();

            let w = config.wand();
            let pts = vec![
                vec3d(0., 0.07, 0.),
                vec3d(-0.0606, -0.035, 0.),
                vec3d(0.0606, -0.035, 0.),
                vec3d(0.02165, -0.025, 0.),
            ];

            let body = tracker_config.new_bodies();
            body.set_id(1u32);

            for pt in pts {
                body.add_points(pt.to_proto());
            }


            out.set_rigid_body_tracker(tracker_config.clone());

            let mut tracker = RigidBodyTracker::default();
            tracker.set_config(tracker_config);
    
            tracker
        };

        
        let start = Instant::now();

        // let profile = executor::spawn(perf::profile_self(Duration::from_secs(10)));
        let mut skeleton_time = Duration::ZERO;

        // TODO: Skip entries without blob data.
        for (i, entry) in entries.into_iter().enumerate() {

            if !entry.has_blobs() {
                continue;
            }

            let idx = out.entries().len();

            matcher.run(entry.blobs());

            rigid_body_tracker.run(&matcher.points());

            for p in skeleton_tracker.to_state_protos() {
                if p.mode() == SkeletonStateProto_Mode::LOST /* || idx == 4352 */ {
                    skeleton_tracker.set_skeleton_searching(p.id(), true);
                }
            }

            let s = Instant::now();
            skeleton_tracker.run(entry.blobs().frame_timestamp(), &matcher.points());
            skeleton_tracker.backpropagate_predicted_points(&mut matcher);
            let e = Instant::now();

            skeleton_time += e - s;

            // println!("# points: {}", points.len());

            let proto = out.new_entries();

            for p in matcher.points() {
                let mut p = p.to_proto();

                // // Mainly to compress the massive json file.
                // if p.camera_ids().len() > 1 {
                //     p.camera_ids_mut().truncate(1);
                // }

                for v in p.position_mut().values_mut() {
                    *v = format!("{:.04}", v).parse()?;
                }

                proto.add_points(p);
            }

            for p in skeleton_tracker.to_state_protos() {
                proto.add_skeletons(p);
            }

            for body in rigid_body_tracker.bodies() {
                if body.transform.is_some() {
                    proto.add_rigid_bodies(body.to_proto());
                }
            }
        }

        let end = Instant::now();

        println!("Skeleton Compute TIme: {:?}", skeleton_time);

        // let profile = profile.join().await?;
        // file::write(project_path!("perf.pb"), profile.serialize()?).await?;


        println!("Matching took: {:?}", end - start);

        println!("Unique tracks: {}", matcher.num_unique_tracks());

        file::write(
            project_path!("pkg/vision/mocap/world/data.json"),
            out.serialize_json(&protobuf_json::SerializerOptions::default())?
        ).await?;

        Ok(())
    }

}



#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    args.command.run().await
}

