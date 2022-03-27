extern crate cmsis_svd;
#[macro_use]
extern crate common;
extern crate automata;

use std::env;
use std::io::Write;
use std::path::PathBuf;

use automata::regexp::vm::instance::RegExp;
use cmsis_svd::compiler::*;
use common::errors::*;

fn main() -> Result<()> {
    let input =
        std::fs::read_to_string(common::project_dir().join("third_party/cmsis_svd/nrf52840.svd"))?;

    let mut options = CompilerOptions::default();

    options.field_rewrites.push(FieldRewriteRule {
        register_name: RegExp::new("^EVENTS_.*$")?,
        register_access: cmsis_svd::spec::RegisterAccess::ReadWrite,
        field_name: RegExp::new(".*")?,
        new_name: "EventState".to_string(),
    });

    options.register_rewrites.push(RegisterRewriteRule {
        register_name: RegExp::new("^EVENTS_.*")?,
        new_name: "EventRegister".to_string(),
    });

    options.field_rewrites.push(FieldRewriteRule {
        register_name: RegExp::new("^(OUT|IN)$")?,
        register_access: cmsis_svd::spec::RegisterAccess::ReadWrite,
        field_name: RegExp::new(".*")?,
        new_name: "PinLevel".to_string(),
    });

    options.field_rewrites.push(FieldRewriteRule {
        register_name: RegExp::new("^(OUTSET|OUTCLR)$")?,
        register_access: cmsis_svd::spec::RegisterAccess::ReadOnly,
        field_name: RegExp::new(".*")?,
        new_name: "PinLevel".to_string(),
    });

    options.field_rewrites.push(FieldRewriteRule {
        register_name: RegExp::new("^TASKS_.*")?,
        register_access: cmsis_svd::spec::RegisterAccess::WriteOnly,
        field_name: RegExp::new(".*")?,
        new_name: "TaskTrigger".to_string(),
    });

    options.register_rewrites.push(RegisterRewriteRule {
        register_name: RegExp::new("^TASKS_.*")?,
        new_name: "TaskRegister".to_string(),
    });

    options.field_rewrites.push(FieldRewriteRule {
        register_name: RegExp::new("^DIR$")?,
        register_access: cmsis_svd::spec::RegisterAccess::ReadWrite,
        field_name: RegExp::new(".*")?,
        new_name: "PinDirection".to_string(),
    });

    // TODO: Validate that multiple rules don't override the same fields.
    options.field_rewrites.push(FieldRewriteRule {
        register_name: RegExp::new("^(INTENSET|INTENCLR)$")?,
        register_access: cmsis_svd::spec::RegisterAccess::ReadOnly,
        field_name: RegExp::new(".*")?,
        new_name: "InterruptState".to_string(),
    });

    options.field_rewrites.push(FieldRewriteRule {
        register_name: RegExp::new("^INTEN$")?,
        register_access: cmsis_svd::spec::RegisterAccess::ReadWrite,
        field_name: RegExp::new(".*")?,
        new_name: "InterruptState".to_string(),
    });

    options.field_rewrites.push(FieldRewriteRule {
        register_name: RegExp::new("^INTENSET$")?,
        register_access: cmsis_svd::spec::RegisterAccess::WriteOnly,
        field_name: RegExp::new(".*")?,
        new_name: "InterruptSet".to_string(),
    });

    options.field_rewrites.push(FieldRewriteRule {
        register_name: RegExp::new("^INTENCLR$")?,
        register_access: cmsis_svd::spec::RegisterAccess::WriteOnly,
        field_name: RegExp::new(".*")?,
        new_name: "InterruptClear".to_string(),
    });

    options.register_rewrites.push(RegisterRewriteRule {
        register_name: RegExp::new("^PSEL\\..*")?,
        new_name: "PinSelectRegister".to_string(),
    });

    /*
    TODO: INTENSET|INTENCLR reading should re-use the same value struct as the corresponding INTEN

    TODO: Also for OUT and IN registers the "PIN[0-9]+" fields
    ^ We should support bit vector where a regexp can extract the
    */

    let compiled = Compiler::compile(&input, &options)?;

    /// Compile here
    let output_dir = PathBuf::from(env::var("OUT_DIR")?);
    let output_path = output_dir.join("nrf52840.rs");
    std::fs::write(&output_path, compiled);

    {
        let res = std::process::Command::new("rustfmt")
            .arg(output_path.to_str().unwrap())
            .output()?;
        if !res.status.success() {
            std::io::stdout().write_all(&res.stdout).unwrap();
            std::io::stderr().write_all(&res.stderr).unwrap();
            return Err(err_msg("rustfmt failed"));
        }
    }

    // let output_path2 = project_path!("pkg/peripherals_raw/src/nrf52840.rs");
    // std::fs::write(&output_path2, std::fs::read(&output_path)?)?;

    Ok(())
}
