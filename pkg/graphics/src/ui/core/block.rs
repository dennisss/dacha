// TODO: Rename to something other than 'Box' to avoid naming (maybe
// 'Block'). We will refer that anything that holds one or more elements as a
// 'Container'

use std::sync::Arc;

use common::errors::*;
use image::Color;

use crate::canvas::*;
use crate::ui::children::Children;
use crate::ui::element::Element;
use crate::ui::event::*;
use crate::ui::view::*;

#[derive(Clone)]
pub struct BlockViewParams {
    pub inner: Element,
    pub padding: f32,
    pub background_color: Option<Color>,
    pub border: Option<Border>,

    /// If set, this overrides the cursor of any internal child.
    pub cursor: Option<MouseCursor>,
}

impl ViewParams for BlockViewParams {
    type View = BlockView;
}

#[derive(Clone)]
pub struct Border {
    pub width: f32,
    pub color: Color,
}

pub struct BlockView {
    params: BlockViewParams,
    children: Children,
}

struct BlockViewLayout {
    outer_box: RenderBox,
    inner_constraints: LayoutConstraints,
    inner_box: RenderBox,
}

impl BlockView {
    fn layout_impl(&self, constraints: &LayoutConstraints) -> Result<BlockViewLayout> {
        let inner = &self.children[0];

        let border_width = self.params.border.as_ref().map(|b| b.width).unwrap_or(0.);

        let inner_constraints = LayoutConstraints {
            max_width: constraints.max_width - 2. * self.params.padding - 2. * border_width,
            max_height: constraints.max_height - 2. * self.params.padding - 2. * border_width,
            start_cursor: constraints.start_cursor.clone(),
        };

        let inner_box = inner.layout(&inner_constraints)?;

        let outer_box = RenderBox {
            width: inner_box.width + 2.0 * self.params.padding + 2.0 * border_width,
            height: inner_box.height + 2.0 * self.params.padding + 2.0 * border_width,
            range: inner_box.range,
            next_cursor: inner_box.next_cursor,
            baseline_offset: inner_box.baseline_offset + self.params.padding + border_width,
        };

        Ok(BlockViewLayout {
            inner_box,
            inner_constraints,
            outer_box,
        })
    }
}

impl ViewWithParams for BlockView {
    type Params = BlockViewParams;

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

impl View for BlockView {
    fn build(&mut self) -> Result<ViewStatus> {
        let mut status = self.children[0].build()?;
        if let Some(cursor) = &self.params.cursor {
            status.cursor = *cursor;
        }

        Ok(status)
    }

    fn layout(&self, constraints: &LayoutConstraints) -> Result<RenderBox> {
        self.layout_impl(constraints).map(|v| v.outer_box)
    }

    fn render(&mut self, constraints: &LayoutConstraints, canvas: &mut dyn Canvas) -> Result<()> {
        let layout = self.layout_impl(constraints)?;
        let inner = &mut self.children[0];

        if let Some(color) = &self.params.background_color {
            canvas.fill_rectangle(
                0.,
                0.,
                layout.outer_box.width,
                layout.outer_box.height,
                color,
            )?;
        }

        let border_width = self.params.border.as_ref().map(|b| b.width).unwrap_or(0.);

        if let Some(border) = &self.params.border {
            canvas.stroke_rectangle(
                border_width / 2.,
                border_width / 2.,
                layout.outer_box.width - border_width,
                layout.outer_box.height - border_width,
                border_width,
                &border.color,
            )?;
        }

        canvas.save();
        canvas.translate(self.params.padding, self.params.padding);

        inner.render(&layout.inner_constraints, canvas)?;

        canvas.restore();
        Ok(())
    }

    fn handle_event(&mut self, event: &Event) -> Result<()> {
        match event {
            Event::Mouse(e) => {
                let border_width = self.params.border.as_ref().map(|b| b.width).unwrap_or(0.);

                let mut inner_event = e.clone();
                inner_event.relative_x -= border_width + self.params.padding;
                inner_event.relative_y -= border_width + self.params.padding;

                return self.children[0].handle_event(&Event::Mouse(inner_event));
            }
            _ => {}
        }

        self.children[0].handle_event(event)
    }
}
