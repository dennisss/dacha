use mocap_proto::mocap::{CameraHardwareConfig, ComputeModuleModel};


pub struct CameraHardwareConfigContainer {
    config: CameraHardwareConfig
}

impl_deref!(CameraHardwareConfigContainer::config as CameraHardwareConfig);

impl CameraHardwareConfigContainer {
    pub fn new(config: CameraHardwareConfig) -> Result<Self> {        
        Self {
            config
        }
    }

    pub fn mcu_reset_pin(&self) -> u32 {
        26
    }

    pub fn mcu_swd_clk_pin(&self) -> u32 { 
        27
    }

    pub fn mcu_swd_io_pin(&self) -> u32 {
        17
    }

    pub fn mcu_serial_device(&self) -> String {
        // GPIO4: MCU_UART_RX
        // GPIO5: MCU_UART_TX
        if self.config.compute_board_revision() < 6 {
            return match self.config.compute_module() {
                ComputeModuleModel::PI_CM4 | ComputeModuleModel::PI_CM4_LITE => {
                    "/dev/ttyAMA3"
                }
                ComputeModuleModel::PI_CM5 | ComputeModuleModel::PI_CM5_LITE => {
                    "/dev/ttyAMA2"
                }
                _ => panic!()
            }.into();
        }

        // GPIO14: MCU_UART_RX
        // GPIO15: MCU_UART_TX
        "/dev/ttyAMA0".into()
    }

    pub fn accelerometer_i2c_device(&self) -> String {
        "/dev/i2c-1".into()
    }

    pub fn enable_pio_trigger_forwarder(&self) -> bool {
        self.config.compute_board_revision() < 4
    }

    pub fn local_strobe_dimming(&self) -> bool {
        self.config.compute_board_revision() < 6
    }

    pub fn local_rgb_control(&self) -> bool {
        self.config.compute_board_revision() < 8
    }

    pub fn ptp_interface(&self) -> String {
        "eth0".into()
    }
}

