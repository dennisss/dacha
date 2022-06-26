// TODO: Rename to TextInput.

/*
TODO: Things to support:
- Focus (with special appearance)
- Disabled mode (greyed out background)
- Scrolling past end if too much text is present
- Placeholder
- Selection
*/

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::errors::*;
use image::Color;
use math::matrix::Vector2f;

use crate::canvas::*;
use crate::font::{CanvasFontRenderer, FontStyle, VerticalAlign};
use crate::ui::event::*;
use crate::ui::view::*;
use crate::ui::virtual_view::*;
use crate::ui::core::text::TextViewParams;
use crate::ui::core::block::*;
use crate::ui::element::*;

const BORDER_SIZE: f32 = 1.;
const PADDING_SIZE: f32 = 10.;

#[derive(Clone)]
pub struct TextboxParams {
    pub value: String,
    pub font: Rc<CanvasFontRenderer>,
    pub font_size: f32,
    pub on_change: Option<Arc<dyn Fn(String)>>,
}

impl VirtualViewParams for TextboxParams {
    type View = Textbox;
}

/// TODO: Convert this to a VirtualView that re-uses the Text view's logic.
pub struct Textbox {
    params: TextboxParams,
}

impl VirtualView for Textbox {
    type Params = TextboxParams;

    fn create_with_params(params: &Self::Params) -> Result<Self> {
        Ok(Self {
            params: params.clone(),
        })
    }

    fn update_with_params(&mut self, new_params: &Self::Params) -> Result<()> {
        self.params = new_params.clone();
        Ok(())
    }

    fn build_element(&mut self) -> Result<Element> {
        let background_color = Color::rgb(0xff, 0xff, 0xff);
        let border_color = Color::rgb(0xcc, 0xcc, 0xcc);
        let font_color = Color::rgb(0, 0, 0);

        // TODO: Make sure that the block stretches out the cursor and click box of the inner text.
        // TODO: This must expand to the full width.
        Ok(BlockViewParams {
            inner: TextViewParams {
                value: self.params.value.clone(),
                font: self.params.font.clone(),
                font_size: self.params.font_size,
                on_change: self.params.on_change.clone(),
                color: font_color,
                editable: true,
                selectable: true,
            }.into(),
            padding: PADDING_SIZE,
            background_color: Some(background_color),
            border: Some(Border {
                width: BORDER_SIZE,
                color: border_color,
            }),
            cursor: None,
        }
        .into())
    }

    fn handle_view_event(&mut self, event: &Event) -> Result<()> {
        Ok(())
    }
}
