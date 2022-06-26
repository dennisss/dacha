use common::errors::*;

use crate::canvas::Canvas;
use crate::ui::container::*;
use crate::ui::element::Element;
use crate::ui::event::*;
use crate::ui::view::*;
use crate::ui::range::*;

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
    container: Container,
    state: GridViewState,
}

#[derive(Default)]
struct GridViewState {
    layout: Option<GridViewLayout>,
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
    fn layout_impl(&self, constraints: &LayoutConstraints) -> Result<GridViewLayout> {
        // TODO: It would be nice if this also supported doing vertical alignment to the
        // baseline.

        if self.params.rows.len() * self.params.cols.len() != self.container.children().len() {
            return Err(err_msg("Incorrect number of children"));
        }

        let mut row_heights = vec![0.; self.params.rows.len()];
        let mut col_widths = vec![0.; self.params.cols.len()];

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

        //
        let mut remaining_height = constraints.max_height;
        let mut remaining_width = constraints.max_width;

        // Step 1: Resolve all 'fixed' size rows/cols.
        Self::calculate_fixed_dims(
            &self.params.rows,
            constraints.max_height,
            &mut row_heights,
            &mut remaining_height,
        );
        Self::calculate_fixed_dims(
            &self.params.cols,
            constraints.max_width,
            &mut col_widths,
            &mut remaining_width,
        );

        // Step 2: Give all the FitContent columns as much space as they want
        // NOTE: Columns sizes are prioritized over row sizes.
        for (col_i, col) in self.params.cols.iter().enumerate() {
            if let GridDimensionSize::FitContent = col {
                let mut max_width: f32 = 0.;
                for row_i in 0..self.params.rows.len() {
                    let i = row_i * self.params.cols.len() + col_i;
                    let inner_box = self.container.children()[i].layout(&LayoutConstraints {
                        max_width: remaining_width,
                        max_height: remaining_height, // TODO: Pick a better value
                        start_cursor: None,
                    })?;

                    max_width = max_width.max(inner_box.width);
                }

                remaining_width -= max_width;
                col_widths[col_i] = max_width;
            }
        }

        // Step 3: Give all Grow columns a fair amount of space.
        for (col_i, col) in self.params.cols.iter().enumerate() {
            if let GridDimensionSize::Grow(v) = col {
                col_widths[col_i] = remaining_width * (*v / col_total_grow);
            }
        }

        // Step 4: Fit Rows (must be calculated after all column dimensions are
        // resolved).
        for (row_i, row) in self.params.rows.iter().enumerate() {
            if let GridDimensionSize::FitContent = row {
                let mut max_height: f32 = 0.;
                for col_i in 0..self.params.cols.len() {
                    let i = row_i * self.params.cols.len() + col_i;
                    let inner_box = self.container.children()[i].layout(&LayoutConstraints {
                        max_width: col_widths[col_i],
                        max_height: remaining_height,
                        start_cursor: None,
                    })?;

                    max_height = max_height.max(inner_box.height);
                }

                remaining_height -= max_height;
                row_heights[row_i] = max_height;
            }
        }

        // Step 3: Calculate remaining grow elements.

        for (row_i, row) in self.params.rows.iter().enumerate() {
            if let GridDimensionSize::Grow(v) = row {
                row_heights[row_i] = remaining_height * (*v / row_total_grow);
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
            outer_box: RenderBox {
                width,
                height,
                baseline_offset: 0.,
                range: CursorRange::zero(),
                next_cursor: None,
            },
            row_starts,
            col_starts,
        })
    }

    fn calculate_fixed_dims(
        dims: &[GridDimensionSize],
        dim_limit: f32,
        dim_sizes: &mut [f32],
        remaining_space: &mut f32,
    ) {
        for (dim_i, dim) in dims.iter().enumerate() {
            if let GridDimensionSize::Absolute(v) = dim {
                dim_sizes[dim_i] = *v;
            } else if let GridDimensionSize::Percentage(v) = dim {
                dim_sizes[dim_i] = dim_limit * v;
            } else {
                continue;
            }

            *remaining_space -= dim_sizes[dim_i];
        }
    }
}

impl ViewWithParams for GridView {
    type Params = GridViewParams;

    fn create_with_params(params: &Self::Params) -> Result<Box<dyn View>> {
        Ok(Box::new(Self {
            params: params.clone(),
            container: Container::new(&params.children)?,
            state: GridViewState::default(),
        }))
    }

    fn update_with_params(&mut self, new_params: &Self::Params) -> Result<()> {
        // TODO: Must also potentially change the mouse focus?

        self.params = new_params.clone();
        self.container.update(&new_params.children)?;
        Ok(())
    }
}

impl View for GridView {
    fn build(&mut self) -> Result<ViewStatus> {
        self.container.build()
    }

    fn layout(&self, constraints: &LayoutConstraints) -> Result<RenderBox> {
        self.layout_impl(constraints).map(|v| v.outer_box)
    }

    fn render(&mut self, constraints: &LayoutConstraints, canvas: &mut dyn Canvas) -> Result<()> {
        let layout = self.layout_impl(constraints)?;

        // TODO: Store the actual rendered box of each child so that mouse events can
        // distinguish between clicking on a child element or just near that element.

        for (child_i, child) in self.container.children_mut().iter_mut().enumerate() {
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

            let inner_constraints = LayoutConstraints {
                max_width: x_max - x_min,
                max_height: y_max - y_min,
                start_cursor: None,
            };

            // TODO: clip the drawn contents to each grid box.
            child.render(&inner_constraints, canvas)?;

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

        self.container.handle_event(event, layout)
    }
}

impl ContainerLayout for GridViewLayout {
    fn find_closest_span(&self, x: f32, y: f32) -> Option<Span> {
        if x < 0. || x > self.outer_box.width || y < 0. || y > self.outer_box.height {
            None
        } else {
            let col_i = common::algorithms::upper_bound(&self.col_starts, &x).unwrap_or(0);
            let row_i = common::algorithms::upper_bound(&self.row_starts, &y).unwrap_or(0);

            let num_cols = self.col_starts.len();
            Some(Span {
                child_index: row_i * num_cols + col_i,
                range: None,
            })
        }
    }

    fn get_span_rect(&self, span: Span) -> Rect {
        let num_cols = self.col_starts.len();

        let row_i = span.child_index / num_cols;
        let col_i = span.child_index % num_cols;

        Rect {
            x: self.col_starts[col_i],
            y: self.row_starts[row_i],
            // TODO: Populate these
            width: 0.,
            height: 0.,
        }
    }
}
