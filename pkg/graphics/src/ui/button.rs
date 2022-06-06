use std::sync::Arc;
use std::time::{Duration, Instant};

use common::errors::*;
use image::Color;
use math::matrix::Vector2f;

use crate::ui::box_view::*;
use crate::ui::children::Children;
use crate::ui::element::*;
use crate::ui::event::*;
use crate::ui::view::*;
use crate::ui::virtual_view::*;

const BORDER_SIZE: f32 = 1.;
const PADDING_SIZE: f32 = 10.;

#[derive(Clone)]
pub struct ButtonParams {
    pub inner: Element,
    pub on_click: Option<Arc<dyn Fn()>>,
}

impl VirtualViewParams for ButtonParams {
    type View = ButtonView;
}

pub struct ButtonView {
    params: ButtonParams,
    click_filter: MouseClickFilter,
}

impl VirtualView for ButtonView {
    type Params = ButtonParams;

    fn create_with_params(params: &Self::Params) -> Result<Self> {
        Ok(Self {
            params: params.clone(),
            click_filter: MouseClickFilter::new(),
        })
    }

    fn update_with_params(&mut self, params: &Self::Params) -> Result<()> {
        self.params = params.clone();
        Ok(())
    }

    fn build_element(&mut self) -> Result<Element> {
        // Regular: #2196F3
        // Pressed: #0D47A1

        let background_color = {
            if self.click_filter.currently_pressed() {
                Color::rgb(0x0D, 0x47, 0xA1)
            } else {
                Color::rgb(0x21, 0x96, 0xF3)
            }
        };

        Ok(BoxViewParams {
            inner: self.params.inner.clone(),
            padding: PADDING_SIZE,
            background_color: Some(background_color),
            border: None,
            cursor: Some(MouseCursor(glfw::StandardCursor::Hand)),
        }
        .into())
    }

    fn handle_view_event(&mut self, event: &Event) -> Result<()> {
        if self.click_filter.process(event) {
            if let Some(listener) = &self.params.on_click {
                listener();
            }
        }

        Ok(())
    }
}
