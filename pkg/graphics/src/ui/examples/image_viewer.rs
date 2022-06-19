use std::rc::Rc;

use common::errors::*;
use image::Image;
use math::matrix::{vec2f, Vector2f};

use crate::canvas::Paint;
use crate::ui::element::*;
use crate::ui::event::*;
use crate::ui::image::ImageViewParams;
use crate::ui::transform::TransformViewParams;
use crate::ui::virtual_view::*;

#[derive(Clone)]
pub struct ImageViewer {
    pub source: Rc<Image<u8>>,

    // TODO: Instead we should support looking this up from the View framework (but we need to be
    // smart enough to support knowing when we are dirty).
    pub outer_width: usize,
    pub outer_height: usize,
}

pub struct ImageViewerView {
    params: ImageViewer,

    translation: Vector2f,

    scale: f32,
    min_scale: f32,
    max_scale: f32,
}

impl VirtualViewParams for ImageViewer {
    type View = ImageViewerView;
}

impl VirtualView for ImageViewerView {
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
        })
    }

    fn update_with_params(&mut self, params: &Self::Params) -> Result<()> {
        self.params = params.clone();
        Ok(())
    }

    fn build_element(&mut self) -> Result<Element> {
        Ok(TransformViewParams {
            translation: Some(self.translation.clone()),
            scaling: Some(vec2f(self.scale, self.scale)),
            inner: ImageViewParams {
                source: self.params.source.clone(),
                paint: Paint::alpha(1.),
            }
            .into(),
        }
        .into())
    }

    fn handle_view_event(&mut self, event: &Event) -> Result<()> {
        use crate::transforms::*;

        if let Event::Mouse(e) = event {
            if let MouseEventKind::Scroll { x, y } = &e.kind {
                // Form the current transform.
                let mut mat =
                    translate2f(self.translation.clone()) * scale2f(&vec2f(self.scale, self.scale));

                // Move origin to mouse point.
                mat = translate2f(vec2f(-1. * e.relative_x, -1. * e.relative_y)) * mat;

                // Apply scaling (at the mouse point which is now the origin).
                {
                    let raw_zoom = 1. + 0.1 * *y;
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
                let max_x_offset = (self.params.outer_width as f32)
                    - self.scale * (self.params.source.width() as f32);
                let max_y_offset = (self.params.outer_height as f32)
                    - self.scale * (self.params.source.height() as f32);

                self.translation = vec2f(
                    mat[(0, 2)].min(0.).max(max_x_offset),
                    mat[(1, 2)].min(0.).max(max_y_offset),
                );
            }
        }

        Ok(())
    }
}
