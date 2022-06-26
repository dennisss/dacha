use common::errors::*;

use crate::canvas::Canvas;
use crate::ui::element::{Element, ViewWithParamsElement};
use crate::ui::event::Event;
use crate::ui::range::CursorRange;

/// Constraints propagated from a parent view that limit how large or how much
/// or a view can be shown.
#[derive(Clone, Debug)]
pub struct LayoutConstraints {
    /// Maximum amount of horizontal space measured in pixels which this view
    /// has to render itself.
    pub max_width: f32,

    /// Similar to max_width, but this is the maximum vertical space.
    pub max_height: f32,

    /// If set, render the view starting at this position until the
    /// max_width/max_height is hit.
    ///
    /// - The meaning of the position is defined by the view. So this may only
    ///   ever be passed a value of Some(0) or render_box.next_cursor from a
    ///   previous call to layout().
    /// - If None, the view may render to any size.
    ///
    /// TODO: When passed to render(), consider directly passing a range of
    /// (start, end) cursor here so it doesn't need to be recalculated.
    pub start_cursor: Option<usize>,
}

#[derive(Clone)]
pub struct RenderBox {
    pub width: f32,
    pub height: f32,

    /// Relative to the top of the box, how many pixels down must we go to reach
    /// the canonical text baseline point of the rendered view.
    ///
    /// This is only used when the view is nested in a paragraph and needs to be
    /// vertically aligned alongside other views.
    pub baseline_offset: f32,

    /// 
    pub range: CursorRange,

    /// When this view was laid out with a start_cursor, this specifies the next
    /// position in this view which didn't fit in the constraints. If None, then
    /// the entire view would fit within the constraints.
    pub next_cursor: Option<usize>,
}

#[derive(Clone, Copy)]
pub struct MouseCursor(pub glfw::StandardCursor);

pub struct ViewStatus {
    /// When the user's mouse position is hovering over the view, what should
    /// the on screen cursor look like.
    pub cursor: MouseCursor,

    /// If true, the view from which this status was returned should gain or
    /// maintain the user's focus (key events are propagated to it).
    ///
    /// A leaf most View which returns true for this MUST support handling the
    /// Blur event. Upon receiving this event, the view MUST internally lose
    /// focus.
    pub focused: bool,

    /// Whether or not this view or any child views have changed and need to be
    /// re-rendered.
    ///
    /// If a view sets this to false, render() may not be called on it in the
    /// current frame. If the layout constraints for a view has changed,
    /// render() will always be called on it even if it returned 'dirty: false'
    /// for the frame.
    pub dirty: bool,
}

impl Default for ViewStatus {
    fn default() -> Self {
        Self {
            cursor: MouseCursor(glfw::StandardCursor::Arrow),
            focused: false,
            dirty: false,
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
    /// This function may be called zero or more times before render() with
    /// different parameters for a single frame so ideally no state should be
    /// maintained. Usage of this function is dependent on the parent view.
    ///
    /// The returned box should be consistent with what is done when render() is
    /// called with the same arguments.
    ///
    /// Arguments:
    /// - parent_box: The available amount of space in which we could draw this
    ///   view. 'Inlineable' views should tend to drawing in the smallest amount
    ///   of space into which they can fit.
    fn layout(&self, constraints: &LayoutConstraints) -> Result<RenderBox>;

    /// NOTE: This allows self mutation primarily for caching information about
    /// how the view was rendered (e.g. boxes of children for handling events).
    /// If no events occur, then multiple sequential calls to render() should
    /// draw the same image.
    ///
    /// TODO: Make it more clear the the constraints passed here have the exact
    /// width/height of the box we should draw into and not just bounds
    /// (similarly for the cursor).
    fn render(&mut self, constraints: &LayoutConstraints, canvas: &mut dyn Canvas) -> Result<()>;

    fn handle_event(&mut self, event: &Event) -> Result<()>;
}

pub trait ViewUpdate {
    /// Should update the view based on any changed parameters in the given
    /// element representing this view.
    ///
    /// When called, it will precede calls to other View methods in the same
    /// frame.
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
