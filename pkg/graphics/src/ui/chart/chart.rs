use std::rc::Rc;
use core::f32::consts::PI;

use common::errors::*;
use image::Color;
use math::matrix::vec2f;
use math::matrix::Vector2f;

use crate::canvas::*;
use crate::font::CanvasFontRenderer;
use crate::font::{FontStyle, VerticalAlign, TextAlign};
use crate::ui::children::Children;
use crate::ui::element::Element;
use crate::ui::event::*;
use crate::ui::view::*;

/*
TODO: Document our naming convention of using 'Options' for native types and 'Config' for serializable types.

For now this will always be 200 x 800.
*/

#[derive(Clone)]
pub struct ChartViewParams {
    pub options: ChartOptions,
    pub data: ChartData,
    // pub inner: Element,
}

#[derive(Clone)]
pub struct ChartOptions {
    // Space in pixels between the boundary of the canvas and the inner plot.
    // This space is used for drawing axis labels, etc.
    pub margin: Margin,

    pub grid: Grid,

    pub data_line_width: f32,    // 1
    pub data_line_color: Color,  // '#4af'
    pub data_point_paint: Paint, // '#4af'
    pub data_point_size: f32,

    pub font_family: Rc<CanvasFontRenderer>,
    pub font_size: f32,
}

#[derive(Clone)]
pub struct Grid {
    pub line_width: f32,
    pub line_color: Color,
    pub label_paint: Paint,
    pub x_ticks: Vec<Tick>,
    pub y_ticks: Vec<Tick>
}

#[derive(Clone)]
pub struct ChartData {
    /// NOTE: This should always be sorted in order of increasing x coordinate.
    pub points: Vec<Vector2f>,
    pub x_range: Range,
    pub y_range: Range,
}

#[derive(Clone)]
pub struct Margin {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

#[derive(Clone)]
pub struct Range {
    pub min: f32,
    pub max: f32,
}

pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone)]
pub struct Tick {
    pub value: f32,
    pub label: String,
}

impl ViewParams for ChartViewParams {
    type View = ChartView;
}

const CHART_HEIGHT: f32 = 200.;
const CHART_WIDTH: f32 = 800.;

pub struct ChartView {
    params: ChartViewParams,

    /// Relative to the top-left corner of the canvas, the location of the
    /// coordinate system in which we will plot user points.
    graph_rect: Rect,

    /// If the user's mouse is hovering over the graph, then this will be the mouse position in the canvas coordinate system.
    mouse_canvas_pos: Option<Vector2f>,
    // children: Children,
}

impl ViewWithParams for ChartView {
    type Params = ChartViewParams;

    fn create_with_params(params: &Self::Params) -> Result<Box<dyn View>> {


        // TODO: This needs to be dynamically recom
        let graph_rect = Rect {
            x: params.options.margin.left,
            width: CHART_WIDTH - (params.options.margin.right + params.options.margin.left),
            y: params.options.margin.top,
            height: CHART_HEIGHT - (params.options.margin.top + params.options.margin.bottom),
        };

        Ok(Box::new(Self {
            params: params.clone(),
            graph_rect,
            mouse_canvas_pos: None,
            // children: Children::new(core::slice::from_ref(&params.inner))?,
        }))
    }

    fn update_with_params(&mut self, new_params: &Self::Params) -> Result<()> {
        self.params = new_params.clone();
        // self.children
        //     .update(core::slice::from_ref(&new_params.inner))?;
        Ok(())
    }
}

impl ChartView {
    fn draw_frame(&self, canvas: &mut dyn Canvas) -> Result<()> {
        canvas.save();

        // this._ctx.clearRect(0, 0, this.state.canvas_width, this.state.canvas_height);

        for tick in &self.params.options.grid.x_ticks {
            if tick.value < self.params.data.x_range.min || tick.value > self.params.data.x_range.max {
                break;
            }

            let mut x_canvas = self.to_canvas_point(&vec2f(tick.value, f32::NAN)).x();

            // Make the lines sharp.
            // TODO: Make this adapt to the configured line width.
            x_canvas = (x_canvas + 0.5).round() - 0.5;

            let mut path = PathBuilder::new();
            path.move_to(vec2f(x_canvas, self.graph_rect.y));

            let y2 = self.graph_rect.y + self.graph_rect.height;
            path.line_to(vec2f(x_canvas, y2));

            canvas.stroke_path(
                &path.build(),
                self.params.options.grid.line_width,
                &self.params.options.grid.line_color,
            )?;

            let label = tick.label.as_str();
            let dims = self
                .params
                .options
                .font_family
                .measure_text(label, self.params.options.font_size)?;

            self.params.options.font_family.fill_text(
                x_canvas,
                y2 + 4.,
                label,
                &FontStyle::from_size(self.params.options.font_size).with_text_align(TextAlign::Center).with_vertical_align(VerticalAlign::Top),
                &self.params.options.grid.label_paint,
                canvas,
            )?;
        }

        for tick in &self.params.options.grid.y_ticks {
            let mut y_canvas = self.to_canvas_point(&vec2f(f32::NAN, tick.value)).y();
            y_canvas = (y_canvas + 0.5).round() - 0.5;

            let mut path = PathBuilder::new();
            path.move_to(vec2f(self.graph_rect.x, y_canvas));
            path.line_to(vec2f(self.graph_rect.x + self.graph_rect.width, y_canvas));

            canvas.stroke_path(
                &path.build(),
                self.params.options.grid.line_width,
                &self.params.options.grid.line_color,
            )?;

            let label = tick.label.as_str();
            let dims = self
                .params
                .options
                .font_family
                .measure_text(label, self.params.options.font_size)?;

            self.params.options.font_family.fill_text(
                self.graph_rect.x - 10.,
                y_canvas,
                label,
                &FontStyle::from_size(self.params.options.font_size).with_text_align(TextAlign::Right).with_vertical_align(VerticalAlign::Center),
                &self.params.options.grid.label_paint,
                canvas,
            )?;
        }

        // this._ctx.beginPath();
        // this._ctx.rect(this.state.graph_rect.x, this.state.graph_rect.y,
        // this.state.graph_rect.width, this.state.graph_rect.height);
        // this._ctx.clip();

        let mut closest_graph_pt = None;
        let mut closest_distance = 10.; // Must be within 10 pixels to allow a match at all.

        {
            let mut path = PathBuilder::new();

            let mut is_first = true;

            for graph_pt in &self.params.data.points {
                let pt = self.to_canvas_point(graph_pt);

                if is_first {
                    path.move_to(pt.clone());
                    is_first = false;
                } else {
                    path.line_to(pt.clone());
                }

                // TODO: Also require a minimum y match.
                if let Some(mouse_pos) = &self.mouse_canvas_pos {
                    let distance = (pt.x() - mouse_pos.x()).abs();
                    if distance < closest_distance {
                        closest_distance = distance;
                        closest_graph_pt = Some(graph_pt);
                    }
                }
            }

            canvas.stroke_path(
                &path.build(),
                self.params.options.data_line_width,
                &self.params.options.data_line_color,
            )?;
        }

        if let Some(graph_pt) = closest_graph_pt.take() {
            let pt = self.to_canvas_point(graph_pt);

            {
                let mut path = PathBuilder::new();
                path.ellipse(
                    pt,
                    vec2f(self.params.options.data_point_size, self.params.options.data_point_size),
                    0.0,
                    2.0 * PI,
                );
                // TODO: Switch to using the color.
                canvas.fill_path(&path.build(), &self.params.options.data_point_paint.color)?;
            }

            // TODO: Draw it.
        }

        /*
        if (closest_graph_pt !== null) {
            let pt = this._to_canvas_pt(closest_graph_pt);

            let position = {
                x: this._mouse_canvas_pos.x + 20,
                y: this._mouse_canvas_pos.y + 20
            }

            this.setState({
                tooltip: {
                    position,
                    right_align: false,

                    x_value: round_digits(closest_graph_pt.x, 2) + '',

                    lines: [
                        {
                            label: 'Sensor 1',
                            y_value: round_digits(closest_graph_pt.y, 2) + '',
                            color: '#4af'
                        }
                    ]
                }
            })
        } else if (this.state.tooltip) {
            this.setState({ tooltip: null })
        }
        */

        canvas.restore();

        Ok(())
    }

    /// Converts a point from the graph's coordinate system to the screen
    /// canvas's coordinate system.
    fn to_canvas_point(&self, pt: &Vector2f) -> Vector2f {
        vec2f(
            ((pt.x() - self.params.data.x_range.min) / (self.params.data.x_range.max - self.params.data.x_range.min))
                * self.graph_rect.width
                + self.graph_rect.x,
            // TODO: Must invert this.
            ((pt.y() - self.params.data.y_range.min) / (self.params.data.y_range.max - self.params.data.y_range.min))
                * self.graph_rect.height
                + self.graph_rect.y,
        )
    }
}

impl View for ChartView {
    fn build(&mut self) -> Result<ViewStatus> {
        Ok(ViewStatus {
            cursor: MouseCursor(glfw::StandardCursor::Hand),
            focused: false,
        })
    }

    fn layout(&self, parent_box: &RenderBox) -> Result<RenderBox> {
        Ok(RenderBox {
            width: 800.,
            height: 200.,
        })
    }

    fn render(&mut self, parent_box: &RenderBox, canvas: &mut dyn Canvas) -> Result<()> {
        self.draw_frame(canvas)?;
        Ok(())
    }

    fn handle_event(&mut self, event: &Event) -> Result<()> {
        match event {
            Event::Mouse(e) => {
                let pos = vec2f(e.relative_x, e.relative_y);

                match e.kind {
                    MouseEventKind::Move => {
                        self.mouse_canvas_pos = Some(pos);
                    }
                    MouseEventKind::Exit => {
                        self.mouse_canvas_pos = None;
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        Ok(())
    }
}

/*

interface TooltipData {
    // Position of the tooltip relative to the top-left corner of the canvas.
    position: Point,

    right_align: boolean,

    x_value: string,

    lines: {
        label: string,
        y_value: string,
        color: string
    }[]
}

interface FigureState {
    // Dimensions of the entire canvas.
    canvas_height?: number,
    canvas_width?: number,

    tooltip?: TooltipData
}

interface TooltipProps {
    data: TooltipData
}

class Tooltip extends React.Component<TooltipProps> {

    render() {
        let data = this.props.data;

        // TODO: It would be a better experience if this was a fixed width.

        return (
            <div style={{ position: 'absolute', top: data.position.y, left: data.position.x, padding: 5, backgroundColor: '#fff', border: '1px solid #ccc', fontSize: 12 }}>
                <div style={{ fontWeight: 'bold', paddingBottom: 4 }}>
                    {data.x_value}
                </div>
                <div>
                    {data.lines.map((line, i) => {
                        return (
                            <div key={i}>
                                <div style={{ display: 'inline-block', backgroundColor: line.color, width: 10, height: 5 }}></div>
                                <div style={{ display: 'inline-block', minWidth: 60, paddingRight: 4, paddingLeft: 4, fontWeight: 'bold' }}>
                                    {line.label + ':'}
                                </div>
                                <div style={{ textAlign: 'right', display: 'inline-block' }}>
                                    {line.y_value}
                                </div>
                            </div>
                        );
                    })}
                </div>
            </div>
        );
    }

}

function round_digits(num: number, digits: number): number {
    let scale = Math.pow(10, digits);
    return Math.round(num * scale) / scale;
}

class Figure extends React.Component<{}, FigureState> {
    _root: HTMLElement;
    _canvas: HTMLCanvasElement;
    _ctx: CanvasRenderingContext2D;

    _options: GraphOptions;

    state = {
        canvas_height: null,
        canvas_width: null,
        graph_rect: null,
        tooltip: null
    };

    componentDidMount() {
        this.setState({
            canvas_height,
            canvas_width,
            graph_rect:
        }, () => {
            this._make_request();
        });
    }

    render() {
        // TODO: Don't update the canvas as it will end up having dynamic width/height.
        return (
            <div style={{ fontSize: 0, position: 'relative' }} ref={(el) => { this._root = el; }} onMouseMove={(e) => this._on_mouse_move(e)} onMouseOut={() => this._on_mouse_out()}>
                <canvas width={this.state.canvas_width} height={this.state.canvas_height} style={{ cursor: 'pointer' }} ref={(el) => { this._canvas = el; }}></canvas>
                {this.state.tooltip ? <Tooltip data={this.state.tooltip} /> : null}
            </div>
        );
    }
};

*/
