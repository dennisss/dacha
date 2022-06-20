pub mod image_viewer;
pub mod shopping_list;

use std::rc::Rc;
use std::sync::Arc;

use common::errors::*;
use image::Color;

use crate::font::{CanvasFontRenderer, OpenTypeFont};
use crate::ui::element::Element;
use crate::ui::examples::shopping_list::ShoppingList;
use crate::ui::render::render_element;

pub async fn run() -> Result<()> {
    const HEIGHT: usize = 650;
    const WIDTH: usize = 800;
    const SCALE: usize = 4;

    let font = Rc::new(CanvasFontRenderer::new(
        OpenTypeFont::read(project_path!("testdata/noto-sans.ttf")).await?,
    ));
    let red = Color::rgb(255, 0, 0);
    let blue = Color::rgb(0, 0, 255);
    let white = Color::rgb(255, 255, 255);
    let black = Color::rgb(0, 0, 0);

    let root_el = Element::from(ShoppingList { font });

    render_element(root_el, HEIGHT, WIDTH).await?;

    Ok(())
}
