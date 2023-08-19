extern crate common;
extern crate linker;

use std::fs;

use common::errors::*;

fn main() -> Result<()> {
    {
        let config = linker::get_chip_config("nrf52840", true)?;
        let script = linker::generate_linker_script(&config)?;
        fs::write("pkg/nordic/link_bootloader.x", script)?;
    }

    {
        let config = linker::get_chip_config("nrf52840", false)?;
        let script = linker::generate_linker_script(&config)?;
        fs::write("pkg/nordic/link.x", script)?;
    }

    {
        let config = linker::get_chip_config("nrf52833", true)?;
        let script = linker::generate_linker_script(&config)?;
        fs::write("pkg/nordic/link_bootloader_33.x", script)?;
    }

    {
        let config = linker::get_chip_config("nrf52833", false)?;
        let script = linker::generate_linker_script(&config)?;
        fs::write("pkg/nordic/link_33.x", script)?;
    }

    Ok(())
}
