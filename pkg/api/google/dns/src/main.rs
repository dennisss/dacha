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

    let client = google_dns::Client::new(sa.project_id(), rest_client)?;

    client
        .set_txt_record("testdata.dacha.page.", 300, &["123"])
        .await?;

    Ok(())
}
