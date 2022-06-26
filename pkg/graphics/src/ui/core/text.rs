use std::rc::Rc;

use common::errors::*;
use image::Color;

use crate::canvas::{Canvas, Paint};
use crate::font::{CanvasFontRenderer, FontStyle, TextMeasurements, VerticalAlign};
use crate::ui::event::*;
use crate::ui::view::*;

#[derive(Clone)]
pub struct TextViewParams {
    pub text: String,
    /// TODO: Replace to a dependency handle so that it is easier to compare the entire thing with PartialEq of the TextViewParams.
    pub font: Rc<CanvasFontRenderer>,
    pub font_size: f32,
    pub color: Color,
}

impl ViewParams for TextViewParams {
    type View = TextView;
}

pub struct TextView {
    params: TextViewParams,
    dirty: bool,
}

struct TextLayout {
    start_index: usize,
    measurements: TextMeasurements,
}

impl TextView {
    fn layout_impl(&self, constraints: &LayoutConstraints) -> Result<TextLayout> {
        let start_index = constraints.start_cursor.unwrap_or(0);

        let remaining_text = &self.params.text[constraints.start_cursor.unwrap_or(0)..];

        let min_length = {
            let word_end = remaining_text.find(' ').unwrap_or(0);
            let first_char = remaining_text
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(0);

            word_end.max(first_char)
        };

        let mut measurements = self.params.font.measure_text(
            remaining_text,
            self.params.font_size,
            if constraints.start_cursor.is_some() {
                Some(constraints.max_width)
            } else {
                None
            },
        )?;

        if measurements.length < min_length {
            measurements = self.params.font.measure_text(
                &remaining_text[0..min_length],
                self.params.font_size,
                None,
            )?;
        }

        Ok(TextLayout {
            start_index,
            measurements,
        })
    }
}

impl ViewWithParams for TextView {
    type Params = TextViewParams;

    fn create_with_params(params: &Self::Params) -> Result<Box<dyn View>> {
        Ok(Box::new(Self {
            params: params.clone(),
            dirty: true,
        }))
    }

    fn update_with_params(&mut self, new_params: &Self::Params) -> Result<()> {
        if self.params.text != new_params.text ||
           !core::ptr::eq::<CanvasFontRenderer>(&*self.params.font, &*new_params.font) ||
           self.params.font_size != new_params.font_size ||
           self.params.color != new_params.color {
            self.dirty = true;
            self.params = new_params.clone();
        }

        Ok(())
    }
}

impl View for TextView {
    fn build(&mut self) -> Result<ViewStatus> {
        let mut status = ViewStatus::default();
        status.cursor = MouseCursor(glfw::StandardCursor::IBeam);
        status.dirty = self.dirty;
        Ok(status)
    }

    fn layout(&self, constraints: &LayoutConstraints) -> Result<RenderBox> {
        let layout = self.layout_impl(constraints)?;

        Ok(RenderBox {
            width: layout.measurements.width,
            height: layout.measurements.height,
            baseline_offset: layout.measurements.height + layout.measurements.descent,
            next_cursor: if layout.start_index + layout.measurements.length < self.params.text.len()
            {
                Some(layout.start_index + layout.measurements.length)
            } else {
                None
            },
        })
    }

    fn render(&mut self, constraints: &LayoutConstraints, canvas: &mut dyn Canvas) -> Result<()> {
        let layout = self.layout_impl(constraints)?;

        let text = &self.params.text
            [layout.start_index..(layout.start_index + layout.measurements.length)];

        self.params.font.fill_text(
            0.,
            0.,
            text,
            &FontStyle::from_size(self.params.font_size).with_vertical_align(VerticalAlign::Top),
            &Paint::color(self.params.color.clone()),
            canvas,
        )?;

        self.dirty = false;

        Ok(())
    }

    fn handle_event(&mut self, start_cursor: usize, event: &Event) -> Result<()> {
        Ok(())
    }
}
