use image::{Color, Image};

pub fn closed_range(mut s: isize, mut e: isize) -> Box<dyn Iterator<Item = isize>> {
    let iter = (s..(e + 1));
    if e > s {
        Box::new((s..=e))
    } else {
        Box::new((e..=s).rev())
    }
}

pub fn add_color(image: &mut Image<u8>, y: usize, x: usize, color: &Color) {
    let color_old = image.get(y, x);
    let alpha = (color[3] as f32) / 255.0;
    image.set(
        y,
        x,
        &(color_old.cast::<f32>() * (1.0 - alpha) + color.cast::<f32>() * alpha)
            .cast::<u8>()
            .into(),
    );
}
