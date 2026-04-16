#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::time::Duration;

use common::errors::*;

/*
cargo build --bin ptp
sudo target/debug/ptp



cargo run --bin builder -- build //pkg/net/ptp:ptp --config=//pkg/builder/config:rpi64
scp -i ~/.ssh/id_cluster built/pkg/net/ptp/ptp cluster-user@10.1.1.3:~/

ssh -i ~/.ssh/id_cluster cluster-user@10.1.1.3
./ptp --iface=eth0


cargo run --bin ptp -- --iface=enp5s0 --send_to=10.1.1.3:9000

*/

#[derive(Args)]
struct Args {
    iface: String,
    send_to: Option<String>
}

#[executor_main]
async fn main() -> Result<()> {

    let args = common::args::parse_args::<Args>()?;

    // TODO: Need to map this to the interface name
    let ptp_dev = ptp::PTPDevice::open_default()?;

    /*

    println!("{:?}", ptp_dev.clock().get_adjustments());

    return Ok(());
    */


    println!("Realtime: {:?}", sys::ClockId::REALTIME.get_time()?);

    println!("PTP Time: {:?}", ptp_dev.get_time()?);



    // net::enable_hardware_timestamp_filters(&args.iface)?;

    let sock = ptp::TimestampedUdpSocket::create("0.0.0.0:9000".parse()?, &args.iface).await?;

    if let Some(send_to) = args.send_to {
        loop {
            let mut data = vec![0u8; 8];
            let time = sock.send_to(&data, &send_to.parse()?).await?;
            println!("Send time: {}", time);

            // TODO: Should be receiving in parallel to avoid missing a packet?
            let (n2, time2, addr2) = sock.recv_from(&mut data).await?;
            println!("RTT: {:?}", Duration::from_nanos(time2 - time));

            executor::sleep(Duration::from_secs(1)).await?;
        }
    } else {
        let mut buf = [0u8; 64];

        loop {
            let (n, time, addr) = sock.recv_from(&mut buf).await?;
            let time2 = sock.send_to(&[], &addr).await?;
            println!("Local time spent: {:?}", Duration::from_nanos(time2 - time));

            println!("Recv {} at time {} from {:?}", n, time, addr);
        }


    }

    Ok(())
}