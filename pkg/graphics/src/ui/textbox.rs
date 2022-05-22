/*
On-click should capture focus (eventually want to support capturing focus whenever)

TODO: Things to support:
- Focus (with special appearance)
- Disabled mode (greyed out background)
- Scrolling past end if too much text is present
- Click on a specific character to move cursor
- Placeholder
- Selection
*/

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::errors::*;
use image::Color;
use math::matrix::Vector2f;

use crate::font::{find_closest_text_index, measure_text, CanvasFontExt, OpenTypeFont};
use crate::raster::canvas::{Canvas, PathBuilder};
use crate::ui::event::*;
use crate::ui::view::*;

const BORDER_SIZE: f32 = 1.;
const PADDING_SIZE: f32 = 10.;

const CURSOR_ON_OFF_TIME_MILLIS: usize = 500;

#[derive(Clone)]
pub struct TextboxParams {
    pub value: String,
    pub font: Arc<OpenTypeFont>,
    pub font_size: f32,
    pub on_change: Option<Arc<dyn Fn(String)>>,
}

impl ViewParams for TextboxParams {
    type View = Textbox;
}

pub struct Textbox {
    params: TextboxParams,

    /// The latest value of the text in this box that was displayed or changed.
    ///
    /// Usually within one frame the params.value value will become this string
    /// if the parent view has accepted the changed text passed to
    /// params.on_change.
    current_value: String,

    /// Index into the current_value in units of bytes at which the edit cursor
    /// is positioned.
    cursor_position: Option<usize>,

    /// Last time an event occured which changed the cursor position.
    last_change: Instant,
}

impl ViewWithParams for Textbox {
    type Params = TextboxParams;

    fn create_with_params(params: &Self::Params) -> Result<Box<dyn View>> {
        Ok(Box::new(Self {
            params: params.clone(),
            current_value: params.value.clone(),
            cursor_position: None,
            last_change: Instant::now(),
        }))
    }

    fn update_with_params(&mut self, new_params: &Self::Params) -> Result<()> {
        if self.current_value != new_params.value {
            self.cursor_position = None;
            self.current_value = new_params.value.clone();
        }

        self.params = new_params.clone();
        Ok(())
    }
}

impl View for Textbox {
    fn build(&mut self) -> Result<ViewStatus> {
        Ok(ViewStatus {
            cursor: MouseCursor(glfw::StandardCursor::IBeam),
            focused: self.cursor_position.is_some(),
        })
    }

    fn layout(&self, parent_box: &RenderBox) -> Result<RenderBox> {
        // let measurements =
        //     measure_text(&self.params.font, &self.params.text,
        // self.params.font_size)?;

        // TODO: Must add text ascent and descent to this.
        Ok(RenderBox {
            width: parent_box.width,
            height: (self.params.font_size + PADDING_SIZE * 2. + BORDER_SIZE * 2.),
        })
    }

    fn render(&mut self, parent_box: &RenderBox, canvas: &mut Canvas) -> Result<()> {
        let background_color = Color::from_slice_with_shape(3, 1, &[0xff, 0xff, 0xff]);
        let border_color = Color::from_slice_with_shape(3, 1, &[0xcc, 0xcc, 0xcc]);
        let font_color = Color::from_slice_with_shape(3, 1, &[0, 0, 0]);

        let measurements = measure_text(
            &self.params.font,
            &self.current_value,
            self.params.font_size,
        )?;

        let full_width = parent_box.width;
        let full_height = self.params.font_size + PADDING_SIZE * 2. + BORDER_SIZE * 2.;

        canvas.fill_rectangle(0., 0., full_width, full_height, &background_color)?;

        canvas.stroke_rectangle(
            BORDER_SIZE / 2.,
            BORDER_SIZE / 2.,
            full_width - BORDER_SIZE,
            full_height - BORDER_SIZE,
            BORDER_SIZE,
            &border_color,
        )?;

        canvas.save();

        canvas.translate(BORDER_SIZE + PADDING_SIZE, BORDER_SIZE + PADDING_SIZE);

        canvas.fill_text(
            0.0,
            measurements.height + measurements.descent,
            &self.params.font,
            &self.params.value,
            self.params.font_size,
            &font_color,
        )?;

        if let Some(cursor_pos) = self.cursor_position.clone() {
            let cursor_visible = {
                let t = Instant::now();
                let cycle = (t.duration_since(self.last_change).as_millis() as usize
                    / CURSOR_ON_OFF_TIME_MILLIS)
                    % 2;
                cycle == 0
            };

            if cursor_visible {
                let measurements = measure_text(
                    &self.params.font,
                    self.current_value.split_at(cursor_pos).0,
                    self.params.font_size,
                )?;

                let mut path = PathBuilder::new();
                path.move_to(Vector2f::from_slice(&[measurements.width, 0.]));
                path.line_to(Vector2f::from_slice(&[
                    measurements.width,
                    measurements.height,
                ]));

                canvas.stroke_path(&path.build(), 1., &font_color)?;
            }
        }

        canvas.restore();

        Ok(())
    }

    fn handle_event(&mut self, event: &Event) -> Result<()> {
        match event {
            Event::Mouse(e) => {
                if e.kind == MouseEventKind::ButtonDown(MouseButton::Left) {
                    // TODO: Use cursor position to figure out best offset.

                    let mut x = e.relative_x;
                    // Should match the translation done in render().
                    x -= BORDER_SIZE + PADDING_SIZE;

                    // TODO: Are measurements accurate if the font has changed since we last
                    // rendered?
                    self.cursor_position = Some(find_closest_text_index(
                        &self.params.font,
                        &self.current_value,
                        self.params.font_size,
                        x,
                    )?);
                    self.last_change = Instant::now();
                }
            }
            Event::Key(e) => {
                let cursor_pos = match self.cursor_position.clone() {
                    Some(v) => v,
                    None => return Ok(()),
                };

                if e.kind == KeyEventKind::Down {
                    let new_value = match e.key {
                        Key::Printable(c) => {
                            let mut new_value = String::new();
                            let (before, after) = self.current_value.split_at(cursor_pos);
                            new_value.push_str(before);
                            new_value.push(c);
                            new_value.push_str(after);

                            self.cursor_position = Some(cursor_pos + c.len_utf8());
                            self.set_current_value(new_value);
                        }
                        Key::Backspace => {
                            let mut new_string = String::new();
                            let (before, after) = self.current_value.split_at(cursor_pos);
                            new_string.push_str(before);
                            if let Some(c) = new_string.pop() {
                                self.cursor_position = Some(cursor_pos - c.len_utf8());
                            }
                            new_string.push_str(after);
                            self.set_current_value(new_string);
                        }
                        Key::LeftArrow => {
                            if cursor_pos > 0 {
                                let first_half = self.current_value.split_at(cursor_pos).0;
                                self.cursor_position = Some(
                                    first_half.char_indices().map(|v| v.0).last().unwrap_or(0),
                                );
                                self.last_change = Instant::now();
                            }
                        }
                        Key::RightArrow => {
                            if cursor_pos < self.current_value.len() {
                                let second_half = self.current_value.split_at(cursor_pos).1;
                                self.cursor_position = Some(
                                    cursor_pos + second_half.chars().next().unwrap().len_utf8(),
                                );
                                self.last_change = Instant::now();
                            }
                        }
                        Key::UpArrow => {
                            self.cursor_position = Some(0);
                            self.last_change = Instant::now();
                        }
                        Key::DownArrow => {
                            self.cursor_position = Some(self.current_value.len());
                            self.last_change = Instant::now();
                        }
                        _ => {}
                    };
                }
            }
            Event::Blur => {
                self.cursor_position = None;
            }
            _ => {}
        }

        Ok(())
    }
}

impl Textbox {
    fn set_current_value(&mut self, new_value: String) {
        self.current_value = new_value.clone();
        self.last_change = Instant::now();

        if let Some(listener) = &self.params.on_change {
            listener(new_value);
        }
    }
}
