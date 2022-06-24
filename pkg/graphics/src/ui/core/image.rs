use std::rc::Rc;

use common::errors::*;
use image::Image;

use crate::canvas::{Canvas, CanvasObject, Paint};
use crate::ui::event::*;
use crate::ui::view::*;

#[derive(Clone)]
pub struct ImageViewParams {
    pub source: Rc<Image<u8>>,
    pub paint: Paint,
}

impl ViewParams for ImageViewParams {
    type View = ImageView;
}

pub struct ImageView {
    params: ImageViewParams,
    object: Option<Box<dyn CanvasObject>>,
}

impl ViewWithParams for ImageView {
    type Params = ImageViewParams;

    fn create_with_params(params: &Self::Params) -> Result<Box<dyn View>> {
        Ok(Box::new(Self {
            params: params.clone(),
            object: None,
        }))
    }

    fn update_with_params(&mut self, new_params: &Self::Params) -> Result<()> {
        if !core::ptr::eq::<Image<u8>>(&*self.params.source, &*new_params.source) {
            self.object = None;
        }

        self.params = new_params.clone();
        Ok(())
    }
}

impl View for ImageView {
    fn build(&mut self) -> Result<ViewStatus> {
        Ok(ViewStatus::default())
    }

    fn layout(&self, constraints: &LayoutConstraints) -> Result<RenderBox> {
        Ok(RenderBox {
            width: self.params.source.width() as f32,
            height: self.params.source.height() as f32,
            baseline_offset: 0.,
            next_cursor: None,
        })
    }

    fn render(&mut self, constraints: &LayoutConstraints, canvas: &mut dyn Canvas) -> Result<()> {
        let obj = match self.object.as_mut() {
            Some(v) => v,
            None => self
                .object
                .insert(canvas.create_image(&self.params.source)?),
        };

        obj.draw(&self.params.paint, canvas)?;

        Ok(())
    }

    fn handle_event(&mut self, start_cursor: usize, event: &Event) -> Result<()> {
        Ok(())
    }
}
