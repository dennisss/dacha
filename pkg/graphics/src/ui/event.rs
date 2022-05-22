/// External feedback received by the UI renderer (e.g. keyboard input, or mouse
/// movement).
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
}

#[derive(Clone, Debug, PartialEq)]
pub enum MouseEventKind {
    Move,
    Enter,
    Exit,
    ButtonDown(MouseButton),
    ButtonUp(MouseButton),
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
