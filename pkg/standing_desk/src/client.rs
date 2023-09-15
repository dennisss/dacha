use std::collections::HashMap;
use std::sync::Arc;

use common::errors::*;
use executor::channel;
use executor::child_task::ChildTask;
use executor::sync::Mutex;
use nordic_tools::proto::bridge::*;

use crate::packet::*;

pub struct Client {
    shared: Arc<ClientShared>,
    receiver_task: ChildTask,
}

struct ClientShared {
    stub: RadioBridgeStub,
    device_name: String,
    state: Mutex<ClientState>,
}

struct ClientState {
    last_subscriber_id: usize,
    subscribers: HashMap<usize, channel::Sender<Response>>,
}

impl Client {
    pub async fn create(radio_bridge_addr: &str, device_name: &str) -> Result<Self> {
        let stub = {
            let resolver =
                container::ServiceResolver::create_with_fallback(radio_bridge_addr, async move {
                    Ok(Arc::new(
                        container::meta::client::ClusterMetaClient::create_from_environment()
                            .await?,
                    ))
                })
                .await?;

            let channel = Arc::new(
                rpc::Http2Channel::create(http::ClientOptions::from_resolver(resolver)).await?,
            );

            RadioBridgeStub::new(channel)
        };

        let shared = Arc::new(ClientShared {
            stub,
            device_name: device_name.to_string(),
            state: Mutex::new(ClientState {
                last_subscriber_id: 0,
                subscribers: HashMap::new(),
            }),
        });

        let receiver_task = ChildTask::spawn(Self::receiver_thread(shared.clone()));

        Ok(Self {
            shared,
            receiver_task,
        })
    }

    async fn receiver_thread(shared: Arc<ClientShared>) {
        let mut request = RadioReceiveRequest::default();
        request.set_device_name(&shared.device_name);

        let request_context = rpc::ClientRequestContext::default();

        // TODO: Restart this with backoff (though some errors like NotFound should be
        // permanent?)
        let mut res = shared.stub.Receive(&request_context, &request).await;

        while let Some(radio_packet) = res.recv().await {
            let packets = Packet::parse_stream(radio_packet.data());
            if packets.len() == 0 {
                println!("Received radio packet with no valid desk packets");
            }

            let mut state = shared.state.lock().await;

            for packet in packets {
                let res = Response::parse(packet);

                for (_, sender) in state.subscribers.iter() {
                    let _ = sender.try_send(res.clone());
                }
            }
        }

        println!("Receiver stopped with: {:?}", res.finish().await);
    }

    pub async fn subscribe(&self) -> ResponseSubscriber {
        let mut state = self.shared.state.lock().await;
        let id = state.last_subscriber_id + 1;
        state.last_subscriber_id = id;

        let (sender, receiver) = channel::unbounded();
        state.subscribers.insert(id, sender);

        ResponseSubscriber {
            id,
            receiver,
            shared: self.shared.clone(),
        }
    }

    async fn send_packet(&self, packet: Packet) -> Result<()> {
        let request_context = rpc::ClientRequestContext::default();
        let mut request = RadioBridgePacket::default();
        request.set_device_name(&self.shared.device_name);
        request.set_data(packet.serialize());
        self.shared
            .stub
            .Send(&request_context, &request)
            .await
            .result?;
        Ok(())
    }

    pub async fn query_state(&self) -> Result<()> {
        self.send_packet(Packet {
            target_address: Address::DeskControlBoard as u8,
            command: RequestCommand::QueryState as u8,
            payload: vec![],
        })
        .await
    }

    pub async fn press_key(&self, num: usize) -> Result<()> {
        self.send_packet(Packet {
            target_address: Address::DeskControlBoard as u8,
            command: match num {
                1 => RequestCommand::PressKey1,
                2 => RequestCommand::PressKey2,
                // 3 => RequestCommand::PressKey3,
                // 4 => RequestCommand::PressKey4,
                _ => {
                    return Err(err_msg("Unknown key index"));
                }
            } as u8,
            payload: vec![],
        })
        .await
    }

    // pub async fn
}

pub struct ResponseSubscriber {
    id: usize,
    receiver: channel::Receiver<Response>,
    shared: Arc<ClientShared>,
}

impl Drop for ResponseSubscriber {
    fn drop(&mut self) {
        let id = self.id;
        let shared = self.shared.clone();

        executor::spawn(async move {
            shared.state.lock().await.subscribers.remove(&id);
        });
    }
}

impl ResponseSubscriber {
    pub async fn recv(&self) -> Result<Response> {
        let res = self.receiver.recv().await?;
        Ok(res)
    }
}

#[derive(Clone, Debug)]
pub enum Response {
    /// The current height in inches.
    CurrentHeight {
        height_in: f32,

        /// Whether or not a valid height has been configured
        keys_set: [bool; 4],
    },

    // The configured position of each key/macro in motor 'step' units.
    // Inches = (Steps / 450) + 14
    Key1Position {
        steps: u16,
    },
    Key2Position {
        steps: u16,
    },
    Key3Position {
        steps: u16,
    },
    Key4Position {
        steps: u16,
    },

    Unknown(Packet),
}

impl Response {
    pub fn parse(packet: Packet) -> Self {
        if packet.target_address == (Address::ExternalDongle as u8)
            && packet.command == (ResponseCommand::CurrentHeight as u8)
            && packet.payload.len() == 3
        {
            let height_in =
                (u16::from_be_bytes(*array_ref![&packet.payload, 0, 2]) as f32) / 10.0f32;

            // TODO: What are the top 4-bits of this for?
            let flags = packet.payload[2];

            return Self::CurrentHeight {
                height_in,
                keys_set: [
                    flags & 0b0001 != 0,
                    flags & 0b0010 != 0,
                    flags & 0b0100 != 0,
                    flags & 0b1000 != 0,
                ],
            };
        }

        if packet.target_address == (Address::ExternalDongle as u8)
            && packet.command == (ResponseCommand::Key1Position as u8)
            && packet.payload.len() == 2
        {
            let steps = u16::from_be_bytes(*array_ref![&packet.payload, 0, 2]);
            return Self::Key1Position { steps };
        }

        if packet.target_address == (Address::ExternalDongle as u8)
            && packet.command == (ResponseCommand::Key2Position as u8)
            && packet.payload.len() == 2
        {
            let steps = u16::from_be_bytes(*array_ref![&packet.payload, 0, 2]);
            return Self::Key2Position { steps };
        }

        if packet.target_address == (Address::ExternalDongle as u8)
            && packet.command == (ResponseCommand::Key3Position as u8)
            && packet.payload.len() == 2
        {
            let steps = u16::from_be_bytes(*array_ref![&packet.payload, 0, 2]);
            return Self::Key3Position { steps };
        }

        if packet.target_address == (Address::ExternalDongle as u8)
            && packet.command == (ResponseCommand::Key4Position as u8)
            && packet.payload.len() == 2
        {
            let steps = u16::from_be_bytes(*array_ref![&packet.payload, 0, 2]);
            return Self::Key4Position { steps };
        }

        Self::Unknown(packet)
    }
}
