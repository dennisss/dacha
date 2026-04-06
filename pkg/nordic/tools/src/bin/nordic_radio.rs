// CLI utility for working with radio devices.

#![feature(let_chains)]

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
extern crate http;
extern crate nordic_proto;
extern crate nordic_tools;
extern crate protobuf;
extern crate rpc;
extern crate usb;

use std::sync::Arc;
use std::time::Duration;
use std::collections::HashMap;

use base_args::define_arg_command;
use common::errors::*;
use nordic_proto::nordic::*;
use nordic_wire::packet::PacketBuffer;
use protobuf::text::ParseTextProto;
use protobuf::Message;
use cluster_client::service::create_rpc_channel;
use cluster_client::ClusterMetaClient;

use nordic_driver::usb_radio::USBRadio;
use nordic_tools::sensor_config::*;
use nordic_tools_proto::nordic::*;

#[derive(Args)]
struct Args {
    // num: usize,
    command: Command,
}



define_arg_command!(Command {
    ListDevicesCommand = "list_devices",

    SetupSensorCommand = "setup_sensor",

    RemoveDeviceCommand = "remove_device",

    GetConfigCommand = "get_config",

    GetSensorConfigCommand = "get_sensor_config",

    /// For testing only, set the config on a USB connected device to a hardcoded value.
    SetConfigCommand = "set_config",

    SetupDeviceCommand = "setup_device",

    PipeCommand = "pipe",

    SendCommand = "send",

    ReadLogCommand = "read_log",
});

#[derive(Args)]
struct ReadLogCommand {
    usb: usb::DeviceSelector,
}

impl ReadLogCommand {
    async fn run(self) -> Result<()> {
        let mut radio = USBRadio::find(&self.usb).await?;
        println!("Log: {:?}", radio.read_log_entries().await?);

        Ok(())
    }
}


#[derive(Args)]
struct GetConfigCommand {
    usb: usb::DeviceSelector,
}

impl GetConfigCommand {
    async fn run(self) -> Result<()> {
        let mut radio = USBRadio::find(&self.usb).await?;
        println!("Config: {:?}", radio.get_network_config().await?);

        Ok(())
    }
}

#[derive(Args)]
struct GetSensorConfigCommand {
    usb: usb::DeviceSelector,
}

impl GetSensorConfigCommand {
    async fn run(self) -> Result<()> {
        let mut radio = USBRadio::find(&self.usb).await?;
        println!("Config: {:?}", radio.get_sensor_config().await?);

        Ok(())
    }
}

#[derive(Args)]
struct SetConfigCommand {
    usb: usb::DeviceSelector,
}

impl SetConfigCommand {
    async fn run(self) -> Result<()> {
        let mut radio = USBRadio::find(&self.usb).await?;

        let network_config = {
            NetworkConfig::parse_text(
                r#"
                "#,
            )?
        };

        radio.set_network_config(&network_config).await?;

        Ok(())
    }
}

// TODO: Deprecate this.
async fn create_bridge_stub(addr: &str) -> Result<RadioBridgeStub> {
    let resolver = cluster_client::ServiceResolver::create_with_fallback(addr, async move {
        Ok(cluster_client::ClusterMetaClient::create_from_environment().await?)
    })
    .await?;

    let channel =
        Arc::new(rpc::Http2Channel::create(http::ClientOptions::from_resolver(resolver)).await?);

    Ok(RadioBridgeStub::new(channel))
}

#[derive(Args)]
struct ListDevicesCommand {
    bridge_addr: String,
}

impl ListDevicesCommand {
    async fn run(self) -> Result<()> {
        let meta_client = ClusterMetaClient::create_from_environment().await?;
        let channel = create_rpc_channel(&self.bridge_addr, meta_client.clone()).await?;
        let stub = RadioBridgeStub::new(channel);

        let request_context = rpc::ClientRequestContext::default();

        let mut req = protobuf_builtins::google::protobuf::Empty::default();
        let res = stub.ListDevices(&request_context, &req).await.result?;

        println!("Devices: {:?}", res);

        Ok(())
    }
}


#[derive(Args)]
struct RemoveDeviceCommand {
    bridge_addr: String,
    device_name: String,
}

impl RemoveDeviceCommand {
    async fn run(self) -> Result<()> {
        let meta_client = ClusterMetaClient::create_from_environment().await?;
        let channel = create_rpc_channel(&self.bridge_addr, meta_client.clone()).await?;
        let stub = RadioBridgeStub::new(channel);

        let request_context = rpc::ClientRequestContext::default();

        let mut req = RadioBridgeRemoveDeviceRequest::default();
        req.set_device_name(&self.device_name);

        let res = stub.RemoveDevice(&request_context, &req).await.result?;

        Ok(())
    }
}




// TODO: Delete "btn1"
#[derive(Args)]
struct SetupSensorCommand {
    name: String,
    config_name: String,
    bridge_addr: String,

    #[arg(default = false)]
    overwrite: bool,
}

impl SetupSensorCommand {
    async fn run(self) -> Result<()> {

        let sensor_config_registry = SensorConfigRegistry::defaults().await?;

        let sensor_config = sensor_config_registry.get(&self.config_name)
            .ok_or_else(|| format_err!("Unknown sensor config: {}", self.config_name))?;

        let meta_client = ClusterMetaClient::create_from_environment().await?;
        
        let channel = create_rpc_channel(&self.bridge_addr, meta_client.clone()).await?;
        let stub = RadioBridgeStub::new(channel);
        let request_context = rpc::ClientRequestContext::default();
        

        let existing_devices = {
            let mut req = protobuf_builtins::google::protobuf::Empty::default();
            let res = stub.ListDevices(&request_context, &req).await.result?;

            let mut out = HashMap::<Vec<u8>, String>::default();

            out.insert(res.bridge_address().to_vec(), "<hub>".to_string());

            for dev in res.devices() {
                out.insert(dev.address().to_vec(), dev.name().to_string());
            }

            out
        };

        let mut radio = {
            let mut selector = usb::DeviceSelector::default();
            selector.vendor_id = Some(0x8888);
            selector.product_id = Some(6); // TODO: OUR_SENSOR_ID
            
            nordic_driver::usb_radio::USBRadio::find(&selector).await?
        };

        println!("Configuring network...");

        if let Some(config) = radio.get_network_config().await? && !self.overwrite {
            if let Some(existing_name) = existing_devices.get(&config.address().to_vec()) {

                if *existing_name != self.name {
                    return Err(format_err!("Device already registered under a different name: {}", existing_name));
                }

                println!("=> Already configured");

            } else {
                return Err(err_msg("Device's networking is already configured but is not registered with the bridge"));
            }
        } else {
            for existing_name in existing_devices.values() {
                if *existing_name == self.name {
                    return Err(format_err!("Another device was already configured with name: {}", self.name));
                }
            }

            let mut req = RadioBridgeNewDeviceRequest::default();
            req.device_mut().set_name(self.name);

            let res = stub.NewDevice(&request_context, &req).await.result?;
            println!("=> Device Created");

            radio.set_network_config(&res.network_config()).await?;

            println!("=> Configured!");
        }

        println!("Configuring sensor...");

        radio.set_sensor_config(&sensor_config).await?;

        println!("=> Done!");

        Ok(())
    }
}


#[derive(Args)]
struct SetupDeviceCommand {
    name: String,
    usb: usb::DeviceSelector,
    bridge_addr: String,
}

impl SetupDeviceCommand {
    async fn run(self) -> Result<()> {

        let meta_client = ClusterMetaClient::create_from_environment().await?;
        let channel = create_rpc_channel(&self.bridge_addr, meta_client.clone()).await?;
        let stub = RadioBridgeStub::new(channel);
    
        let mut radio = USBRadio::find(&self.usb).await?;

        if let Some(config) = radio.get_network_config().await? {
            return Err(format_err!(
                "Device already configured with address: {:02x?}",
                config.address()
            ));
        }

        let request_context = rpc::ClientRequestContext::default();

        let mut req = RadioBridgeNewDeviceRequest::default();
        req.device_mut().set_name(self.name);

        let res = stub.NewDevice(&request_context, &req).await.result?;
        println!("Device Created: {:?}", res);

        radio.set_network_config(&res.network_config()).await?;

        Ok(())
    }

}

#[derive(Args)]
struct SendCommand {
    // bridge_addr: String,
    // device_name: String,
    usb: usb::DeviceSelector,
    // to_address: String,
}

impl SendCommand {
    async fn run(self) -> Result<()> {
        let mut radio = USBRadio::find(&self.usb).await?;

        let mut packet = PacketBuffer::new();
        packet.set_counter(0);
        packet.resize_data(4);
        packet.data_mut().copy_from_slice(b"ABCD");
        packet
            .remote_address_mut()
            .copy_from_slice(b"\x96.\x16\x14");

        // TODO:
        // radio.send_packet(&packet).await?;

        Ok(())
    } 

}


#[derive(Args)]
struct PipeCommand {
    bridge_addr: String,
    device_name: String,
    // usb: String,
    // to_address: String,
}

impl PipeCommand {
    async fn run(self) -> Result<()> {
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

        // let mut radio = USBRadio::find(&self.usb).await?;

        let stub = create_bridge_stub(&self.bridge_addr).await?;
        let request_context = rpc::ClientRequestContext::default();

        let mut bundle = executor::bundle::TaskResultBundle::new();

        bundle.add(
            "Transmitter",
            transmit_thread(stub.clone(), self.device_name.clone()),
        );

        bundle.add(
            "Receiver",
            recieve_thread(stub.clone(), self.device_name.clone()),
        );

        bundle.join().await?;

        Ok(())

        /*
        let (sender, receiver) = channel::bounded(1);

        let reader_task = executor::spawn(async move {
            println!("{:?}", line_reader(sender).await);
        });

        let to_address = base_radix::hex_decode(&cmselfd.to_address)?;

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
}


#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    args.command.run().await
}
