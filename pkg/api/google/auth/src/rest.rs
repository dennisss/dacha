use std::convert::TryFrom;
use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;
use http::query::QueryParamsBuilder;
use parsing::ascii::AsciiString;
use reflection::{ParseFrom, SerializeTo};

use crate::{GoogleServiceAccount, GoogleServiceAccountOAuth2Credentials};

/// Base client library for querying Google REST APIs.
///
/// Normally you won't use this directly but rather use it indirectly via one of
/// the autogenerated discovery client libraries.
pub struct GoogleRestClient {
    http_client: http::SimpleClient,
    credentials: GoogleServiceAccountOAuth2Credentials,
}

impl GoogleRestClient {
    pub fn create(service_account: Arc<GoogleServiceAccount>) -> Result<Self> {
        let http_client = http::SimpleClient::new(http::SimpleClientOptions::default());

        let credentials = GoogleServiceAccountOAuth2Credentials::create(
            service_account,
            &[
                // Catch all scope.
                "https://www.googleapis.com/auth/cloud-platform",
            ],
        )?;

        Ok(Self {
            http_client,
            credentials,
        })
    }

    pub async fn request_json<Request: SerializeTo, Response: for<'a> ParseFrom<'a>>(
        &self,
        method: http::Method,
        url: &str,
        query: &str,
        request_body: &Request,
    ) -> Result<Response> {
        let mut uri = http::uri::Uri::try_from(url)?;
        if uri.query.is_some() {
            return Err(err_msg("Did not expect the uri to already contain a query"));
        }

        if !query.is_empty() {
            uri.query = Some(AsciiString::new(query));
        }

        let request = http::RequestBuilder::new()
            .method(method)
            .uri2(uri)
            .header(
                "Authorization",
                self.credentials.get_authorization_value().await?,
            )
            .header("Content-Type", "application/json; charset=UTF-8")
            .build()?;

        let body = Bytes::from(json::stringify(request_body)?);

        let res = self
            .http_client
            .request(&request.head, body, &http::ClientRequestContext::default())
            .await?;

        // TODO: Parse the error payload as in https://cloud.google.com/apis/design/errors
        // For this we will need to support parsing Any protos from JSON.
        if res.head.status_code != http::status_code::OK {
            return Err(format_err!("RPC failed: {:?}", res.body));
        }

        let value = json::parse(std::str::from_utf8(&res.body)?)?;

        let object = Response::parse_from(json::ValueParser::new(&value))?;

        Ok(object)
    }

    /// Performs a media upload (mainly relevant to GCS object uploads).
    ///
    /// 1. For small objects that don't require any special metadata we will use
    /// a single request with a single part body to upload it.
    ///
    /// TODO: Implement these two as well:
    /// 2. For small objects that do require special metadata we will use a
    /// single request with a multipart body.
    ///
    /// 3. Else, use a resumable upload.
    ///
    /// References:
    /// - Single request: https://cloud.google.com/storage/docs/uploading-objects#uploading-an-object
    /// - Resumable: https://cloud.google.com/storage/docs/performing-resumable-uploads
    pub async fn request_upload<Request: SerializeTo, Response: for<'a> ParseFrom<'a>>(
        &self,
        method: http::Method,
        simple_url: &str,
        resumable_url: &str,
        mut query_builder: QueryParamsBuilder,
        content_type: &str,
        request_body: &Request,
        data: Box<dyn http::Body>,
    ) -> Result<Response> {
        // TODO:
        // - check size of 'data' to see if a resumable upload is needed.
        // - check if request_body has non-trivial data in it (doesn't just serialize to
        //   '{}')

        query_builder.add(b"uploadType", b"media");

        ////
        // TODO: Debug this part with request_json

        let mut uri = http::uri::Uri::try_from(simple_url)?;
        if uri.query.is_some() {
            return Err(err_msg("Did not expect the uri to already contain a query"));
        }

        let query = query_builder.build();
        if !query.as_str().is_empty() {
            uri.query = Some(query);
        }

        let request = http::RequestBuilder::new()
            .method(method)
            .uri2(uri)
            .header(
                "Authorization",
                self.credentials.get_authorization_value().await?,
            )
            .header("Content-Type", content_type)
            .body(data)
            .build()?;

        let mut res = self
            .http_client
            .request_raw(request, http::ClientRequestContext::default())
            .await?;

        // TODO: Parse the error payload as in https://cloud.google.com/apis/design/errors
        // For this we will need to support parsing Any protos from JSON.
        if res.head.status_code != http::status_code::OK {
            return Err(err_msg("RPC failed!"));
        }

        // TODO: Check for max response size.
        let mut s = String::new();
        res.body.read_to_string(&mut s).await?;

        let value = json::parse(&s)?;

        let object = Response::parse_from(json::ValueParser::new(&value))?;

        Ok(object)
    }
}
