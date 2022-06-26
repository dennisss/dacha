use common::errors::*;

use crate::canvas::*;
use crate::ui::element::Element;
use crate::ui::event::*;
use crate::ui::view::*;
use crate::ui::range::*;

#[derive(Clone)]
pub struct EmptyViewParams {}

impl ViewParams for EmptyViewParams {
    type View = EmptyView;
}

pub struct EmptyView {
    params: EmptyViewParams,
}

impl ViewWithParams for EmptyView {
    type Params = EmptyViewParams;

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

impl View for EmptyView {
    fn build(&mut self) -> Result<ViewStatus> {
        Ok(ViewStatus::default())
    }

    fn layout(&self, constraints: &LayoutConstraints) -> Result<RenderBox> {
        Ok(RenderBox {
            width: 0.,
            height: 0.,
            next_cursor: None,
            range: CursorRange::zero(),
            baseline_offset: 0.,
        })
    }

    fn render(&mut self, constraints: &LayoutConstraints, canvas: &mut dyn Canvas) -> Result<()> {
        Ok(())
    }

    fn handle_event(&mut self, event: &Event) -> Result<()> {
        Ok(())
    }
}
