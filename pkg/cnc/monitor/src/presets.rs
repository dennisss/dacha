use base_error::*;
use cnc_monitor_proto::cnc::*;

pub fn get_machine_presets() -> Result<Vec<MachineConfig>> {
    Ok(vec![get_prusa_i3_mk3sp_config()?])
}

/*
TODO: Variables from the Prusa I3 MK3s Firmware that we should reference:

    // Home position
    #define MANUAL_X_HOME_POS 0
    #define MANUAL_Y_HOME_POS -2.2
    #define MANUAL_Z_HOME_POS 0.2

    #define X_CANCEL_POS 50
    #define Y_CANCEL_POS 190
    #define Z_CANCEL_LIFT 50

    //Pause print position
    #define X_PAUSE_POS 50
    #define Y_PAUSE_POS 190
    #define Z_PAUSE_LIFT 20

    #define MANUAL_FEEDRATE {2700, 2700, 1000, 100}   // set the speeds for manual moves (mm/min)
*/

/// Config for a Prusa i3 MK3s+.
///
/// When connected via USB, it shows up as `/dev/ttyACM0` with baud rate 115200.
///
/// Sample lsusb information:
///   idVendor           0x2c99 Prusa
///   idProduct          0x0002
///   bcdDevice            1.30
///   iManufacturer           1 Prusa Research (prusa3d.com)
///   iProduct                2 Original Prusa i3 MK3
///   iSerial                 3 CZPX1419X004XK19470
pub fn get_prusa_i3_mk3sp_config() -> Result<MachineConfig> {
    let mut preset = MachineConfig::default();
    protobuf::text::parse_text_proto(
        r#"
        base_config: "prusa_i3_mk3sp"
        model_name: "Prusa i3 MK3s+"
        auto_connect: true

        work_area {
            x_range { min: 0 max: 250 }
            y_range { min: 0 max: 210 }
        }

        device {
            usb {
                vendor: 0x2c99
                product: 0x0002
            }
        }

        firmware: MARLIN
        baud_rate: 115200
        reset_using_dtr: true
        silent_mode: true

        axes: [
            { id: "X" type: POSITION range { min: 0 max: 255 } },
            { id: "Y" type: POSITION range { min: -4 max: 212.5 } },
            { id: "Z" type: POSITION range { min: 0.15 max: 210 } },
            { id: "E" type: POSITION },
            
            { id: "E0" type: FAN_TACHOMETER_RPM name: "Extruder Fan (RPM)" },
            { id: "PRN1" type: FAN_TACHOMETER_RPM name: "Part Cooling Fan (RPM)" },
            { id: "E0@" type: FAN_PWM_VALUE },
            { id: "PRN1@" type: FAN_PWM_VALUE },

            { id: "T" type: HEATER name: "Hotend"  },
            { id: "T0" type: HEATER name: "T0" hide: true  },
            { id: "B" type: HEATER name: "Bed"  },
            { id: "@" type: GENERIC_SENSOR name: "Hotend Power" },
            { id: "B@" type: GENERIC_SENSOR name: "Bed Power" },
            { id: "P" type: GENERIC_SENSOR name: "PINDAv2" },
            { id: "A" type: GENERIC_SENSOR name: "Ambient" }
        ]

        "#,
        &mut preset,
    )?;

    Ok(preset)
}
