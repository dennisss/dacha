



/*
JSON fieldS:
    _COMM
    MESSAGE
*/


use std::process::{Command, Stdio};

use base_error::*;

pub fn read_journald_pretty() -> Result<()> {
    let child = Command::new("journald")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().unwrap();

    for line in stdout.lines() {
        println!("{:?}", line);
    }

    Ok(())
}

fn main() -> Result<()> {

    read_journald_pretty()

}

