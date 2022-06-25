// TODO: Rename to TextInput.

/*
TODO: Things to support:
- Focus (with special appearance)
- Disabled mode (greyed out background)
- Scrolling past end if too much text is present
- Placeholder
- Selection
*/

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::errors::*;
use image::Color;
use math::matrix::Vector2f;

use crate::canvas::*;
use crate::font::{CanvasFontRenderer, FontStyle, VerticalAlign};
use crate::ui::event::*;
use crate::ui::view::*;

const BORDER_SIZE: f32 = 1.;
const PADDING_SIZE: f32 = 10.;

const CURSOR_ON_OFF_TIME_MILLIS: usize = 500;

#[derive(Clone)]
pub struct TextboxParams {
    pub value: String,
    pub font: Rc<CanvasFontRenderer>,
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
    cursor: Option<Cursor>,

    /// Last time an event occured which changed the cursor position.
    last_change: Instant,
}

#[derive(Clone)]
struct Cursor {
    start: usize,
    end: usize,
}

impl Cursor {
    fn update(&mut self, idx: usize, holding_shift: bool) {
        self.end = idx;
        if !holding_shift {
            self.start = self.end;
        }
    }
}

impl ViewWithParams for Textbox {
    type Params = TextboxParams;

    fn create_with_params(params: &Self::Params) -> Result<Box<dyn View>> {
        Ok(Box::new(Self {
            params: params.clone(),
            current_value: params.value.clone(),
            cursor: None,
            last_change: Instant::now(),
        }))
    }

    fn update_with_params(&mut self, new_params: &Self::Params) -> Result<()> {
        if self.current_value != new_params.value {
            self.cursor = None;
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
            focused: self.cursor.is_some(),
            dirty: true,
        })
    }

    fn layout(&self, constraints: &LayoutConstraints) -> Result<RenderBox> {
        let measurements = self
            .params
            .font
            .measure_text("", self.params.font_size, None)?;
        let line_height = measurements.height;

        // TODO: Must add text ascent and descent to this.
        Ok(RenderBox {
            width: constraints.max_width,
            height: (line_height + PADDING_SIZE * 2. + BORDER_SIZE * 2.),
            baseline_offset: PADDING_SIZE + (measurements.height + measurements.descent),
            next_cursor: None,
        })
    }

    fn render(&mut self, constraints: &LayoutConstraints, canvas: &mut dyn Canvas) -> Result<()> {
        let background_color = Color::rgb(0xff, 0xff, 0xff);
        let border_color = Color::rgb(0xcc, 0xcc, 0xcc);
        let font_color = Color::rgb(0, 0, 0);

        let measurements =
            self.params
                .font
                .measure_text(&self.current_value, self.params.font_size, None)?;

        let full_width = constraints.max_width;
        let full_height = measurements.height + PADDING_SIZE * 2. + BORDER_SIZE * 2.;

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

        if let Some(cursor) = self.cursor.clone() {
            if cursor.start == cursor.end {
                let cursor_visible = {
                    let t = Instant::now();
                    let cycle = (t.duration_since(self.last_change).as_millis() as usize
                        / CURSOR_ON_OFF_TIME_MILLIS)
                        % 2;
                    cycle == 0
                };

                if cursor_visible {
                    let measurements = self.params.font.measure_text(
                        self.current_value.split_at(cursor.start).0,
                        self.params.font_size,
                        None,
                    )?;

                    let mut path = PathBuilder::new();
                    path.move_to(Vector2f::from_slice(&[measurements.width, 0.]));
                    path.line_to(Vector2f::from_slice(&[
                        measurements.width,
                        measurements.height,
                    ]));

                    canvas.stroke_path(&path.build(), 1., &font_color)?;
                }
            } else {
                let mut start = cursor.start;
                let mut end = cursor.end;
                if end < start {
                    core::mem::swap(&mut start, &mut end);
                }

                // TODO: Optimize these to compute both measurements in one pass.

                let measurements_start = self.params.font.measure_text(
                    self.current_value.split_at(start).0,
                    self.params.font_size,
                    None,
                )?;

                let measurements_end = self.params.font.measure_text(
                    self.current_value.split_at(end).0,
                    self.params.font_size,
                    None,
                )?;

                // TODO: Implement a 40% opacity mixing for this fill
                // (we may also want to invert the font to be a different color).
                canvas.fill_rectangle(
                    measurements_start.width,
                    0.,
                    measurements_end.width - measurements_start.width,
                    measurements.height,
                    &Color::rgb(0xB8, 0xFA, 0xFF), // &Color::rgb(0x00, 0xBB, 0xFF),
                )?;
            }
        }

        self.params.font.fill_text(
            0.,
            0.,
            &self.params.value,
            &FontStyle::from_size(self.params.font_size).with_vertical_align(VerticalAlign::Top),
            &Paint::color(font_color),
            canvas,
        )?;

        canvas.restore();

        Ok(())
    }

    fn handle_event(&mut self, start_cursor: usize, event: &Event) -> Result<()> {
        match event {
            Event::Mouse(e) => {
                if e.kind == MouseEventKind::ButtonDown(MouseButton::Left) {
                    // TODO: Use cursor position to figure out best offset.

                    let mut x = e.relative_x;
                    // Should match the translation done in render().
                    x -= BORDER_SIZE + PADDING_SIZE;

                    // TODO: Are measurements accurate if the font has changed since we last
                    // rendered?
                    let idx = self.params.font.find_closest_text_index(
                        &self.current_value,
                        self.params.font_size,
                        x,
                    )?;

                    // TODO: Implement holding the shift key while clicking (or mouse moving while
                    // the mouse is down to select more).
                    self.cursor = Some(Cursor {
                        start: idx,
                        end: idx,
                    });
                    self.last_change = Instant::now();
                }
            }
            Event::Key(e) => {
                let mut cursor = match self.cursor.clone() {
                    Some(v) => v,
                    None => return Ok(()),
                };

                if e.kind == KeyEventKind::Down {
                    let new_value = match e.key {
                        Key::Printable(c) => {
                            let mut new_value = String::new();
                            let (before, after) = self.current_value.split_at(cursor.start);
                            new_value.push_str(before);
                            new_value.push(c);
                            new_value.push_str(after.split_at(cursor.end - cursor.start).1);

                            let idx = cursor.start + c.len_utf8();
                            self.cursor = Some(Cursor {
                                start: idx,
                                end: idx,
                            });
                            self.set_current_value(new_value);
                        }
                        Key::Backspace => {
                            // TODO: Implement deleting a range with this!

                            let mut new_string = String::new();
                            let (before, after) = self.current_value.split_at(cursor.end);
                            new_string.push_str(before);
                            if let Some(c) = new_string.pop() {
                                cursor.update(cursor.end - c.len_utf8(), false);
                                self.cursor = Some(cursor);
                            }
                            new_string.push_str(after);
                            self.set_current_value(new_string);
                        }
                        Key::LeftArrow => {
                            if cursor.end > 0 {
                                let first_half = self.current_value.split_at(cursor.end).0;

                                cursor.update(
                                    first_half.char_indices().map(|v| v.0).last().unwrap_or(0),
                                    e.shift,
                                );
                                self.cursor = Some(cursor);

                                self.last_change = Instant::now();
                            }
                        }
                        Key::RightArrow => {
                            if cursor.end < self.current_value.len() {
                                let second_half = self.current_value.split_at(cursor.end).1;

                                cursor.update(
                                    cursor.end + second_half.chars().next().unwrap().len_utf8(),
                                    e.shift,
                                );
                                self.cursor = Some(cursor);
                                self.last_change = Instant::now();
                            }
                        }
                        Key::UpArrow => {
                            cursor.update(0, e.shift);
                            self.cursor = Some(cursor);
                            self.last_change = Instant::now();
                        }
                        Key::DownArrow => {
                            cursor.update(self.current_value.len(), e.shift);
                            self.cursor = Some(cursor);
                            self.last_change = Instant::now();
                        }
                        _ => {}
                    };
                }
            }
            Event::Blur => {
                self.cursor = None;
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
