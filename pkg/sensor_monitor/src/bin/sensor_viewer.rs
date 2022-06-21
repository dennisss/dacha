#[macro_use]
extern crate common;
extern crate graphics;
extern crate image;
extern crate crypto;
extern crate sensor_monitor;
extern crate rpc;
extern crate math;

use std::sync::Arc;
use std::rc::Rc;

use common::errors::*;
use image::Color;
use math::matrix::vec2f;
use graphics::canvas::Paint;
use graphics::font::{CanvasFontRenderer, OpenTypeFont};
use graphics::ui;
use graphics::ui::chart::*;
use sensor_monitor::proto::data::*;

use sensor_monitor::viewer::MetricViewer;

async fn run() -> Result<()> {
    let channel = Arc::new(rpc::Http2Channel::create(http::ClientOptions::from_uri(
        &"http://127.0.0.1:8001".parse()?,
    )?)?);

    let stub = MetricStub::new(channel);

    let font_family = Rc::new(CanvasFontRenderer::new(
        OpenTypeFont::read(project_path!("testdata/noto-sans.ttf")).await?,
    ));

    let root_el = ui::Element::from(MetricViewer {
        font_family,
        metric_stub: stub
    });

    ui::render_element(root_el, 800, 1000).await
}

fn main() -> Result<()> {
    common::async_std::task::block_on(run())
}