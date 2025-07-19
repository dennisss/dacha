use common::errors::*;
use common::bytes::Bytes;
use net::ip::IPAddress;
use json::ValuePath;

pub async fn public_ip() -> Result<IPAddress> {
    let client = http::SimpleClient::new(http::SimpleClientOptions::default());

    let req = http::RequestBuilder::new()
        .method(http::Method::GET)
        .uri("https://whats-my-ip-922444230686.us-west2.run.app/")
        .build()?;

    let res = client
        .request(
            &req.head,
            Bytes::new(),
            &http::ClientRequestContext::default(),
        )
        .await?;

    if !res.ok() {
        return Err(format_err!(
            "Request failed: {:?}: {:?}",
            res.head.status_code,
            res.body
        ));
    }

    let obj = json::parse(std::str::from_utf8(&res.body)?)?;
    
    let ip_str = obj.get_field("ip").and_then(|v| v.get_string())
        .ok_or_else(|| err_msg("Unexpected output format"))?;

    Ok(ip_str.parse()?)
}