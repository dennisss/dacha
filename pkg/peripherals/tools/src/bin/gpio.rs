// Utility for interfacing with GPIO pins (via the Linux GPIO API).

#[macro_use]
extern crate macros;

use common::errors::*;
use peripherals::gpio::*;

#[derive(Args)]
struct Args {
    command: Command,
}

#[derive(Args)]
enum Command {
    #[arg(name = "list")]
    List,

    #[arg(name = "set")]
    Set(SetCommand),
}

#[derive(Args)]
struct SetCommand {
    line_offset: u32,
    line_value: bool,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    match args.command {
        Command::List => {
            for chip in GPIOChip::list()? {
                let info = chip.info()?;
                println!("{:?}", info);

                for i in 0..info.lines {
                    println!("- Line: {}: {:?}", i, chip.line_info(i)?);

                    // EBUSY for 'USED' pins.
                    // let p = chip.pin(i)?;
                    // println!("  => level: {:?}", p.read()?);
                }
            }
        }
        Command::Set(cmd) => {
            let chip = GPIOChip::default_chip()?;

            let mut pin = chip.pin(cmd.line_offset)?;
            pin.configure(GPIOLineFlags::OUTPUT)?;
            pin.write(cmd.line_value)?;
        }
    }

    Ok(())
}
