use std::io::Read;
use std::sync::Arc;
use std::{fs::File, time::Duration};
use std::time::Instant;

use common::errors::*;
use common::io::{Readable, Writeable};
use executor::bundle::TaskResultBundle;
use file::{LocalPath, LocalPathBuf};
use file::{project_path, project_dir};

use crate::components::*;

#[derive(Args)]
pub struct BuildCommand {
    #[arg(positional)]
    component: String
}

impl BuildCommand {

    pub async fn run(self) -> Result<()> {
        let components = Component::all();

        let component = components.iter().find(|c| c.name == self.component)
            .ok_or_else(|| format_err!("Unknown component: {}", self.component))?;
        
        let artifact_path = project_dir().join(&component.artifact);

        match &component.source {
            ComponentSource::SoftwarePackage(pkg) => {
                build_package(&pkg, &artifact_path).await?;
            }
            ComponentSource::BashCommand(cmd) => {
                let mut child = std::process::Command::new("bash")
                    .arg("-c")
                    .arg(&cmd)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    .spawn()?;

                let status = child.wait()?;
                if !status.success() {
                    return Err(err_msg("Command failed!"));
                }
            }
        }
        
        Ok(())
    }
}


async fn build_package(config: &PackageConfig, deb_path: &LocalPath) -> Result<()> {
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

    file::write(pkg_dir.join(&config.install_path).join("WORKSPACE"), "").await?;

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

    file::create_dir_all(deb_path.parent().unwrap()).await?;

    let start_time = Instant::now();
    {
        let status = command_args!("
                dpkg-deb
                -Z zstd
                -z 3
                --root-owner-group
                --build {pkg_dir.as_str()}
                {deb_path.as_str()}
            ")
            .env("SOURCE_DATE_EPOCH", "962409600")
            .status()?;
        if !status.success() {
            return Err(err_msg("Failed to create package"));
        }
    }
    let end_time = Instant::now();


    println!("dpkg-deb generated {} in {:?}", deb_path.display(), end_time - start_time);

    Ok(())
}
