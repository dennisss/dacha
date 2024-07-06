use base_error::*;
use cnc_monitor_proto::cnc::*;
use file::{project_path, LocalPath};

pub async fn get_machine_presets() -> Result<Vec<MachineConfig>> {
    let mut out = vec![];

    let dir = project_path!("pkg/cnc/monitor/presets");
    for entry in file::read_dir(&dir)? {
        let data = file::read_to_string(&dir.join(entry.name())).await?;

        let mut preset = MachineConfig::default();
        protobuf::text::parse_text_proto(&data, &mut preset)?;
        preset.set_base_config(
            LocalPath::new(entry.name())
                .file_stem()
                .ok_or_else(|| err_msg("File has no name"))?,
        );

        out.push(preset);
    }

    Ok(out)
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

pub async fn get_prusa_i3_mk3sp_config() -> Result<MachineConfig> {
    let presets = get_machine_presets().await?;

    presets
        .into_iter()
        .find(|preset| preset.base_config() == "prusa_i3_mk3sp")
        .ok_or_else(|| err_msg("Config not found"))
}
