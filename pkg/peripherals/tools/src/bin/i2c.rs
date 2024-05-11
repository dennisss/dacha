//!

/*
cargo build --target aarch64-unknown-linux-gnu --release --bin i2c

scp -i ~/.ssh/id_cluster target/aarch64-unknown-linux-gnu/release/i2c cluster-user@10.1.1.1:~

ssh -i ~/.ssh/id_cluster cluster-user@10.1.0.112


let bus = peripherals::i2c::I2CDevice::open(&args.bus)?;


./i2c scan --bus=/dev/i2c-1
=> For the Pi Rack R5 one

*/

#[macro_use]
extern crate macros;

use common::errors::*;
use peripherals::i2c::I2CHostController;
use peripherals_devices::ds3231::*;
use peripherals_devices::sgp30::SGP30;
use peripherals_devices::trust_m::TrustM;

#[derive(Args)]
struct Args {
    /// Path to the I2C bus (e.g. "/dev/i2c-1")
    bus: String,

    #[arg(positional)]
    command: Command,
}

#[derive(Args)]
enum Command {
    // Attempts to find all devices attached to an I2C bus by polling all possible 7-bit
    // addresses.
    //
    // TODO: Don't search the last few 7-bit numbers which correspond to 10-bit addresses.
    #[arg(name = "scan")]
    Scan,

    #[arg(name = "trust_m")]
    TrustM,

    #[arg(name = "sgp30")]
    SGP30,

    #[arg(name = "ds3231")]
    DS3231,
}

async fn run_scan(mut bus: I2CHostController) -> Result<()> {
    for i in 0..8 {
        let mut line = format!("{}_:", i);

        for j in 0..16 {
            let addr = (i << 4) | j;
            let valid = bus.test(addr).await.is_ok();

            line = format!(
                "{} {}",
                line,
                if valid {
                    format!("{:02x}", addr)
                } else {
                    "--".into()
                }
            );
        }

        println!("{}", line)
    }

    Ok(())
}

async fn run_trust_m(bus: I2CHostController) -> Result<()> {
    let mut dev = TrustM::open(&bus).await?;

    dev.read_coprocessor_uid().await?;

    // dev.get_random()?;

    Ok(())
}

async fn run_sgp30(bus: I2CHostController) -> Result<()> {
    let mut dev = SGP30::open(&bus);

    let serial = dev.get_serial().await?;
    println!("SERIAL {:?}", serial);

    dev.init_air_quality().await?;

    let mut i = 0;
    loop {
        let quality = dev.measure_air_quality().await?;
        println!("{:?}", quality);

        if i % 10 == 0 {
            let baseline = dev.get_baseline().await?;
            println!("Baseline: {:?}", baseline);
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
        i += 1;
    }

    Ok(())
}

async fn run_ds3231(bus: I2CHostController) -> Result<()> {
    let mut clock = DS3231::open(&bus);

    println!("Temp: {}", clock.read_temperature().await?);
    clock
        .write_time(&DS3231Time::from_atomic_seconds(0))
        .await?;
    for i in 0..100 {
        let time = clock.read_time().await?;
        println!("Time: {}", time.to_atomic_seconds());

        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let bus = peripherals::i2c::I2CHostController::open(&args.bus)?;

    match args.command {
        Command::Scan => run_scan(bus).await?,
        Command::TrustM => run_trust_m(bus).await?,
        Command::SGP30 => run_sgp30(bus).await?,
        Command::DS3231 => run_ds3231(bus).await?,
    }

    Ok(())
}
