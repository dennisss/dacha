extern crate common;
#[macro_use]
extern crate macros;

use std::sync::Arc;

use common::errors::*;
use google_auth::*;

#[executor_main]
async fn main() -> Result<()> {
    let data =
        file::read_to_string("/home/dennis/.credentials/dacha-main-748d2acba112.json").await?;

    let sa: Arc<GoogleServiceAccount> =
        Arc::new(google_auth::GoogleServiceAccount::parse_json(&data)?);

    let rest_client = Arc::new(google_auth::GoogleRestClient::create(sa.clone())?);

    let client = google_storage::Client::new(rest_client)?;

    println!("Start upload");

    client
        .upload("da-sources", "test.txt", http::BodyFromData("hello world"))
        .await?;

    let mut body = client.download("da-sources", "test.txt").await?;
    let mut out = vec![];
    body.read_to_end(&mut out).await?;

    assert_eq!(&out, b"hello world");

    println!("{:?}", client.get("da-sources", "test.txt").await?);

    // client.get("da-sources", "nonexistent.txt").await?;

    Ok(())
}
