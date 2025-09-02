use std::sync::Arc;

use common::errors::*;
use graphics::{
    canvas::{Canvas, CanvasHelperExt, Paint},
    font::{CanvasFontRenderer, FontStyle, OpenTypeFont, TextAlign, VerticalAlign},
    image_show::ImageShow,
    raster::canvas::RasterCanvas,
};
use image::{BinaryImage, Color, Image};
use labeler_proto::labeler::{LabelDatamatrix_Position, LabelPage, LabelText};
use ptouch::*;

use crate::datamatrix::{encode_datamatrix, MIN_DATAMATRIX_MARGIN};

/// Maximum length of a page (including margins).
const ABSOLUTE_MAX_PAGE_LENGTH: usize = 2000;

pub struct LabelRenderer {
    font: Arc<OpenTypeFont>,
}

impl LabelRenderer {
    pub async fn create() -> Result<Self> {
        let font = Arc::new(
            OpenTypeFont::read(file::project_path!("third_party/noto_sans/font_normal.ttf"))
                .await?,
        );

        Ok(Self { font })
    }

    /// Renders a page to an image.
    ///
    /// Internally this works but length wise concatenating 'blocks' that take
    /// up the whole width of the tape.
    pub fn render_page(&self, page: &LabelPage, tape: &Tape) -> Result<Image<u8>> {
        if page.max_length_mm() < 0.0 || page.length_mm() < 0.0 {
            return Err(rpc::Status::invalid_argument("Lengths must be positive").into());
        }

        let mut max_length = ABSOLUTE_MAX_PAGE_LENGTH;
        if page.max_length_mm() != 0.0 {
            max_length = max_length.min(tape.mm_to_dots(page.max_length_mm()).ceil() as usize);
        }
        if page.length_mm() != 0.0 {
            max_length = max_length.min(tape.mm_to_dots(page.length_mm()).ceil() as usize);
        }

        max_length = max_length.checked_sub(2 * tape.margin).unwrap_or(0);

        let datamatrix_block = {
            if page.has_datamatrix() {
                let block = DatamatrixBlock::create(page, tape)?;
                max_length = max_length.checked_sub(block.length).unwrap_or(0);
                Some(block)
            } else {
                None
            }
        };

        let text_block = TextBlock::create(page.text(), tape, max_length, self.font.clone())?;

        // NOTE: This is the print area length.
        let length = {
            let l = if page.length_mm() != 0.0 {
                tape.mm_to_dots(page.length_mm()).ceil() as usize
            } else {
                let mut total_len = text_block.length;
                if let Some(block) = &datamatrix_block {
                    total_len += block.length;
                }

                total_len += 2 * tape.margin;

                total_len
            };

            l.min(ABSOLUTE_MAX_PAGE_LENGTH)
                .checked_sub(2 * tape.margin)
                .unwrap_or(0)
        };

        let mut canvas = RasterCanvas::create_grayscale(tape.print_area, length as usize);
        let c = &mut canvas as &mut dyn Canvas;
        c.clear_rect(
            0.,
            0.,
            length as f32,
            tape.print_area as f32,
            &Color::rgb(255, 255, 255),
        )?;

        let mut text_x_range = (0, length);

        if let Some(block) = &datamatrix_block {
            if length >= block.length {
                let x_range = match page.datamatrix().position() {
                    LabelDatamatrix_Position::LEFT_OF_TEXT => {
                        text_x_range.0 += block.length;
                        (0, block.length)
                    }
                    LabelDatamatrix_Position::RIGHT_OF_TEXT => {
                        text_x_range.1 -= block.length;
                        (length - block.length, length)
                    }
                    LabelDatamatrix_Position::UNKNOWN => {
                        return Err(rpc::Status::invalid_argument(
                            "Unsupported data matrix position",
                        )
                        .into());
                    }
                };

                block.render(x_range, &mut canvas)?;
            }
        }

        text_block.render(text_x_range, &mut canvas)?;

        // TODO: Ensure the image is entirely black and white.

        Ok(canvas.drawing_buffer.clone())
    }
}

trait LabelBlock {
    /// x_range is the '[xmin, xmax)' range reserved for rendering this block.
    fn render(&self, x_range: (usize, usize), canvas: &mut RasterCanvas) -> Result<()>;
}

struct DatamatrixBlock {
    image: BinaryImage,

    /// Integer multiplier (>= 1) of the size of image that will be rendered.
    scale: usize,

    /// Overall length of the area needed for this datamatrix.
    length: usize,

    /// Position of the top edge of the data matrix within the block.
    top: usize,

    /// Position of the left edge of the data matrix within the block.
    left: usize,
}

impl DatamatrixBlock {
    fn create(page: &LabelPage, tape: &Tape) -> Result<Self> {
        if !page.has_datamatrix() {
            return Err(err_msg("Page has no datamatrix defined."));
        }

        let image = encode_datamatrix(page.datamatrix().data().as_bytes())?;

        // Number of datamatrix image units we will keep as blank around the symbol.
        let margin_size = MIN_DATAMATRIX_MARGIN;

        // Find the largest scale
        let mut scale = 0;
        loop {
            let next_scale = scale + 1;

            // Image must fit in the printable region
            if next_scale * image.height() > tape.print_area {
                break;
            }

            // After including the margin, we must fit on the tape.
            if next_scale * (image.height() + 2 * margin_size) > tape.width {
                break;
            }

            scale = next_scale;
        }

        // TODO: Also add a warning if the scale is <2 since we need to allow for some
        // imprecision in the printing.
        if scale == 0 {
            return Err(rpc::Status::invalid_argument("Datamatrix can't fit on the tape").into());
        }

        // Vertically center the code.
        let top = (tape.print_area - (image.height() * scale)) / 2;

        // Left/right padding (in dot images)
        let mut left = margin_size * scale;
        let mut right = margin_size * scale;

        // Remove the mandatory tape margin from left/right if there is no text on each
        // side.
        // TODO: It would be better if we handled this at a higher level
        if page.text().value().is_empty()
            || page.datamatrix().position() == LabelDatamatrix_Position::LEFT_OF_TEXT
        {
            left = left.checked_sub(tape.margin).unwrap_or(0);
        }

        if page.text().value().is_empty()
            || page.datamatrix().position() == LabelDatamatrix_Position::RIGHT_OF_TEXT
        {
            right = right.checked_sub(tape.margin).unwrap_or(0);
        }

        let length = left + right + (image.width() * scale);

        Ok(Self {
            image,
            scale,
            length,
            top,
            left,
        })
    }
}

impl LabelBlock for DatamatrixBlock {
    fn render(&self, x_range: (usize, usize), canvas: &mut RasterCanvas) -> Result<()> {
        for y_i in 0..(self.image.height() * self.scale) {
            for x_i in 0..(self.image.width() * self.scale) {
                let v = self.image.get(y_i / self.scale, x_i / self.scale);

                let color = {
                    if v != 0 {
                        Color::zero()
                    } else {
                        Color::rgb(0xff, 0xff, 0xff)
                    }
                };

                // NOTE: We assume that sufficient x space has been allocated in the image that
                // we won't overflow.
                canvas
                    .drawing_buffer
                    .set(self.top + y_i, x_range.0 + self.left + x_i, &color);
            }
        }

        Ok(())
    }
}

struct TextBlock<'a> {
    lines: Vec<&'a str>,
    font_size: f32,
    font_renderer: CanvasFontRenderer,
    length: usize,
    line_interval: f32,
}

impl<'a> TextBlock<'a> {
    fn create(
        text: &'a LabelText,
        tape: &'a Tape,
        max_length: usize,
        font: Arc<OpenTypeFont>,
    ) -> Result<Self> {
        let lines = text.value().lines().collect::<Vec<&str>>();

        let mut font_size = {
            if text.font_size_mm() > 0.0 {
                tape.mm_to_dots(text.font_size_mm())
            } else {
                // TODO: Return the resolved font size.

                // 1.0 is the desired line height as a fraction of the font size.
                (tape.print_area as f32) / (lines.len() as f32) / 1.0
            }
        };

        let font_renderer = CanvasFontRenderer::new(font);

        // Length of the block is the width of the longest line.
        let mut length = 0;
        for line in &lines[..] {
            let measurements = font_renderer.measure_text(*line, font_size, None)?;
            length = core::cmp::max(measurements.width.ceil() as usize, length);
        }

        // Decrease font size if it exceeds the max length.
        if length > max_length {
            font_size *= (max_length as f32) / (length as f32);
            length = max_length; 
        }

        let line_interval = (tape.print_area as f32) / (lines.len() as f32);

        Ok(Self {
            lines,
            font_size,
            font_renderer,
            length,
            line_interval,
        })
    }
}

impl<'a> LabelBlock for TextBlock<'a> {
    fn render(&self, x_range: (usize, usize), canvas: &mut RasterCanvas) -> Result<()> {
        let font_style = FontStyle::from_size(self.font_size)
            .with_text_align(TextAlign::Left)
            .with_vertical_align(VerticalAlign::Center);

        let paint = Paint::color(Color::hex(0));

        for (i, line) in self.lines.iter().enumerate() {
            // Adding 0.5 since the lines are vertically centered.
            let y = ((i as f32) + 0.5) * self.line_interval;
            self.font_renderer.fill_text(
                x_range.0 as f32,
                y,
                *line,
                &font_style,
                &paint,
                canvas,
            )?;
        }

        Ok(())
    }
}
