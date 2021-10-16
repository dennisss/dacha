use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::Arc;

use common::async_std::channel;
use common::async_std::net::{TcpListener, TcpStream};
use common::async_std::sync::Mutex;
use common::async_std::task;
use common::errors::*;
use common::futures::future::ok;
use common::futures::io::Write;
use common::futures::stream;
use common::futures::stream::{Stream, StreamExt};
use common::futures::{Future, FutureExt};
use common::io::{Readable, Sinkable, StreamExt2, Streamable, StreamableExt, Writeable};

use crate::redis::resp::*;

// TODO: Current issue: We currently don't support disabling pub/sub
// functionality all together as we currently don't decouple the low-level
// stream nature of those commands, so if you want to implement a server without
// pub/sub, then you need to implement handlers ^ Additionally our service
// overlay is totally stateless and does not deal with client semantics right
// now (which is problematic for transactions )

// /// The response to a typical request/response model command
// pub type CommandResponse = Box<Future<Item = RESPObject, Error = Error> +
// Send>;

/// The response to a push (aka pub/sub) model command
/// Only ever really used internally for certain commands
type CommandStream = Pin<Box<dyn Stream<Item = Result<PushObject>> + Send>>;

// Convert a Vector to a stream

//impl common::

//

#[async_trait]
pub trait Service {
    async fn get(&self, key: RESPString) -> Result<RESPObject>;

    async fn set(&self, key: RESPString, value: RESPString) -> Result<RESPObject>;

    async fn del(&self, key: RESPString) -> Result<RESPObject>;

    /// Given a message, this should send it to all remote clients for that
    /// message
    async fn publish(&self, channel: &RESPString, object: &RESPObject) -> Result<usize>;

    /// Called whenever a client subscribes to a channel locally
    async fn subscribe(&self, channel: RESPString) -> Result<()>;

    /// Called whenever a client un-subscribes from a channel locally
    async fn unsubscribe(&self, channel: RESPString) -> Result<()>;
}

enum PushObject {
    Message(RESPString, RESPObject),
    Subscribe(RESPString, usize),
    Unsubscribe(RESPString, usize),
    Pong(RESPString),
}

enum Packet {
    Push(PushObject),
    Response(RESPObject),
}

type ChannelName = Vec<u8>;
type ClientId = u64;

enum ServerClientMode {
    RequestResponse,
    Push,
}

struct ServerClient {
    id: ClientId,
    //mode: ServerClientMode,
    /// A handle for pushing packets to this client
    /// NOTE: Only push messages should be sendable through this interface
    sender: channel::Sender<(RESPString, RESPObject)>,

    /// All channels which this client is subscribed to
    /// TODO: Probably cheaper to assign each channel a unique id to avoid
    /// having to store many copies to its name (or represent it as a shared
    /// Bytes array)
    channels: HashSet<ChannelName>,
}

//pub type CommandHandler<T> = (Fn(&T, RESPCommand) -> Result<RESPObject>) +
// Sync;

/*
    Implementing BRPOP
    - Generally a separate Hashmap storing client ids for all keys which someone is currently blocking on
        - Clients may also be connected to a remote server
*/

/// Internal return value for representing the raw output of trying to execute a
/// command
enum CommandResult {
    /// A single response sent back to the client.
    Resp(RESPObject),
    /// A stream of responses that are eventually sent back to the client.
    Push(CommandStream),
}

pub struct Server<T: 'static> {
    service: T,
    state: Mutex<ServerState>,
}

struct ServerState {
    last_id: ClientId,

    // TODO: Possibly use Slabs for the clients list
    /// All clients connected to this server
    clients: HashMap<ClientId, Arc<Mutex<ServerClient>>>,

    /// Listing of all clients in each channel
    channels: HashMap<ChannelName, HashSet<ClientId>>,
}

/*
    The main issue:
    - A client that is subscribing may end up blocking while subscriptions are running (because we must query the service)
        - The better idea:
        - Must be able to split off the stream and respond

    - End up selecting between three futures:
        - Future 1: Either<Receive Request, Respond To Request>
        - Future 2: Either<mpsc poll, Never>
            - Ideally only poll for the mpsc upon first transition to becoming a subscriber
            - This will also allow us to gurantee that we don't read from the mpsc until we are actually latched into this mode
*/

// Need to take a Stream<>

pub struct RESPStream {
    buffer: Vec<u8>,
    remaining: Option<(usize, usize)>,
    parser: RESPParser,
    inner: Box<dyn Readable>,
}

impl RESPStream {
    pub fn new(inner: Box<dyn Readable>) -> Self {
        Self {
            buffer: vec![0; 512],
            remaining: None,
            parser: RESPParser::new(),
            inner,
        }
    }
}

#[async_trait]
impl Streamable for RESPStream {
    type Item = Result<RESPObject>;
    async fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (mut start, end) = if let Some(v) = self.remaining.take() {
                v
            } else {
                let n = match self.inner.read(&mut self.buffer[..]).await {
                    Ok(v) => v,
                    Err(e) => {
                        return Some(Err(e));
                    }
                };
                (0, n)
            };

            if end - start == 0 {
                return None;
            }

            let (n, obj) = match self.parser.parse(&self.buffer[start..end]) {
                Ok(v) => v,
                Err(e) => {
                    return Some(Err(e));
                }
            };

            start += n;
            if n < end {
                self.remaining = Some((start, end));
            }

            if let Some(obj) = obj {
                return Some(Ok(obj));
            }
        }
    }
}

pub struct RESPSink {
    inner: Box<dyn Writeable>,
    buffer: Vec<u8>,
}

impl RESPSink {
    pub fn new(inner: Box<dyn Writeable>) -> Self {
        Self {
            inner,
            buffer: Vec::new(),
        }
    }
}

#[async_trait]
impl Sinkable<RESPObject> for RESPSink {
    async fn send(&mut self, obj: RESPObject) -> Result<()> {
        self.buffer.clear();
        obj.serialize_to(&mut self.buffer);
        self.inner.write(&self.buffer).await?;
        Ok(())
    }
}

macro_rules! return_as_ok {
    ($e:expr) => {{
        if let Err(e) = $e {
            return Ok(e);
        }
    }};
}

impl<T: 'static> Server<T>
where
    T: Service + Send + Sync,
{
    pub fn new(service: T) -> Self {
        Server {
            service,
            state: Mutex::new(ServerState {
                last_id: 0,
                clients: HashMap::new(),
                channels: HashMap::new(),
            }),
        }
    }

    pub async fn run(inst: Arc<Self>, port: u16) -> Result<()> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;

        let mut incoming = listener.incoming();

        while let Some(stream) = incoming.next().await {
            let mut stream = stream?;
            task::spawn(Self::handle_connection(inst.clone(), stream));
        }

        Ok(())
    }

    /// Publishes a messages to all clients connected to the local server
    /// Returns the number of clients that were notified
    /// TODO: from_id is trivially not necessary as a publisher should never be
    /// in a subscriber mode
    pub async fn publish(&self, channel: ChannelName, obj: &RESPObject) -> usize {
        let state = self.state.lock().await;

        let client_ids = match state.channels.get(&channel) {
            Some(arr) => arr,
            None => return 0,
        };

        let mut futs = vec![];

        for id in client_ids.iter() {
            let mut client = match state.clients.get(id) {
                Some(c) => c.lock().await,
                None => continue, // Inconsistent map
            };

            // TODO: Possibly convert into an unbounded sender if we are going to clone it
            // anyway
            let mut sender = client.sender.clone();
            let channel = channel.clone();
            let obj = obj.clone();
            futs.push(async move { sender.send((channel.into(), obj)).await });
        }

        let num = futs.len();
        common::futures::future::join_all(futs).await;
        num
    }

    async fn handle_connection(inst: Arc<Self>, sock: TcpStream) {
        sock.set_nodelay(true).expect("Failed to set nodelay");
        //sock.set_recv_buffer_size(128).expect("Failed to set rcv buffer");

        let (tx, rx) = channel::bounded::<(RESPString, RESPObject)>(16);

        let client = {
            let mut server_state = inst.state.lock().await;

            server_state.last_id += 1;
            let id = server_state.last_id;

            let client = Arc::new(Mutex::new(ServerClient {
                id,
                channels: HashSet::new(),
                sender: tx,
            }));

            server_state.clients.insert(id, client.clone());

            println!("Start conn {}", id);

            client
        };

        let reader: Box<dyn Readable> = Box::new(sock.clone());
        let writer: Box<dyn Writeable> = Box::new(sock);

        Self::handle_connection_body(&inst, &client, reader, writer, rx)
            .await
            .map_err(|e| {
                // Ignoring typical errors
                if let Some(eio) = e.downcast_ref::<std::io::Error>() {
                    // This is triggered by a client that disconnects early while we are sending it
                    // data
                    if eio.kind() == std::io::ErrorKind::ConnectionReset {
                        return ();
                    }
                }

                eprintln!("Client Error: {:?}", e);
            })
            .ok();

        // NOTE: This should always run regardless of the status of the 'body' above.
        if let Err(e) = Self::cleanup_client(inst, client).await {
            eprintln!("Error while disconnecting {:?}", e);
        }
    }

    /// Inner part of the handle_connection() function above that runs after the
    /// client has been initialized, but before it has been cleaned up.
    async fn handle_connection_body(
        inst: &Arc<Self>,
        client: &Arc<Mutex<ServerClient>>,
        reader: Box<dyn Readable>,
        writer: Box<dyn Writeable>,
        rx: channel::Receiver<(RESPString, RESPObject)>,
    ) -> Result<()> {
        // The gist is that we need a read and write end.

        let mut sink = RESPSink::new(writer);
        let mut stream = RESPStream::new(reader).into_stream();

        enum Event {
            Request(RESPObject),
            Message(RESPString, RESPObject),
        }

        let mut is_push = false;

        let mut event_stream = {
            let request_stream = stream.map(|res| res.map(|obj| Event::Request(obj)));
            let message_stream = rx.map(|(channel, pkt)| Ok(Event::Message(channel, pkt)));
            common::futures::stream::select(request_stream, message_stream)
        };

        while let Some(item) = event_stream.next().await {
            // Get the next packet(s) to send
            let mut out: Pin<Box<dyn Stream<Item = Result<Packet>> + Send>> = match item? {
                Event::Request(req) => {
                    let cmd = req.into_command()?;

                    let res = Self::run_command(&inst, &client, is_push, cmd).await?;

                    match res {
                        CommandResult::Resp(v) => Box::pin(ok(Packet::Response(v)).into_stream()),
                        CommandResult::Push(s) => Box::pin(s.map(|r| r.map(|v| Packet::Push(v)))),
                    }
                }
                Event::Message(channel, message) => {
                    Box::pin(ok(Packet::Push(PushObject::Message(channel, message))).into_stream())
                }
            };

            // Send them
            // TODO: Currently this means that a blocking request will prevent more messages
            // to be taken out of the mpsc (the solution to this would be to select on a
            // list of promises which would change after each cycle if the response produces
            // a new promise)
            Self::handle_connection_sender(out, &mut sink, &mut is_push).await?;
        }

        Ok(())
    }

    /// Responsible for all sending of responses/pushes back to the client
    /// Waits for packets on a shared mpsc to come from the response server and
    /// from external clients and serially sends them back through the tcp
    /// connection
    fn handle_connection_sender<'a: 'c, 'b: 'c, 'c>(
        mut out: Pin<Box<dyn Stream<Item = Result<Packet>> + Send>>,
        sink: &'a mut RESPSink,
        is_push: &'b mut bool,
    ) -> impl Future<Output = Result<()>> + Send + 'c {
        async move {
            while let Some(pkt) = out.next().await {
                let obj = match pkt? {
                    Packet::Push(push) => {
                        match push {
                            PushObject::Message(channel, msg) => {
                                if !*is_push {
                                    return Ok(());
                                }

                                RESPObject::Array(vec![
                                    RESPObject::BulkString(b"message"[..].into()),
                                    RESPObject::BulkString(channel.into()),
                                    msg,
                                ])
                            }
                            PushObject::Subscribe(channel, count) => {
                                // The first subscribe should make us
                                if count > 0 {
                                    *is_push = true;
                                }

                                RESPObject::Array(vec![
                                    RESPObject::BulkString(b"subscribe"[..].into()),
                                    RESPObject::BulkString(channel.into()),
                                    RESPObject::Integer(count as i64),
                                ])
                            }
                            PushObject::Unsubscribe(channel, count) => {
                                if count == 0 {
                                    *is_push = false;
                                }

                                RESPObject::Array(vec![
                                    RESPObject::BulkString(b"unsubscribe"[..].into()),
                                    RESPObject::BulkString(channel.into()),
                                    RESPObject::Integer(count as i64),
                                ])
                            }
                            PushObject::Pong(data) => RESPObject::Array(vec![
                                RESPObject::BulkString(b"pong"[..].into()),
                                RESPObject::BulkString(data.into()),
                            ]),
                        }
                    }
                    Packet::Response(obj) => {
                        if *is_push {
                            // Generally this means that the client is not writes things in the
                            // right order
                            return Err(err_msg("Rejected to send request response in push mode"));
                        }

                        obj
                    }
                };

                sink.send(obj).await?;
            }

            Ok(())
        }
    }

    // Any box<Stream>

    async fn cleanup_client(inst: Arc<Self>, client: Arc<Mutex<ServerClient>>) -> Result<()> {
        let (id, channels) = {
            let client = client.lock().await;
            (
                client.id,
                client
                    .channels
                    .iter()
                    .map(|s| RESPString::from(s.clone()))
                    .collect::<Vec<_>>(),
            )
        };

        // TODO: Check all are successful
        //        let mut stream = Self::run_command_unsubscribe(&inst, &client,
        // &channels).await;        while let Some(_) = stream.next().await {}

        Self::run_command_unsubscribe(&inst, &client, &channels)
            .await
            .collect::<Vec<_>>()
            .await;

        // TODO: Make sure that this always happens regardless of errors
        // Now that all channels are unsubscribed, we can remove the client compltely
        let mut state = inst.state.lock().await;
        state.clients.remove(&id);

        println!("Client disconnected!");

        Ok(())
    }

    /// TODO: Must also implement errors for running commands that don't work
    /// in the current mode (currently the responses will cause failures anyway
    /// though)
    async fn run_command(
        inst: &Arc<Self>,
        client: &Arc<Mutex<ServerClient>>,
        is_push: bool,
        cmd: RESPCommand,
    ) -> Result<CommandResult> {
        use self::CommandResult::*;

        if cmd.len() == 0 {
            return Ok(Resp(RESPObject::Error(b"No command specified"[..].into())));
        }

        let name = match std::str::from_utf8(cmd[0].as_ref()) {
            Ok(v) => v,
            // TODO: Should this immediately close the connection with a real error
            _ => {
                return Ok(Resp(RESPObject::Error(
                    b"Invalid command format"[..].into(),
                )))
            }
        };

        // Normalize the name of the command
        let name_norm = name.to_uppercase();

        const MAX_ARG: usize = 100;
        let arity = |min: usize, max: usize| -> std::result::Result<(), CommandResult> {
            let valid = cmd.len() >= min && cmd.len() <= max;

            if valid {
                Ok(())
            } else {
                Err(Resp(RESPObject::Error(
                    format!("ERR wrong number of arguments for '{}' command", name)
                        .as_bytes()
                        .into(),
                )))
            }
        };

        let out = match name_norm.as_str() {
            "GET" => {
                return_as_ok!(arity(2, 2));
                Resp(inst.service.get(cmd[1].clone()).await?)
            }
            "DEL" => {
                return_as_ok!(arity(2, 2));
                Resp(inst.service.del(cmd[1].clone()).await?)
            }
            "SET" => {
                return_as_ok!(arity(3, 3));
                Resp(inst.service.set(cmd[1].clone(), cmd[2].clone()).await?)
            }
            "SUBSCRIBE" => {
                return_as_ok!(arity(2, MAX_ARG));
                Push(Self::run_command_subscribe(inst, client, &cmd[1..]).await)
            }
            "UNSUBSCRIBE" => {
                return_as_ok!(arity(2, MAX_ARG));
                Push(Self::run_command_unsubscribe(inst, client, &cmd[1..]).await)
            }
            "PUBLISH" => {
                return_as_ok!(arity(3, 3));
                Resp(Self::run_command_publish(inst, client, cmd[1].clone(), cmd[2].clone()).await?)
            }
            "COMMAND" => {
                return_as_ok!(arity(1, 1));
                Resp(RESPObject::SimpleString(b"OK"[..].into()))
            }
            "PING" => {
                return_as_ok!(arity(1, 2));

                if is_push {
                    if cmd.len() == 1 {
                        Push(Box::pin(
                            ok(PushObject::Pong(RESPString::from(vec![]))).into_stream(),
                        ))
                    } else {
                        Push(Box::pin(ok(PushObject::Pong(cmd[1].clone())).into_stream()))
                    }
                } else {
                    if cmd.len() == 1 {
                        Resp(RESPObject::SimpleString(b"PONG"[..].into()))
                    } else {
                        Resp(RESPObject::BulkString(cmd[1].clone().into()))
                    }
                }
            }
            _ => Resp(RESPObject::Error(
                format!("ERR unknown command '{}'", name).as_bytes().into(),
            )),
        };

        Ok(out)
    }

    async fn run_command_subscribe(
        inst: &Arc<Self>,
        client: &Arc<Mutex<ServerClient>>,
        channels: &[RESPString],
    ) -> CommandStream {
        let inst = inst.clone();
        let client = client.clone();

        let res = {
            let mut state = inst.state.lock().await;
            let mut client = client.lock().await;

            channels
                .iter()
                .map(|c| {
                    let cur_subscribed = client.channels.contains(c.as_ref());

                    let changed = if !cur_subscribed {
                        client.channels.insert(c.as_ref().to_vec());

                        let global_channels = {
                            if !state.channels.contains_key(c.as_ref()) {
                                state.channels.insert(c.as_ref().to_vec(), HashSet::new());
                            }

                            state.channels.get_mut(c.as_ref()).unwrap()
                        };

                        assert!(global_channels.insert(client.id));

                        true
                    } else {
                        false
                    };

                    (c.clone(), client.channels.len(), changed)
                })
                .collect::<Vec<_>>()
        };

        // TODO: These can be running in parallel.
        let s = stream::iter(res)
            .bind_then(inst, async move |inst, (c, client_count, changed)| {
                if changed {
                    inst.service.subscribe(c.clone()).await?;
                }

                Ok(PushObject::Subscribe(c, client_count))
            })
            .into_stream();

        Box::pin(s)
    }

    async fn run_command_unsubscribe(
        inst: &Arc<Self>,
        client: &Arc<Mutex<ServerClient>>,
        channels: &[RESPString],
    ) -> CommandStream {
        let inst = inst.clone();
        let client = client.clone();

        let res = {
            let mut state = inst.state.lock().await;
            let mut client = client.lock().await;

            channels
                .into_iter()
                .map(|c| {
                    let cur_subscribed = client.channels.contains(c.as_ref());

                    let changed = if cur_subscribed {
                        client.channels.remove(c.as_ref());

                        let last_removed = {
                            let global_channels = state.channels.get_mut(c.as_ref()).unwrap();
                            global_channels.remove(&client.id);

                            global_channels.len() == 0
                        };

                        if last_removed {
                            state.channels.remove(c.as_ref());
                        }

                        true
                    } else {
                        false
                    };

                    (c.clone(), client.channels.len(), changed)
                })
                .collect::<Vec<_>>()
        };

        let s = stream::iter(res)
            .bind_then(inst, async move |inst, (c, client_count, changed)| {
                if changed {
                    inst.service.unsubscribe(c.clone()).await?;
                }

                Ok(PushObject::Unsubscribe(c, client_count))
            })
            .into_stream();

        Box::pin(s)
    }

    /// Executes the 'PUBLISH channel message' command.
    async fn run_command_publish(
        inst: &Arc<Self>,
        client: &Arc<Mutex<ServerClient>>,
        channel: RESPString,
        message: RESPString,
    ) -> Result<RESPObject> {
        let inst = inst.clone();

        let obj = RESPObject::BulkString(message.into());

        let num_local = inst.publish(channel.to_vec(), &obj).await;
        //            .map_err(|_| err_msg("Failed to publish message"))?;

        let num_remote = inst.service.publish(&channel, &obj).await?;

        let num = num_local + num_remote;
        Ok(RESPObject::Integer(num as i64))
    }
}
