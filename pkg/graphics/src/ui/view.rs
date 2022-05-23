use common::errors::*;

use crate::raster::canvas::Canvas;
use crate::ui::element::{Element, ViewWithParamsElement};
use crate::ui::event::Event;

pub struct RenderBox {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy)]
pub struct MouseCursor(pub glfw::StandardCursor);

pub struct ViewStatus {
    /// When the user's
    pub cursor: MouseCursor,

    /// If true, the view from which this status was returned should gain or
    /// maintain the user's focus (key events are propagated to it).
    ///
    /// A leaf most View which returns true for this MUST support handling the
    /// Blur event. Upon receiving this event, the view MUST internally lose
    /// focus.
    pub focused: bool,
}

impl Default for ViewStatus {
    fn default() -> Self {
        Self {
            cursor: MouseCursor(glfw::StandardCursor::Arrow),
            focused: false,
        }
    }
}

pub trait View: ViewUpdate {
    /// Called before the rendering process starts on all nodes.
    ///
    /// TODO: Should we call this multiple times if the first time we had focus
    /// but later we lost it? (if something is focused, prefer to call its build
    /// method last)
    fn build(&mut self) -> Result<ViewStatus>;

    /// Calculates the box which is occupied when drawing this view.
    ///
    /// This function may be called multiple times before render() with
    /// different parameters for a single frame so ideally no state should be
    /// maintained.
    ///
    /// The returned box should be consistent with what is done when render() is
    /// called with the same arguments.
    ///
    /// Arguments:
    /// - parent_box: The available amount of space in which we could draw this
    ///   view. 'Inlineable' views should tend to drawing in the smallest amount
    ///   of space into which they can fit.
    fn layout(&self, parent_box: &RenderBox) -> Result<RenderBox>;

    /// NOTE: This allows self mutation primarily for caching information about
    /// how the view was rendered (e.g. boxes of children for handling events).
    /// If no events occur, then multiple sequential calls to render() should
    /// draw the same image.
    fn render(&mut self, parent_box: &RenderBox, canvas: &mut Canvas) -> Result<()>;

    fn handle_event(&mut self, event: &Event) -> Result<()>;
}

pub trait ViewUpdate {
    /// Should update the view based on any changed parameters in the given
    /// element representing this view.
    ///
    /// This is the first method caleld on each frame.
    fn update(&mut self, new_element: &Element) -> Result<()>;
}

pub trait ViewParams {
    type View: ViewWithParams<Params = Self> + 'static;
}

pub trait ViewWithParams {
    type Params: 'static;

    fn create_with_params(params: &Self::Params) -> Result<Box<dyn View>>;

    fn update_with_params(&mut self, new_params: &Self::Params) -> Result<()>;
}

impl<V: ViewWithParams + 'static> ViewUpdate for V {
    fn update(&mut self, new_element: &Element) -> Result<()> {
        let el = new_element
            .inner
            .as_any()
            .downcast_ref::<ViewWithParamsElement<V>>()
            .ok_or_else(|| err_msg("Type mismatch"))?;

        self.update_with_params(el.params())
    }
}
