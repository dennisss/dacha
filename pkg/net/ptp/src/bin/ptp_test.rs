#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::sync::Arc;
use std::time::Duration;

use common::errors::*;

/*
cargo build --bin ptp_test
sudo target/debug/ptp_test

my ip: 169.254.104.190
camera ip: 169.254.155.139

cargo run --bin builder -- build //pkg/net/ptp:ptp_test --config=//pkg/builder/config:rpi64

scp built/pkg/net/ptp/ptp_test mocap@169.254.155.139:~/

ssh mocap@169.254.155.139


# Send to port 319:
    sudo ./ptp_test --iface=eth0 --local_addr=169.254.155.139:319 --send_to=169.254.104.190:319 --enable_filters
    => Works

# Send to other port:
    sudo ./ptp_test --iface=eth0 --local_addr=169.254.155.139:319 --send_to=169.254.104.190:8123 --enable_filters
    => Fails



ssh -i ~/.ssh/id_cluster cluster-user@10.1.1.14
./ptp --iface=eth0 --send_to=169.254.104.190:219


sudo sysctl -w net.ipv4.ip_unprivileged_port_start=0
cargo run --bin ptp -- --iface=enp5s0 --send_to=10.1.1.14:319



*/

#[derive(Args)]
struct Args {
    iface: String,
    local_addr: Option<String>,
    send_to: Option<String>,
 
    #[arg(default = false)]
    enable_filters: bool
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    if args.enable_filters {
        net::enable_hardware_timestamp_filters(&args.iface)?;
    }

    let ptp_dev = Arc::new(ptp::PTPDevice::open_default()?);


    let local_addr = args.local_addr.as_ref().map(|s| s.as_str()).unwrap_or("0.0.0.0:319");


    let sock = ptp::TimestampedUdpSocket::create(local_addr.parse()?, &args.iface).await?;

    if let Some(send_to) = args.send_to {
        println!("[Sending Data]");
        
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
        println!("[Receiving data]");

        let mut buf = [0u8; 64];

        loop {
            let (n, time, addr) = sock.recv_from(&mut buf).await?;
            
            // executor::sleep(Duration::from_millis(100)).await?;
            
            let time2 = sock.send_to(&[], &addr).await?;
            println!("Local time spent: {:?}", Duration::from_nanos(time2 - time));

            println!("Recv {} at time {} from {:?}", n, time, addr);
        }


    }

    Ok(())
}