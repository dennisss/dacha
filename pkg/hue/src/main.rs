#[macro_use]
extern crate common;
extern crate http;
extern crate json;
extern crate net;
#[macro_use]
extern crate macros;
extern crate reflection;

extern crate hue;

use std::time::Duration;

use common::async_std::task;
use common::errors::*;
use hue::*;

/*
#[derive(Parseable, Debug)]
struct Group {
    name: String,
    #[parse(name = "type")]
    typ: String,
    lights: Vec<String>,
    sensors: Vec<String>,
    action: GroupAction,
    state: GroupState,

    // state: State,
    // presence:
    // lightlevel
    recycle: bool,
}

#[derive(Parseable, Debug)]
struct GroupAction {
    on: Option<bool>,
    bri: Option<u8>,
    hue: Option<u16>,
    sat: Option<u8>,
    xy: Option<Vec<f32>>,
    ct: Option<u16>,
    alert: Option<String>,
    effect: Option<String>,
    transitiontime: Option<u16>,
    bri_inc: Option<isize>,
    sat_inc: Option<isize>,
    hue_inc: Option<isize>,
    ct_inc: Option<isize>,
    xy_inc: Option<f32>,
    scene: Option<String>,
}

#[derive(Parseable, Debug)]
struct GroupState {
    all_on: bool,
    all_off: bool,
}

*/

async fn run() -> Result<()> {
    let client = AnonymousHueClient::create().await?;

    let client = HueClient::create(client, "xxx");

    /*
    Issues with keeping a persistent HTTP v1 connection:
    - Without any packets sent, the server will timeout the connection after a few seconds.

    Solutions:
    - Keep the connection stable:
        - Can set the socket TCP_KEEPIDLE and SO_KEEPALIVE socket options.
    - Just retry the requests
    - Limit the maximum amount of time for which v1 connections are held open.

    */

    loop {
        println!("{:?}", client.get_groups().await?);
        println!("{:?}", client.get_groups().await?);
        common::async_std::task::sleep(Duration::from_secs(10)).await;
    }

    // client.create_user("hue_client", "cluster").await?;

    // println!("{:#?}", groups_by_id);

    // client.set_group_on("1", false).await?;

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}
