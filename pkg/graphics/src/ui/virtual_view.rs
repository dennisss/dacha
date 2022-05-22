use common::errors::*;
use core::any::TypeId;
use std::sync::Arc;

use crate::raster::canvas::Canvas;
use crate::ui::children::*;
use crate::ui::element::*;
use crate::ui::event::*;
use crate::ui::view::*;

/// A view which doesn't directly draw to the screen but instead produces a
/// child view tree which is rendered. This child tree may be change based on
/// internal state of the virtual view.
pub trait VirtualView {
    type Params;

    fn create_with_params(params: &Self::Params) -> Result<Self>
    where
        Self: Sized;

    fn update_with_params(&mut self, params: &Self::Params) -> Result<()>;

    fn build(&mut self) -> Result<Element>;
}

pub trait VirtualViewParams {
    type View: VirtualView<Params = Self> + 'static;
}

impl<T: VirtualViewParams> ViewParams for T {
    type View = VirtualViewContainer<T::View>;
}

pub struct VirtualViewContainer<V: VirtualView + 'static> {
    inner: V,
    children: Children,
}

impl<V: VirtualView + 'static> ViewWithParams for VirtualViewContainer<V> {
    type Params = V::Params;

    fn create_with_params(params: &Self::Params) -> Result<Box<dyn View>> {
        let mut inner = V::create_with_params(params)?;

        let initial_el = inner.build()?;

        Ok(Box::new(Self {
            inner: V::create_with_params(params)?,
            children: Children::new(&[initial_el])?,
        }))
    }

    fn update_with_params(&mut self, new_params: &Self::Params) -> Result<()> {
        self.inner.update_with_params(new_params);
        // NOTE: Updating of children is performed in build() where we may recreate the
        // child tree.

        Ok(())
    }
}

impl<V: VirtualView + 'static> View for VirtualViewContainer<V> {
    fn build(&mut self) -> Result<ViewStatus> {
        let el = self.inner.build()?;
        self.children.update(&[el])?;

        self.children[0].build()
    }

    fn layout(&self, parent_box: &RenderBox) -> Result<RenderBox> {
        self.children[0].layout(parent_box)
    }

    fn render(&mut self, parent_box: &RenderBox, canvas: &mut Canvas) -> Result<()> {
        self.children[0].render(parent_box, canvas)
    }

    fn handle_event(&mut self, event: &Event) -> Result<()> {
        self.children[0].handle_event(event)
    }
}
