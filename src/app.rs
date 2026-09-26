use std::time::{Duration, Instant};

use glam::Vec2;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, Modifiers, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::game::{ClickMode, GameState, ui_projection};
use crate::icon;
use crate::renderer::{DrawBatch, Renderer};

/// Cap on the render loop's frame rate, so it doesn't load the GPU with
/// frames the display can't show.
const TARGET_FPS: u64 = 165;
const FRAME_DURATION: Duration = Duration::from_micros(1_000_000 / TARGET_FPS);

/// Window icon sizes, in pixels; Windows scales them to fit.
const WINDOW_ICON_SIZE: u32 = 64;
#[cfg(windows)]
const TASKBAR_ICON_SIZE: u32 = 256;

pub struct App {
    // Declared before `window` so it's dropped first: the Vulkan surface
    // must be destroyed before the window it was created from.
    renderer: Option<Renderer>,
    window: Option<Window>,
    game: GameState,
    last_frame: Option<Instant>,
    minimized: bool,
    cursor_pos: Option<Vec2>,
    panning: bool,
    modifiers: Modifiers,
}

impl Default for App {
    fn default() -> Self {
        Self {
            renderer: None,
            window: None,
            game: GameState::new(),
            last_frame: None,
            minimized: false,
            cursor_pos: None,
            panning: false,
            modifiers: Modifiers::default(),
        }
    }
}

impl App {
    /// F5: switches between a borderless fullscreen window on the current
    /// monitor and a normal window. The renderer picks up the new size from
    /// the resize event.
    fn toggle_fullscreen(&self) {
        let Some(window) = &self.window else { return };
        let fullscreen = match window.fullscreen() {
            Some(_) => None,
            None => Some(Fullscreen::Borderless(None)),
        };
        window.set_fullscreen(fullscreen);
    }

    fn screen_size(&self) -> Option<Vec2> {
        let (width, height) = self.renderer.as_ref()?.window_size();
        Some(Vec2::new(width as f32, height as f32))
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Hex Combat Sandbox")
            .with_inner_size(PhysicalSize::new(1280, 720))
            .with_window_icon(Some(icon::icon(WINDOW_ICON_SIZE)));
        // Windows shows a separate, larger icon on the taskbar.
        #[cfg(windows)]
        let attributes = {
            use winit::platform::windows::WindowAttributesExtWindows;
            attributes.with_taskbar_icon(Some(icon::icon(TASKBAR_ICON_SIZE)))
        };
        let window = event_loop
            .create_window(attributes)
            .expect("failed to create window");

        // Safety: `App` drops the renderer before the window (see field order).
        match unsafe { Renderer::new(&window) } {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(err) => {
                log::error!("failed to initialize renderer: {err:?}");
                event_loop.exit();
                return;
            }
        }
        self.window = Some(window);
        self.last_frame = Some(Instant::now());
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                if let Some(renderer) = &self.renderer {
                    renderer.wait_idle();
                }
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.minimized = size.width == 0 || size.height == 0;
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let pos = Vec2::new(position.x as f32, position.y as f32);
                if self.panning
                    && let (Some(last), Some(size)) = (self.cursor_pos, self.screen_size())
                {
                    self.game.camera.pan(pos - last, size);
                }
                self.cursor_pos = Some(pos);
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers,
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, MouseButton::Left) => {
                    if let (Some(cursor), Some(size)) = (self.cursor_pos, self.screen_size()) {
                        let keys = self.modifiers.state();
                        let mode = if keys.shift_key() {
                            ClickMode::Attack
                        } else if keys.control_key() {
                            ClickMode::Swap
                        } else {
                            ClickMode::Normal
                        };
                        self.game.handle_click(cursor, size, mode);
                    }
                }
                (ElementState::Pressed, MouseButton::Right) => self.game.handle_right_click(),
                (ElementState::Pressed, MouseButton::Middle) => self.panning = true,
                (ElementState::Released, MouseButton::Middle) => self.panning = false,
                _ => {}
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y / 100.0) as f32,
                };
                self.game.camera.zoom(steps);
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(key),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => match key {
                KeyCode::Space => self.game.hold_selected_unit(),
                KeyCode::Tab => self.game.select_next_unit(),
                KeyCode::KeyQ => self.game.toggle_selected_ability(),
                KeyCode::F5 => self.toggle_fullscreen(),
                _ => {}
            },
            WindowEvent::RedrawRequested => {
                if self.minimized {
                    return;
                }

                let now = Instant::now();
                let dt = now - self.last_frame.unwrap_or(now);
                self.last_frame = Some(now);
                self.game.update(dt.as_secs_f32());

                let Some(size) = self.screen_size() else {
                    return;
                };
                let world = self.game.build_vertices();
                let ui = self.game.build_ui(size);
                let batches = [
                    DrawBatch {
                        view_proj: self.game.camera.view_proj(size),
                        vertices: &world,
                    },
                    DrawBatch {
                        view_proj: ui_projection(size),
                        vertices: &ui,
                    },
                ];
                if let Some(renderer) = &mut self.renderer
                    && let Err(err) = renderer.draw_frame(&batches)
                {
                    log::error!("draw_frame failed: {err:?}");
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = &self.window else { return };
        if self.minimized {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }

        let next_frame_at = self
            .last_frame
            .map_or_else(Instant::now, |t| t + FRAME_DURATION);
        if Instant::now() >= next_frame_at {
            window.request_redraw();
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(next_frame_at));
        }
    }
}
