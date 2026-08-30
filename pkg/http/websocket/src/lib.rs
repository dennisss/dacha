#[macro_use]
extern crate common;
#[macro_use]
extern crate file;
#[macro_use]
extern crate macros;

mod frame;

use std::sync::Arc;

use common::io::*;
use common::errors::*;
use http::header::*;
use http::status_code::*;
use crypto::sha1::*;
use crypto::hasher::*;
use executor::sync::AsyncMutex;
use executor_multitask::TaskResource;
use executor::lock_async;

use crate::frame::*;

/// NOTE: This will be the max websocket frame size we allow reading.
const READ_BUFFER_SIZE: usize = 64 * 1024;


#[async_trait]
pub trait WebSocketHandler: 'static + Send + Sync {
    /// NOTE: We will block reading more data from the socket until this function returns.
    async fn handle_message(&self, is_text: bool, data: &[u8]);
}

pub struct WebSocket {
    shared: Arc<Shared>,
    task: TaskResource,
}

struct Shared {
    writer: AsyncMutex<Box<dyn SharedWriteable>>
}

impl WebSocket {
    // TODO: Things in here that fail will log messages in the http::Server (so probably not ideal to have this error out)
    pub async fn create_server(
        handler: Arc<dyn WebSocketHandler>,
        req_head: http::RequestHead,
        reader: Box<dyn Readable>,
        mut writer: Box<dyn SharedWriteable>
    ) -> Result<Self> {
        // TODO: Check the other websocket headers and stuff.

        let client_key = req_head.headers.find_one("Sec-WebSocket-Key")?.value.to_ascii_str()?;
        let accept = form_server_accept_string(client_key);

        let response = format!("HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Accept: {}\r\n\r\n", accept);
        writer.write_all(response.as_bytes()).await?;

        let shared = Arc::new(Shared {
            writer: AsyncMutex::new(writer)
        });

        let task = TaskResource::spawn_interruptable("WebSocket", Self::reader_thread(shared.clone(), handler, reader));

        Ok(Self {
            shared, task
        })
    }

    // TODO: On failures, close both ends of the socket.
    async fn reader_thread(
        shared: Arc<Shared>,
        handler: Arc<dyn WebSocketHandler>,
        mut reader: Box<dyn Readable>
    ) -> Result<()> {

        let mut buffer = vec![0u8; READ_BUFFER_SIZE];
        let mut buffer_len = 0;

        loop {
            let n = reader.read(&mut buffer[buffer_len..]).await?;
            if n == 0 {
                break;
            }

            buffer_len += n;

            let mut n_consumed = 0;
            while let Some((frame, n)) = Frame::try_decode(&mut buffer[n_consumed..buffer_len]) {
                n_consumed += n;

                if !frame.fin {
                    return Err(err_msg("Non-final frames not supported"));
                }

                match frame.opcode {
                    OpCode::Binary => {
                        handler.handle_message(false, frame.data).await;
                    }
                    OpCode::Text => {
                        handler.handle_message(true, frame.data).await;
                    }
                    OpCode::Ping => {
                        let shared = shared.clone();

                        let mut pkt = vec![];
                        Frame {
                            fin: true,
                            opcode: OpCode::Pong,
                            mask: None,
                            data: frame.data
                        }.serialize(&mut pkt);

                        // TODO: Don't block on this.
                        executor::spawn(async move {
                            lock_async!(writer <= shared.writer.lock().await?, {
                                writer.write_all(&pkt).await
                            })
                        }).join().await?;
                    }
                    OpCode::Close => {
                        // TODO: Explicitly close the writer too here.
                        return Ok(());
                    }
                    _ => {
                        println!("Received unsupported websocket packet (opcode: {:?})", frame.opcode);
                    }

                }


            }

            buffer.copy_within(n_consumed..buffer_len, 0);
            buffer_len -= n_consumed;

            if buffer_len == buffer.len() {
                return Err(err_msg("Frame overflowed read buffer size"));
            }
        }

        // TODO: Explicitly close the writer too here.

        Ok(())
    }

    pub async fn write_binary(&self, data: &[u8]) -> Result<()> {
        let shared = self.shared.clone();

        let mut pkt = vec![];
        Frame {
            fin: true,
            opcode: OpCode::Binary,
            mask: None,
            data
        }.serialize(&mut pkt);

        executor::spawn(async move {
            lock_async!(writer <= shared.writer.lock().await?, {
                writer.write_all(&pkt).await
            })
        }).join().await
    }

    pub async fn write_text(&self, data: &[u8]) -> Result<()> {
        let shared = self.shared.clone();

        let mut pkt = vec![];
        Frame {
            fin: true,
            opcode: OpCode::Text,
            mask: None,
            data
        }.serialize(&mut pkt);

        executor::spawn(async move {
            lock_async!(writer <= shared.writer.lock().await?, {
                writer.write_all(&pkt).await
            })
        }).join().await
    }


}


fn form_server_accept_string(client_key: &str) -> String {

    let mut hasher = SHA1Hasher::default();
    hasher.update(client_key.trim().as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let hash = hasher.finish();

    base_radix::base64_encode(&hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_server_accept_string_test() {
        // Example from the RFC
        assert_eq!(
            form_server_accept_string("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

}
