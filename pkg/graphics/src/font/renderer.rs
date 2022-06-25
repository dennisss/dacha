use core::cell::RefCell;
use std::collections::HashMap;

use common::errors::*;
use image::Color;

use crate::canvas::*;
use crate::font::style::*;
use crate::font::{HorizontalMetricRecord, OpenTypeFont, SimpleGlyph};

#[derive(Debug)]
pub struct TextMeasurements {
    /// Distance in pixels from the left to right size of the text's bounding
    /// box when rendered.
    pub width: f32,

    ///
    pub height: f32,

    /// NOTE: This is a negative value.
    pub descent: f32,

    /// Number of bytes read from the input str which were used to create this
    /// measurement.
    pub length: usize,
}

struct FontSizeMeasurements {
    scale: f32,
    height: f32,
    descent: f32,
}

/// NOTE: One renderer should only ever be associated with a single canvas.
pub struct CanvasFontRenderer {
    font: OpenTypeFont,
    glyph_paths: RefCell<HashMap<u16, (Box<dyn CanvasObject>, HorizontalMetricRecord)>>,
}

impl CanvasFontRenderer {
    pub fn new(font: OpenTypeFont) -> Self {
        Self {
            font,
            glyph_paths: RefCell::new(HashMap::new()),
        }
    }

    pub fn font(&self) -> &OpenTypeFont {
        &self.font
    }

    /// NOTE: This always renders the text left aligned at the baseline.
    pub fn fill_text(
        &self,
        mut x: f32,
        y: f32,
        text: &str,
        font_style: &FontStyle,
        paint: &Paint,
        canvas: &mut dyn Canvas,
    ) -> Result<()> {
        let sizing = self.measure_font_size(font_style.size);

        let mut x_offset = match font_style.text_align {
            TextAlign::Left => 0.,
            TextAlign::Center => -(self.measure_text_width(&sizing, text)? / 2.),
            TextAlign::Right => -(self.measure_text_width(&sizing, text)?),
        };
        let mut y_offset = match font_style.vertical_align {
            VerticalAlign::Top => (sizing.height + sizing.descent), // ascent
            VerticalAlign::Baseline => 0.,
            VerticalAlign::Bottom => sizing.descent,
            VerticalAlign::Center => (sizing.height / 2.) + sizing.descent,
        };

        for c in text.chars() {
            let char_code = c as u32;
            if char_code > u16::MAX as u32 {
                return Err(err_msg("Character overflowed supported range"));
            }

            let mut glyph_paths_guard = self.glyph_paths.borrow_mut();

            let (path_obj, metrics) =
                self.create_glyph(char_code as u16, &mut glyph_paths_guard, canvas)?;

            canvas.save();

            canvas.translate(x_offset + x, y_offset + y);

            canvas.scale(sizing.scale, -1.0 * sizing.scale);

            // NOTE: We assume that x_min == left_side_bearing so no translation is needed.
            // self.translate(-1.0 * ((x_min - metrics.left_side_bearing) as f32), 0.0);

            path_obj.draw(paint, canvas)?;

            // draw_glyph(self, &g, &color)?;

            canvas.restore()?;

            x += (metrics.advance_width as f32) * sizing.scale;
        }

        Ok(())
    }

    fn measure_font_size(&self, font_size: f32) -> FontSizeMeasurements {
        let scale = font_size / (self.font.head.units_per_em as f32);

        FontSizeMeasurements {
            scale,
            // TODO: Incorporate the line gap?
            height: ((self.font.hhea.ascender - self.font.hhea.descender) as f32) * scale,
            descent: (self.font.hhea.descender as f32) * scale,
        }
    }

    fn measure_text_width(&self, sizing: &FontSizeMeasurements, text: &str) -> Result<f32> {
        Ok(self.measure_text_width_with_limit(sizing, text, None)?.0)
    }

    fn measure_text_width_with_limit(
        &self,
        sizing: &FontSizeMeasurements,
        text: &str,
        max_width: Option<f32>,
    ) -> Result<(f32, usize)> {
        let mut width = 0.0;

        for (i, c) in text.char_indices() {
            let char_code = c as u32;
            if char_code > u16::MAX as u32 {
                return Err(err_msg("Character overflowed supported range"));
            }

            let (g, metrics) = self.font.char_glyph(char_code as u16)?;

            let increment = (metrics.advance_width as f32) * sizing.scale;
            if let Some(max_width) = max_width.clone() {
                if increment + width > max_width {
                    return Ok((width, i));
                }
            }

            width += increment;
        }

        Ok((width, text.len()))
    }

    fn create_glyph<'a>(
        &self,
        code: u16,
        glyph_paths: &'a mut HashMap<u16, (Box<dyn CanvasObject>, HorizontalMetricRecord)>,
        canvas: &mut Canvas,
    ) -> Result<(&'a mut dyn CanvasObject, &'a HorizontalMetricRecord)> {
        if !glyph_paths.contains_key(&code) {
            let (g, metrics) = self.font.char_glyph(code)?;

            let path = Self::build_glyph_path(&g)?;

            let obj = canvas.create_path_fill(&path)?;

            glyph_paths.insert(code, (obj, metrics.clone()));
        }

        let (path_obj, metrics) = glyph_paths.get_mut(&code).unwrap();

        Ok((path_obj.as_mut(), metrics))
    }

    fn build_glyph_path(g: &SimpleGlyph) -> Result<Path> {
        let mut path_builder = PathBuilder::new();

        for contour in &g.contours {
            // TODO: Check that there are at least two points in the contour. Otherwise it
            // is invalid.

            if !contour.is_empty() {
                if !contour[0].on_curve {
                    return Err(err_msg("Expected first point to be on curve"));
                }

                path_builder.move_to(contour[0].to_vector().cast());
            }

            let mut i = 1;
            while i < contour.len() {
                let p = contour[i].to_vector();
                let p_on_curve = contour[i].on_curve;
                i += 1;

                if p_on_curve {
                    path_builder.line_to(p.cast());
                } else {
                    let mut curve = vec![p.cast()];
                    while i < contour.len() && !contour[i].on_curve {
                        curve.push(contour[i].to_vector().cast());
                        i += 1;
                    }

                    // TODO: Check if this is correct.
                    if i == contour.len() {
                        curve.push(contour[0].to_vector().cast());
                    } else {
                        curve.push(contour[i].to_vector().cast());
                        i += 1;
                    }

                    path_builder.curve_to(&curve);
                }
            }

            path_builder.close();
        }

        Ok(path_builder.build())
    }

    pub fn measure_text(
        &self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
    ) -> Result<TextMeasurements> {
        let sizing = self.measure_font_size(font_size);
        let (width, length) = self.measure_text_width_with_limit(&sizing, text, max_width)?;

        Ok(TextMeasurements {
            width,
            height: sizing.height,
            descent: sizing.descent,
            length,
        })
    }

    pub fn find_closest_text_index(&self, text: &str, font_size: f32, x: f32) -> Result<usize> {
        if x < 0. {
            return Ok(0);
        }

        let sizing = self.measure_font_size(font_size);

        let mut width = 0.0;

        for (idx, c) in text.char_indices() {
            let char_code = c as u32;
            if char_code > u16::MAX as u32 {
                return Err(err_msg("Character overflowed supported range"));
            }

            let (g, metrics) = self.font.char_glyph(char_code as u16)?;

            let next_width = width + ((metrics.advance_width as f32) * sizing.scale);
            if next_width > x {
                let distance_before = (width - x).abs();
                let distance_after = (next_width - x).abs();

                if distance_before < distance_after {
                    return Ok(idx);
                } else {
                    return Ok(idx + c.len_utf8());
                }
            }

            width = next_width;
        }

        Ok(text.len())
    }
}

/*
pub trait CanvasFontExt {
    fn fill_text(
        &mut self,
        x: f32,
        y: f32,
        font: &OpenTypeFont,
        text: &str,
        font_size: f32,
        color: &Color,
    ) -> Result<()>;
}

impl CanvasFontExt for dyn Canvas + '_ {
    fn fill_text(
        &mut self,
        mut x: f32,
        y: f32,
        font: &OpenTypeFont,
        text: &str,
        font_size: f32,
        color: &Color,
    ) -> Result<()> {
    }
}

// This is separate from the 'dyn Canvas' impl as you need to be Sized in order
// to do the cast to '&mut dyn Canvas'.
//
// TODO: Have a macro for automatically
// deriving this from the dyn impl.
impl<C: Canvas> CanvasFontExt for C {
    fn fill_text(
        &mut self,
        x: f32,
        y: f32,
        font: &OpenTypeFont,
        text: &str,
        font_size: f32,
        color: &Color,
    ) -> Result<()> {
        let dyn_self = self as &mut dyn Canvas;
        dyn_self.fill_text(x, y, font, text, font_size, color)
    }
}

fn draw_glyph(canvas: &mut dyn Canvas, g: &SimpleGlyph, color: &Color) -> Result<()> {
    if g.contours.is_empty() {
        return Ok(());
    }

    canvas.fill_path(&path, color)?;

    Ok(())
}
*/
