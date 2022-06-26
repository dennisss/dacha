use std::sync::Arc;
use std::rc::Rc;
use std::time::Instant;

use common::errors::*;
use image::Color;
use math::matrix::vec2;

use crate::canvas::{Canvas, Paint, CanvasHelperExt, PathBuilder};
use crate::font::{CanvasFontRenderer, FontStyle, TextMeasurements, VerticalAlign};
use crate::ui::event::*;
use crate::ui::view::*;
use crate::ui::range::*;

const CURSOR_ON_OFF_TIME_MILLIS: usize = 500;

#[derive(Clone)]
pub struct TextViewParams {
    pub value: String,
    /// TODO: Replace to a dependency handle so that it is easier to compare the entire thing with PartialEq of the TextViewParams.
    pub font: Rc<CanvasFontRenderer>,
    pub font_size: f32,
    pub color: Color,
    pub editable: bool,
    pub selectable: bool,

    /// Callback to invoke if the user modifies the text.
    /// (only relevant if editable == true).
    pub on_change: Option<Arc<dyn Fn(String)>>,
}

impl ViewParams for TextViewParams {
    type View = TextView;
}

pub struct TextView {
    params: TextViewParams,

    /// The latest value of the text in this box that was displayed or changed.
    ///
    /// Usually within one frame the params.value value will become this string
    /// if the parent view has accepted the changed text passed to
    /// params.on_change.
    current_value: String,

    /// Index into the current_value in units of bytes at which the edit cursor
    /// is positioned.
    cursor: Option<CursorRange>,

    cursor_visible: bool,

    /// Last time an event occured which changed the cursor position.
    last_change: Instant,
    
    dirty: bool,
}

struct TextLayout {
    start_index: usize,
    measurements: TextMeasurements,
}

impl TextView {
    fn layout_impl(&self, constraints: &LayoutConstraints) -> Result<TextLayout> {
        let start_index = constraints.start_cursor.unwrap_or(0);

        let remaining_text = &self.params.value[constraints.start_cursor.unwrap_or(0)..];

        let min_length = {
            let word_end = remaining_text.find(' ').unwrap_or(0);
            let first_char = remaining_text
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(0);

            word_end.max(first_char)
        };

        let mut measurements = self.params.font.measure_text(
            remaining_text,
            self.params.font_size,
            if constraints.start_cursor.is_some() {
                Some(constraints.max_width)
            } else {
                None
            },
        )?;

        if measurements.length < min_length {
            measurements = self.params.font.measure_text(
                &remaining_text[0..min_length],
                self.params.font_size,
                None,
            )?;
        }

        Ok(TextLayout {
            start_index,
            measurements,
        })
    }

    fn render_cursor(&self, text: &str, layout: &TextLayout, canvas: &mut dyn Canvas) -> Result<()> {
        let mut cursor = match self.cursor.clone() {
            Some(cursor) => cursor,
            None => return Ok(())
        };

        if cursor.end < cursor.start {
            core::mem::swap(&mut cursor.start, &mut cursor.end);
        }


        let span_start_index = layout.start_index;
        let span_end_index = layout.start_index + layout.measurements.length;

        // Each span renders cursors in the range [span_start, span_end) with the exception of the
        // final span which must also render the cursor at span_end. 
        let cursor_in_span = (cursor.start < span_end_index || span_end_index == self.current_value.len()) &&
            cursor.end >= span_start_index;
        if !cursor_in_span {
            return Ok(());
        }

        // Clip to current span and make relative to the span's text.
        cursor.start = cursor.start.max(span_start_index) - span_start_index;
        cursor.end = cursor.end.min(span_end_index) - span_start_index;

        if cursor.start == cursor.end {
            if !self.params.editable {
                return Ok(());
            }

            if self.cursor_visible {
                let measurements = self.params.font.measure_text(
                    text.split_at(cursor.start).0,
                    self.params.font_size,
                    None,
                )?;

                let mut path = PathBuilder::new();
                path.move_to(vec2(measurements.width, 0.));
                path.line_to(vec2(
                    measurements.width,
                    measurements.height,
                ));

                canvas.stroke_path(&path.build(), 1., &self.params.color)?;
            }
        } else {
            // TODO: Optimize these to compute both measurements in one pass.

            let measurements_start = self.params.font.measure_text(
                text.split_at(cursor.start).0,
                self.params.font_size,
                None,
            )?;

            let measurements_end = self.params.font.measure_text(
                text.split_at(cursor.end).0,
                self.params.font_size,
                None,
            )?;

            // TODO: Implement a 40% opacity mixing for this fill
            // (we may also want to invert the font to be a different color).
            canvas.fill_rectangle(
                measurements_start.width,
                0.,
                measurements_end.width - measurements_start.width,
                layout.measurements.height,
                &Color::rgb(0xB8, 0xFA, 0xFF), // &Color::rgb(0x00, 0xBB, 0xFF),
            )?;
        }

        Ok(())
    } 

    fn set_current_value(&mut self, new_value: String) {
        self.current_value = new_value.clone();
        self.last_change = Instant::now();

        if let Some(listener) = &self.params.on_change {
            listener(new_value);
        }
    }
}

impl ViewWithParams for TextView {
    type Params = TextViewParams;

    fn create_with_params(params: &Self::Params) -> Result<Box<dyn View>> {
        Ok(Box::new(Self {
            params: params.clone(),
            current_value: params.value.clone(),
            cursor: None,
            cursor_visible: false,
            last_change: Instant::now(),
            dirty: true,
        }))
    }

    fn update_with_params(&mut self, new_params: &Self::Params) -> Result<()> {
        if self.params.value != new_params.value ||
           !core::ptr::eq::<CanvasFontRenderer>(&*self.params.font, &*new_params.font) ||
           self.params.font_size != new_params.font_size ||
           self.params.color != new_params.color {
            self.dirty = true;
            self.params = new_params.clone();
        }

        if self.current_value != new_params.value {
            self.cursor = None;
            self.current_value = new_params.value.clone();
            self.dirty = true;
        }

        if self.cursor.is_some() && self.params.editable {
            let cursor_visible = {
                let t = Instant::now();
                let cycle = (t.duration_since(self.last_change).as_millis() as usize
                    / CURSOR_ON_OFF_TIME_MILLIS)
                    % 2;
                cycle == 0
            };

            if self.cursor_visible != cursor_visible {
                self.dirty = true;
            }

            self.cursor_visible = cursor_visible;
        }

        Ok(())
    }
}

impl View for TextView {
    fn build(&mut self) -> Result<ViewStatus> {
        let mut status = ViewStatus::default();
        status.cursor = MouseCursor(glfw::StandardCursor::IBeam);
        status.dirty = self.dirty;
        status.focused = self.cursor.is_some();
        Ok(status)
    }

    fn layout(&self, constraints: &LayoutConstraints) -> Result<RenderBox> {
        let layout = self.layout_impl(constraints)?;

        Ok(RenderBox {
            width: layout.measurements.width,
            height: layout.measurements.height,
            baseline_offset: layout.measurements.height + layout.measurements.descent,
            range: CursorRange { start: layout.start_index, end: layout.start_index + layout.measurements.length },
            next_cursor: if layout.start_index + layout.measurements.length < self.params.value.len()
            {
                Some(layout.start_index + layout.measurements.length)
            } else {
                None
            },
        })
    }

    fn render(&mut self, constraints: &LayoutConstraints, canvas: &mut dyn Canvas) -> Result<()> {
        let layout = self.layout_impl(constraints)?;

        let text = &self.params.value
            [layout.start_index..(layout.start_index + layout.measurements.length)];

        // TODO: Update this doc to factor in spans.
        self.render_cursor(text, &layout, canvas)?;

        self.params.font.fill_text(
            0.,
            0.,
            text,
            &FontStyle::from_size(self.params.font_size).with_vertical_align(VerticalAlign::Top),
            &Paint::color(self.params.color.clone()),
            canvas,
        )?;

        self.dirty = false;

        Ok(())
    }

    fn handle_event(&mut self, event: &Event) -> Result<()> {
        match event {
            Event::Mouse(e) => {
                if !self.params.selectable {
                    return Ok(());
                }

                // TODO: Document gurantees that this will always be within the text's range (consistency between when handle_event and render is called).
                // I guess it could be problematic if a click and a keystroke occur at the same time.
                let range = match e.range {
                    Some(r) => r,
                    None => {
                        CursorRange { start: 0, end: self.current_value.len() }
                    }
                };

                if e.kind == MouseEventKind::ButtonDown(MouseButton::Left) {
                    // TODO: Use cursor position to figure out best offset.

                    // TODO: Are measurements accurate if the font has changed since we last
                    // rendered?
                    let idx = range.start + self.params.font.find_closest_text_index(
                        &self.current_value[range.start..range.end],
                        self.params.font_size,
                        e.relative_x,
                    )?;

                    // TODO: Implement holding the shift key while clicking (or mouse moving while
                    // the mouse is down to select more).
                    self.cursor = Some(CursorRange {
                        start: idx,
                        end: idx,
                    });
                    self.last_change = Instant::now();
                    self.dirty = true;
                }
            }
            Event::Key(e) => {
                if !self.params.editable {
                    return Ok(());
                }

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
                            self.cursor = Some(CursorRange {
                                start: idx,
                                end: idx,
                            });
                            self.set_current_value(new_value);
                            self.dirty = true;
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
                            self.dirty = true;
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
                                self.dirty = true;
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
                                self.dirty = true;
                            }
                        }
                        Key::UpArrow => {
                            cursor.update(0, e.shift);
                            self.cursor = Some(cursor);
                            self.last_change = Instant::now();
                            self.dirty = true;
                        }
                        Key::DownArrow => {
                            cursor.update(self.current_value.len(), e.shift);
                            self.cursor = Some(cursor);
                            self.last_change = Instant::now();
                            self.dirty = true;
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
