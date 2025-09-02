use std::process::{Command, Stdio};

use base_error::*;
use file::{LocalPath, LocalPathBuf};

// TODO: Also make a script for doing JLCPCB format exports.

#[derive(Debug)]
pub struct KicadPCBExport {
    pub drill_file: LocalPathBuf,
    pub front_copper: LocalPathBuf,
    pub front_mask: LocalPathBuf,
    pub front_paste: LocalPathBuf,
    pub back_copper: LocalPathBuf,
    pub back_mask: LocalPathBuf,
    pub back_paste: LocalPathBuf,
    pub edge_cuts: LocalPathBuf,
}

impl KicadPCBExport {
    /// Exports gerber/excellon files from Kicad suitable for
    ///
    /// pcb_path should be an absolute path to a '.kicad_pcb' file.
    ///
    /// TODO: Make async
    pub fn generate(pcb_path: &LocalPath, output_dir: &LocalPath) -> Result<Self> {
        let pcb_path_string = pcb_path.to_string();

        // NOTE: Output path must end with a '/5'
        let output_dir_string = format!("{}/", output_dir.to_string());

        let out = Command::new("kicad-cli")
            .args([
                "pcb",
                "export",
                "drill",
                "--output",
                output_dir_string.as_str(),
                "--format",
                "excellon",
                "--excellon-units",
                "mm",
                "--drill-origin",
                "absolute",
                pcb_path_string.as_str(),
            ])
            .output()?;

        if !out.status.success() {
            return Err(err_msg("Failed to generate drill file."));
        }

        let out = Command::new("kicad-cli")
            .args([
                "pcb",
                "export",
                "gerbers",
                "--output",
                output_dir_string.as_str(),
                "--no-protel-ext",
                "--layers",
                "F.Paste,F.Cu,F.Mask,B.Paste,B.Cu,B.Mask,Edge.Cuts",
                pcb_path_string.as_str(),
            ])
            .output()?;

        if !out.status.success() {
            return Err(err_msg("Failed to generate gerber files."));
        }

        let base_name = pcb_path
            .file_stem()
            .ok_or_else(|| err_msg("Unknown file stem"))?;

        Ok(Self {
            drill_file: output_dir.join(format!("{}.drl", base_name)),
            front_copper: output_dir.join(format!("{}-F_Cu.gbr", base_name)),
            front_mask: output_dir.join(format!("{}-F_Mask.gbr", base_name)),
            front_paste: output_dir.join(format!("{}-F_Paste.gbr", base_name)),
            back_copper: output_dir.join(format!("{}-B_Cu.gbr", base_name)),
            back_mask: output_dir.join(format!("{}-B_Mask.gbr", base_name)),
            back_paste: output_dir.join(format!("{}-B_Paste.gbr", base_name)),
            edge_cuts: output_dir.join(format!("{}-Edge_Cuts.gbr", base_name)),
        })
    }
}
