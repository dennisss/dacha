// UI for manually generating geometry (points, lines, polygons) and running
// simple post-processing on them.
//
// Coordinates are printed with the bottom-left corner of the screen as the (0,
// 0) origin.
//
// Controls:
// - 'i': Toggle the background image showing.
// - 'c': Clear the screen.
// - 'm': Change the current mode
// - 'n': When in polygon mode, close the current polygon by connecting the last
//   and first vertex.
// - 'p': Print out the current state.
// - 'a': Togllge showing arrows to indicate line direction.
// - 1-9: Mode specific functions

use std::f32::consts::PI;

use common::async_std::fs;
use common::errors::*;
use image::Color;
use image::Image;
use math::geometry::half_edge::*;
use math::geometry::line_segment::LineSegment2;
use math::matrix::{vec2f, Vector2f};

use crate::canvas::*;
use crate::opengl::canvas::OpenGLCanvas;
use crate::opengl::window::Window;
use crate::raster::canvas::RasterCanvas;
use crate::raster::canvas_render_loop::WindowOptions;

const POINT_SIZE: usize = 4;

fn format_points(points: &[Vector2f]) -> String {
    let mut s = String::new();
    s.push_str("vec![");
    for v in points {
        s.push_str(&format!("vec2f({:.2}, {:.2}), ", v.x(), v.y()));
    }
    s.push_str("]");

    s
}

struct PointPicker {
    mode: Mode,
    style: Style,
    points: Vec<Vector2f>,
    background_image: Option<Image<u8>>,
    background_image_object: Option<Box<dyn CanvasObject>>,
    background_image_visible: bool,
}

struct Style {
    background_point_color: Color,
    neutral_point_color: Color,
    highlight_point_color: Color,

    background_line_color: Color,
    neutral_line_color: Color,
    secondary_line_color: Color,
    primary_line_color: Color,
}

enum Tone {
    Background,
    Neutral,
    Secondary,
    Primary,
}

enum Mode {
    None,

    /// Picking individual unconnected points.
    ///
    /// Features
    /// - Convex hull
    Points(PointsState),

    /// Picking unconnected lines (each 2 points is one line)
    ///
    /// Features:
    /// - Intersections
    Lines(LinesState),

    /// Picking connecting points which form one or more connected polygons.
    ///
    /// Features:
    /// - Overlap repair (enumerate all faces)
    /// - Further make monotone
    /// - Further triangulate.
    Polygons(PolygonsState),
}

#[derive(Default)]
struct PointsState {
    convex_hull: Option<Vec<Vector2f>>,
}

#[derive(Default)]
struct LinesState {
    intersections: Option<Vec<Vector2f>>,
}

struct PolygonsState {
    /// Index of the first point in each polygon (always has at least one
    /// element of value 0).
    start_indices: Vec<usize>,

    view_mode: PolygonViewMode,
}

struct Polygon<'a> {
    points: &'a [Vector2f],
    closed: bool,
}

impl Default for PolygonsState {
    fn default() -> Self {
        Self {
            start_indices: vec![0],
            view_mode: PolygonViewMode::Raw { focus_index: None },
        }
    }
}

enum PolygonViewMode {
    Raw {
        focus_index: Option<usize>,
    },
    Faces {
        focus_index: Option<usize>,
        faces: Vec<FaceDebug<()>>,
    },
}

impl PointPicker {
    pub fn handle_events(
        &mut self,
        canvas: &dyn Canvas,
        window: &mut Window,
        events: &[glfw::WindowEvent],
    ) -> Result<()> {
        if let Mode::None = &self.mode {
            self.mode = Mode::Points(PointsState::default());
            self.print_usage();
        }

        for e in events {
            if let glfw::WindowEvent::MouseButton(glfw::MouseButtonLeft, glfw::Action::Press, _) = e
            {
                let (x, y) = window.raw().get_cursor_pos();

                // Transform to canvas dimensions.
                let (x, y) = (x as f32, window.height() as f32 - y as f32);

                println!("X: {},  Y: {}", x, y);

                let point = Vector2f::from_slice(&[x, y]);
                self.handle_new_point(point);
            }

            if let glfw::WindowEvent::Key(key, _, glfw::Action::Press, _) = e {
                self.handle_key_press(*key)?;
            }
        }

        Ok(())
    }

    fn handle_new_point(&mut self, point: Vector2f) {
        self.points.push(point);

        if let Mode::Polygons(state) = &mut self.mode {
            state.view_mode = PolygonViewMode::Raw { focus_index: None };
        }
    }

    fn handle_key_press(&mut self, key: glfw::Key) -> Result<()> {
        if key == glfw::Key::I {
            self.background_image_visible = !self.background_image_visible;
            return Ok(());
        }

        if key == glfw::Key::C {
            self.clear();
            return Ok(());
        }

        if key == glfw::Key::M {
            self.cycle_mode();
            return Ok(());
        }

        if key == glfw::Key::P {
            println!("All points: {}", format_points(&self.points));
            return Ok(());
        }

        match &mut self.mode {
            Mode::None => {}
            Mode::Points(state) => match key {
                glfw::Key::Num1 => {
                    if state.convex_hull.is_some() {
                        state.convex_hull = None;
                    } else if self.points.len() >= 3 {
                        let hull = math::geometry::convex_hull::convex_hull(&self.points)?;
                        println!("Convex Hull: {}", format_points(&hull));
                        state.convex_hull = Some(hull);
                    }
                }
                _ => {}
            },
            Mode::Lines(state) => match key {
                glfw::Key::Num1 => {
                    if state.intersections.is_some() {
                        state.intersections = None;
                        return Ok(());
                    }

                    let mut segments = vec![];
                    for pair in self.points.chunks_exact(2) {
                        segments.push(LineSegment2 {
                            start: pair[0].clone(),
                            end: pair[1].clone(),
                        });
                    }

                    println!("Segments: {:?}", segments);

                    let ints = LineSegment2::intersections(&segments, 1e-3)
                        .into_iter()
                        .map(|i| i.point)
                        .collect::<Vec<_>>();

                    println!("Intersections: {}", format_points(&ints));
                    state.intersections = Some(ints);
                }
                _ => {}
            },
            Mode::Polygons(state) => {
                match key {
                    glfw::Key::N => {
                        // TODO: Verify each polygon has at least 3 points.
                        state.start_indices.push(self.points.len());
                    }
                    glfw::Key::Num1 => {
                        let mut next_idx = if let PolygonViewMode::Raw {
                            focus_index: Some(idx),
                        } = &state.view_mode
                        {
                            *idx + 1
                        } else {
                            0
                        };

                        loop {
                            if next_idx >= state.start_indices.len() {
                                state.view_mode = PolygonViewMode::Raw { focus_index: None };
                                println!("Raw: Showing all polygons");
                                break;
                            }

                            let poly = Self::get_polygon(&self.points, state, next_idx);
                            if !poly.points.is_empty() {
                                state.view_mode = PolygonViewMode::Raw {
                                    focus_index: Some(next_idx),
                                };
                                println!("Raw: Showing polygon[{}]", next_idx);
                                break;
                            }

                            next_idx += 1;
                        }
                    }
                    glfw::Key::Num2 | glfw::Key::Num3 | glfw::Key::Num4 => {
                        if let PolygonViewMode::Faces { focus_index, faces } = &mut state.view_mode
                        {
                            let next_index = focus_index.clone().map(|v| v + 1).unwrap_or(0);
                            if next_index < faces.len() {
                                *focus_index = Some(next_index);
                            } else {
                                *focus_index = None;
                            }

                            return Ok(());
                        }

                        let mut data = HalfEdgeStruct::<()>::new();

                        for poly_i in 0..state.start_indices.len() {
                            let poly = Self::get_polygon(&self.points, state, poly_i);
                            if !poly.closed || poly.points.len() < 3 {
                                continue;
                            }

                            let first_edge = data.add_first_edge(
                                poly.points[0].clone(),
                                poly.points[1].clone(),
                                (),
                            );
                            let mut next_edge =
                                data.add_next_edge(first_edge, poly.points[2].clone());
                            for point in &poly.points[3..] {
                                next_edge = data.add_next_edge(next_edge, point.clone());
                            }
                            data.add_close_edge(next_edge, first_edge);
                        }

                        data.repair();

                        if key == glfw::Key::Num3 || key == glfw::Key::Num4 {
                            println!("Make monotone!");
                            data.make_y_monotone();
                            data.repair();
                        }

                        if key == glfw::Key::Num4 {
                            println!("Triangulate!");
                            data.triangulate_monotone();
                            data.repair();
                        }

                        let faces = FaceDebug::get_all(&data);

                        state.view_mode = PolygonViewMode::Faces {
                            focus_index: None,
                            faces,
                        };
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn clear(&mut self) {
        self.points.clear();

        match &mut self.mode {
            Mode::None => {}
            Mode::Points(state) => {
                *state = PointsState::default();
            }
            Mode::Lines(state) => {
                *state = LinesState::default();
            }
            Mode::Polygons(state) => {
                *state = PolygonsState::default();
            }
        }
    }

    fn cycle_mode(&mut self) {
        self.points.clear();

        match &self.mode {
            Mode::None => {}
            Mode::Points(state) => {
                self.mode = Mode::Lines(LinesState::default());
            }
            Mode::Lines(state) => {
                self.mode = Mode::Polygons(PolygonsState::default());
            }
            Mode::Polygons(state) => {
                self.mode = Mode::Points(PointsState::default());
            }
        }

        self.print_usage();
    }

    fn print_usage(&self) {
        println!("=====================");
        match self.mode {
            Mode::None => {}
            Mode::Points(_) => {
                println!("Points mode:");
                println!("- 1: Convex hull (toggle)");
            }
            Mode::Lines(_) => {
                println!("Lines mode:");
                println!("- 1: Intersections (toggle)");
            }
            Mode::Polygons(_) => {
                println!("Polygons mode:");
                println!("- n: Start next polygon (closing current one)");
                println!("- 1: Raw polygon cycle (show all or just one)");
                println!("- 2: Face cycle (red is outer. green is inner)");
                println!("- 3: Monotone face cycle");
                println!("- 4: Triangulation cycle");
            }
        }
    }

    /// NOTE: self is only mutable to support caching CanvasObjects.
    pub fn draw(&mut self, canvas: &mut dyn Canvas, window: &Window) -> Result<()> {
        canvas.clear_rect(
            0.,
            0.,
            window.width() as f32,
            window.height() as f32,
            &Color::rgb(255, 255, 255),
        )?;

        // Draw the background image.

        if let (Some(image), true) = (&self.background_image, self.background_image_visible) {
            let mut img = match self.background_image_object.as_mut() {
                Some(v) => v,
                None => self
                    .background_image_object
                    .insert(canvas.create_image(image)?),
            };

            img.draw(&Paint::alpha(0.2), canvas)?;
        }

        canvas.save();
        let height = window.height() as f32;
        canvas.translate(0., height);
        canvas.scale(1., -1.);

        // (0, height) first becomes (0, -height) and then we add (0, height) so it
        // becomes (0, 0)

        match &self.mode {
            Mode::None => {}
            Mode::Points(state) => self.draw_points_mode(state, canvas)?,
            Mode::Lines(state) => self.draw_lines_mode(state, canvas)?,
            Mode::Polygons(state) => self.draw_polygons_mode(state, canvas)?,
        }

        canvas.restore()?;

        Ok(())
    }

    fn draw_points_mode(&self, state: &PointsState, canvas: &mut dyn Canvas) -> Result<()> {
        for point in &self.points {
            self.draw_point(point.clone(), Tone::Neutral, canvas)?;
        }

        if let Some(hull) = &state.convex_hull {
            self.draw_polygon_edge(&hull, true, Tone::Neutral, canvas)?;
        }

        Ok(())
    }

    fn draw_lines_mode(&self, state: &LinesState, canvas: &mut dyn Canvas) -> Result<()> {
        for pair in self.points.chunks_exact(2) {
            let mut path = PathBuilder::new();
            path.move_to(pair[0].clone());
            path.line_to(pair[1].clone());
            canvas.stroke_path(&path.build(), 2., &self.style.neutral_line_color);
        }

        if self.points.len() > 0 {
            let last_point = self.points.last().unwrap();
            self.draw_point(last_point.clone(), Tone::Neutral, canvas)?;
        }

        if let Some(intersections) = &state.intersections {
            for point in intersections {
                self.draw_point(point.clone(), Tone::Primary, canvas)?;
            }
        }

        Ok(())
    }

    fn draw_polygons_mode(&self, state: &PolygonsState, canvas: &mut dyn Canvas) -> Result<()> {
        if self.points.len() == 0 {
            return Ok(());
        }

        match &state.view_mode {
            PolygonViewMode::Raw { focus_index } => {
                for poly_i in 0..state.start_indices.len() {
                    let poly = Self::get_polygon(&self.points, state, poly_i);

                    if poly.points.len() == 0 {
                        continue;
                    }

                    self.draw_polygon_edge(
                        poly.points,
                        poly.closed,
                        if focus_index.is_some() {
                            Tone::Background
                        } else {
                            Tone::Neutral
                        },
                        canvas,
                    )?;

                    // Draw the final point of the final polygon as an
                    // indication to the user of the
                    // last clicked position.
                    if !poly.closed {
                        let last_point = poly.points.last().unwrap().clone();
                        self.draw_point(last_point, Tone::Neutral, canvas)?;
                    }
                }

                if let Some(idx) = focus_index.clone() {
                    let poly = Self::get_polygon(&self.points, state, idx);
                    self.draw_polygon_edge(poly.points, poly.closed, Tone::Neutral, canvas)?;
                }
            }
            PolygonViewMode::Faces { faces, focus_index } => {
                for face in faces {
                    for points in face
                        .outer_component
                        .iter()
                        .chain(face.inner_components.iter())
                    {
                        self.draw_polygon_edge(
                            &points,
                            true,
                            if focus_index.is_some() {
                                Tone::Background
                            } else {
                                Tone::Neutral
                            },
                            canvas,
                        )?;
                    }
                }

                if let Some(idx) = focus_index.clone() {
                    let face = &faces[idx];

                    if let Some(points) = &face.outer_component {
                        self.draw_polygon_edge(&points, true, Tone::Primary, canvas)?;
                    }

                    for points in face.inner_components.iter() {
                        self.draw_polygon_edge(&points, true, Tone::Secondary, canvas)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn get_polygon<'a>(
        all_points: &'a [Vector2f],
        state: &PolygonsState,
        index: usize,
    ) -> Polygon<'a> {
        let start_index = state.start_indices[index];
        let (end_index, closed) = if index + 1 < state.start_indices.len() {
            (state.start_indices[index + 1], true)
        } else {
            (all_points.len(), false)
        };

        Polygon {
            points: &all_points[start_index..end_index],
            closed,
        }
    }

    fn draw_point(&self, point: Vector2f, tone: Tone, canvas: &mut dyn Canvas) -> Result<()> {
        // TODO: Ideally re-use the same path object (with any linearization or
        // triangulation applied) per scale.

        // TODO: Cache these paths.

        let mut path = PathBuilder::new();
        path.ellipse(
            point,
            Vector2f::from_slice(&[POINT_SIZE as f32, POINT_SIZE as f32]),
            0.,
            2. * PI,
        );

        canvas.fill_path(
            &path.build(),
            match tone {
                Tone::Background => &self.style.background_point_color,
                Tone::Neutral => &self.style.neutral_point_color,
                Tone::Secondary => todo!(),
                Tone::Primary => &self.style.highlight_point_color,
            },
        )
    }

    fn draw_polygon_edge(
        &self,
        points: &[Vector2f],
        closed: bool,
        tone: Tone,
        canvas: &mut dyn Canvas,
    ) -> Result<()> {
        if points.len() < 2 {
            return Ok(());
        }

        let mut path = PathBuilder::new();
        path.move_to(points[0].clone());
        for point in &points[1..] {
            path.line_to(point.clone());
        }

        if closed {
            path.close();
        }

        canvas.stroke_path(
            &path.build(),
            2.,
            match tone {
                Tone::Background => &self.style.background_line_color,
                Tone::Neutral => &self.style.neutral_line_color,
                Tone::Secondary => &self.style.secondary_line_color,
                Tone::Primary => &self.style.primary_line_color,
            },
        )
    }
}

/*
Some thoughts on
*/

pub async fn run() -> Result<()> {
    const HEIGHT: usize = 800;
    const WIDTH: usize = 800;

    let window_options = WindowOptions::new("Point Picker", WIDTH, HEIGHT);

    // let mut canvas = RasterCanvas::create(HEIGHT, WIDTH);

    let mut mode = Mode::Lines;

    // let mut points = vec![];
    let mut points = vec![
        // Vector2f::from_slice(&[335.00, 172.00]),
        // Vector2f::from_slice(&[207.00, 260.00]),
        // Vector2f::from_slice(&[221.00, 377.00]),
        // Vector2f::from_slice(&[295.00, 505.00]),
        // Vector2f::from_slice(&[502.00, 590.00]),
        // Vector2f::from_slice(&[599.00, 482.00]),
        // Vector2f::from_slice(&[596.00, 338.00]),
        // Vector2f::from_slice(&[462.00, 263.00]),
        // Vector2f::from_slice(&[511.00, 209.00]),
        // Vector2f::from_slice(&[301.00, 272.00]),
        // Vector2f::from_slice(&[410.00, 409.00]),
        // Vector2f::from_slice(&[421.00, 516.00]),
        // Vector2f::from_slice(&[540.00, 502.00]),
        // Vector2f::from_slice(&[525.00, 396.00]),
        // Vector2f::from_slice(&[309.00, 415.00]),
        // Vector2f::from_slice(&[241.00, 313.00]),
        // Vector2f::from_slice(&[391.00, 223.00]),
        // Vector2f::from_slice(&[346.00, 342.00]),
        // Vector2f::from_slice(&[497.00, 337.00]),
        // Vector2f::from_slice(&[391.00, 286.00]),
        // Vector2f::from_slice(&[361.00, 464.00]),
        vec2f(209.0, 247.0),
        vec2f(433.0, 441.0),
        vec2f(427.0, 229.0),
        vec2f(186.0, 461.0),
        vec2f(321.0, 457.0),
        vec2f(434.0, 340.0),
        vec2f(335.0, 266.0),
        vec2f(449.0, 420.0),
        ////
        // vec2f(186.14809, 334.0022),
        // vec2f(321.42886, 338.01053),
        // vec2f(435.73544, 354.34003),
        // vec2f(454.1976, 378.5716),
        // vec2f(431.4132, 576.3814),
        // vec2f(204.47021, 557.643),
        // vec2f(181.02863, 339.5343),
        // vec2f(190.97137, 338.4657),
        // vec2f(213.52979, 548.357),
        // vec2f(422.5868, 565.6186),
        // vec2f(443.80243, 381.4284),
        // vec2f(430.26456, 363.65997),
        // vec2f(320.57114, 347.98947),
        // vec2f(185.85193, 343.9978),
    ];

    let background_image =
        Image::read(project_path!("third_party/comp_geom/triangulate_d.qoi")).await?;

    let mut picker = PointPicker {
        mode: Mode::None,
        style: Style {
            background_point_color: Color::rgb(0xcc, 0xcc, 0xcc),
            background_line_color: Color::rgb(0xcc, 0xcc, 0xcc),
            neutral_point_color: Color::rgb(0, 0, 0),
            neutral_line_color: Color::rgb(0, 0, 0),
            highlight_point_color: Color::rgb(255, 0, 0),
            secondary_line_color: Color::rgb(0, 255, 0),
            primary_line_color: Color::rgb(255, 0, 0),
        },
        points,
        background_image: Some(background_image),
        background_image_object: None,
        background_image_visible: true,
    };

    // let mut canvas = RasterCanvas::create(HEIGHT, WIDTH);

    OpenGLCanvas::render_loop(window_options, |canvas, window, events| {
        picker.handle_events(canvas, window, events)?;
        picker.draw(canvas, window)?;
        Ok(())
    })
    .await?;

    Ok(())
}
