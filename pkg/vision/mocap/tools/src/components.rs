

pub struct Component {
    pub name: String,
    pub artifact: String,
    pub source: ComponentSource,
    pub updater: ComponentUpdater,
}

impl Component {
    pub fn all() -> Vec<Component> {
        vec![
            Component {
                name: "supervisor".into(),
                artifact: "dist/pkg/vision/mocap/mocap-supervisor.deb".into(),
                source: ComponentSource::SoftwarePackage(PackageConfig {
                    build_target: "//pkg/vision/mocap/camera/supervisor:mocap_camera_supervisor_deps".into(),
                    service_name: "mocap-supervisor".into(),
                    bin_name: "built/pkg/vision/mocap/camera/supervisor/mocap_camera_supervisor".into(),
                    install_path: "opt/mocap/supervisor/bundle".into()
                }),
                updater: ComponentUpdater::DebUpdate,
            },
            Component {
                name: "camera".into(),
                artifact: "dist/pkg/vision/mocap/mocap-camera.deb".into(),
                source: ComponentSource::SoftwarePackage(PackageConfig {
                    build_target: "//pkg/vision/mocap/camera:mocap_camera_deps".into(),
                    service_name: "mocap-camera".into(),
                    bin_name: "built/pkg/vision/mocap/camera/mocap_camera --rpc_port=82 --ptp_port=319 --hardware_config=/boot/firmware/camera_hardware.pb --mode=RPC_INSECURE".into(),
                    install_path: "opt/mocap/camera/bundle".into()
                }),
                updater: ComponentUpdater::DebUpdate,
            },
            Component {
                name: "kernel".into(),
                artifact: "dist/pkg/rpi/linux-kernel-dacha-rpi-arm64.deb".into(),
                source: ComponentSource::BashCommand("./pkg/rpi/scripts/compile_kernel.sh".into()),
                updater: ComponentUpdater::DebUpdate
            },
            Component {
                name: "ar0234".into(),
                artifact: "dist/pkg/rpi/ar0234-driver-rpi-arm64.deb".into(),
                source: ComponentSource::BashCommand("./pkg/rpi/scripts/compile_ar0234.sh".into()),
                updater: ComponentUpdater::DebUpdate                
            },
            Component {
                name: "mcu".into(),
                artifact: "dist/pkg/vision/mocap/pps_divider.bin".into(),
                source: ComponentSource::BashCommand("pkg/vision/mocap/pps_divider/build.sh".into()),
                updater: ComponentUpdater::MCUFlash
            }
        ]
    }

}

pub enum ComponentSource {
    SoftwarePackage(PackageConfig),
    BashCommand(String),
}

pub enum ComponentUpdater {
    DebUpdate,
    MCUFlash
}


pub struct PackageConfig {
    pub build_target: String,
    pub service_name: String,
    // TODO: Verify this exists in the final file system
    pub bin_name: String,
    pub install_path: String,
}
