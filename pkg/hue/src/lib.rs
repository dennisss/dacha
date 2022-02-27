#[macro_use]
extern crate common;
extern crate http;
extern crate json;
extern crate net;
#[macro_use]
extern crate macros;
extern crate reflection;

use std::collections::HashMap;

use common::async_std::task;
use common::errors::*;
use http::ClientInterface;
use parsing::ascii::AsciiString;
use reflection::ParseFrom;

/// Client instance for communicating with a Hue Bridge without any
/// authentication.
pub struct AnonymousHueClient {
    client: http::Client,
}

impl AnonymousHueClient {
    pub async fn create() -> Result<Self> {
        let mut dns = net::dns::Client::create_multicast_insecure().await?;
        let (addr, _) = dns.resolve_service_addr("_hue._tcp.local.").await?;

        let client = http::Client::create(http::ClientOptions::from_uri(&http::uri::Uri {
            scheme: Some(AsciiString::from("http")?),
            authority: Some(http::uri::Authority {
                user: None,
                host: http::uri::Host::IP(addr),
                port: Some(80),
            }),
            path: AsciiString::from("")?,
            query: None,
            fragment: None,
        })?)?;

        Ok(Self { client })
    }

    async fn request(
        &self,
        method: http::Method,
        path: &str,
        request_body: Option<&json::Value>,
    ) -> Result<json::Value> {
        let request_body = match request_body {
            Some(value) => json::stringify(value)?,
            None => String::new(),
        };

        let request = http::RequestBuilder::new()
            .method(method)
            .path(path)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(http::BodyFromData(request_body))
            .build()?;

        let context = http::ClientRequestContext::default();

        let mut response = self.client.request(request, context).await?;

        let mut response_body = vec![];
        response.body.read_to_end(&mut response_body).await?;

        // NOTE: API errors will have a 200 OK status code but will have the actual
        // error in the json response.
        if !response.ok() {
            return Err(format_err!(
                "Request failed: {:?}: {:?}",
                response.head.status_code,
                common::bytes::Bytes::from(response_body)
            ));
        }

        // TODO: How can we check for errors in a standard way.
        let response_obj = json::parse(std::str::from_utf8(&response_body)?)?;

        // println!("{:#?}", response_obj);

        Ok(response_obj)
    }

    /// Creates a new user on the connected hue bridge that can be used to make
    /// authenticated requests.
    ///
    /// NOTE: This must be run interactively for a human user as the link button
    /// on the bridge must be physically pressed within 30 seconds of calling
    /// this for a successful response to be returned.
    ///
    /// Returns the username (a secret string).
    pub async fn create_user(&self, application_name: &str, device_name: &str) -> Result<String> {
        println!("Please press the link button...");

        let response = self.request(http::Method::POST, "/api", Some(&json::Value::Object(map! {
            "devicetype" => &json::Value::String(format!("{}#{}", application_name, device_name))
        }))).await?;

        Ok(String::new())
    }
}

/// Client instance for communicating with a Hue Bridge which is configured with
/// a username so can make authenticated requests.
pub struct HueClient {
    inner_client: AnonymousHueClient,
    username: String,
}

impl HueClient {
    pub fn create(inner_client: AnonymousHueClient, username: &str) -> Self {
        Self {
            inner_client,
            username: username.to_string(),
        }
    }

    /// Retrieves all groups defined on the bridge.
    ///
    /// Returns a map from group id to group entity value.
    pub async fn get_groups(&self) -> Result<HashMap<String, Group>> {
        let res = self
            .inner_client
            .request(
                http::Method::GET,
                &format!("/api/{}/groups", self.username),
                None,
            )
            .await?;

        let obj = match res {
            json::Value::Object(obj) => obj,
            _ => {
                return Err(err_msg("Expected map from group id to object"));
            }
        };

        let mut groups_by_id = HashMap::new();

        for (id, value) in obj {
            // let parser = json::ValueParser::new(&value);
            // let group = Group::parse_from(parser)?;

            let name = value
                .get_field("name")
                .and_then(|v| v.get_string())
                .ok_or_else(|| err_msg("Bad group 'name'"))?
                .to_string();

            let all_on = value
                .get_field("state")
                .and_then(|v| v.get_field("all_on"))
                .and_then(|v| v.get_bool())
                .ok_or_else(|| err_msg("Bad group 'all_on'"))?;

            groups_by_id.insert(id, Group { name, all_on });
        }

        Ok(groups_by_id)
    }

    pub async fn set_group_on(&self, id: &str, on: bool) -> Result<()> {
        self.inner_client
            .request(
                http::Method::PUT,
                &format!("/api/{}/groups/{}/action", self.username, id),
                Some(&json::Value::Object(map! {
                    "on" => &json::Value::Bool(on)
                })),
            )
            .await?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Group {
    pub name: String,
    pub all_on: bool,
}
