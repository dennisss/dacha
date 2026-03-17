/*


cargo run --bin builder -- build //pkg/rpi/pio_rp1:pio_rp1 --config=//pkg/builder/config:rpi64
*/


#[macro_use]
extern crate macros;

use std::time::Duration;

use base_error::*;

#[executor_main]
async fn main() -> Result<()> {
    println!("Hi!");

    let inst = pio_rp1::PIO::create()?;


    println!("Working..");

    loop {
        executor::sleep(Duration::from_secs(1)).await?;

    }

    drop(inst);

    Ok(())
}