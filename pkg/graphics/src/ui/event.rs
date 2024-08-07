use crate::ui::range::CursorRange;

/// External feedback received by the UI renderer (e.g. keyboard input, or mouse
/// movement).
///
/// NOTE: Stateful events like Enter, Blur will only be triggered once on a view
/// even if the focus transitions between different spans of the view.
#[derive(Clone, Debug)]
pub enum Event {
    Mouse(MouseEvent),
    Key(KeyEvent),
    Blur,
}

#[derive(Clone, Debug)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub relative_x: f32,
    pub relative_y: f32,

    /// When the View receiving this event was rendered as multiple spans, this is the
    /// range of the RenderBox which intersects this mouse event. The above relative_x/y
    /// coordinates are relative to that range's box.
    /// 
    /// NOTE: Not available for Exit right now.
    pub range: Option<CursorRange> 
}

#[derive(Clone, Debug, PartialEq)]
pub enum MouseEventKind {
    Move,
    Enter,
    Exit,
    ButtonDown(MouseButton),
    ButtonUp(MouseButton),
    Scroll { x: f32, y: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum MouseButton {
    Left,
    Right,
}

#[derive(Clone, Debug)]
pub struct KeyEvent {
    pub kind: KeyEventKind,
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Key {
    Printable(char),
    LeftArrow,
    RightArrow,
    DownArrow,
    UpArrow,
    Backspace,
    Tab,
    Enter,
    Escape,
    Delete,
}

#[derive(Clone, Debug, PartialEq)]
pub enum KeyEventKind {
    Up,
    Down,
}

pub struct MouseClickFilter {
    down: bool,
}

impl MouseClickFilter {
    pub fn new() -> Self {
        Self { down: false }
    }

    pub fn currently_pressed(&self) -> bool {
        self.down
    }

    /// Update the filter's state based on the next received event.
    ///
    /// This MUST be called with every single event received by the view.
    ///
    /// Returns whether or not a 'click' just happened.
    pub fn process(&mut self, next_event: &Event) -> bool {
        let mouse = match next_event {
            Event::Mouse(v) => v,
            _ => {
                return false;
            }
        };

        match mouse.kind {
            MouseEventKind::Move => {}
            MouseEventKind::ButtonDown(MouseButton::Left) => {
                self.down = true;
            }
            MouseEventKind::ButtonUp(MouseButton::Left) => {
                let v = self.down;
                self.down = false;
                return v;
            }
            _ => {
                self.down = false;
            }
        }

        false
    }
}
