use std::sync::Arc;

use common::errors::*;
use image::Color;
use math::matrix::Vector2f;

use crate::raster::canvas::Canvas;
use crate::raster::canvas::PathBuilder;
use crate::ui::event::*;
use crate::ui::view::*;

struct ClickFilter {
    down: bool,
}

impl ClickFilter {
    pub fn new() -> Self {
        Self { down: false }
    }

    /// NOTE: Should be called with every single event received by the view.
    fn process(&mut self, next_event: &Event) -> bool {
        let mouse = match next_event {
            Event::Mouse(v) => v,
            _ => {
                return false;
            }
        };

        match mouse.kind {
            MouseEventKind::Move => {}
            MouseEventKind::ButtonDown(MouseButton::Left) => {
                self.down = true;
            }
            MouseEventKind::ButtonUp(MouseButton::Left) => {
                let v = self.down;
                self.down = false;
                return v;
            }
            _ => {
                self.down = false;
            }
        }

        false
    }
}

#[derive(Clone)]
pub struct CheckboxParams {
    pub value: bool,
    pub on_change: Option<Arc<dyn Fn(bool)>>,
}

impl ViewParams for CheckboxParams {
    type View = CheckboxView;
}

pub struct CheckboxView {
    params: CheckboxParams,
    click_filter: ClickFilter,
}

impl ViewWithParams for CheckboxView {
    type Params = CheckboxParams;

    fn create_with_params(params: &Self::Params) -> Result<Box<dyn View>> {
        Ok(Box::new(Self {
            params: params.clone(),
            click_filter: ClickFilter::new(),
        }))
    }

    fn update_with_params(&mut self, new_params: &Self::Params) -> Result<()> {
        self.params = new_params.clone();
        Ok(())
    }
}

impl View for CheckboxView {
    fn build(&mut self) -> Result<ViewStatus> {
        Ok(ViewStatus {
            cursor: MouseCursor(glfw::StandardCursor::Hand),
            // TODO: Support focus for this.
            focused: false,
        })
    }

    fn layout(&self, parent_box: &RenderBox) -> Result<RenderBox> {
        Ok(RenderBox {
            width: 54.,
            height: 54.,
        })
    }

    fn render(&mut self, parent_box: &RenderBox, canvas: &mut Canvas) -> Result<()> {
        // #2196F3
        let bg_color = Color::from_slice_with_shape(3, 1, &[0x21, 0x96, 0xF3]);

        let border_color = Color::from_slice_with_shape(3, 1, &[0x77, 0x77, 0x77]);
        let white = Color::from_slice_with_shape(3, 1, &[0xff, 0xff, 0xff]);

        let dim = 54.;
        let border_width = 6.;

        if self.params.value {
            canvas.fill_rectangle(0., 0., dim, dim, &bg_color)?;

            let mut path = PathBuilder::new();
            path.move_to(Vector2f::from_slice(&[7.5, 23.5]));
            path.line_to(Vector2f::from_slice(&[20.5, 36.5]));
            path.line_to(Vector2f::from_slice(&[44.5, 13.5]));

            canvas.stroke_path(&path.build(), 5., &white)?;
        } else {
            let offset = border_width / 2.;
            // TODO: Fill white behind it.
            canvas.stroke_rectangle(
                offset,
                offset,
                dim - offset,
                dim - offset,
                border_width,
                &border_color,
            )?;
        }

        Ok(())
    }

    fn handle_event(&mut self, event: &Event) -> Result<()> {
        if self.click_filter.process(event) {
            if let Some(listener) = &self.params.on_change {
                listener(!self.params.value);
            }
        }

        Ok(())
    }
}
