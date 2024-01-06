// CLI utility for working with radio devices.
//
// Functions include:
// - Setting up a local

/*

Action plan:
- Upload nordic_radio_serial to Pi
- Program to the board.
- Use 'cargo run --bin nordic_radio -- setup_device --usb=3:110 --name=uplift_desk --bridge_addr=127.0.0.1:8000'
- Pipe to it.


CLUSTER_ZONE=svl cargo run --bin nordic_radio_bridge -- --state_object_name=nordic_radio_bridge_config --rpc_port=8000 --usb_device_id=8888:0004

cargo build --package nordic --target thumbv7em-none-eabihf --release --no-default-features

// scp -i ~/.ssh/id_cluster target/thumbv7em-none-eabihf/release/nordic_radio_serial pi@10.1.0.90:~/binary

// openocd -f nrf52_pi.cfg -c init -c "reset init" -c halt -c "nrf5 mass_erase" -c "program /home/pi/binary verify" -c reset -c exit

cargo run --bin nordic_radio -- setup_device --usb=3:110 --name=uplift_desk --bridge_addr=127.0.0.1:8000

cargo run --bin nordic_radio -- pipe --device_name=uplift_desk --bridge_addr=127.0.0.1:8000


Desk has "\x91\x07\xad\xa9"

*/

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
extern crate container;
extern crate http;
extern crate nordic_proto;
extern crate nordic_tools;
extern crate protobuf;
extern crate rpc;
extern crate usb;

use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use nordic_proto::packet::PacketBuffer;
use nordic_proto::proto::net::*;
use protobuf::text::ParseTextProto;
use protobuf::Message;

use nordic_tools::proto::bridge::*;
use nordic_tools::usb_radio::USBRadio;

#[derive(Args)]
struct Args {
    // num: usize,
    command: Command,
}

#[derive(Args)]
enum Command {
    #[arg(name = "get_config")]
    GetConfig(GetConfigCommand),

    #[arg(name = "set_config")]
    SetConfig(SetConfigCommand),

    #[arg(name = "setup_device")]
    SetupDevice(SetupDeviceCommand),

    #[arg(name = "pipe")]
    Pipe(PipeCommand),

    #[arg(name = "send")]
    Send(SendCommand),
}

#[derive(Args)]
struct GetConfigCommand {
    usb: usb::DeviceSelector,
}

async fn run_get_config_command(cmd: GetConfigCommand) -> Result<()> {
    let mut radio = USBRadio::find(&cmd.usb).await?;
    println!("Config: {:?}", radio.get_network_config().await?);

    Ok(())
}

#[derive(Args)]
struct SetConfigCommand {
    usb: usb::DeviceSelector,
}

async fn run_set_config_command(cmd: SetConfigCommand) -> Result<()> {
    let mut radio = USBRadio::find(&cmd.usb).await?;

    let num = 1;
    let network_config = {
        if num == 1 {
            NetworkConfig::parse_text(
                r#"
                address: "\xE7\xE7\xE7\xE7"
                last_packet_counter: 1

                links {
                    address: "\xE8\xE8\xE8\xE8"
                    key: "\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10"
                    iv: "\x11\x12\x13\x14\x15"
                }
                "#,
            )?
        } else if num == 2 {
            NetworkConfig::parse_text(
                r#"
                address: "\xE8\xE8\xE8\xE8"
                last_packet_counter: 1

                links {
                    address: "\xE7\xE7\xE7\xE7"
                    key: "\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10"
                    iv: "\x11\x12\x13\x14\x15"
                }
                "#,
            )?
        } else {
            return Err(err_msg("Unknown device number"));
        }
    };

    radio.set_network_config(&network_config).await?;

    Ok(())
}

async fn create_bridge_stub(addr: &str) -> Result<RadioBridgeStub> {
    let resolver = cluster_client::ServiceResolver::create_with_fallback(addr, async move {
        Ok(Arc::new(
            cluster_client::meta::client::ClusterMetaClient::create_from_environment().await?,
        ))
    })
    .await?;

    let channel =
        Arc::new(rpc::Http2Channel::create(http::ClientOptions::from_resolver(resolver)).await?);

    Ok(RadioBridgeStub::new(channel))
}

#[derive(Args)]
struct SetupDeviceCommand {
    name: String,
    usb: usb::DeviceSelector,
    bridge_addr: String,
}

async fn run_setup_device_command(cmd: SetupDeviceCommand) -> Result<()> {
    let stub = create_bridge_stub(&cmd.bridge_addr).await?;

    let mut radio = USBRadio::find(&cmd.usb).await?;

    if let Some(config) = radio.get_network_config().await? {
        return Err(format_err!(
            "Device already configured with address: {:02x?}",
            config.address()
        ));
    }

    let request_context = rpc::ClientRequestContext::default();

    let mut req = RadioBridgeNewDeviceRequest::default();
    req.device_mut().set_name(cmd.name);

    let res = stub.NewDevice(&request_context, &req).await.result?;
    println!("Device Created: {:?}", res);

    radio.set_network_config(&res.network_config()).await?;

    Ok(())
}

#[derive(Args)]
struct SendCommand {
    // bridge_addr: String,
    // device_name: String,
    usb: usb::DeviceSelector,
    // to_address: String,
}

async fn run_send_command(cmd: SendCommand) -> Result<()> {
    let mut radio = USBRadio::find(&cmd.usb).await?;

    let mut packet = PacketBuffer::new();
    packet.set_counter(0);
    packet.resize_data(4);
    packet.data_mut().copy_from_slice(b"ABCD");
    packet
        .remote_address_mut()
        .copy_from_slice(b"\x96.\x16\x14");

    radio.send_packet(&packet).await?;

    Ok(())
}

#[derive(Args)]
struct PipeCommand {
    bridge_addr: String,
    device_name: String,
    // usb: String,
    // to_address: String,
}

async fn run_pipe_command(cmd: PipeCommand) -> Result<()> {
    async fn transmit_thread(stub: RadioBridgeStub, device_name: String) -> Result<()> {
        loop {
            let mut line = String::new();
            common::async_std::io::stdin().read_line(&mut line).await?;

            let mut req = RadioBridgePacket::default();
            req.set_device_name(&device_name);
            req.data_mut().extend_from_slice(line.as_bytes());

            print!("> {}", line);

            stub.Send(&rpc::ClientRequestContext::default(), &req)
                .await
                .result?;
        }
    }

    async fn recieve_thread(stub: RadioBridgeStub, device_name: String) -> Result<()> {
        let mut req = RadioReceiveRequest::default();
        req.set_device_name(device_name);

        let mut res = stub
            .Receive(&rpc::ClientRequestContext::default(), &req)
            .await;

        while let Some(packet) = res.recv().await {
            println!("< {:?}", common::bytes::Bytes::from(packet.data()));
        }

        // TODO: Need to support graceful shutdown of servers that can receive streaming
        // requests.
        Err(err_msg("Receive request stopped early"))
    }

    // let mut radio = USBRadio::find(&cmd.usb).await?;

    let stub = create_bridge_stub(&cmd.bridge_addr).await?;
    let request_context = rpc::ClientRequestContext::default();

    let mut bundle = executor::bundle::TaskResultBundle::new();

    bundle.add(
        "Transmitter",
        transmit_thread(stub.clone(), cmd.device_name.clone()),
    );

    bundle.add(
        "Receiver",
        recieve_thread(stub.clone(), cmd.device_name.clone()),
    );

    bundle.join().await?;

    Ok(())

    /*
    let (sender, receiver) = channel::bounded(1);

    let reader_task = executor::spawn(async move {
        println!("{:?}", line_reader(sender).await);
    });

    let to_address = base_radix::hex_decode(&cmd.to_address)?;

    loop {
        // TODO: If we get a packet, continue reading up to some number of frames until
        // the device's buffer is empty.

        {
            let mut packet_buffer = PacketBuffer::new();

            let start_time = std::time::Instant::now();

            let maybe_packet = radio.recv_packet().await?;

            let end_time = std::time::Instant::now();

            println!("{:?}", end_time.duration_since(start_time));

            if let Some(packet) = maybe_packet {
                println!("From: {:02x?}", packet.remote_address());
                println!("{:?}", common::bytes::Bytes::from(packet.data()));
            }
        }

        if let Ok(v) = receiver.try_recv() {
            println!("> {}", v);

            let mut packet_buffer = PacketBuffer::new();
            packet_buffer
                .remote_address_mut()
                .copy_from_slice(&to_address);
            // .copy_from_slice(network_config.links()[0].address());

            // last_counter += 1;
            // packet_buffer.set_counter(last_counter);

            packet_buffer.set_counter(0);

            packet_buffer.resize_data(v.len());
            packet_buffer.data_mut().copy_from_slice(v.as_bytes());

            radio.send_packet(&packet_buffer).await?;

            println!("<");
        }

        executor::sleep(Duration::from_millis(1000)).await;
    }
    */
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    match args.command {
        Command::GetConfig(cmd) => run_get_config_command(cmd).await,
        Command::SetConfig(cmd) => run_set_config_command(cmd).await,
        Command::SetupDevice(cmd) => run_setup_device_command(cmd).await,
        Command::Send(cmd) => run_send_command(cmd).await,
        Command::Pipe(cmd) => run_pipe_command(cmd).await,
    }
}
