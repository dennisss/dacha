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
use protobuf::Message;


#[derive(Args)]
struct Args {
    image: LocalPathBuf,
    disk: String,
    hardware_config: String,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let mut hardware_config = CameraHardwareConfig::default();
    protobuf::text::parse_text_proto(&args.hardware_config, &mut hardware_config)?;

    let inner_cmd = rpi_imager::WriteCommand {
        image: args.image,
        disk: args.disk,
        hardware_model: Some(match hardware_config.compute_module() {
            ComputeModuleModel::UNKNOWN => todo!(),
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