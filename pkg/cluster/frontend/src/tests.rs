

/*
Things to test:

- Request with random data in Client-Id/Auth-Key cookies fails.

- Request with valid data but wrong Auth-Key fails

- Request with valid Client-Id/Auth-Key

- Proxy to an unauthenticated backend.

- Proxy to an authenticated backend

- Redirect Location if not logged in to the unauthenticated backend


- Test having a new connection which immediately presents a Client-Id from an old session.
    - Verify able to re-use the old session.

- Verify Login request will set cookie for Auth-Key

*/

use std::sync::Arc;
use std::convert::TryFrom;

use base_error::*;
use cluster_client::ClusterMetaClient;
use http::ClientInterface;
use cluster_auth::*;
use executor_multitask::ServiceResource;
use cluster_client::meta::SessionTable;
use container_proto::cluster::Session;

use crate::handler::*;


pub struct TestHttpServer {
    shared: Arc<TestHttpServerShared>,
    server: Arc<dyn ServiceResource>,
}

struct TestHttpServerShared {
    name: String,
    received_requests: Vec<String>,
}

impl TestHttpServer {
    pub fn create(name: &str, port: u16) -> Self {
        let shared = Arc::new(TestHttpServerShared {
            name: name.to_string(),
            received_requests: vec![]
        });

        let handler = TestHttpHandler { shared: shared.clone() };

        let mut options = http::ServerOptions::default();
        options.port = Some(port);
        options.tls = None;
        options.force_http2 = true;

        let mut server = http::Server::new(handler, options);
        let server = Arc::new(server.start());

        Self {
            shared,
            server
        }
    }
}

struct TestHttpHandler {
    shared: Arc<TestHttpServerShared>
}

#[async_trait]
impl http::ServerHandler for TestHttpHandler {
    async fn handle_request<'a>(
        &self,
        request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        http::ResponseBuilder::new()
            .status(http::status_code::OK)
            .body(http::BodyFromData(format!("Hello from {}", self.shared.name)))
            .build()
            .unwrap()
    }
}



// TODO: Track all the resources in this test and make sure none fail.
// TODO: Ideally change this to do an E2E test with the cluster_auth binary.
#[testcase]
async fn works() -> Result<()> {


    let backend_a = TestHttpServer::create("ServerA", 9002);
    let backend_b = TestHttpServer::create("ServerB", 9003);

    let mut config = container_proto::cluster::FrontendConfig::default();
    protobuf::text::parse_text_proto(r#"
        backends {
            sub_domain: "auth"
            backend_address: "localhost:9002"
            allowed_principals: ["unauthenticated"]
        }
        backends {
            sub_domain: "cool"
            backend_address: "localhost:9003"
            allowed_principals: ["authenticated"]
        }
    "#, &mut config)?;

    let meta_client = ClusterMetaClient::create_testing().await?;

    let valid_auth_key = generate_session_auth_key().await;
    let valid_client_id = ClientId::generate().await;

    {
        let mut session = Session::default();
        session.set_user_name("bob");
        session.set_id(generate_session_id().await);
        session.set_auth_key_hash(create_session_auth_key_hash(&valid_auth_key));
        session.set_client_id(valid_client_id.0);
        meta_client.db().insert::<SessionTable>(&session).await?;
    }

    let domain_name = "testing.internal".to_string();    

    let test_port = 9001;

    // TODO: Deduplicate more with the main.rs file.
    let server = {
        let handler = FrontendHttpHandler::create(config, domain_name, meta_client.clone()).await?;

        let mut options = http::ServerOptions::default();
        options.port = Some(test_port);
        options.tls = None; // Some(public_credentials.server_options());
        options.force_http2 = true;

        let mut server = http::Server::new(handler, options);

        Arc::new(server.start())
    };

    let mut options = http::ClientOptions::try_from("http://localhost:9001")?;
    options.backend_balancer.backend.force_http2 = true;
    let client = http::Client::create(options).await?;

    let mut request_context = http::ClientRequestContext::default();
    request_context.wait_for_ready = true;

    // Request with an unknown host returns an error and no backends are called.
    {
        let req = http::RequestBuilder::new()
            .method(http::Method::GET)
            .uri("http://example.com/hello")
            .build()?;

        let mut res = client.request(
            req,
            request_context.clone(),
            &mut http::ClientResponseContext::default()
        ).await?;
        println!("{:?}", res.head);

        assert_eq!(res.head.status_code, http::status_code::BAD_REQUEST);
    }

    // Request to valid domain but there is no backend.
    {
        let req = http::RequestBuilder::new()
            .method(http::Method::GET)
            .uri("http://unknown.testing.internal/")
            .build()?;

        let mut res = client.request(
            req,
            request_context.clone(),
            &mut http::ClientResponseContext::default()
        ).await?;
        println!("{:?}", res.head);

        assert_eq!(res.head.status_code, http::status_code::NOT_FOUND);

        // TODO: Check we get back a Client-Id cookie
    }

    // Bad cookie values.
    {
        let req = http::RequestBuilder::new()
            .method(http::Method::GET)
            .uri("http://unknown.testing.internal/")
            .header("Cookie", "Auth-Key=bad_value; Client-Id=a")
            .build()?;

        let mut res = client.request(
            req,
            request_context.clone(),
            &mut http::ClientResponseContext::default()
        ).await?;
        println!("{:?}", res.head);

        // tODO: Check that the client-id is set and we deleted the auth-key cookie
    }

    // Providing wrong client-id
    {
        let other_client_id = ClientId::generate().await;

        let req = http::RequestBuilder::new()
            .method(http::Method::GET)
            .uri("http://unknown.testing.internal/")
            // TODO: Add valid but incorrect values.
            .header("Cookie", format!("Client-Id={}", other_client_id.to_string()))
            .build()?;

        let mut res = client.request(
            req,
            request_context.clone(),
            &mut http::ClientResponseContext::default()
        ).await?;
        
        println!("{:?}", res.head);

    }

    let mut options = http::ClientOptions::try_from("http://localhost:9001")?;
    options.backend_balancer.backend.force_http2 = true;
    let client = http::Client::create(options).await?;

    // Request to unauthenticated backend.
    {
        let req = http::RequestBuilder::new()
            .method(http::Method::GET)
            .uri("http://auth.testing.internal/")
            .header("Cookie", format!("Client-Id={}", valid_client_id.to_string()))
            .build()?;

        let mut res = client.request(
            req,
            request_context.clone(),
            &mut http::ClientResponseContext::default()
        ).await?;
        assert_eq!(res.head.status_code, http::status_code::OK);

        let mut body_buf = vec![];
        res.body.read_to_end(&mut body_buf).await?;
        assert_eq!(&body_buf[..], b"Hello from ServerA");
    }

    // Request to authenticated backend (no auth-key).
    {
        let req = http::RequestBuilder::new()
            .method(http::Method::GET)
            .uri("http://cool.testing.internal/")
            .header("Cookie", format!("Client-Id={}", valid_client_id.to_string()))
            .build()?;

        let mut res = client.request(
            req,
            request_context.clone(),
            &mut http::ClientResponseContext::default()
        ).await?;
        assert_eq!(res.head.status_code, http::status_code::FORBIDDEN);
    }

    // Request to authenticated backend (wrong auth-key).
    {
        let auth_key = base_radix::base64url_encode(&generate_session_auth_key().await);

        let req = http::RequestBuilder::new()
            .method(http::Method::GET)
            .uri("http://cool.testing.internal/")
            .header("Cookie", format!("Client-Id={}; Auth-Key={}", valid_client_id.to_string(), auth_key))
            .build()?;

        let mut res = client.request(
            req,
            request_context.clone(),
            &mut http::ClientResponseContext::default()
        ).await?;
        assert_eq!(res.head.status_code, http::status_code::FORBIDDEN);
    }

    // Request to authenticated backend (right auth-key).
    {
        let auth_key = base_radix::base64url_encode(&valid_auth_key);

        let req = http::RequestBuilder::new()
            .method(http::Method::GET)
            .uri("http://cool.testing.internal/")
            .header("Cookie", format!("Client-Id={}; Auth-Key={}", valid_client_id.to_string(), auth_key))
            .build()?;

        let mut res = client.request(
            req,
            request_context.clone(),
            &mut http::ClientResponseContext::default()
        ).await?;
        assert_eq!(res.head.status_code, http::status_code::OK);

        let mut body_buf = vec![];
        res.body.read_to_end(&mut body_buf).await?;
        assert_eq!(&body_buf[..], b"Hello from ServerB");
    }

    Ok(())
}