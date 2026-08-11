/*
cargo run --bin mocap_deb -- supervisor
cargo run --bin mocap_deb -- camera
*/


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
use executor::bundle::TaskResultBundle;
use file::LocalPathBuf;
use macros::executor_main;
use file::project_path;

#[derive(Args)]
struct Args {
    #[arg(positional)]
    component: String
}

struct PackageConfig {
    build_target: String,
    service_name: String,
    // TODO: Verify this exists in the final file system
    bin_name: String,
    install_path: String,
}

async fn build_package(config: &PackageConfig) -> Result<()> {
    let mut b = builder::Builder::default()?;

    let result = b
        .build_target_cwd(
            &config.build_target,
            "//pkg/builder/config:rpi64"
        )
        .await?;

    println!("{:#?}", result);

    let tmp_dir = file::temp::TempDir::create()?;

    let pkg_dir = tmp_dir.path().join("pkg");

    for (rel_path, output_file) in result.outputs.output_files {

        let pkg_path = pkg_dir.join(&config.install_path).join(rel_path);
        file::create_dir_all(pkg_path.parent().unwrap()).await?;

        file::copy(&output_file.location, &pkg_path).await?;

        let mut perms = file::metadata(&output_file.location).await?.permissions();
        file::set_permissions(&pkg_path, perms).await?;
    }

    file::write(pkg_dir.join("WORKSPACE"), "").await?;

    // (input_path, output_path, mode)
    let templated_files: &[(LocalPathBuf, LocalPathBuf, u32)] = &[
        (
            project_path!("pkg/vision/mocap/camera/config/service"),
            pkg_dir.join(&format!("etc/systemd/system/{}.service", &config.service_name)),
            0o644
        ),
        (
            project_path!("pkg/vision/mocap/camera/config/debian-control"),
            pkg_dir.join("DEBIAN/control"),
            0o644
        ),
        (
            project_path!("pkg/vision/mocap/camera/config/debian-postinst"),
            pkg_dir.join("DEBIAN/postinst"),
            0o755
        ),
        (
            project_path!("pkg/vision/mocap/camera/config/debian-prerm"),
            pkg_dir.join("DEBIAN/prerm"),
            0o755
        ),
    ];

    for (input_path, output_path, mode) in templated_files {
        // TODO: Also change the working directory.
        let data = file::read_to_string(input_path).await?
            .replace("{SERVICE_NAME}", &config.service_name)
            .replace("{PACKAGE_NAME}", &config.service_name)    
            .replace("{COMMAND}", &format!("/{}/{}", &config.install_path, &config.bin_name))
            .replace("{WORKING_DIR}", &format!("/{}", &config.install_path));

        file::create_dir_all(output_path.parent().unwrap()).await?;

        file::write(&output_path, &data).await?;

        let mut perms = file::metadata(&output_path).await?.permissions();
        perms.set_mode(*mode);
        file::set_permissions(&output_path, perms).await?;
    }


    let deb_path = project_path!(format!("third_party/pi-gen/data/{}.deb", &config.service_name)); 

    // TODO: Use command_args!
    {
        let status = std::process::Command::new("dpkg-deb")
            .args(&[
                "--root-owner-group",
                "--build", pkg_dir.as_str(),
                deb_path.as_str()
            ])
            .status()?;
        if !status.success() {
            return Err(err_msg("Failed to create package"));
        }
    }

    println!("Generated {}", deb_path.display());

    Ok(())
}


#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let pkg_config = match args.component.as_str() {
        "supervisor" => {
            PackageConfig {
                build_target: "//pkg/vision/mocap/camera/supervisor:mocap_camera_supervisor_deps".into(),
                service_name: "mocap-camera-supervisor".into(),
                bin_name: "built/pkg/vision/mocap/camera/supervisor/mocap_camera_supervisor".into(),
                install_path: "opt/mocap/supervisor/bundle".into()
            }
        },
        "camera" => {
            PackageConfig {
                build_target: "//pkg/vision/mocap/camera:mocap_camera_deps".into(),
                service_name: "mocap-camera".into(),
                bin_name: "built/pkg/vision/mocap/camera/mocap_camera".into(),
                install_path: "opt/mocap/camera/bundle".into()
            }
        }
        _ => {
            return Err(format_err!("Unknown component: {}", args.component));
        }
    };

    build_package(&pkg_config).await?;

    Ok(())
}