use std::f32::consts::PI;

use common::errors::*;
use image::Color;
use math::geometry::line_segment::LineSegment2f;
use math::matrix::Vector2f;

use crate::raster::canvas::{Canvas, PathBuilder};
use crate::raster::canvas_render_loop::WindowOptions;

const POINT_SIZE: usize = 4;

fn vec2f(x: f32, y: f32) -> Vector2f {
    Vector2f::from_slice(&[x, y])
}

fn format_points(points: &[Vector2f]) -> String {
    let mut s = String::new();
    s.push_str("vec![");
    for v in points {
        s.push_str(&format!(
            "Vector2f::from_slice(&[{:.2}, {:.2}]), ",
            v.x(),
            v.y()
        ));
    }
    s.push_str("]");

    s
}

enum Mode {
    Points,
    Lines,
}

pub async fn run() -> Result<()> {
    const HEIGHT: usize = 800;
    const WIDTH: usize = 800;

    let window_options = WindowOptions {
        name: "Point Picker".into(),
        width: WIDTH,
        height: HEIGHT,
    };

    let mut canvas = Canvas::create(HEIGHT, WIDTH);

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
    ];

    let mut hull = None;
    let mut intersections = None;

    canvas
        .render_loop(window_options, |canvas, window, events| {
            for e in events {
                if let glfw::WindowEvent::MouseButton(
                    glfw::MouseButtonLeft,
                    glfw::Action::Press,
                    _,
                ) = e
                {
                    let (x, y) = window.raw().get_cursor_pos();

                    println!("X: {},  Y: {}", x, y);

                    let point = Vector2f::from_slice(&[x as f32, y as f32]);

                    points.push(point);
                }

                if let glfw::WindowEvent::Key(glfw::Key::C, _, glfw::Action::Press, _) = e {
                    hull = Some(math::geometry::convex_hull::convex_hull(&points)?);

                    println!("{}", format_points(&hull.as_ref().unwrap()));
                }

                if let glfw::WindowEvent::Key(glfw::Key::I, _, glfw::Action::Press, _) = e {
                    let mut segments = vec![];
                    for pair in points.chunks_exact(2) {
                        segments.push(LineSegment2f {
                            start: pair[0].clone(),
                            end: pair[1].clone(),
                        });
                    }

                    println!("{:?}", segments);

                    intersections = Some(
                        LineSegment2f::intersections(&segments)
                            .into_iter()
                            .map(|i| i.point)
                            .collect::<Vec<_>>(),
                    );

                    println!("Intersect: {:?}", intersections);
                }

                if let glfw::WindowEvent::Key(glfw::Key::P, _, glfw::Action::Press, _) = e {
                    println!("{}", format_points(&points));
                }
            }

            let black = Color::rgb(0, 0, 0);
            let red = Color::rgb(255, 0, 0);

            canvas.drawing_buffer.clear_white();

            match mode {
                Mode::Points => {
                    for point in &points {
                        let mut path = PathBuilder::new();
                        path.ellipse(
                            point.clone(),
                            Vector2f::from_slice(&[POINT_SIZE as f32, POINT_SIZE as f32]),
                            0.,
                            2. * PI,
                        );

                        canvas.fill_path(&path.build(), &black)?;
                    }
                }
                Mode::Lines => {
                    for pair in points.chunks_exact(2) {
                        let mut path = PathBuilder::new();
                        path.move_to(pair[0].clone());
                        path.line_to(pair[1].clone());
                        canvas.stroke_path(&path.build(), 2., &black);
                    }

                    if points.len() > 0 {
                        let last_point = points.last().unwrap();

                        let mut path = PathBuilder::new();
                        path.ellipse(
                            last_point.clone(),
                            Vector2f::from_slice(&[POINT_SIZE as f32, POINT_SIZE as f32]),
                            0.,
                            2. * PI,
                        );

                        canvas.fill_path(&path.build(), &black)?;
                    }
                }
            }

            if let Some(hull) = &hull {
                let mut path = PathBuilder::new();
                path.move_to(hull[0].clone());
                for point in &hull[1..] {
                    path.line_to(point.clone());
                }
                path.close();

                canvas.stroke_path(&path.build(), 2., &black)?;
            }

            if let Some(intersections) = &intersections {
                for point in intersections {
                    let mut path = PathBuilder::new();
                    path.ellipse(
                        point.clone(),
                        Vector2f::from_slice(&[POINT_SIZE as f32, POINT_SIZE as f32]),
                        0.,
                        2. * PI,
                    );

                    canvas.fill_path(&path.build(), &red)?;
                }

                //
            }

            Ok(())
        })
        .await?;

    Ok(())
}
