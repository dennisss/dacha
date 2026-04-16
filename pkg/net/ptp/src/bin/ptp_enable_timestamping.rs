#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::time::Duration;

use common::errors::*;

/*
cargo build --bin ptp_enable_timestamping
sudo target/debug/ptp_enable_timestamping enp5s0


ssh -i ~/.ssh/id_cluster cluster-user@10.1.1.3

cargo run --bin builder -- build //pkg/net/ptp:ptp_enable_timestamping --config=//pkg/builder/config:rpi64
scp -i ~/.ssh/id_cluster built/pkg/net/ptp/ptp_enable_timestamping cluster-user@10.1.1.3:~/

sudo ./ptp_enable_timestamping eth0


*/

#[derive(Args)]
struct Args {
    #[arg(positional)]
    iface: String
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    net::enable_hardware_timestamp_filters(&args.iface)?;
    Ok(())
}