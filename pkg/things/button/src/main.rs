#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use base_args::define_arg_command;
use common::errors::*;
use nordic_tools_proto::nordic::*;
use nordic_proto::nordic::*;
use cluster_client::service::create_rpc_channel;
use cluster_client::ClusterMetaClient;
use file::LocalPathBuf;
use protobuf::{StaticMessage, Message};

/*
cargo run --bin hue -- create_user \
    --application_name=dacha --device_name=button

cargo run --bin cluster_cli -- set_object button_hue_key --value=[user_name]

cargo run --bin button -- \
    light-button \
    --bridge_addr=localhost:8002 \
    --button_device_name=btn2 \
    --hue_key_object=button_hue_key \
    --hue_group_name=Bedroom

cargo run --bin button -- \
    hdc2080-logger \
    --bridge_addr=localhost:8002 \
    --device_name=env1 \
    --output_path=hdc_log.csv

*/

#[derive(Args)]
struct Args {
    mode: Mode
}

define_arg_command!(Mode {
    LightButtonMode = "light-button",
    HDC2080Logger = "hdc2080-logger"
});


#[derive(Args)]
struct LightButtonMode {
    bridge_addr: String,
    button_device_name: String,
    hue_key_object: String,
    hue_group_name: String,
}

impl LightButtonMode {
    async fn run(self) -> Result<()> {

        let meta_client = ClusterMetaClient::create_from_environment().await?;
        let channel = create_rpc_channel(&self.bridge_addr, meta_client.clone()).await?;
        let stub = RadioBridgeStub::new(channel);

        let hue_key = meta_client
            .get_object_data(&self.hue_key_object)
            .await?
            .ok_or_else(|| err_msg("No config found in cluster"))?;

        let hue_client = {
            let client = hue::AnonymousHueClient::create().await?;
            hue::HueClient::create(client, std::str::from_utf8(&hue_key)?)
        };

        let groups = hue_client.get_groups().await?;

        let mut selected_group = None;
        for (group_id, group) in groups {
            if group.name == self.hue_group_name {
                selected_group = Some((group_id, group.all_on));
                break;
            }
        }

        let (group_id, initial_state) = selected_group
            .ok_or_else(|| err_msg("Failed to find requested group"))?;

        println!("Hue Group id: {}", group_id);
        println!("Initial State: {}", initial_state);

        let mut state = initial_state;


        // TOOD: Make sure that we validate that the device_name exists.
        let mut request = RadioReceiveRequest::default();
        request.set_device_name(&self.button_device_name);
        let request_context = rpc::ClientRequestContext::default();

        let mut res = stub.Receive(&request_context, &request).await;

        while let Some(packet_proto) = res.recv().await {
            let packet = SensorPacket::parse(packet_proto.data())?;
            
            /*
            {
                let mut ack_packet = SensorPacket::default();
                // TOOD: Use the original counter.
                ack_packet.set_ack(1u32);

                let request_context = rpc::ClientRequestContext::default();
                let mut request = RadioBridgePacket::default();
                request.set_device_name(&self.button_device_name);
                request.set_data(ack_packet.serialize()?);
                stub
                    .Send(&request_context, &request)
                    .await
                    .result?; 
            }
            */

            println!("{:?}", packet);

            if packet.edge_trigger().triggered() {
                state = !state;

                hue_client.set_group_on(&group_id, state).await?;
            }


        }

        println!("Receiver stopped with: {:?}", res.finish().await);

        Ok(())

    }

}

#[derive(Args)]
struct HDC2080Logger {
    bridge_addr: String,
    device_name: String,
    output_path: LocalPathBuf,
}

impl HDC2080Logger {
    async fn run(self) -> Result<()> {
        let meta_client = ClusterMetaClient::create_from_environment().await?;
        let channel = create_rpc_channel(&self.bridge_addr, meta_client.clone()).await?;
        let stub = RadioBridgeStub::new(channel);

        if !file::exists(&self.output_path).await? {
            file::write(&self.output_path, b"time,temp,humid\n").await?;
        }

        let mut request = RadioReceiveRequest::default();
        request.set_device_name(&self.device_name);
        let request_context = rpc::ClientRequestContext::default();

        let mut res = stub.Receive(&request_context, &request).await;

        while let Some(packet_proto) = res.recv().await {
            let packet = SensorPacket::parse(packet_proto.data())?;

            if packet.has_hdc2080() {

                let data = packet.hdc2080();

                let temp_celsius = (data.temperature_raw() as f32 / 65536.0) * 165.0 - 40.5;
                
                // Humidity (%RH) = (HUMIDITY[15:0] / 2^16) * 100
                let humidity_rh = (data.humidity_raw() as f32 / 65536.0) * 100.0;

                println!("Temp: {}     Humid: {}", temp_celsius, humidity_rh);


                file::append(&self.output_path, format!("{},{},{}\n",
                    Self::now(),
                    temp_celsius,
                    humidity_rh
                    ).as_bytes()
                ).await?;
            }

        }

        println!("Receiver stopped with: {:?}", res.finish().await);

        Ok(())
    }

    fn now() -> u64 {
        let now = SystemTime::now();
        now.duration_since(UNIX_EPOCH).unwrap().as_secs_f64().round() as u64
    }

}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    args.mode.run().await
}
