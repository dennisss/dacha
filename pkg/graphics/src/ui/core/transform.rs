use common::errors::*;
use image::Color;
use math::matrix::Vector2f;

use crate::canvas::*;
use crate::ui::children::Children;
use crate::ui::element::Element;
use crate::ui::event::*;
use crate::ui::view::*;

#[derive(Clone)]
pub struct TransformViewParams {
    pub inner: Element,

    pub translation: Option<Vector2f>,
    pub scaling: Option<Vector2f>,
}

impl ViewParams for TransformViewParams {
    type View = TransformView;
}

pub struct TransformView {
    params: TransformViewParams,
    children: Children,
}

impl ViewWithParams for TransformView {
    type Params = TransformViewParams;

    fn create_with_params(params: &Self::Params) -> Result<Box<dyn View>> {
        Ok(Box::new(Self {
            params: params.clone(),
            children: Children::new(core::slice::from_ref(&params.inner))?,
        }))
    }

    fn update_with_params(&mut self, new_params: &Self::Params) -> Result<()> {
        self.params = new_params.clone();
        self.children
            .update(core::slice::from_ref(&new_params.inner))?;
        Ok(())
    }
}

impl View for TransformView {
    fn build(&mut self) -> Result<ViewStatus> {
        self.children[0].build()
    }

    fn layout(&self, parent_box: &RenderBox) -> Result<RenderBox> {
        self.children[0].layout(parent_box)
    }

    fn render(&mut self, parent_box: &RenderBox, canvas: &mut dyn Canvas) -> Result<()> {
        canvas.save();

        if let Some(v) = &self.params.translation {
            canvas.translate(v.x(), v.y());
        }

        if let Some(v) = &self.params.scaling {
            canvas.scale(v.x(), v.y());
        }

        self.children[0].render(parent_box, canvas)?;

        canvas.restore();
        Ok(())
    }

    fn handle_event(&mut self, event: &Event) -> Result<()> {
        self.children[0].handle_event(event)
    }
}
