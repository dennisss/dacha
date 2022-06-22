use std::rc::Rc;

use image::Color;
use math::matrix::Vector2d;

use crate::canvas::Paint;
use crate::font::CanvasFontRenderer;


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
pub struct Margin {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

#[derive(Clone)]
pub struct Range {
    pub min: f64,
    pub max: f64,
}

#[derive(Clone, Debug)]
pub struct Tick {
    pub value: f64,
    pub label: String,
}

#[derive(Clone)]
pub struct ChartData {
    /// NOTE: This should always be sorted in order of increasing x coordinate.
    pub points: Vec<Vector2d>,
    pub x_range: Range,
    pub y_range: Range,
}