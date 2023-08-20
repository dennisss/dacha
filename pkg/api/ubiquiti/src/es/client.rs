use std::{convert::TryFrom, str::FromStr};

use common::errors::*;
use http::uri::Uri;
use http::ClientInterface;
use parsing::ascii::AsciiString;
use reflection::{ParseFrom, SerializeTo};

pub struct EdgeSwitchClient {
    uri: Uri,
    client: http::Client,
    auth_token: Option<String>,
}

impl EdgeSwitchClient {
    /// Creates a client from a remote URI
    pub fn new(uri: Uri, client: http::Client) -> Self {
        Self {
            uri,
            client,
            auth_token: None,
        }
    }

    pub async fn login(&mut self, username: &str, password: &str) -> Result<()> {
        let (_, head) = self
            .make_request(
                http::Method::POST,
                "/api/v1.0/user/login",
                Some(&Login {
                    username: username.to_string(),
                    password: password.to_string(),
                }),
            )
            .await?;

        let auth_token = head
            .headers
            .find_one("x-auth-token")?
            .value
            .to_ascii_str()?;
        self.auth_token = Some(auth_token.to_string());
        Ok(())
    }

    pub async fn get_interfaces(&self) -> Result<json::Value> {
        let (v, _) = self
            .make_request::<json::Value>(http::Method::GET, "/api/v1.0/interfaces", None)
            .await?;
        Ok(v)
    }

    pub async fn put_interfaces(&self, value: &json::Value) -> Result<json::Value> {
        let (v, _) = self
            .make_request::<json::Value>(http::Method::PUT, "/api/v1.0/interfaces", Some(value))
            .await?;
        Ok(v)
    }

    async fn make_request<Input: SerializeTo>(
        &self,
        method: http::Method,
        path: &str,
        value: Option<&Input>,
    ) -> Result<(json::Value, http::ResponseHead)> {
        let mut request_builder = http::RequestBuilder::new()
            .method(method)
            .path(path)
            .header("Accept", "application/json")
            .header("Origin", {
                // Will have a value of the form "https://hostname"
                let mut u = self.uri.clone();
                u.path = AsciiString::from("").unwrap();
                u.to_string()?
            })
            .header("Referer", {
                // Will have a value of the form "https://hostname/"
                let mut u = self.uri.clone();
                u.path = AsciiString::from("/").unwrap();
                u.to_string()?
            });

        if let Some(auth_token) = &self.auth_token {
            request_builder = request_builder.header("x-auth-token", auth_token.as_str());
        }

        if let Some(value) = value {
            request_builder = request_builder
                .body(http::BodyFromData(json::stringify(value)?))
                .header(http::header::CONTENT_TYPE, "application/json");
        }

        let request = request_builder.build()?;

        let mut response = self.client.request(request).await?;

        let response_body = {
            let mut data = vec![];
            response.body.read_to_end(&mut data).await?;
            String::from_utf8(data)?
        };

        if !response.ok() {
            return Err(format_err!(
                "Request failed: {:?}: {}",
                response.head.status_code,
                response_body
            ));
        }

        // TODO: Ignore any charset specifier in the header
        let content_type = response
            .head
            .headers
            .get_one(http::header::CONTENT_TYPE)?
            .ok_or_else(|| err_msg("Response missing content type"))?
            .value
            .to_ascii_str()?;

        if content_type != "application/json" {
            return Err(format_err!(
                "Expected response to be json, instead got: {}",
                content_type
            ));
        }

        Ok((json::parse(&response_body)?, response.head))
    }
}

#[derive(Parseable)]
struct Login {
    // TODO: Change these to '&'a str'
    username: String,
    password: String,
}
