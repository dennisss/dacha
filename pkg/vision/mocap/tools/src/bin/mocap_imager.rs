#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::io::Read;
use std::sync::Arc;
use std::{fs::File, time::Duration};
use std::time::Instant;

use common::errors::*;
use common::io::{Readable, Writeable};
use file::LocalPathBuf;
use file::project_path;
use mocap_proto::mocap::*;
use protobuf::{Message, StaticMessage};
use cluster_client::id::{entity_id_to_string, normalize_entity_id};


#[derive(Args)]
struct Args {
    image: LocalPathBuf,
    disk: String,
    base_config_file: Option<LocalPathBuf>,
    hardware_config: String,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let mut hardware_config = CameraHardwareConfig::default();
    if let Some(path) = args.base_config_file {
        hardware_config = CameraHardwareConfig::parse(&file::read(path).await?)?;
    }

    {
        let mut patch = CameraHardwareConfig::default();
        protobuf::text::parse_text_proto(&args.hardware_config, &mut patch)?;
        hardware_config.merge_from(&patch)?;        
    }

    if hardware_config.camera_id() == 0 {
        let mut id = [0u8; 8];
        crypto::random::secure_random_bytes(&mut id).await?;

        let id = normalize_entity_id(u64::from_be_bytes(id));
        hardware_config.set_camera_id(id);

        println!("Assigned new camera id: {}", entity_id_to_string(id).unwrap());
    }

    let inner_cmd = rpi_imager::WriteCommand {
        image: args.image,
        disk: args.disk,
        hardware_model: Some(match hardware_config.compute_module() {
            ComputeModuleModel::UNKNOWN_CM => todo!(),
            ComputeModuleModel::PI_CM4 | ComputeModuleModel::PI_CM4_LITE => {
                rpi_imager::HardwareModel::Cm4
            },
            ComputeModuleModel::PI_CM5 => rpi_imager::HardwareModel::Cm5Regular,
            ComputeModuleModel::PI_CM5_LITE => rpi_imager::HardwareModel::Cm5Lite,
        }),
        config_txt_patch_file: Some(project_path!("pkg/vision/mocap/config/camera_config_patch.txt")),
        generate_first_boot: true,

        // Unused args
        ssh_public_key: None,
        wpa_ssid: None,
        wpa_password: None,
        ip_address: None,
        netmask: None,
        gateway: None,
        network_config_type: None,
        no_confirm: false
    };

    let mut extra_args = rpi_imager::WriteExtraArgs::default();

    extra_args.extra_files.push((
        "/boot/firmware/camera_hardware.pb".into(),
        hardware_config.serialize()?
    ));

    rpi_imager::run_write_command_ext(inner_cmd, extra_args).await?;

    Ok(())
}