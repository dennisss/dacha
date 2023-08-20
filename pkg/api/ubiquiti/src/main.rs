#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
extern crate http;
extern crate json;
extern crate parsing;
extern crate reflection;

use std::{convert::TryFrom, str::FromStr};

use common::errors::*;
use http::uri::Uri;
use http::ClientInterface;
use parsing::ascii::AsciiString;
use reflection::{ParseFrom, SerializeTo};

use ubiquiti::es::EdgeSwitchClient;

#[derive(Args)]
struct Args {
    addr: String,
    user: String,
    password: String,
}

/*
"port.poe", true
*/

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let mut uri = http::uri::Uri::from_str(args.addr.as_str())?;

    let mut client_options = http::ClientOptions::from_uri(&uri)?;
    client_options
        .backend_balancer
        .backend
        .tls
        .as_mut()
        .unwrap()
        .certificate_request
        .trust_remote_certificate = true;

    let client = http::Client::create(client_options)?;

    let mut client = EdgeSwitchClient::new(uri, client);

    client.login(&args.user, &args.password).await?;

    let ifaces = client.get_interfaces().await?;

    for iface in ifaces
        .get_elements()
        .ok_or_else(|| err_msg("Not an array"))?
    {
        // TODO: Filter to "identification.type" == "port"

        let id = iface["identification"]["id"].get_string().unwrap();
        let poe = match iface.get_field("port").and_then(|v| v.get_field("poe")) {
            Some(v) => v.get_string().unwrap(),
            None => "n/a",
        };

        println!("ID: {}, POE: {}", id, poe);
    }

    /*
    PoE can be either "off" or "active" (PoE+)
    */

    let mut iface0 = ifaces.get_element(0).unwrap().clone();
    iface0["port"]["poe"] = json::Value::String("off".to_string()).into();

    let res = client
        .put_interfaces(&json::Value::Array(vec![iface0]))
        .await?;
    // println!("PUT INTERFACES RESULT: {:#?}", res);

    Ok(())
}
