#[macro_use]
extern crate macros;

use base_error::*;
use file::project_path;

#[executor_main]
async fn main() -> Result<()> {
    let mut inputs = vec![];

    file::recursively_list_dir(&project_path!("testdata/cnc"), &mut |path| {
        inputs.push(path.to_owned());
    })?;

    file::recursively_list_dir(
        &project_path!("/media/dennis/carvera-inner/gcodes/Examples/"),
        &mut |path| {
            inputs.push(path.to_owned());
        },
    )?;

    inputs
        .push("/home/dennis/Downloads/prusa_sample_gcodes/3DBenchy_PLA_150um_MINI_2h.gcode".into());

    inputs.sort();

    for path in inputs {
        let ext = path.extension();
        if ext != Some("nc") && ext != Some("gcode") {
            continue;
        }

        if path.file_name().unwrap().starts_with(".") {
            continue;
        }

        println!("{}", path.as_str());

        let data = file::read(path).await?;
        let s = std::str::from_utf8(&data)?;

        let mut parser = gcode::ProgramParser::default();
        for line in s.split('\n') {
            let mut out = vec![];
            if let Err(e) = parser.parse_line(line.as_bytes(), &mut out) {
                eprintln!("{}", e);
            }
        }
    }

    Ok(())
}
