use std::rc::Rc;

use common::errors::*;
use image::Color;

use crate::canvas::{Canvas, Paint};
use crate::font::CanvasFontRenderer;
use crate::ui::event::*;
use crate::ui::view::*;

#[derive(Clone)]
pub struct TextViewParams {
    pub text: String,
    pub font: Rc<CanvasFontRenderer>,
    pub font_size: f32,
    pub color: Color,
}

impl ViewParams for TextViewParams {
    type View = TextView;
}

pub struct TextView {
    params: TextViewParams,
}

impl ViewWithParams for TextView {
    type Params = TextViewParams;

    fn create_with_params(params: &Self::Params) -> Result<Box<dyn View>> {
        Ok(Box::new(Self {
            params: params.clone(),
        }))
    }

    fn update_with_params(&mut self, new_params: &Self::Params) -> Result<()> {
        self.params = new_params.clone();
        Ok(())
    }
}

impl View for TextView {
    fn build(&mut self) -> Result<ViewStatus> {
        Ok(ViewStatus {
            cursor: MouseCursor(glfw::StandardCursor::IBeam),
            focused: false,
        })
    }

    fn layout(&self, parent_box: &RenderBox) -> Result<RenderBox> {
        let measurements = self
            .params
            .font
            .measure_text(&self.params.text, self.params.font_size)?;
        Ok(RenderBox {
            width: measurements.width,
            height: measurements.height,
        })
    }

    fn render(&mut self, parent_box: &RenderBox, canvas: &mut dyn Canvas) -> Result<()> {
        let measurements = self
            .params
            .font
            .measure_text(&self.params.text, self.params.font_size)?;

        self.params.font.fill_text(
            0.0,
            measurements.height + measurements.descent,
            &self.params.text,
            self.params.font_size,
            &Paint::color(self.params.color.clone()),
            canvas,
        )?;

        Ok(())
    }

    fn handle_event(&mut self, event: &Event) -> Result<()> {
        Ok(())
    }
}
