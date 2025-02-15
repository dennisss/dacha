#[macro_use]
extern crate macros;

use std::convert::TryFrom;
use std::sync::Arc;

use base_error::*;
use google_auth::GoogleServiceAccount;
use http::uri::Uri;
use protobuf::{Enum, EnumReflection};

// TODO: Document what scopes are needed.

#[executor_main]
async fn main() -> Result<()> {
    let data = file::read_to_string("/home/dennis/.credentials/da-cha-c2d195c05521.json").await?;

    let service_account: Arc<GoogleServiceAccount> =
        Arc::new(google_auth::GoogleServiceAccount::parse_json(&data)?);

    let client = gemini::GeminiClient::create(service_account).await?;

    let res = client.generate_text("What is the color of the sky?", None).await?;

    println!("{}", res);

    Ok(())
}
