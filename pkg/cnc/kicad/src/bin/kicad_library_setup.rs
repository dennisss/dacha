
#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use std::process::Command;
use std::io::Write;
use std::collections::{HashMap, HashSet};

use common::errors::*;
use kicad::library::*;
use kicad::reader::*;
use kicad::serializer::*;
use reflection::ParseFrom;
use reflection::SerializeTo;
use file::{LocalPath, LocalPathBuf};
use kicad_proto::kicad::*;
use protobuf::{text::ParseTextProto, StaticMessage};

async fn check_kicad_version() -> Result<()> {
    let output = Command::new("kicad-cli").args(vec!["--version"]).output()?;
    if !output.status.success() {
        std::io::stdout().write_all(&output.stdout).unwrap();
        std::io::stderr().write_all(&output.stderr).unwrap();
        return Err(err_msg("Command failed"));
    }

    let version = String::from_utf8(output.stdout)?;
    let v = version.trim();

    if !v.starts_with("9.0.") {
        return Err(format_err!("Unsupported kicad version: {}", v))
    }

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    check_kicad_version().await?;

    let project_dir = file::project_dir();

    let home = LocalPath::new(&std::env::var("HOME")?).to_owned();

    let config_path = home.join(".config/kicad/9.0/kicad_common.json");
    let sym_table_path = home.join(".config/kicad/9.0/sym-lib-table");
    let fp_table_path = home.join(".config/kicad/9.0/fp-lib-table");

    {
        let config_data = file::read_to_string(&config_path).await?;
        let mut config_val = json::parse(&config_data)?;

        let auto_backup = config_val.get_field_mut("auto_backup")
            .ok_or_else(|| err_msg("No auto_backup settings"))?;
        // TODO: THese will panic if auto_backup isn't an object
        auto_backup.set_field("backup_on_autosave", false);
        auto_backup.set_field("enabled", false);

        let env = config_val.get_field_mut("environment")
            .ok_or_else(|| err_msg("No environment settings"))?;
        let env_vars = env.get_field_mut("vars")
            .ok_or_else(|| err_msg("No environment.vars settings"))?;
        match env_vars {
            json::Value::Object(_) => {},
            json::Value::Null => { *env_vars = json::Value::Object(HashMap::default()) },
            _ => return Err(err_msg("Unknown format for environment.vars settings"))
        };
        env_vars.set_field("DACHA_DIR", project_dir.as_str());

        file::write(&config_path, json::pretty_stringify(&config_val)).await?;
        println!("Wrote to {}", config_path.as_str());
    }


    let mut lib_paths = vec![ &sym_table_path, &fp_table_path ];
    let mut lib_tables = vec![];
    let mut used_names = HashSet::new();

    let system_prefixes = &[
        "${KICAD7_FOOTPRINT_DIR}/",
        "${KICAD7_SYMBOL_DIR}/",
        "${KICAD9_FOOTPRINT_DIR}/",
        "${KICAD9_SYMBOL_DIR}/",
    ];

    for path in lib_paths.iter().cloned() {
        if !file::exists(path).await? {
            return Err(format_err!("Missing kicad table at path '{}'. Have you opened kicad at least once?", path.as_str()));
        }

        let data = file::read_to_string(path).await?;
        let e = kicad::sexpr::SExpr::parse(&data)?;
        let input = SExprReader::new(&e);
        let mut table = LibraryTable::parse_from(input)?;

        let mut i = 0;
        while i < table.lib.len() {
            let uri = &table.lib[i].uri;
            if uri.starts_with(project_dir.as_str()) {
                // println!("Clean up old: {}", table.lib[i].name);
                table.lib.remove(i);
                continue;
            }

            let mut is_system_library = false;
            for prefix in system_prefixes {
                if uri.starts_with(prefix) {
                    is_system_library = true;
                    break;
                }
            }

            if !is_system_library {
                println!("WARNING: Unmanaged symbol or footprint library: {}", table.lib[i].name);
            }

            // NOTE: We don't check for duplicates within the existing file of unmanaged libraries.
            used_names.insert(table.lib[i].name.clone());

            i += 1;
        }

        lib_tables.push(table);
    }

    let config = LibrariesConfig::parse_text(
        &file::read_to_string(project_path!("pkg/cnc/kicad/config/libraries.txtpb")).await?)?;

    for lib in config.libraries() {
        if lib.path().starts_with("/") {
            return Err(err_msg("Not a relative library path"));
        }

        let path = project_dir.join(lib.path());
        if !path.starts_with(&project_dir) {
            return Err(err_msg("Library path not within the repository."));
        }

        if !file::metadata(&path).await?.is_dir() {
            return Err(err_msg("Library path is not a directory"));
        }

        let mut name = lib.name();
        if name.is_empty() {
            name = path.file_name().unwrap();
        }

        if !used_names.insert(name.to_string()) {
            return Err(format_err!("Duplicate library name: {}", name));
        }

        let mut all_files = vec![];
        file::recursively_list_dir(
            &path,
            &mut |path| all_files.push(path.to_owned()),
        )?;

        let mut sym_path: Option<LocalPathBuf> = None;
        let mut fp_path: Option<LocalPathBuf> = None;

        for path in all_files {
            let ext = match path.extension() {
                Some(v) => v.to_ascii_lowercase(),
                None => continue
            };

            match ext.as_str() {
                "kicad_sym" => {
                    if sym_path.is_some() {
                        return Err(err_msg("Multiple symbol paths in directory"));
                    }

                    sym_path = Some(path.normalized());
                }
                "kicad_mod" => {
                    if let Some(p) = fp_path.clone() {
                        if *p != *path.parent().unwrap() {
                            return Err(err_msg("Multiple sub directories containing footprints"));
                        }
                    }

                    fp_path = Some(path.parent().unwrap().to_owned());
                }
                "stl" | "step" | "stp" => {
                    // ${DACHA_DIR}
                    // TODO: Verify 3d files are linked
                    // println!("{:?}", path);
                }
                _ => {}
            }
        }

        // TODO: Check that the same paths aren't being references across multiple libraries.

        for (i, path) in [sym_path, fp_path].into_iter().enumerate() {
            if let Some(path) = path {
                lib_tables[i].lib.push(Library {
                    name: name.to_string(),
                    typ: "KiCad".into(),
                    uri: path.as_str().to_string(),
                    options: String::new(),
                    descr: String::new(),
                });
            }
        }
    }

    for (i, token) in ["sym_lib_table", "fp_lib_table"].into_iter().enumerate() {
        let mut out = SExprSerializer::new();
        lib_tables[i].serialize_to(out.root(token)?)?;
        let data = out.finish();

        file::write(&lib_paths[i], &data).await?;
        println!("Wrote to {}", lib_paths[i].as_str());
    }

    Ok(())
}