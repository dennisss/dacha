use common::errors::*;

use image::Color;

use crate::canvas::*;
use crate::opengl::canvas::*;
use crate::opengl::canvas_render_loop::CanvasFrameHandler;
use crate::raster::canvas::*;
use crate::raster::canvas_render_loop::WindowOptions;
use crate::ui::element::Element;
use crate::ui::event::*;
use crate::ui::view::*;

struct ViewFrameHandler {
    view: Box<dyn View>,
}

impl CanvasFrameHandler for ViewFrameHandler {
    fn render(
        &mut self,
        canvas: &mut dyn Canvas,
        window: &mut crate::opengl::window::Window,
        events: &[glfw::WindowEvent],
    ) -> Result<()> {
        let outer_box = RenderBox {
            width: window.width() as f32,
            height: window.height() as f32,
        };

        canvas.clear_rect(
            0.,
            0.,
            window.width() as f32,
            window.height() as f32,
            &Color::rgb(255, 255, 255),
        )?;

        for e in events {
            let view_event = match e {
                glfw::WindowEvent::CursorEnter(entered) => {
                    let (x, y) = window.raw().get_cursor_pos();

                    Event::Mouse(MouseEvent {
                        kind: if *entered {
                            MouseEventKind::Enter
                        } else {
                            MouseEventKind::Exit
                        },
                        relative_x: x as f32,
                        relative_y: y as f32,
                    })
                }
                glfw::WindowEvent::CursorPos(x, y) => Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Move,
                    relative_x: *x as f32,
                    relative_y: *y as f32,
                }),
                glfw::WindowEvent::MouseButton(button, action, modifiers) => {
                    let (x, y) = window.raw().get_cursor_pos();

                    let button = match button {
                        glfw::MouseButtonLeft => MouseButton::Left,
                        glfw::MouseButtonRight => MouseButton::Right,
                        _ => continue,
                    };

                    Event::Mouse(MouseEvent {
                        kind: match action {
                            glfw::Action::Press => MouseEventKind::ButtonDown(button),
                            glfw::Action::Release => MouseEventKind::ButtonUp(button),
                            _ => continue,
                        },
                        relative_x: x as f32,
                        relative_y: y as f32,
                    })
                }
                glfw::WindowEvent::CharModifiers(c, modifiers) => Event::Key(KeyEvent {
                    kind: KeyEventKind::Down,
                    key: Key::Printable(*c),
                    shift: modifiers.contains(glfw::Modifiers::Shift),
                    ctrl: modifiers.contains(glfw::Modifiers::Control),
                }),
                glfw::WindowEvent::Key(key, scancode, action, modifiers) => {
                    let key = {
                        match key {
                            glfw::Key::Left => Key::LeftArrow,
                            glfw::Key::Right => Key::RightArrow,
                            glfw::Key::Down => Key::DownArrow,
                            glfw::Key::Up => Key::UpArrow,
                            glfw::Key::Backspace => Key::Backspace,
                            glfw::Key::Tab => Key::Tab,
                            glfw::Key::Enter => Key::Enter,
                            glfw::Key::Escape => Key::Escape,
                            glfw::Key::Delete => Key::Delete,
                            _ => continue,
                        }
                    };

                    let kind = match action {
                        glfw::Action::Press => KeyEventKind::Down,
                        glfw::Action::Release => KeyEventKind::Up,
                        _ => continue,
                    };

                    Event::Key(KeyEvent {
                        kind,
                        key,
                        shift: modifiers.contains(glfw::Modifiers::Shift),
                        ctrl: modifiers.contains(glfw::Modifiers::Control),
                    })
                }
                _ => {
                    continue;
                }
            };

            self.view.handle_event(&view_event)?;
        }

        let status = self.view.build()?;

        self.view.render(&outer_box, canvas)?;

        // TODO: Cache the cursor instances if nothing has changed since last time.
        window
            .raw()
            .set_cursor(Some(glfw::Cursor::standard(status.cursor.0)));

        Ok(())
    }
}

pub async fn render_element(root_element: Element, height: usize, width: usize) -> Result<()> {
    let mut view = root_element.inner.instantiate()?;

    // NOTE: The element may store references to canvas objects (e.g. path object
    // caches) so it can't outlike the window.
    drop(root_element);

    // const SCALING: usize = 4;

    // let mut canvas = RasterCanvas::create(height * SCALING, width * SCALING);
    // canvas.scale(SCALING as f32, SCALING as f32);

    let window_options = WindowOptions::new("Canvas", width, height);

    OpenGLCanvas::render_loop(window_options, ViewFrameHandler { view }).await?;

    Ok(())
}
