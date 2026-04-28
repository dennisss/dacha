extern crate common;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate base_args;

use std::collections::BTreeMap;

use common::errors::*;
use file::{LocalPathBuf, LocalPath};

const LOCAL_VERSION_KEY: &'static str = "CONFIG_LOCALVERSION";

#[derive(Args)]
struct Args {
    cmd: Command
}


define_arg_command!(Command {
    DiffConfigsCommand = "diff-configs",
    ApplyConfigDiffCommand = "apply-config-diff",
});

#[derive(Args)]
struct DiffConfigsCommand {
    base_path: LocalPathBuf,
    modified_path: LocalPathBuf,
    output_path: Option<LocalPathBuf>,
}

impl DiffConfigsCommand {
    async fn run(self) -> Result<()> {

        let base = read_config_file(&self.base_path).await?;
        let modified = read_config_file(&self.modified_path).await?;

        let mut merged_keys = vec![];
        for key in base.keys() {
            merged_keys.push(key);
        }
        for key in modified.keys() {
            merged_keys.push(key);
        }
        merged_keys.sort();
        merged_keys.dedup();

        let mut diff = String::new();

        for key in merged_keys {
            if key == LOCAL_VERSION_KEY {
                continue;
            }

            if let Some(base_value) = base.get(key) {
                if let Some(modified_value) = modified.get(key) {
                    if base_value == modified_value {
                        continue;
                    }

                    diff.push_str(&format!("-{}", serialize_config_line(key, base_value)));
                    diff.push_str(&format!("+{}", serialize_config_line(key, modified_value)));

                } else {
                    // Deleting a base key

                    diff.push_str(&format!("-{}", serialize_config_line(key, base_value)));    

                }
            } else {
                // Must be a newly added key/value

                let modified_value = modified.get(key).unwrap();
                diff.push_str(&format!("+{}", serialize_config_line(key, modified_value)));
            }
        }

        if let Some(output_path) = self.output_path {
            file::write(&output_path, diff.as_bytes()).await?;
        } else {
            println!("{}", diff);
        }



        Ok(())
    }

}

#[derive(Args)]
pub struct ApplyConfigDiffCommand { 
    config_path: LocalPathBuf,

    diff_path: LocalPathBuf,

    version: String,

    output_path: LocalPathBuf,
}

impl ApplyConfigDiffCommand {
    async fn run(self) -> Result<()> {
        let mut config = read_config_file(&self.config_path).await?;

        let diff = read_diff_file(&self.diff_path).await?;

        let mut remove_errors = vec![];

        for (key, action, value) in &diff {
            match action {
                DiffMode::Remove => {
                    let old_value = match config.remove(key) {
                        Some(v) => v,
                        None => {
                            remove_errors.push(format!("Old config did not contain key: {}", key));
                            continue;
                        }
                    };

                    if old_value != *value {
                        remove_errors.push(format!(
                            "Value mismatch for deleted key {}: '{:?}' (actual) vs '{:?}' (expected)",
                            key, old_value, value
                        ));
                    }
                }
                _ => {}
            }
        }

        if !remove_errors.is_empty() {
            eprintln!("ERRORS: {:#?}", remove_errors);
            return Err(err_msg("One or more errors while removing old config keys"));
        }

        for (key, action, value) in diff {
            match action {
                DiffMode::Add => {
                    if config.contains_key(&key) {
                        return Err(format_err!("Key already/still present in output: {}", key));
                    }

                    config.insert(key, value);
                }
                _ => {}
            }
        }

        config.insert(LOCAL_VERSION_KEY.to_string(), Some(format!("\"{}\"", self.version)));

        let mut out = String::new();

        for (key, value) in config {
            out.push_str(&serialize_config_line(&key, &value));
        }

        file::write(&self.output_path, out.as_bytes()).await?;

        Ok(())
    }
}

fn serialize_config_line(key: &str, value: &Option<String>) -> String {
    if let Some(value) = value {
        format!("{}={}\n", key, value)
    } else {
        format!("# {} is not set\n", key)
    }
}

async fn read_config_file(path: &LocalPath) -> Result<BTreeMap<String, Option<String>>> {

    let data = file::read_to_string(path).await?;

    let mut out = BTreeMap::new();

    for line in data.lines() {

        let (key, value) = match parse_config_line(line)? {
            Some(v) => v,
            None => continue
        };

        out.insert(key, value);
    }

    Ok(out)
}

fn parse_config_line(mut line: &str) -> Result<Option<(String, Option<String>)>> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }

    if let Some(rest) = line.strip_prefix("# ") {
        if let Some(rest) = rest.strip_suffix(" is not set") {
            if rest.contains("=") {
                return Err(format_err!("Invalid config line: {}", line));
            }

            return Ok(Some((rest.to_string(), None)));
        }
    }

    if line.starts_with("#") {
        return Ok(None);
    }

    let (key, value) = line.split_once("=").ok_or_else(|| format_err!("Line missing = separator: {}", line))?;
    Ok(Some((key.to_string(), Some(value.to_string()))))
}


#[derive(Clone, Copy, Debug, PartialEq)]
enum DiffMode {
    Add,
    Remove
}

async fn read_diff_file(path: &LocalPath) -> Result<Vec<(String, DiffMode, Option<String>)>> {
    let data = file::read_to_string(path).await?;

    let mut out = vec![];

    for line in data.lines() {
        let mut line = line.trim();
        if line.is_empty() {
            continue;
        }

        let mode = {
            if let Some(rest) = line.strip_prefix("+") {
                line = rest;
                DiffMode::Add
            } else if let Some(rest) = line.strip_prefix("-") {
                line = rest;
                DiffMode::Remove
            } else {
                return Err(format_err!("Invalid diff line: {}", line))
            }
        };

        let (key, value) = parse_config_line(line)?
            .ok_or_else(|| err_msg("Empty diff line"))?;

        out.push((key, mode, value));
    }

    Ok(out)

}


#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    args.cmd.run().await
}
