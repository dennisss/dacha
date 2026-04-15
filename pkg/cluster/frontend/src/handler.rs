
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use common::errors::*;
use common::hash::FastHasherBuilder;
use cluster_proto::cluster::*;
use executor::sync::AsyncMutex;
use executor_multitask::ServiceResource;
use http::ServerHandler;
use rpc_util::AddReflection;
use http_util::{internal_server_error, bad_request, not_found, forbidden};
use http::headers::cookie::*;
use http::headers::host::*;
use http::ClientInterface;
use http::ResponseBuilder;
use cluster_client::ClusterMetaClient;
use cluster_client::service::create_http_client;
use cluster_client::acl::principal::Principal;
use cluster_client::acl::principal::PrincipalSet;
use cluster_client::meta::SessionTable;
use cluster_client::acl::checker::check_entity_allowed;
use cluster_client::service::address::ServiceName;
use cluster_client::acl::proxy::FORWARDED_ENTITY_HEADER;
use db_table::query_one;
use http::header::{LOCATION, SET_COOKIE};
use http::Header;
use executor::lock;
use cluster_auth::{ClientId, AUTH_KEY_DELETED_VALUE, AUTH_KEY_HEADER, AUTH_KEY_LEN, create_session_auth_key_hash};
use cluster_client::acl::proxy::CLIENT_ID_HEADER;
use cluster_client::acl::proxy::SESSION_ID_HEADER;
use cluster_client::acl::proxy::FORWARDED_IP_HEADER;
use cluster_client::throttler::*;
use net::ip::SocketAddr;

use crate::cookies::*;

// TODO: Need lots more TCP/TLS/request rate limits on this server to mitigate denial of service attempts.

// TODO: Need a token bucket to limit requests. If there are failures, this should count against.

/// How long we will cache 
///
/// TODO: Periodically refresh this in the background 
///
/// TODO: For long running RPCs, terminate the request early if we find
/// that the user has lost credentials access.
const SESSION_CACHE_DURATION: Duration = Duration::from_secs(5 * 60);

/// TODO: Need a graceful stop mechanism for this?
/// TODO: Use this.
const MAX_REQUEST_DEADLINE: Duration = Duration::from_secs(60);

/// TODO: Use this.
const MAX_UNAUTHENTICATED_DEADLINE: Duration = Duration::from_secs(5);

const NUM_IP_BINS: usize = 1024;

const MAX_CONNECTIONS_PER_IP: u32 = 16;

const TOTAL_TOKENS: usize = 1000;

const TOKEN_REFRESH_WINDOW: Duration = Duration::from_secs(10);

const CONNECT_COST: usize = 50;

const AUTHENTICATE_COST: usize = 25;

const REQUEST_UNAUTH_COST: usize = 10;

const REQUEST_AUTH_COST: usize = 1;

pub struct FrontendHttpHandler {
    domain_name: String,
    config: FrontendConfig,
    meta_client: Arc<ClusterMetaClient>,
    backends: HashMap<String, Backend, FastHasherBuilder>,
    ip_throttler: HashedTokenBucketThrottler,
    ip_admission: HashedAdmissionLimiter,
}

struct Backend {
    config: FrontendBackendConfig,
    client: http::Client,
    allowed_principals: PrincipalSet,
}

struct ParsedRequest {
    sub_domain: String,
    credentials: RequestCredentials,
}

struct RequestCredentials {
    auth_key: Option<Vec<u8>>,
    client_id: Option<ClientId>,
    // These will be set if the above are None and some invalid un-unable
    // value is present in the request's cookies.
    has_invalid_auth_key: bool,
    has_invalid_client_id: bool,
}

#[derive(Default)]
struct ConnectionData {
    state: AsyncMutex<ConnectionState>,
    ticket: Option<HashedAdmissionLimiterTicket>
}

#[derive(Default)]
struct ConnectionState {
    client_id: Option<ClientId>,

    /// Last session that we observed the connection being logged into.
    cached_session: Option<CachedSession>
}

struct CachedSession {
    auth_key: Vec<u8>,
    session: Session,
    check_time: Instant,
}

// TODO: Need good cache-control headers for all the RPCs.

// NOTE: Mainly we need to block passing through of Cookies since we don't want backends to
// mutate or see raw user credentials.
//
// Also any X-Forwarded- style headers should be dropped to avoid allowing the client to
// misrepresent its identity to the backend 
fn allow_header_passthrough(header: &Header) -> bool {
    if header.is_transport_level() {
        return false;
    }

    if header.is_content_level() {
        return true;
    }

    if header.name.as_str().starts_with("grpc-") {
        return true;
    }

    const EXTRA_ALLOWED: &'static [&'static str] = &[
        LOCATION,
        "Accept",
        "Vary",
        "User-Agent",
        "Origin",
        "Referer",
    ];

    for s in EXTRA_ALLOWED {
        if s.eq_ignore_ascii_case(header.name.as_str()) {
            return true;
        }
    }

    false
}

impl FrontendHttpHandler {
    pub async fn create(config: FrontendConfig, domain_name: String, meta_client: Arc<ClusterMetaClient>) -> Result<Self> {
        let mut backends = HashMap::default();
        for config in config.backends() {
            let client = create_http_client(config.backend_address(), meta_client.clone()).await?;

            let mut allowed_principals = PrincipalSet::default();
            if config.allowed_principals().is_empty() {
                return Err(err_msg("Empty principals list in rule"));
            }

            for s in config.allowed_principals() {
                allowed_principals.insert(Principal::parse_relative(s, Some(meta_client.zone()))?);
            }

            backends.insert(config.sub_domain().to_string(), Backend {
                config: config.as_ref().clone(),
                client,
                allowed_principals,
            });
        }

        let ip_throttler = HashedTokenBucketThrottler::create(
            NUM_IP_BINS,
            TOTAL_TOKENS,
            TOKEN_REFRESH_WINDOW,
        ).await;

        let ip_admission = HashedAdmissionLimiter::create(
            NUM_IP_BINS,
            MAX_CONNECTIONS_PER_IP,
        ).await;

        Ok(Self {
            domain_name,
            config,
            meta_client,
            backends,
            ip_throttler,
            ip_admission,
        })
    }

    fn handle_connecting_impl(&self, context: &mut http::ServerConnectionContext) -> bool {
        let allowed = self.ip_throttler.take_with(context.peer.ip(), CONNECT_COST);
        if !allowed {
            eprintln!("Reject connection due to cost from {:?}", context.peer.ip());
            return false;
        }

        let mut connection_data = ConnectionData::default();

        // TODO: Consider doing this check on all ClusterServer instances.
        connection_data.ticket = match self.ip_admission.take_with(context.peer.ip()) {
            Some(v) => Some(v),
            None => {
                eprintln!("Too many connections from {:?}", context.peer.ip());
                return false;
            }
        };

        context.handler_data = Some(Arc::new(connection_data));
        true
    }

    async fn handle_connection_impl(&self, context: &mut http::ServerConnectionContext) -> bool {
        true
    }

    async fn handle_request_impl<'a>(
        &self,
        mut request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        match self.handle_request_with_result(request, context).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Request handling failed: {}", e);
                internal_server_error()
            }
        }
    }

    async fn handle_request_with_result<'a>(
        &self,
        mut request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> Result<http::Response> {
        let connection_data: &ConnectionData =
            context.connection_context.handler_data
            .as_ref().ok_or_else(|| err_msg("Missing connection data"))?
            .downcast_ref().ok_or_else(|| err_msg("Wrong connection data type"))?;

        let mut parsed_req = match self.parse_request(&request.head) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Bad Request: {}", e);
                return Ok(bad_request());
            }
        };

        // TODO: Need a max limit on request deadline unless a request is explicitly allowlisted (this will need to be implemented in the HTTP2 server).

        // NOTE: To limit the authentication rate, holding this limits us to one
        // concurrent user authentication check per connection.
        //
        // This also ensures that only one client id is assigned to each connection.
        let connection_state = connection_data.state.lock().await?.read_exclusive();

        // Figure out the client id for the request.
        if let Some(request_client_id) = &parsed_req.credentials.client_id {
            if let Some(conn_client_id) = &connection_state.client_id {
                if conn_client_id != request_client_id {
                    eprintln!("Different client ids seen on one client connection");
                    parsed_req.credentials.has_invalid_client_id = true;
                    parsed_req.credentials.client_id = Some(conn_client_id.clone());
                }
            }
        } else {
            parsed_req.credentials.client_id = Some(ClientId::generate().await);

            // To trigger sending back a new header.
            parsed_req.credentials.has_invalid_client_id = true;
        }

        let mut session = None;

        let mut now = Instant::now();

        // Maybe restore session from cache
        let mut same_auth_key = false;
        if let Some(auth_key) = &parsed_req.credentials.auth_key {
            if let Some(cached_session) = &connection_state.cached_session {
                if &cached_session.auth_key == auth_key {
                    same_auth_key = true;
                    if now.duration_since(cached_session.check_time) < SESSION_CACHE_DURATION {
                        session = Some(cached_session.session.clone());
                    }
                }
            }
        }

        let cost = {
            if same_auth_key {
                // NOTE: Reauthenticating after cache timeout is free.
                REQUEST_AUTH_COST
            } else if parsed_req.credentials.auth_key.is_some() {
                AUTHENTICATE_COST
            } else {
                REQUEST_UNAUTH_COST
            }
        };

        let allowed = self.ip_throttler.take_with(context.connection_context.peer.ip(), cost);
        if !allowed {
            eprintln!("Reject request due to cost from {:?}", context.connection_context.peer.ip());
            return ResponseBuilder::new()
                .status(http::status_code::TOO_MANY_REQUESTS)
                .build();
        }

        // TODO: Avoid checking user credentials if a backend is marked as needing no credentials (e.g. static file hosting).
        let mut newly_authenticated = false;
        if session.is_none() && parsed_req.credentials.auth_key.is_some() {
            session = self.authenticate_user(&mut parsed_req.credentials).await?;
            newly_authenticated = true;
        }

        // Caching client id and session in the connection state. 
        lock!(connection_state <= connection_state.upgrade(), {
            if connection_state.client_id.is_none() {
                connection_state.client_id = parsed_req.credentials.client_id.clone();
            }

            if newly_authenticated {
                if let Some(session) = &session {
                    connection_state.cached_session = Some(CachedSession {
                        auth_key: parsed_req.credentials.auth_key.clone().unwrap(),
                        session: session.clone(),
                        check_time: now
                    });
                } else {
                    connection_state.cached_session = None;
                }
            }
        });

        // This will be final response we return.
        // TODO: Ensure that past this point we are using this response builder only.
        let mut response_builder = http::ResponseBuilder::new();

        // NOTE: Both of these headers are only set after we finish storing their final values in the connection_state.
        if parsed_req.credentials.has_invalid_client_id {
            response_builder = self.set_cookie(CLIENT_ID_COOKIE,
                &parsed_req.credentials.client_id.as_ref().unwrap().to_string(), response_builder);
        }
        // Ask the client to delete its auth key if it is bad.
        if parsed_req.credentials.has_invalid_auth_key {
            response_builder = self.delete_cookie(AUTH_KEY_COOKIE, response_builder);
        }

        let backend = match self.backends.get(&parsed_req.sub_domain) {
            Some(v) => v,
            None => return response_builder.status(http::status_code::NOT_FOUND).build()
        };

        let entity = session.as_ref().map(|s| {
            ServiceName::for_user(self.meta_client.zone(), s.user_name()).unwrap()
        });

        let allowed = check_entity_allowed(
            entity.as_ref(),
            &backend.allowed_principals,
            self.meta_client.zone(),
            Some(self.meta_client.db()),
        )
        .await?;

        if !allowed {
            eprintln!("Rejecting request to: \"{}.\" from {:?}", parsed_req.sub_domain, entity);

            // When not logged in, redirect to the login page 
            if entity.is_none() {
                let mut requesting_web_page = false;
                if let Ok(Some(header)) = request.head.headers.get_one("Accept") {
                    if let Ok(v) = header.value.to_ascii_str() {
                        requesting_web_page = v.contains("text/html");
                    }
                }
                requesting_web_page &= request.head.method == http::Method::GET;

                if requesting_web_page {
                    let mut query = http::query::QueryParamsBuilder::new();
                    query.add(b"referer", request.head.uri.to_string()?.as_bytes());

                    // URL of the authentication/login page.
                    // NOTE: This will preserve whatever protocol/port was used in the request.
                    let mut auth_uri = request.head.uri.clone();
                    auth_uri.authority.as_mut().unwrap().host = http::uri::Host::Name(format!("auth.{}", self.domain_name));
                    auth_uri.query = Some(query.build().into());

                    // TODO: Dynamically find which backend is used for authentication.
                    return response_builder
                        .status(http::status_code::FOUND)
                        .header("Location", auth_uri.to_string()?)
                        .build();
                }
            }

            return response_builder.status(http::status_code::FORBIDDEN).build();
        }

        // Proxy the request to the backend.

        let mut inner_request_builder = http::RequestBuilder::new()
            .method(request.head.method)
            // TODO: Clear the host?
            .uri2(request.head.uri.clone())
            .accept_trailers(request.head.accepts_trailers);

        // TODO: X-Forwarded-For (needed for login metadata tracking if it don't do in on the frontend).

        for header in request.head.headers.raw_headers {
            if allow_header_passthrough(&header) {
                inner_request_builder = inner_request_builder.header2(header);
            }
        }

        // TODO: Wrap body to verify no trailers as sent in the request trailers
        inner_request_builder = inner_request_builder.body(request.body);

        // Forward identity of peer to the backend.
        inner_request_builder = inner_request_builder
            .header(FORWARDED_ENTITY_HEADER, {
                // TODO: Deduplicate this conversion.
                match &entity {
                    Some(v) => Principal::Entity(v.clone()),
                    None => Principal::Unauthenticated
                }.to_string()
            })
            .header(FORWARDED_IP_HEADER, context.connection_context.peer.ip().to_string());

        inner_request_builder = inner_request_builder
            .header(CLIENT_ID_HEADER,
                    parsed_req.credentials.client_id.as_ref().unwrap().to_string());

        if let Some(session) = &session {
            inner_request_builder = inner_request_builder
                .header(SESSION_ID_HEADER, base_radix::base64url_encode(&session.id().to_be_bytes()));
        }

        let inner_request = inner_request_builder.build()?;

        let inner_response = backend.client.request(
            inner_request,
            http::ClientRequestContext::default(),
            &mut http::ClientResponseContext::default(),
        )
        .await?;

        // TODO: Must wrap the body to filter trailers.
        response_builder = response_builder
            .status(inner_response.head.status_code)
            .body(inner_response.body);

        // TODO: Filtering and pass through of headers.
        for header in inner_response.head.headers.raw_headers {
            if AUTH_KEY_HEADER.eq_ignore_ascii_case(header.name.as_str()) {
                let value = header.value.to_utf8_str()?;
                if value == AUTH_KEY_DELETED_VALUE {
                    response_builder = self.delete_cookie(AUTH_KEY_COOKIE, response_builder);

                    lock!(connection_state <= connection_data.state.lock().await?, {
                        connection_state.cached_session = None;
                    });

                } else {
                    response_builder = self.set_cookie(AUTH_KEY_COOKIE, value, response_builder);
                }

                continue;
            }

            if allow_header_passthrough(&header) {
                response_builder = response_builder.header2(header);
            }
        }

        // TODO: We must prevent one sub domain from talking to another sub domain for security

        // TODO: Should we check that the user didn't authenticate over TLS with a different host name.

        // TODO: Responses from backends should not be allowed to set cookies.

        // TODO: How to safely forward forward/back gRPC metadata headers/trailers.

        // TODO: Limit max concurrent ACL checks per frontend worker

        response_builder.build()
    }

    fn set_binary_cookie(&self, key: &str, value: &[u8], response: ResponseBuilder) -> ResponseBuilder {
        let value = base_radix::base64url_encode(value);
        self.set_cookie(key, &value, response)
    }

    fn set_cookie(&self, key: &str, value: &str, response: ResponseBuilder) -> ResponseBuilder {
        response.header(
            SET_COOKIE,
            format!(
                "{key}={value}; HttpOnly; Secure; Domain=.{domain}; SameSite=Strict; Path=/",
                key = key,
                value = value,
                domain = self.domain_name
            )
        )
    }

    fn delete_cookie(&self, key: &str, response: ResponseBuilder) -> ResponseBuilder {
        response.header(
            SET_COOKIE,
            format!(
                "{key}=deleted; HttpOnly; Secure; Domain=.{domain}; SameSite=Strict; Path=/; Expires=Thu, 01 Jan 1970 00:00:00 GMT",
                key = key,
                domain = self.domain_name
            )
        )
    }

    /// Any errors from this function are expected to be due to the client
    /// providing a malformed request.
    ///
    /// For cookies, malformed values are treated as missing values. The caller is expected to
    /// clear or re-generate these if needed.
    fn parse_request(&self, request_head: &http::RequestHead) -> Result<ParsedRequest> {
        let authority = request_head.uri.authority.clone()
            .ok_or_else(|| err_msg("Request is missing an authority"))?;

        let host_name = match authority.host {
            http::uri::Host::Name(name) => name,
            _ => {
                return Err(format_err!("Unsupported Host header value: {:?}", authority));
            }
        };

        let sub_domain = {
            host_name.strip_suffix(&self.domain_name)
                .and_then(|p| {
                    if p.is_empty() {
                        Some(p)
                    } else {
                        p.strip_suffix(".")
                    }
                })
                .ok_or_else(|| format_err!("Unknown host name: {}", host_name))?
        };

        let cookies = parse_cookie_header(&request_head.headers)
            .map_err(|e| format_err!("Failed to parse request cookies: {}", e))?;

        let mut auth_key = None;
        let mut client_id = None;

        let mut has_invalid_auth_key = false;
        let mut has_invalid_client_id = false;
        for cookie in cookies {
            // TODO: Complain if given duplicate cookies (this may happen by accident though if different cookies are set on different paths or domains that overlap)

            if cookie.name == AUTH_KEY_COOKIE {
                match Self::parse_binary_cookie(&cookie.value, AUTH_KEY_LEN) {
                    Ok(v) => {
                        auth_key = Some(v);
                    }
                    Err(e) => {
                        eprintln!("Invalid Auth-Key cookie received: {}", e);
                        has_invalid_auth_key = true;
                        auth_key = None;
                    }
                };
            } else if cookie.name == CLIENT_ID_COOKIE {
                match Self::parse_u64_cookie(&cookie.value) {
                    Ok(v) => {
                        client_id = Some(ClientId(v));
                    }
                    Err(e) => {
                        eprintln!("Invalid Client-Id cookie received: {}", e);
                        has_invalid_client_id = true;
                        client_id = None;
                    }
                };
            }
        }

        Ok(ParsedRequest {
            sub_domain: sub_domain.to_string(),
            credentials: RequestCredentials {
                auth_key,
                client_id,
                has_invalid_auth_key,
                has_invalid_client_id
            }
        })
    }

    fn parse_binary_cookie(data: &str, binary_length: usize) -> Result<Vec<u8>> {
        let data = base_radix::base64url_decode(data)?;
        if data.len() != binary_length {
            return Err(format_err!("Incorrect length. Length: {}", data.len()));
        }

        Ok(data)
    }

    fn parse_u64_cookie(data: &str) -> Result<u64> {
        let data = Self::parse_binary_cookie(data, 8)?;
        Ok(u64::from_be_bytes(*array_ref![data, 0, 8]))
    }

    /// NOTE: This function needs to be hardened againt timing leaks.
    async fn authenticate_user(&self, credentials: &mut RequestCredentials) -> Result<Option<Session>> {
        let auth_key = match &credentials.auth_key {
            Some(v) => &v[..],
            None => return Ok(None)
        };

        let client_id = match &credentials.client_id {
            Some(v) => *v,
            None => return Ok(None)
        };

        let db = self.meta_client.db();

        let auth_key_hash = create_session_auth_key_hash(auth_key);
        let session = match query_one!(db, SessionTable, "auth_key_hash = ?", auth_key_hash) {
            Some(v) => v,
            None => {
                eprintln!("Unknown auth key given");
                credentials.has_invalid_auth_key = true;
                return Ok(None);
            }
        };

        if session.deleted() {
            // Logged out.
            credentials.has_invalid_auth_key = true;
            return Ok(None);
        }

        if session.client_id() != client_id.0 {
            eprintln!("Mismatching client id received");
            credentials.has_invalid_auth_key = true;
            return Ok(None);
        }

        Ok(Some(session))
    }
}

#[async_trait]
impl http::ServerHandler for FrontendHttpHandler {
    fn handle_connecting(&self, context: &mut http::ServerConnectionContext) -> bool {
        self.handle_connecting_impl(context)
    }

    async fn handle_connection(&self, context: &mut http::ServerConnectionContext) -> bool {
        self.handle_connection_impl(context).await
    }

    async fn handle_request<'a>(
        &self,
        request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        self.handle_request_impl(request, context).await
    }
}
