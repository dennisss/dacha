#[macro_use]
extern crate macros;

use std::convert::TryFrom;
use std::sync::Arc;

use base_error::*;
use google_auth::GoogleServiceAccount;
use http::uri::Uri;

// TODO: Document what scopes are needed.

const PRODUCTION_TARGET: &'static str = "https://vision.googleapis.com";

use googleapis_proto::google::cloud::vision::v1::*;

#[executor_main]
async fn main() -> Result<()> {
    let data = file::read_to_string("/home/dennis/.credentials/da-cha-c2d195c05521.json").await?;

    let service_account: Arc<GoogleServiceAccount> =
        Arc::new(google_auth::GoogleServiceAccount::parse_json(&data)?);

    let service_uri: Uri = Uri::try_from(PRODUCTION_TARGET)?;

    let creds = google_auth::GoogleServiceAccountJwtCredentials::create(
        service_uri.clone(),
        service_account.clone(),
    )?;

    let mut channel_options =
        rpc::Http2ChannelOptions::try_from(http::ClientOptions::from_uri(&service_uri)?)?;
    channel_options.credentials = Some(Box::new(creds));

    let channel = Arc::new(rpc::Http2Channel::create(channel_options).await?);

    let stub = ImageAnnotatorStub::new(channel.clone());

    let image =
        file::read("/home/dennis/Pictures/Screenshots/Screenshot from 2025-02-08 19-47-41.png")
            .await?;

    let mut request = BatchAnnotateImagesRequest::default();

    let mut req = request.new_requests();
    req.image_mut().set_content(image);
    // TODO: Maybe use DOCUMENT_TEXT_DETECTION?
    req.new_features().set_typ(Feature_Type::TEXT_DETECTION);

    let response = stub
        .BatchAnnotateImages(&rpc::ClientRequestContext::default(), &request)
        .await
        .result?;

    println!("{:#?}", response);

    if response.responses().len() != 1 {
        return Err(err_msg("Expected one response"));
    }

    // if response.responses()[0].full_text_annotation().pages().len() != 1 {
    //     return Err(err_msg("Expected one page to be detected"));
    // }

    println!(
        "Text: {}",
        response.responses()[0].full_text_annotation().text()
    );

    // Raw text is in "response."

    // for annotation in response.responses()[0].text_annotations() {
    //     println!("=> {}", annotation.description());
    // }

    /*
    Usually the

    */

    // for annotation in

    Ok(())
}
