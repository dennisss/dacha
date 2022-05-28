use std::f32::consts::PI;

use common::errors::*;
use image::Color;
use math::matrix::Vector2f;

use crate::raster::canvas::{Canvas, PathBuilder};
use crate::raster::canvas_render_loop::WindowOptions;

const POINT_SIZE: usize = 4;

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

pub async fn run() -> Result<()> {
    const HEIGHT: usize = 800;
    const WIDTH: usize = 800;

    let window_options = WindowOptions {
        name: "Point Picker".into(),
        width: WIDTH,
        height: HEIGHT,
    };

    let mut canvas = Canvas::create(HEIGHT, WIDTH);

    // let mut points = vec![];
    let mut points = vec![
        Vector2f::from_slice(&[335.00, 172.00]),
        Vector2f::from_slice(&[207.00, 260.00]),
        Vector2f::from_slice(&[221.00, 377.00]),
        Vector2f::from_slice(&[295.00, 505.00]),
        Vector2f::from_slice(&[502.00, 590.00]),
        Vector2f::from_slice(&[599.00, 482.00]),
        Vector2f::from_slice(&[596.00, 338.00]),
        Vector2f::from_slice(&[462.00, 263.00]),
        Vector2f::from_slice(&[511.00, 209.00]),
        Vector2f::from_slice(&[301.00, 272.00]),
        Vector2f::from_slice(&[410.00, 409.00]),
        Vector2f::from_slice(&[421.00, 516.00]),
        Vector2f::from_slice(&[540.00, 502.00]),
        Vector2f::from_slice(&[525.00, 396.00]),
        Vector2f::from_slice(&[309.00, 415.00]),
        Vector2f::from_slice(&[241.00, 313.00]),
        Vector2f::from_slice(&[391.00, 223.00]),
        Vector2f::from_slice(&[346.00, 342.00]),
        Vector2f::from_slice(&[497.00, 337.00]),
        Vector2f::from_slice(&[391.00, 286.00]),
        Vector2f::from_slice(&[361.00, 464.00]),
    ];

    let mut hull = None;

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

                if let glfw::WindowEvent::Key(glfw::Key::P, _, glfw::Action::Press, _) = e {
                    println!("{}", format_points(&points));
                }
            }

            let black = Color::rgb(0, 0, 0);

            canvas.drawing_buffer.clear_white();
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

            if let Some(hull) = &hull {
                let mut path = PathBuilder::new();
                path.move_to(hull[0].clone());
                for point in &hull[1..] {
                    path.line_to(point.clone());
                }
                path.close();

                canvas.stroke_path(&path.build(), 2., &black)?;
            }

            Ok(())
        })
        .await?;

    Ok(())
}
