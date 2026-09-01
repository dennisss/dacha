use std::sync::Arc;
use std::collections::HashMap;
use std::time::Instant;

use common::bytes::Bytes;
use common::hash::FastHasherBuilder;
use common::errors::*;
use webview::*;
use mocap_proto::mocap::*;
use mocap_manager::*;
use mocap_manager::side_channel::*;
use file::{project_path, LocalPath, LocalPathBuf};
use executor::channel::spsc;
use executor::sync::SyncMutex;
use image::format::qoi::QOIDecoder;


use crate::protocol::*;
use crate::config::*;
use crate::http_server::*;

pub struct MocapApp {
    shared: Arc<Shared>
}

/*
TODOs:
- Give the app a secret key and the port it is running on.
*/

struct Shared {
    http_server: AppHttpServer,
}

impl MocapApp {

    pub async fn create(data_dir: LocalPathBuf) -> Result<()> {
        let config = read_base_config(&data_dir).await?;

        let manager = Arc::new(MocapManager::create(
            config,
            data_dir.clone(),
        ).await?);

        let side_channel = Arc::new(DataSideChannel::create());
        manager.set_side_channel(side_channel.clone()).await?;

        let http_server = AppHttpServer::create(&data_dir, manager.to_service(), side_channel).await?;
        let http_port = http_server.port();

        let shared = Arc::new(Shared {
            http_server,
        });

        /*
        TODO
        pub fn with_cookie(mut self, name: &str, value: &str, domain: &str, path: &str, http_only: bool, secure: bool) -> Self {
        */

        // TODO: Should also exit on internal manager failures.

        WebViewBuilder::new("Mocap Manager", 1920, 1080)
            .load_url(&format!("http://127.0.0.1:{}", http_port))
            .with_icon(Self::load_icon().await?)
            // .with_devtools(true)
            // .with_devtools_auto_open(true)
            .with_prefer_dark_theme(true)
            .with_user_data_dir(data_dir.join("webview").to_str().unwrap())
            .run()?;

        Ok(())
    }

    async fn load_icon() -> Result<webview::Icon> {
        let data = file::read_asset("out/mocap_app/icons/icon.qoi").await?;
        let image = QOIDecoder::new().decode(&data)?;

        Ok(webview::Icon {
            width: image.width() as u32,
            height: image.height() as u32,
            rgba: image.array.data
        })
    }
}