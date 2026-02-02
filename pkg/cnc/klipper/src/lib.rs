#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;


// https://www.klipper3d.org/Protocol.html#protocol
// <1 byte length><1 byte sequence><n-byte content><2 byte crc><1 byte sync>


/*
https://github.com/Klipper3d/klipper/blob/abda66d6efafdcd12fb423e72cda1e936f6ac226/klippy/msgproto.py#L9
https://github.com/Klipper3d/klipper/blob/master/klippy/chelper/serialqueue.c
https://github.com/Klipper3d/klipper/blob/master/klippy/chelper/msgblock.c



The "identify_response" response id is 0, the "identify" command id is 1.


https://www.amazon.com/dp/B0CHJQ6CXF

ADXL345

- 4 wire SPI
- https://www.analog.com/media/en/technical-documentation/data-sheets/adxl345.pdf

*/

mod vlq;
mod crc;

use std::time::Duration;
use std::sync::Arc;
use std::collections::HashMap;

use common::errors::*;
use common::io::{Readable, SharedWriteable, Writeable};
use executor::sync::AsyncMutex;
use executor::channel;
use executor::{lock, lock_async};
use executor_multitask::{impl_resource_passthrough, TaskResource};
use peripherals::serial::SerialPort;
use common::format::format_bytes;


use crate::vlq::*;
use crate::crc::*;

const SYNC_BYTE: u8 = 0x7e;


pub struct MessageFormat {
    pub id: u32,
    pub name: String,
    pub params: Vec<MessageParameterFormat>,
}

pub struct MessageParameterFormat {
    pub name: String,
    pub typ: MessageParameterType,
}

pub enum MessageParameterType {
    Integer,
    String,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub name: String,
    pub params: Vec<MessageParameter>
}

#[derive(Debug, Clone)]
pub struct MessageParameter {
    pub name: String,
    pub value: MessageParameterValue
}

#[derive(Debug, Clone)]
pub enum MessageParameterValue {
    Integer(i32),
    String(Vec<u8>),
}

impl Message {

    pub fn get_integer_param(&self, name: &str) -> Result<i32> {
        for param in &self.params {
            if param.name == name {
                match &param.value {
                    MessageParameterValue::Integer(v) => return Ok(*v),
                    _ => return Err(err_msg("Wrong type"))
                }
            }
        }

        Err(err_msg("Unknown param"))
    }

    pub fn get_string_param(&self, name: &str) -> Result<&[u8]> {
        for param in &self.params {
            if param.name == name {
                match &param.value {
                    MessageParameterValue::String(v) => return Ok(&v),
                    _ => return Err(err_msg("Wrong type"))
                }
            }
        }

        Err(err_msg("Unknown param"))
    }

}





#[derive(Debug)]
pub struct MessageBlock<'a> {
    pub seq: u8,
    pub content: &'a [u8]
}

impl<'a> MessageBlock<'a> {

    pub fn serialize(&self) -> Result<Vec<u8>> {
        assert!(self.content.len() + 5 <= 64);
        assert!(self.seq & 0x0F == self.seq);

        let mut out = vec![];
        out.push((self.content.len() + 5) as u8);
        out.push(self.seq | 0x10);
        out.extend_from_slice(self.content);

        let sum = klipper_crc16(&out);
        out.extend_from_slice(&sum.to_be_bytes());
        out.push(SYNC_BYTE);

        Ok(out)
    }

    // If None is returned, then there is not enough data yet to
    // parse a full message block.
    pub fn parse(data: &'a [u8]) -> Result<Option<(Self, usize)>> {
        if data.is_empty() {
            return Ok(None);
        }

        let len = data[0] as usize;
        if len > 64 || len < 5 {
            return Err(err_msg("Invalid message block length"));
        }

        let raw_seq = data[1];
        if raw_seq & 0x10 != 0x10 {
            return Err(err_msg("Invalid sequence"));
        }

        if data.len() < len {
            return Ok(None);
        }

        let sync = data[len - 1];
        if sync != SYNC_BYTE {
            return Err(err_msg("Invalid sync byte"));
        }

        let sum = u16::from_be_bytes(*array_ref![data, len - 3, 2]);
        let expected_sum = klipper_crc16(&data[0..(len - 3)]);

        // println!("{:x}", sum);
        // println!("{:x}", expected_sum);

        if sum != expected_sum {
            return Err(err_msg("Wrong checksum"));
        }

        let content = &data[2..(len - 3)];

        Ok(Some((
            Self {
                seq: raw_seq & 0x0F,
                content,
            },
            len
        )))
    }

}

pub struct KlipperDevice {
    shared: Arc<Shared>,
    task: TaskResource
}

struct Shared {
    state: AsyncMutex<State>,
}

struct State {
    last_seq: u8,
    writer: Box<dyn SharedWriteable>,
    message_formats: HashMap<u32, MessageFormat>,
    subscribers: HashMap<u32, Vec<channel::Sender<Message>>>
}



// Box<dyn Readable + Sync>
// Box<dyn SharedWriteable>

impl KlipperDevice {


    pub async fn create() -> Result<Self> {

        let port = SerialPort::open("/dev/ttyACM0", 250_000)?;
        let (mut reader, writer) = port.split();

        // Read any data left in the input serial buffer.
        loop {
            let mut buf = [0u8; 128];

            let n = match executor::timeout(
                Duration::from_millis(10),
                reader.read(&mut buf),

            ).await {
                Ok(v) => v,
                Err(e) => Ok(0)
            }?;

            if n == 0 {
                break;
            }
        }

        let mut message_formats = HashMap::new();

        // "identify_response offset=%u data=%.*s": 0,
        // "identify offset=%u count=%c": 1,

        message_formats.insert(0, MessageFormat {
            id: 0,
            name: "identify_response".into(),
            params: vec![
                MessageParameterFormat { name: "offset".into(), typ: MessageParameterType::Integer },
                MessageParameterFormat { name: "data".into(), typ: MessageParameterType::String },
            ]
        });


        let shared = Arc::new(Shared {
            state: AsyncMutex::new(State {
                last_seq: 4,
                writer,
                message_formats,
                subscribers: HashMap::new(),
            })
        });

        let task = TaskResource::spawn_interruptable("KlipperDevice", Self::reader_thread(shared.clone(), reader));

        let inst = Self { shared, task };


        println!("Writing...");

        let sub = inst.subscribe(0).await?;

        let mut identity_buf = vec![];

        loop {
            let offset = identity_buf.len() as i32;
            {
                let mut buf = vec![];
                klipper_encode_vlq(1, &mut buf);
                klipper_encode_vlq(identity_buf.len() as i32, &mut buf);
                klipper_encode_vlq(40, &mut buf);
                inst.send_message_block(&buf).await?;
            }

            let res = sub.recv().await?;

            let res_offset = res.get_integer_param("offset")?;
            if res_offset != offset {
                return Err(err_msg("Received wrong offset"));
            }

            let data = res.get_string_param("data")?;
            if data.len() == 0 {
                break;
            }

            identity_buf.extend_from_slice(data);
        }

        println!("Got: {} bytes", identity_buf.len());

        file::write("identity.zlib", &identity_buf).await?;



        {


            // println!("{:?}", msg);

            // let (msg2, _) = decode_message_block(&msg)?;
            // assert_eq!(msg2.content, &buf[..]);
            // assert_eq!(msg2.seq, 0);


            // buf.insert(0, SYNC_BYTE);

        }

        // println!("Got: {:?}", sub.recv().await?);

        println!("Reading...");

        loop {
            executor::sleep(Duration::from_secs(1)).await?;
        }

        Ok(inst)
    }

    async fn subscribe(&self, message_id: u32) -> Result<channel::Receiver<Message>> {
        let (sender, receiver) = channel::unbounded();
        
        lock!(state <= self.shared.state.lock().await?, {
            state.subscribers.entry(message_id).or_default().push(sender);
        });

        Ok(receiver)
    }

    async fn send_message_block(&self, content: &[u8]) -> Result<()> {
        lock_async!(state <= self.shared.state.lock().await?, {
            let seq = (state.last_seq + 1) & 0x0f;
            state.last_seq = seq;

            let buf = MessageBlock { seq, content }.serialize()?;
            state.writer.write_all(&buf).await
        })
    }

    // TODO: When this fails, drop all subscribers.
    async fn reader_thread(
        shared: Arc<Shared>,
        mut reader: Box<dyn Readable + Sync>
    ) -> Result<()> {
        let mut buf = vec![];

        loop {
            let original_len = buf.len();
            buf.resize(original_len + 256, 0);
            let n_read = reader.read(&mut buf[original_len..]).await?;
            buf.truncate(original_len + n_read);

            let mut i = 0;

            lock!(state <= shared.state.lock().await?, {

                while let Some((msg_block, msg_block_len)) = MessageBlock::parse(&buf[i..])? {
                    i += msg_block_len;

                    println!("Block: {:?}", msg_block);

                    // seq > last send seq is an ack.

                    let mut content = msg_block.content;
                    
                    while !content.is_empty() {
                        let (msg_id, n) = klipper_decode_vlq(content)?;
                        content = &content[n..];

                        let msg_id = msg_id as u32;

                        let msg_format = match state.message_formats.get(&msg_id) {
                            Some(v) => v,
                            None => {
                                eprintln!("Unknown message id: {}", msg_id);
                                break;
                            }
                        };

                        let mut params = vec![];
                        for param_format in &msg_format.params {
                            let (v, n) = klipper_decode_vlq(content)?;
                            content = &content[n..];

                            let value = match param_format.typ {
                                MessageParameterType::Integer => {
                                    MessageParameterValue::Integer(v)
                                }
                                MessageParameterType::String => {
                                    let len = v as usize;
                                    if content.len() < len {
                                        return Err(err_msg("Incomplete string received"));
                                    }

                                    let data = content[..len].to_vec();
                                    content = &content[len..];

                                    MessageParameterValue::String(data)
                                }

                            };

                            params.push(MessageParameter {
                                name: param_format.name.clone(),
                                value
                            });
                        }
 
                        let msg = Message {
                            name: msg_format.name.clone(),
                            params
                        };

                        println!("Message: {:?}", msg);

                        if let Some(subs) = state.subscribers.get(&msg_id) {
                            for sub in subs {
                                // TODO: Remoe subscribers that are dropped.
                                sub.try_send(msg.clone());
                            }
                        }


                    }
                }

                Result::<_, Error>::Ok(())
            })?;

            buf.drain(..i);
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_message_block_test() {

        // This is some error
        let data = [11, 16, 84, 57, 140, 21, 129, 94, 147, 204, 126];

        // let data = [42, 216, 140, 126, 11, 16, 84, 51, 138, 40, 129, 42, 216, 140, 126, 11, 16, 84, 51, 138, 40, 129, 42, 216, 140, 126, 11, 16, 84, 51, 138, 40, 129, 42, 216, 140, 126, 11, 16, 84, 51, 138, 40, 129, 42, 216, 140, 126];

        let (msg, consumed) = decode_message_block(&data[..]).unwrap();

        println!("{:?}", msg);


        let (i, n) = klipper_decode_vlq(msg.content).unwrap();

        println!("{}", i);




    }

}
