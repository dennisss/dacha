use image::{Image, Colorspace, Color};
use math::matrix::{Vector2f, vec2f, vec3f, MatrixXf, Vector3f, vec2d, Vector2d};
use math::array::Array;


pub fn gray_to_color(img: &Image<u8>) -> Image<u8> {
    let mut data = vec![];
    data.reserve_exact(img.height() * img.width() * 3);
    for v in img.array.data.iter().cloned() {
        for i in 0..3 {
            data.push(v);
        }
    }

    Image {
        array: Array {
            shape: vec![img.height(), img.width(), 3],
            data,
        },
        colorspace: Colorspace::RGB,
    }
}


pub fn draw_color_circle(center_pt: &Vector2f, color: &Color, image: &mut Image<u8>) {

    image.set(center_pt.y().round() as usize, center_pt.x().round() as usize, color);

    let radius = 3.0;
    let radius_squared = radius * radius;

    
    /*
    for y in 0..image.height() {
        for x in 0..image.width() {
            let pt = vec2f(x as f32, y as f32);
            if (pt - center_pt).norm_squared() <= radius_squared {
                image.set(y, x, &Color::rgb(0xff, 0, 0));
            }
        }
    }
    */
}

pub fn draw_big_color_circle(center_pt: &Vector2f, color: &Color, image: &mut Image<u8>) {
    let radius = 6.0;
    let radius_squared = radius * radius;

    // image.set(center_pt.y().round() as usize, center_pt.x().round() as usize, color);

    // image.

    let y_min = (center_pt.y() - radius - 1.0).max(0.0) as usize;
    let y_max = (y_min + ((2.0 * radius) as usize) + 3).min(image.height());

    for y in y_min..y_max {
        for x in 0..image.width() {
            let pt = vec2f(x as f32, y as f32);
            if (pt - center_pt).norm_squared() <= radius_squared {
                image.set(y, x, color);
            }
        }
    }
}