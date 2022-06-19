// TODO: Show a custom cursor when we are able to pan / are panning.

use std::rc::Rc;

use common::errors::*;
use image::Image;
use math::matrix::{vec2f, Vector2f};

use crate::canvas::Paint;
use crate::ui;

#[derive(Clone)]
pub struct ImageViewer {
    pub source: Rc<Image<u8>>,

    // TODO: Instead we should support looking this up from the View framework (but we need to be
    // smart enough to support knowing when we are dirty).
    pub outer_width: usize,
    pub outer_height: usize,
}

impl ui::VirtualViewParams for ImageViewer {
    type View = ImageViewerView;
}

pub struct ImageViewerView {
    params: ImageViewer,

    translation: Vector2f,

    scale: f32,
    min_scale: f32,
    max_scale: f32,

    last_mouse_pressed_pos: Option<(f32, f32)>,
}

impl ImageViewerView {
    fn mouse_transform(
        &mut self,
        e: &ui::MouseEvent,
        scale_increment: f32,
        x_increment: f32,
        y_increment: f32,
    ) {
        use crate::transforms::*;

        // Form the current transform.
        let mut mat =
            translate2f(self.translation.clone()) * scale2f(&vec2f(self.scale, self.scale));

        // Move origin to mouse point.
        mat = translate2f(vec2f(-1. * e.relative_x, -1. * e.relative_y)) * mat;

        // Apply pan
        mat = translate2f(vec2f(x_increment, y_increment)) * mat;

        // Apply zoom scaling (at the mouse point which is now the origin).
        {
            let raw_zoom = 1. + 0.1 * scale_increment;
            let new_scale = (self.scale * raw_zoom)
                .min(self.max_scale)
                .max(self.min_scale);
            let zoom = new_scale / self.scale;

            mat = scale2f(&vec2f(zoom, zoom)) * mat;
        }

        // Move origin back.
        mat = translate2f(vec2f(e.relative_x, e.relative_y)) * mat;

        // Extract scale from matrix.
        let raw_scale = mat[(0, 0)];

        self.scale = mat[(0, 0)];

        // Only allow translation from this value up to 0.
        // This is computed to disallow seeing any padding around the image.
        let max_x_offset =
            (self.params.outer_width as f32) - self.scale * (self.params.source.width() as f32);
        let max_y_offset =
            (self.params.outer_height as f32) - self.scale * (self.params.source.height() as f32);

        self.translation = vec2f(
            mat[(0, 2)].min(0.).max(max_x_offset),
            mat[(1, 2)].min(0.).max(max_y_offset),
        );
    }
}

impl ui::VirtualView for ImageViewerView {
    type Params = ImageViewer;

    fn create_with_params(params: &Self::Params) -> Result<Self> {
        let min_scale = (params.outer_width as f32) / (params.source.width() as f32);
        let max_scale = 1. / min_scale;

        Ok(Self {
            params: params.clone(),
            translation: vec2f(0., 0.),
            scale: min_scale,
            min_scale,
            max_scale,
            last_mouse_pressed_pos: None,
        })
    }

    fn update_with_params(&mut self, params: &Self::Params) -> Result<()> {
        self.params = params.clone();
        Ok(())
    }

    fn build_element(&mut self) -> Result<ui::Element> {
        Ok(ui::Transform {
            translation: Some(self.translation.clone()),
            scaling: Some(vec2f(self.scale, self.scale)),
            inner: ui::Image {
                source: self.params.source.clone(),
                paint: Paint::alpha(1.),
            }
            .into(),
        }
        .into())
    }

    fn handle_view_event(&mut self, event: &ui::Event) -> Result<()> {
        if let ui::Event::Mouse(e) = event {
            match &e.kind {
                ui::MouseEventKind::Scroll { x, y } => {
                    self.mouse_transform(e, *y, 0., 0.);
                }
                ui::MouseEventKind::ButtonDown(ui::MouseButton::Left) => {
                    self.last_mouse_pressed_pos = Some((e.relative_x, e.relative_y));
                }
                ui::MouseEventKind::ButtonUp(ui::MouseButton::Left) | ui::MouseEventKind::Exit => {
                    self.last_mouse_pressed_pos = None;
                }
                ui::MouseEventKind::Move => {
                    if let Some((last_x, last_y)) = self.last_mouse_pressed_pos.take() {
                        let rel_x = e.relative_x - last_x;
                        let rel_y = e.relative_y - last_y;
                        self.last_mouse_pressed_pos = Some((e.relative_x, e.relative_y));

                        self.mouse_transform(e, 0., rel_x, rel_y);
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }
}
