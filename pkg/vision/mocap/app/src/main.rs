#![windows_subsystem = "windows"]

#[macro_use]
extern crate macros;

use std::sync::Arc;
use std::collections::HashMap;

use common::hash::FastHasherBuilder;
use common::errors::*;
use webview::*;
use mocap_proto::mocap::*;
use mocap_manager::*;
use file::{project_path, LocalPath, LocalPathBuf};
use executor::channel::spsc;
use executor::sync::SyncMutex;
use executor::child_task::ChildTask;
use reflection::ParseFrom;
use rpc::Channel;
use protobuf::Message;
use crypto::hasher::Hasher;

use mocap_app::*;


include!(concat!(env!("OUT_DIR"), "/register_assets.rs"));


async fn main_inner() -> Result<()> {
    register_assets()?;

    let args = common::args::parse_args::<MocapAppArgs>()?;
    MocapApp::create(args).await?;

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    let res = main_inner().await;
    
    // TODO: Only do if running in UI mode.
    if let Err(e) = &res {
        webview::show_error_dialog("Mocap Internal Error", &e.to_string());
    }

    res
}
