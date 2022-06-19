use common::errors::*;

use crate::canvas::Canvas;
use crate::ui::children::Children;
use crate::ui::element::Element;
use crate::ui::event::*;
use crate::ui::view::*;

#[derive(Clone)]
pub struct GridViewParams {
    pub rows: Vec<GridDimensionSize>,
    pub cols: Vec<GridDimensionSize>,
    pub children: Vec<Element>,
}

impl ViewParams for GridViewParams {
    type View = GridView;
}

#[derive(Clone, Copy)]
pub enum GridDimensionSize {
    Percentage(f32),
    Absolute(f32),
    /// Consume all remaining space in the parent container
    /// TODO: E
    Grow(f32),
    FitContent,
}

pub struct GridView {
    params: GridViewParams,
    children: Children,
    state: GridViewState,
}

#[derive(Default)]
struct GridViewState {
    layout: Option<GridViewLayout>,

    /// Index of the last child element which has had the user's mouse cursor in
    /// it.
    last_mouse_focus: Option<usize>,

    last_key_focus: Option<usize>,
}

struct GridViewLayout {
    outer_box: RenderBox,

    /// Starting y position of each row.
    /// - The first element is 0.
    /// - For N rows, there are N values in this array.
    row_starts: Vec<f32>,

    /// Starting x position of each column.
    /// Has a similar format to row_starts.
    col_starts: Vec<f32>,
}

impl GridView {
    fn layout_impl(&self, parent_box: &RenderBox) -> Result<GridViewLayout> {
        // TODO: It would be nice if this also supported doing vertical alignment to the
        // baseline.

        if self.params.rows.len() * self.params.cols.len() != self.children.len() {
            return Err(err_msg("Incorrect number of children"));
        }

        let mut row_heights = vec![0.; self.params.rows.len()];
        let mut col_widths = vec![0.; self.params.cols.len()];

        //
        let mut remaining_height = parent_box.height;
        let mut remaining_width = parent_box.width;

        // Step 1: Resolve have 'fixed' size rows/cols.
        for (row_i, row) in self.params.rows.iter().enumerate() {
            if let GridDimensionSize::Absolute(v) = row {
                row_heights[row_i] = *v;
            } else if let GridDimensionSize::Percentage(v) = row {
                row_heights[row_i] = parent_box.height * v;
            } else {
                continue;
            }

            remaining_height -= row_heights[row_i];
        }
        for (col_i, col) in self.params.cols.iter().enumerate() {
            if let GridDimensionSize::Absolute(v) = col {
                col_widths[col_i] = *v;
            } else if let GridDimensionSize::Percentage(v) = col {
                col_widths[col_i] = parent_box.width * v;
            } else {
                continue;
            }

            remaining_width -= col_widths[col_i];
        }

        // Step 2: Give all the FitContent columns as much space as they want
        // NOTE: Columns sizes are prioritized over row sizes.
        for (col_i, col) in self.params.cols.iter().enumerate() {
            if let GridDimensionSize::FitContent = col {
                let mut max_width: f32 = 0.;
                for row_i in 0..self.params.rows.len() {
                    let i = row_i * self.params.cols.len() + col_i;
                    let inner_box = self.children[i].layout(&RenderBox {
                        width: remaining_width,
                        height: parent_box.height, // TODO: Pick a better value
                    })?;

                    max_width = max_width.max(inner_box.width);
                }

                remaining_width -= max_width;
                col_widths[col_i] = max_width;
            }
        }
        for (row_i, row) in self.params.rows.iter().enumerate() {
            if let GridDimensionSize::FitContent = row {
                let mut max_height: f32 = 0.;
                for col_i in 0..self.params.cols.len() {
                    let i = row_i * self.params.cols.len() + col_i;
                    let inner_box = self.children[i].layout(&RenderBox {
                        width: col_widths[col_i],
                        height: remaining_height,
                    })?;

                    max_height = max_height.max(inner_box.height);
                }

                remaining_height -= max_height;
                row_heights[row_i] = max_height;
            }
        }

        // Step 3: Calculate remaining grow elements.
        let sum_grow_dims = |dims: &[GridDimensionSize]| -> f32 {
            dims.iter()
                .map(|d| match d {
                    GridDimensionSize::Grow(v) => *v,
                    _ => 0.,
                })
                .sum()
        };

        let row_total_grow = sum_grow_dims(&self.params.rows);
        let col_total_grow = sum_grow_dims(&self.params.cols);

        for (row_i, row) in self.params.rows.iter().enumerate() {
            if let GridDimensionSize::Grow(v) = row {
                row_heights[row_i] = remaining_height * (*v / row_total_grow);
            }
        }
        for (col_i, col) in self.params.cols.iter().enumerate() {
            if let GridDimensionSize::Grow(v) = col {
                col_widths[col_i] = remaining_width * (*v / col_total_grow);
            }
        }

        let make_cumulative = |mut dim_sizes: Vec<f32>| {
            let mut last_end = 0.;
            for i in 0..dim_sizes.len() {
                let v = last_end;
                last_end += dim_sizes[i].max(0.);
                dim_sizes[i] = v;
            }

            (dim_sizes, last_end)
        };

        let (row_starts, height) = make_cumulative(row_heights);
        let (col_starts, width) = make_cumulative(col_widths);

        Ok(GridViewLayout {
            outer_box: RenderBox { width, height },
            row_starts,
            col_starts,
        })
    }
}

impl ViewWithParams for GridView {
    type Params = GridViewParams;

    fn create_with_params(params: &Self::Params) -> Result<Box<dyn View>> {
        Ok(Box::new(Self {
            params: params.clone(),
            children: Children::new(&params.children)?,
            state: GridViewState::default(),
        }))
    }

    fn update_with_params(&mut self, new_params: &Self::Params) -> Result<()> {
        // TODO: Must also potentially change the mouse focus?

        self.params = new_params.clone();
        self.children.update(&new_params.children)?;
        Ok(())
    }
}

impl View for GridView {
    fn build(&mut self) -> Result<ViewStatus> {
        let mut status = ViewStatus::default();

        for i in 0..self.children.len() {
            let status_i = self.children[i].build()?;

            if self.state.last_mouse_focus == Some(i) {
                status.cursor = status_i.cursor;
            }

            if status_i.focused {
                if self.state.last_key_focus != Some(i) && self.state.last_key_focus.is_some() {
                    let last_i = self.state.last_key_focus.unwrap();

                    self.children[last_i].handle_event(&Event::Blur)?;
                    if last_i < i {
                        // TODO: May also need to see if the other fields in this return value have
                        // changed and would impact the overall status.
                        let _ = self.children[last_i].build()?;
                    }
                }

                self.state.last_key_focus = Some(i);
                status.focused = true;
            }
        }

        if !status.focused {
            self.state.last_key_focus = None;
        }

        Ok(status)
    }

    fn layout(&self, parent_box: &RenderBox) -> Result<RenderBox> {
        self.layout_impl(parent_box).map(|v| v.outer_box)
    }

    fn render(&mut self, parent_box: &RenderBox, canvas: &mut dyn Canvas) -> Result<()> {
        let layout = self.layout_impl(parent_box)?;

        // TODO: Store the actual rendered box of each child so that mouse events can
        // distinguish between clicking on a child element or just near that element.

        for (child_i, child) in self.children.iter_mut().enumerate() {
            let row_i = child_i / self.params.cols.len();
            let col_i = child_i % self.params.cols.len();

            let x_min = layout.col_starts[col_i];
            let y_min = layout.row_starts[row_i];
            let x_max = layout
                .col_starts
                .get(col_i + 1)
                .unwrap_or(&layout.outer_box.width);
            let y_max = layout
                .row_starts
                .get(row_i + 1)
                .unwrap_or(&layout.outer_box.height);

            canvas.save();
            canvas.translate(x_min, y_min);

            let inner_box = RenderBox {
                width: x_max - x_min,
                height: y_max - y_min,
            };

            // TODO: clip the drawn contents to each grid box.
            child.render(&inner_box, canvas)?;

            canvas.restore();
        }

        self.state.layout = Some(layout);

        Ok(())
    }

    fn handle_event(&mut self, event: &Event) -> Result<()> {
        let layout = match self.state.layout.as_ref() {
            Some(v) => v,
            None => {
                return Ok(());
            }
        };

        match event {
            Event::Mouse(e) => {
                let child_idx = if e.relative_x < 0.
                    || e.relative_x > layout.outer_box.width
                    || e.relative_y < 0.
                    || e.relative_y > layout.outer_box.height
                {
                    None
                } else {
                    let col_i = common::algorithms::upper_bound(&layout.col_starts, &e.relative_x)
                        .unwrap_or(0);
                    let row_i = common::algorithms::upper_bound(&layout.row_starts, &e.relative_y)
                        .unwrap_or(0);

                    Some(row_i * self.params.cols.len() + col_i)
                };

                // Send exit event if child has changed.
                // TODO: Also send an enter exit on changes
                // TODO: Make sure the child still exists!
                if self.state.last_mouse_focus != child_idx {
                    // Send exit event
                    if let Some(old_child) = self.state.last_mouse_focus.clone() {
                        let mut exit_event = e.clone();
                        exit_event.kind = MouseEventKind::Exit;
                        // TODO: Calculate right offset.

                        self.children[old_child].handle_event(&Event::Mouse(exit_event))?;
                    }

                    // Send enter event
                    if let Some(new_child) = child_idx.clone() {
                        let mut enter_event = e.clone();
                        enter_event.kind = MouseEventKind::Enter;

                        // TODO: Dedup
                        {
                            let row_i = new_child / self.params.cols.len();
                            let col_i = new_child % self.params.cols.len();
                            enter_event.relative_x -= layout.col_starts[col_i];
                            enter_event.relative_y -= layout.row_starts[row_i];
                        }

                        self.children[new_child].handle_event(&Event::Mouse(enter_event))?;
                    }
                }

                // Send event itself
                if let Some(new_child) = child_idx.clone() {
                    let mut inner_event = e.clone();
                    if inner_event.kind == MouseEventKind::Enter
                        || inner_event.kind == MouseEventKind::Exit
                    {
                        inner_event.kind = MouseEventKind::Move;
                    }

                    // TODO: Dedup
                    {
                        let row_i = new_child / self.params.cols.len();
                        let col_i = new_child % self.params.cols.len();
                        inner_event.relative_x -= layout.col_starts[col_i];
                        inner_event.relative_y -= layout.row_starts[row_i];
                    }

                    self.children[new_child].handle_event(&Event::Mouse(inner_event))?;
                }

                // Clicking outside of a focused element should blur it.
                if let Some(key_focus_idx) = self.state.last_key_focus.clone() {
                    if let MouseEventKind::ButtonDown(_) = e.kind {
                        if Some(key_focus_idx) != child_idx {
                            self.children[key_focus_idx].handle_event(&Event::Blur)?;
                            self.state.last_key_focus = None;
                        }
                    }
                }

                self.state.last_mouse_focus = child_idx;
            }
            Event::Blur => {
                if let Some(idx) = self.state.last_key_focus.clone() {
                    self.children[idx].handle_event(event)?;
                }
            }
            Event::Key(e) => {
                // TODO: Also pass along focused path.
                for c in &mut self.children[..] {
                    c.handle_event(event)?;
                }
            }
        }

        Ok(())
    }
}
