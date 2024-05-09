#[macro_use]
extern crate common;
extern crate http;
extern crate json;
extern crate net;
#[macro_use]
extern crate macros;
extern crate reflection;

use std::collections::HashMap;

use common::errors::*;
use http::ClientInterface;
use json::ValuePath;
use parsing::ascii::AsciiString;
use reflection::ParseFrom;

/// Client instance for communicating with a Hue Bridge without any
/// authentication.
pub struct AnonymousHueClient {
    client: http::SimpleClient,
    base_url: http::uri::Uri,
}

impl AnonymousHueClient {
    pub async fn create() -> Result<Self> {
        let mut dns = net::dns::Client::create_multicast_insecure().await?;
        let (addr, _) = dns.resolve_service_addr("_hue._tcp.local.").await?;

        let base_url = http::uri::Uri {
            scheme: Some(AsciiString::from("http")?),
            authority: Some(http::uri::Authority {
                user: None,
                host: http::uri::Host::IP(addr),
                port: Some(80),
            }),
            path: AsciiString::new(""),
            query: None,
            fragment: None,
        };

        let client = http::SimpleClient::new(http::SimpleClientOptions::default());

        Ok(Self { client, base_url })
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

        let mut url = self.base_url.clone();
        url.path = AsciiString::new(path); // TODO: check is ascii.

        let request = http::RequestBuilder::new()
            .method(method)
            .uri2(url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .build()?;

        let context = http::ClientRequestContext::default();

        let mut response = self
            .client
            .request(&request.head, request_body.into(), &context)
            .await?;

        // NOTE: API errors will have a 200 OK status code but will have the actual
        // error in the json response.
        if !response.ok() {
            return Err(format_err!(
                "Request failed: {:?}: {:?}",
                response.head.status_code,
                response.body
            ));
        }

        let response_obj = json::parse(std::str::from_utf8(&response.body)?)?;

        // See https://developers.meethue.com/develop/hue-api/error-messages/
        // While not formally documented, failures seem to always be returned as an
        // array of objects regardless of what the normal response type of the request
        // is.
        if let Some(els) = response_obj.get_elements() {
            for el in els {
                if let Some(err) = el.get_field("error") {
                    if let Some(s) = err.get_field("description").and_then(|v| v.get_string()) {
                        return Err(format_err!("Hue request failed: {}", s));
                    }

                    return Err(err_msg("Hue request failed with unknown error."));
                }
            }
        }

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

        let username = response
            .path("$[0].success.username")?
            .and_then(|v| v.get_string())
            .ok_or_else(|| err_msg("Unexpected output format"))?;

        Ok(username.to_string())
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
