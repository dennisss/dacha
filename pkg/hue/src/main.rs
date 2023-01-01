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

#[derive(Args)]
enum Args {
    #[arg(name = "create_user")]
    CreateUser {
        application_name: String,
        device_name: String,
    },
    #[arg(name = "poll_state")]
    PollState { username: String },
}

async fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let client = AnonymousHueClient::create().await?;

    match args {
        Args::CreateUser {
            application_name,
            device_name,
        } => {
            let user = client.create_user(&application_name, &device_name).await?;
            println!("Created username: {}", user);
        }
        Args::PollState { username } => {
            let client = HueClient::create(client, &username);

            loop {
                println!("{:?}", client.get_groups().await?);
                executor::sleep(Duration::from_secs(20)).await;
            }
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    executor::run(run())?
}
