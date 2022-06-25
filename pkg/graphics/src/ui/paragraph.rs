/*

Implementation notes:
- A single child will never appear twice in one line.

Basically we need to incrementally render elements:

The paragraph does the following:
- First, figure out how much space we have to create a line.
- Then, call layout() on child with box with an 'allow_splitting' flag.
- Child element will return it's box + a cursor to the next position.
- After the line is figured out, paragraph calls render() on all children with their cursor position (or 0)

- Repeat for next line, but now layout() is called starting at the

*/

use common::errors::*;
use math::matrix::Vector2f;

use crate::canvas::Canvas;
use crate::ui::container::*;
use crate::ui::element::Element;
use crate::ui::event::*;
use crate::ui::view::*;

#[derive(Clone)]
pub struct ParagraphViewParams {
    pub children: Vec<Element>,
}

impl ViewParams for ParagraphViewParams {
    type View = ParagraphView;
}

pub struct ParagraphView {
    params: ParagraphViewParams,
    container: Container,
    state: ParagraphViewState,
}

#[derive(Default)]
struct ParagraphLine {
    /// Position of the top-left corner of this line.
    /// This is relative to (0, 0) being the top-left corner of the whole
    /// paragraph.
    x: f32,
    y: f32,

    width: f32,
    ascent: f32,
    descent: f32,

    first_child_index: usize,
    first_child_start: usize,

    last_child_index: usize,

    /// NOTE: This data could be recomputed in render_line, but is cached for
    /// simplicity and to ensure that render() is called with the exact same
    /// constraints as layout().
    spans: Vec<ParagraphSpan>,
}

struct ParagraphSpan {
    x: f32,
    layout_constraints: LayoutConstraints,
    render_box: RenderBox,
}

#[derive(Default)]
struct ParagraphViewState {
    layout: Option<ParagraphViewLayout>,
}

struct ParagraphViewLayout {
    outer_box: RenderBox,

    lines: Vec<ParagraphLine>,
}

impl ParagraphView {
    fn layout_impl(&self, constraints: &LayoutConstraints) -> Result<ParagraphViewLayout> {
        let mut current_y = 0.;

        let mut lines = vec![];

        let mut current_line = None;

        for child_i in 0..self.container.children().len() {
            let mut next_child_cursor = 0;

            loop {
                let line = match current_line.as_mut() {
                    Some(v) => v,
                    None => {
                        let mut line = ParagraphLine::default();
                        line.first_child_index = child_i;
                        line.first_child_start = next_child_cursor;
                        current_line.insert(line)
                    }
                };

                let span_constraints = LayoutConstraints {
                    max_width: constraints.max_width - line.width,
                    max_height: constraints.max_height - current_y,
                    start_cursor: Some(next_child_cursor),
                };

                let span_render_box =
                    self.container.children()[child_i].layout(&span_constraints)?;

                // If adding this span to the line would result in us exceeding the width of the
                // container, we must break off the current line.
                //
                // The main exception is that we require each line has at least one spane to
                // ensure we continuously make progress.
                let first_span_in_line = line.first_child_index == child_i;
                if line.width + span_render_box.width > constraints.max_width && !first_span_in_line
                {
                    self.finalize_line(line, &mut current_y)?;
                    lines.push(current_line.take().unwrap());
                    // current_line = None;

                    // Note that this will re-run the layout() on the same starting cursor which may
                    // result in a wider span being renderable on a new line.
                    continue;
                }

                line.ascent = line.ascent.max(span_render_box.baseline_offset);
                line.descent = line
                    .descent
                    .max(span_render_box.height - span_render_box.baseline_offset);
                line.width += span_render_box.width;
                line.spans.push(ParagraphSpan {
                    x: 0., // TODO: Compute this in the next pass.
                    layout_constraints: span_constraints,
                    render_box: span_render_box.clone(),
                });

                line.last_child_index = child_i;

                // TODO: Compute line.y and line.x

                if let Some(next) = &span_render_box.next_cursor {
                    if *next <= next_child_cursor {
                        return Err(err_msg("Made no progress in incrementally rendering child"));
                    }

                    // Even though the span doesn't exceed the width, we use this as a signal that
                    // the child prefers to break lines early (e.g. to not render a partial word).
                    self.finalize_line(line, &mut current_y)?;
                    lines.push(current_line.take().unwrap());
                    // current_line = None;

                    next_child_cursor = *next;
                } else {
                    break;
                }
            }
        }

        if let Some(mut line) = current_line {
            self.finalize_line(&mut line, &mut current_y)?;
            lines.push(line);
        }

        Ok(ParagraphViewLayout {
            lines,
            outer_box: RenderBox {
                width: constraints.max_width,
                height: current_y,
                baseline_offset: 0.,
                next_cursor: None,
            },
        })
    }

    fn finalize_line(&self, line: &mut ParagraphLine, current_y: &mut f32) -> Result<()> {
        line.x = 0.;

        let mut current_x = 0.;
        for span in &mut line.spans {
            span.x = current_x;
            current_x += span.render_box.width;
        }

        line.y = *current_y;
        *current_y += line.ascent + line.descent;

        Ok(())
    }

    fn render_line(&mut self, line: &ParagraphLine, canvas: &mut dyn Canvas) -> Result<()> {
        for i in line.first_child_index..(line.last_child_index + 1) {
            let span = &line.spans[i - line.first_child_index];

            canvas.save();
            canvas.translate(
                line.x,
                line.y + (line.ascent - span.render_box.baseline_offset),
            );
            self.container.children_mut()[i].render(&span.layout_constraints, canvas)?;
            canvas.restore();
        }

        Ok(())
    }
}

impl ViewWithParams for ParagraphView {
    type Params = ParagraphViewParams;

    fn create_with_params(params: &Self::Params) -> Result<Box<dyn View>> {
        Ok(Box::new(Self {
            params: params.clone(),
            container: Container::new(&params.children)?,
            state: ParagraphViewState::default(),
        }))
    }

    fn update_with_params(&mut self, new_params: &Self::Params) -> Result<()> {
        self.params = new_params.clone();
        self.container.update(&new_params.children)?;
        Ok(())
    }
}

impl View for ParagraphView {
    fn build(&mut self) -> Result<ViewStatus> {
        self.container.build()
    }

    fn layout(&self, constraints: &LayoutConstraints) -> Result<RenderBox> {
        self.layout_impl(constraints).map(|v| v.outer_box)
    }

    fn render(&mut self, constraints: &LayoutConstraints, canvas: &mut dyn Canvas) -> Result<()> {
        let layout = self.layout_impl(constraints)?;
        for line in &layout.lines {
            self.render_line(line, canvas)?;
        }

        self.state.layout = Some(layout);

        Ok(())
    }

    fn handle_event(&mut self, start_cursor: usize, event: &Event) -> Result<()> {
        let layout = match self.state.layout.as_ref() {
            Some(v) => v,
            None => {
                return Ok(());
            }
        };

        self.container.handle_event(start_cursor, event, layout)
    }
}

impl ContainerLayout for ParagraphViewLayout {
    fn find_closest_span(&self, x: f32, y: f32) -> Option<Span> {
        None
    }

    fn get_span_rect(&self, span: Span) -> Rect {
        todo!()
    }
}
