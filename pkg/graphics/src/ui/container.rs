use common::errors::*;

use crate::ui::children::*;
use crate::ui::element::*;
use crate::ui::event::*;
use crate::ui::view::*;
use crate::ui::range::*;

/// An object which has knowledge about the positioning of each child view's
/// render box within a container view.
///
/// Typically when a container is done rendering its children, it will cache one
/// of these objects for use in event handling.
pub trait ContainerLayout {
    /// Finds the child which is closest to a given point in the container.
    ///
    /// - If any child contains the given point, that child should be returned.
    /// - If multiple children contain the given point, some deterministic z
    ///   ordering should be used to pick one of them to return.
    fn find_closest_span(&self, x: f32, y: f32) -> Option<Span>;

    fn get_span_rect(&self, span: Span) -> Rect;
}

/// A single rendered component of a view. A single view may be rendered as one
/// or more spans (more than one only if the container supports it).
#[derive(Clone, Copy)]
pub struct Span {
    /// Index of the View in the parent container view's children list.
    pub child_index: usize,

    /// If none, then the entire child was rendered.
    /// TODO: Switch this back to a 
    pub range: Option<CursorRange>,
}

pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A collection of views.
///
/// This struct contains shared state/methods used for
/// implementing other types of Views which contain other views (e.g.
/// paragraphs, grids, tables).
pub struct Container {
    children: Children,
    state: ContainerState,
}

#[derive(Default)]
struct ContainerState {
    // TODO: Ensure we never look up the rectangles for these if they may be destroyed by a recent render?

    /// Index of the last child element which has had the user's mouse cursor in
    /// it.
    last_mouse_focus: Option<Span>,

    last_key_focus: Option<Span>,
}

impl Container {
    pub fn new(elements: &[Element]) -> Result<Self> {
        Ok(Self {
            children: Children::new(elements)?,
            state: ContainerState::default(),
        })
    }

    pub fn update(&mut self, new_elements: &[Element]) -> Result<()> {
        // TODO: Must also potentially change the mouse focus?
        self.children.update(new_elements)
    }

    pub fn children(&self) -> &[Box<dyn View>] {
        &self.children
    }

    pub fn children_mut(&mut self) -> &mut [Box<dyn View>] {
        &mut self.children
    }

    /// Standard implementation of View::build() for container views.
    pub fn build(&mut self) -> Result<ViewStatus> {
        let mut status = ViewStatus::default();

        for i in 0..self.children.len() {
            let status_i = self.children[i].build()?;

            status.dirty |= status_i.dirty;

            // Inherit the cursor of whichever child we are currently hovering over.
            // Note that a child with multiple spans has the same cursor for all spans.
            if self.state.last_mouse_focus.map(|s| s.child_index) == Some(i) {
                status.cursor = status_i.cursor;
            }

            if status_i.focused {
                if self.state.last_key_focus.map(|s| s.child_index) != Some(i)
                    && self.state.last_key_focus.is_some()
                {
                    let last_span = self.state.last_key_focus.unwrap();

                    self.children[last_span.child_index]
                        .handle_event(&Event::Blur)?;
                    if last_span.child_index < i {
                        // TODO: May also need to see if the other fields in this return value have
                        // changed and would impact the overall status.
                        let _ = self.children[last_span.child_index].build()?;
                    }
                }

                self.state.last_key_focus = Some(Span {
                    child_index: i,
                    range: None,
                });
                status.focused = true;
            }
        }

        // TODO: Ensure that a Blur is always issued even if no new view is focused.
        if !status.focused {
            self.state.last_key_focus = None;
        }

        Ok(status)
    }

    /// Standard implementation of View::handle_event() for container views.
    pub fn handle_event<L: ContainerLayout>(
        &mut self,
        event: &Event,
        layout: &L,
    ) -> Result<()> {
        match event {
            Event::Mouse(e) => {
                // TODO: Verify this check is applied to all views that hold other children.
                if e.range.is_some() {
                    return Err(err_msg("Containers do not have cursors"));
                }

                let child_span = layout.find_closest_span(e.relative_x, e.relative_y);

                // Send exit event if child has changed.
                // TODO: Also send an enter exit on changes
                // TODO: Make sure the child still exists!
                if self.state.last_mouse_focus.map(|s| s.child_index)
                    != child_span.map(|s| s.child_index)
                {
                    // Send exit event
                    if let Some(old_span) = self.state.last_mouse_focus.clone() {
                        let mut exit_event = e.clone();
                        exit_event.kind = MouseEventKind::Exit;
                        // TODO: Calculate right offset.

                        self.children[old_span.child_index].handle_event(&Event::Mouse(exit_event))?;
                    }

                    // Send enter event
                    if let Some(new_span) = child_span.clone() {
                        let mut enter_event = e.clone();
                        enter_event.kind = MouseEventKind::Enter;

                        // TODO: Dedup this.
                        let new_rect = layout.get_span_rect(new_span);
                        enter_event.relative_x -= new_rect.x;
                        enter_event.relative_y -= new_rect.y;
                        enter_event.range = new_span.range;

                        self.children[new_span.child_index]
                            .handle_event(&Event::Mouse(enter_event))?;
                    }
                }

                // Send event itself
                if let Some(new_span) = child_span.clone() {
                    let mut inner_event = e.clone();
                    if inner_event.kind == MouseEventKind::Enter
                        || inner_event.kind == MouseEventKind::Exit
                    {
                        inner_event.kind = MouseEventKind::Move;
                    }

                    // TODO: Dedup this.
                    let new_rect = layout.get_span_rect(new_span);
                    inner_event.relative_x -= new_rect.x;
                    inner_event.relative_y -= new_rect.y;
                    inner_event.range = new_span.range;

                    self.children[new_span.child_index]
                        .handle_event(&Event::Mouse(inner_event))?;
                }

                // Clicking outside of a focused element should blur it.
                if let Some(key_focus_span) = self.state.last_key_focus.clone() {
                    if let MouseEventKind::ButtonDown(_) = e.kind {
                        if Some(key_focus_span.child_index) != child_span.map(|s| s.child_index) {
                            self.children[key_focus_span.child_index].handle_event(&Event::Blur)?;
                            self.state.last_key_focus = None;
                        }
                    }
                }

                self.state.last_mouse_focus = child_span;
            }
            Event::Blur => {
                if let Some(span) = self.state.last_key_focus.clone() {
                    self.children[span.child_index].handle_event(event)?;
                }
            }
            Event::Key(e) => {
                if let Some(span) = self.state.last_key_focus.clone() {
                    self.children[span.child_index].handle_event(event)?;
                }
            }
        }

        Ok(())
    }
}
