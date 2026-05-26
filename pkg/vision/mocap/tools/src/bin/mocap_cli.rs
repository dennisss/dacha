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
use file::project_path;
use net::ip::SocketAddr;
use mocap_manager::calibration::*;
use mocap_manager::*;
use cluster_client::id::entity_id_from_string;
use protobuf_json::MessageJsonSerialize;
use mocap_manager::matching::*;
use math::matrix::axis_angle::*;
use protobuf::Message;

/*



Checkerboard algorithms:
- ROCHADE
- http://vigir.ee.missouri.edu/~gdesouza/Research/Conference_CDs/ECCV_2014/papers/8692/86920766.pdf

Node Id: pq2n7e8rx5622
    cargo run --bin mocap_cli -- grab_frames \
        --camera_addr=h206fq5m2pbe9.mocap_camera.worker.home.cluster.internal \
        --output_dir=data/mocap_camera_calib/h206fq5m2pbe9/



Node Id: na4sqzecvh7mb
    cargo run --bin mocap_cli -- grab_frames \
        --camera_addr=rs3gvvb179szh.mocap_camera.worker.home.cluster.internal \
        --output_dir=data/mocap_camera_calib/rs3gvvb179szh/


Node Id: mj1dwhmrk75ze
    cargo run --bin mocap_cli -- grab_frames \
        --camera_addr=ab21z2zt1gf6w.mocap_camera.worker.home.cluster.internal \
        --output_dir=data/mocap_camera_calib/ab21z2zt1gf6w/



First run:

    make -C pkg/vision/mocap/pps_divider PLATFORM=stm32g031

cargo run --bin mocap_cli -- flash_mcu \
        --camera_addr=h206fq5m2pbe9.mocap_camera.worker.home.cluster.internal

cargo run --bin mocap_cli -- flash_mcu \
        --camera_addr=q3nn1z18yq6q9.mocap_camera.worker.home.cluster.internal

        

*/

const NUM_SAMPLES: usize = 1;

#[derive(Args)]
struct Args {
    command: Command,
}


define_arg_command!(Command {
    PowerOffCommand = "power_off",
    FlashMCUCommand = "flash_mcu",
    GrabFramesCommand = "grab_frames",
    CalibrateExtrinsicsCommand = "calibrate_extrinsics",
    DumpMatchesCommand = "dump_matches",
});


/*
PoE power on the switch drops to <1W when in halt mode.

cargo run --bin mocap_cli -- power_off
*/

#[derive(Args)]
struct PowerOffCommand {

}

impl PowerOffCommand {

    async fn run(self) -> Result<()> {
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


}

#[derive(Args)]
struct FlashMCUCommand {
    camera_addr: String
}

impl FlashMCUCommand {
    async fn run(self) -> Result<()> {
        let firmware = file::read(project_path!("pkg/vision/mocap/pps_divider/build/stm32g031/pps_divider.bin")).await?;
        
        let meta_client = ClusterMetaClient::create_from_environment().await?;
        
        let channel = create_rpc_channel(
            &self.camera_addr,
            meta_client.clone()
        ).await?;

        let stub = Arc::new(MocapCameraStub::new(channel.clone()));

        let mut req = FlashMCURequest::default();
        req.set_firmware(firmware);

        let ctx = rpc::ClientRequestContext::default();

        let res = stub.FlashMCU(&ctx, &req).await.result?;

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

        let stub = Arc::new(MocapCameraStub::new(channel.clone()));

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
cargo run --bin mocap_cli --release -- calibrate_extrinsics
*/

#[derive(Args)]
struct CalibrateExtrinsicsCommand {
    // log_path: LocalPathBuf,
    // output_path: LocalPathBuf
}

impl CalibrateExtrinsicsCommand {

    async fn run(self) -> Result<()> {

        let mut config = MocapManagerConfig::default();
        protobuf::text::parse_text_proto(
            &file::read_to_string(project_path!("pkg/vision/mocap/config/manager.txtpb")).await?,
            &mut config
        )?;

        let entries = read_log_file(&project_path!("data/mocap/calibration.log")).await?;
        println!("Num Entries: {}", entries.len());

        let extrinsics = MocapCameraExtrinsicsCalibrator::calibrate(&config, &entries)?;

        for cam in config.per_camera_mut() {
            let camera_id = entity_id_from_string(cam.camera_id_str()).unwrap();
            let extrinsics = extrinsics.get(&camera_id).unwrap();
            cam.set_extrinsics(extrinsics_to_proto(&extrinsics));
        }

        println!("{:?}", config);

        Ok(())
    }
}

#[derive(Args)]
struct DumpMatchesCommand {

}


impl DumpMatchesCommand {

    async fn run(self) -> Result<()> {

        let mut config = MocapManagerConfig::default();
        protobuf::text::parse_text_proto(
            &file::read_to_string(project_path!("pkg/vision/mocap/config/manager.txtpb")).await?,
            &mut config
        )?;

        let mut params = vec![];


        for per_cam in config.per_camera() {
            let camera_id = entity_id_from_string(per_cam.camera_id_str()).unwrap();
            params.push(CameraParameters {
                id: camera_id,
                intrinsics: intrinsics_from_proto(per_cam.intrinsics()),
                extrinsics: extrinsics_from_proto(per_cam.extrinsics()),
            });
        }
        

        let entries = read_log_file(&project_path!("data/mocap/calibration.log")).await?;
        println!("Num Entries: {}", entries.len());

        let mut matcher = BlobMatcher::new(config.matching(), &params);

        let num_cameras = params.len();

        let mut out = MocapTrackingLog::default();

        let start = Instant::now();

        // let profile = executor::spawn(perf::profile_self(Duration::from_secs(5)));

        // TODO: Skip entries without blob data.
        for entry in entries {

            let points = matcher.run(entry.blobs());

            // println!("# points: {}", points.len());

            let proto = out.new_entries();

            for cam in &params {
                let proto = proto.new_cameras();
                proto.set_id(cam.id);
                for v in cam.extrinsics.position().as_ref() {
                    proto.add_position(*v);
                }

                let rot = from_axis_angle(&cam.extrinsics.rotation).transpose();

                for v in rot.as_ref() {
                    proto.add_rotation(*v);
                }
            }


            for (i, p) in points.iter().enumerate() {
                let proto = proto.new_points();
                proto.set_id(p.id);

                proto.set_radius(0.02);

                for v in p.position.as_ref() {
                    proto.add_position(*v);
                }

                for id in &p.camera_ids {
                    proto.add_camera_ids(*id);
                }

            }
        }

        let end = Instant::now();

        // let profile = profile.join().await?;
        // file::write(project_path!("perf.pb"), profile.serialize()?).await?;


        println!("Matching took: {:?}", end - start);

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

