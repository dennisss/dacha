use std::sync::Arc;

use base_error::*;
use common::bytes::Bytes;
use common::io::{Readable, Writeable, IoError, IoErrorKind};
use executor_multitask::{impl_resource_passthrough, TaskResource};
use cluster_client::{ClusterMetaClient, ServiceResolver};
use http::Resolver;
use net::ip::{IPAddress, SocketAddr};
use net::tcp::{TcpListener, TcpStream};
use crypto::tls::handshake::Handshake;
use executor::bundle::TaskResultBundle;
use executor::channel;
use executor::FromErrno;


pub struct BridgeTLSServer {
    task_resource: TaskResource
}

impl_resource_passthrough!(BridgeTLSServer, task_resource);

struct Shared {
    client: Arc<ClusterMetaClient>
}

impl BridgeTLSServer {
    
    pub async fn create(client: Arc<ClusterMetaClient>) -> Result<Self> {
        let server_listener =
            TcpListener::bind("127.0.0.80:443".parse::<SocketAddr>()?).await?;

        let shared = Arc::new(Shared {
            client
        });

        let task_resource = TaskResource::spawn_interruptable(
            "BridgeTLSServer::run()",
            Self::run(shared, server_listener),
        );
    
        Ok(Self { task_resource })

    }

    async fn run(shared: Arc<Shared>, mut server_listener: TcpListener) -> Result<()> {
        loop {
            let mut stream = server_listener.accept().await?;

            // Block external requestors.
            if !stream.peer_addr().ip().is_v4() ||
                !stream.peer_addr().ip().as_bytes().starts_with(&[ 127, 0, 0 ]) {
                continue;
            }

            // TODO: Limit the max number of concurrent streams.
            executor::spawn(Self::handle_stream(shared.clone(), stream));
        }
    }

    async fn handle_stream(shared: Arc<Shared>, stream: TcpStream) {
        if let Err(e) = Self::handle_stream_inner(shared, stream).await {
            if let Some(IoError {
                kind: IoErrorKind::RemoteReaderClosed,
                ..
            }) = e.downcast_ref()
            {
                return;
            }

            eprintln!("Stream failed {:?}", e);
        }
    }

    /*
    Resolver caching:
    - 
    */

    async fn handle_stream_inner(shared: Arc<Shared>, mut stream: TcpStream) -> Result<()> {
        let mut head = vec![0u8; 4096];
        let n = stream.read(&mut head).await?;
        head.truncate(n);
        let head = Bytes::from(head);

        let record = crypto::tls::record::Record::parse(head.slice(0..n))?;
        if record.typ != crypto::tls::record::ContentType::Handshake {
            return Err(err_msg("Expecting first TLS record to be a handshake packet"));
        }

        let (handshake, _) = Handshake::parse(
            record.data, crypto::tls::handshake::TLS_1_0_VERSION)?;

        let client_hello = match handshake {
            Handshake::ClientHello(v) => v,
            _ => return Err(err_msg("Expected first TLS handshake record to be a ClientHello"))
        };

        let server_name = crypto::tls::extensions_util::find_server_name_from_client(&client_hello.extensions)?
            .ok_or_else(|| err_msg("Missing server name"))?;

        println!("[TLS Conn Started] {}", server_name);

        // TODO: Cache this instance for several minutes (if we can generally verify it is a good service name)
        // TODO: Disable interprating any port info in this.
        let resolver = ServiceResolver::create(server_name, shared.client.clone())?;

        let endpoints = resolver.resolve().await?;
        if endpoints.is_empty() {
            return Ok(());
        }

        // TODO: Randomly pick one.
        let mut backend_stream = TcpStream::connect(endpoints[0].address.clone()).await?;

        // NOTE: These are very important to reduce latency.
        stream.set_nodelay(true)?;
        backend_stream.set_nodelay(true)?;

        backend_stream.write_all(&head[..]).await?;

        let (mut client_reader, mut client_writer) = stream.split();
        let (mut backend_reader, mut backend_writer) = backend_stream.split();

        let mut bundle = TaskResultBundle::new();

        // NOTE: Linux sendfile doesn't support connecting two sockets.
        bundle.add("a", async move {
            client_reader.pipe(backend_writer.as_mut()).await
        });
        bundle.add("b", async move {
            backend_reader.pipe(client_writer.as_mut()).await
        });

        bundle.join().await
    }


}

