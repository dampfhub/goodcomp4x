use std::time::{Duration, Instant};

use glam::Vec2;
use imgui::{Context as ImGuiContext, FontConfig, FontSource};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{
    ElementState, Event, KeyEvent, Modifiers, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::game::{
    ClickMode, GameState, Scenario, font_atlas, quit_prompt, selection_box, ui_projection,
};
use crate::icon;
use crate::renderer::{DrawBatch, Renderer};

/// Cap on the render loop's frame rate, so it doesn't load the GPU with
/// frames the display can't show.
const TARGET_FPS: u64 = 165;
const FRAME_DURATION: Duration = Duration::from_micros(1_000_000 / TARGET_FPS);
const DRAG_THRESHOLD: f32 = 6.0;
/// The window opens at this fraction of the primary monitor's size.
const WINDOW_SCREEN_FRACTION: f32 = 0.8;
/// Window size when the monitor's size can't be found.
const DEFAULT_WINDOW_SIZE: PhysicalSize<u32> = PhysicalSize::new(1600, 900);

/// How long Escape must be held to quit.
const QUIT_HOLD: Duration = Duration::from_secs(1);

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
    imgui: Option<ImGuiContext>,
    imgui_platform: Option<WinitPlatform>,
    use_imgui: bool,
    last_frame: Option<Instant>,
    minimized: bool,
    cursor_pos: Option<Vec2>,
    panning: bool,
    left_press: Option<(Vec2, ClickMode, bool)>,
    left_dragging: bool,
    queue_scroll_dragging: bool,
    queue_item_dragging: bool,
    modifiers: Modifiers,
    /// When Escape was pressed, while it's held; the game quits once it's
    /// been held for `QUIT_HOLD`.
    quit_held_since: Option<Instant>,
    /// Where an Alt-drag selection box started, while the button is down.
    box_start: Option<Vec2>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            renderer: None,
            window: None,
            game: GameState::city_scenario(),
            imgui: None,
            imgui_platform: None,
            use_imgui: true,
            last_frame: None,
            minimized: false,
            cursor_pos: None,
            panning: false,
            left_press: None,
            left_dragging: false,
            queue_scroll_dragging: false,
            queue_item_dragging: false,
            modifiers: Modifiers::default(),
            quit_held_since: None,
            box_start: None,
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

        // Most of the screen, centered, so there's room for the map and the UI.
        let mut attributes = Window::default_attributes()
            .with_title("Hex Combat Sandbox")
            .with_inner_size(DEFAULT_WINDOW_SIZE)
            .with_window_icon(Some(icon::icon(WINDOW_ICON_SIZE)));
        // Windows shows a separate, larger icon on the taskbar.
        #[cfg(windows)]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attributes = attributes.with_taskbar_icon(Some(icon::icon(TASKBAR_ICON_SIZE)));
        }
        if let Some(monitor) = event_loop.primary_monitor() {
            let (screen, origin) = (monitor.size(), monitor.position());
            let size = PhysicalSize::new(
                (screen.width as f32 * WINDOW_SCREEN_FRACTION) as u32,
                (screen.height as f32 * WINDOW_SCREEN_FRACTION) as u32,
            );
            let position = PhysicalPosition::new(
                origin.x + (screen.width - size.width) as i32 / 2,
                origin.y + (screen.height - size.height) as i32 / 2,
            );
            attributes = attributes.with_inner_size(size).with_position(position);
        }
        let window = event_loop
            .create_window(attributes)
            .expect("failed to create window");

        let mut imgui = ImGuiContext::create();
        imgui.set_ini_filename(None);
        imgui.fonts().add_font(&[FontSource::DefaultFontData {
            config: Some(FontConfig {
                size_pixels: 17.0,
                ..FontConfig::default()
            }),
        }]);
        let mut imgui_platform = WinitPlatform::new(&mut imgui);
        imgui_platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Default);

        // Safety: `App` drops the renderer before the window (see field order).
        match unsafe { Renderer::new(&window, font_atlas(), &mut imgui) } {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(err) => {
                log::error!("failed to initialize renderer: {err:?}");
                event_loop.exit();
                return;
            }
        }
        self.window = Some(window);
        self.imgui = Some(imgui);
        self.imgui_platform = Some(imgui_platform);
        self.last_frame = Some(Instant::now());
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let (Some(imgui), Some(platform), Some(window)) =
            (&mut self.imgui, &mut self.imgui_platform, &self.window)
        {
            platform.handle_event(
                imgui.io_mut(),
                window,
                &Event::<()>::WindowEvent {
                    window_id: id,
                    event: event.clone(),
                },
            );
        }
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
                if !self.use_imgui && self.queue_scroll_dragging {
                    if let Some(size) = self.screen_size() {
                        self.game.drag_queue_scrollbar_at(pos, size, true);
                    }
                    self.cursor_pos = Some(pos);
                    return;
                }
                if !self.use_imgui && self.queue_item_dragging {
                    if let Some(size) = self.screen_size() {
                        self.game.update_queue_drag_at(pos, size);
                    }
                    self.cursor_pos = Some(pos);
                    return;
                }
                let mut pan_from = self.cursor_pos;
                if self.use_imgui
                    && self
                        .imgui
                        .as_ref()
                        .is_some_and(|ctx| ctx.io().want_capture_mouse)
                {
                    self.cursor_pos = Some(pos);
                    return;
                }
                if let Some((origin, _, _)) = self.left_press
                    && !self.left_dragging
                    && pos.distance(origin) >= DRAG_THRESHOLD
                {
                    self.left_dragging = true;
                    // Include motion below the threshold when the drag begins.
                    if !self.panning {
                        pan_from = Some(origin);
                    }
                }
                if (self.panning || self.left_dragging)
                    && let (Some(last), Some(size)) = (pan_from, self.screen_size())
                {
                    self.game.camera.pan(pos - last, size);
                }
                self.cursor_pos = Some(pos);
            }
            WindowEvent::Focused(false) | WindowEvent::CursorLeft { .. } => {
                self.left_press = None;
                self.left_dragging = false;
                self.queue_scroll_dragging = false;
                self.queue_item_dragging = false;
                self.game.cancel_queue_drag();
                self.panning = false;
                self.cursor_pos = None;
                // The release may never arrive once focus is gone.
                self.quit_held_since = None;
                self.box_start = None;
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers,
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, MouseButton::Left) => {
                    if self.use_imgui
                        && self
                            .imgui
                            .as_ref()
                            .is_some_and(|ctx| ctx.io().want_capture_mouse)
                    {
                        return;
                    }
                    if let Some(cursor) = self.cursor_pos {
                        if !self.use_imgui
                            && let Some(size) = self.screen_size()
                            && self.game.drag_queue_scrollbar_at(cursor, size, false)
                        {
                            self.queue_scroll_dragging = true;
                            return;
                        }
                        let keys = self.modifiers.state();
                        // Alt starts a selection box instead of a click or pan.
                        if keys.alt_key() {
                            self.box_start = Some(cursor);
                            return;
                        }
                        if !self.use_imgui
                            && !keys.shift_key()
                            && !keys.control_key()
                            && let Some(size) = self.screen_size()
                            && self.game.start_queue_drag_at(cursor, size)
                        {
                            self.queue_item_dragging = true;
                            return;
                        }
                        let mode = if keys.shift_key() {
                            ClickMode::Attack
                        } else if keys.control_key() {
                            ClickMode::Swap
                        } else {
                            ClickMode::Normal
                        };
                        self.left_press = Some((cursor, mode, !self.game.is_resolving()));
                        self.left_dragging = self.panning;
                    }
                }
                (ElementState::Released, MouseButton::Left) => {
                    if self.queue_scroll_dragging {
                        self.queue_scroll_dragging = false;
                        return;
                    }
                    if self.queue_item_dragging {
                        self.queue_item_dragging = false;
                        if let (Some(cursor), Some(size)) = (self.cursor_pos, self.screen_size()) {
                            self.game.finish_queue_drag_at(cursor, size);
                        } else {
                            self.game.cancel_queue_drag();
                        }
                        return;
                    }
                    if let (Some(start), Some(end), Some(size)) =
                        (self.box_start.take(), self.cursor_pos, self.screen_size())
                    {
                        // Barely moving makes it an Alt-click on one unit.
                        if start.distance(end) < DRAG_THRESHOLD {
                            self.game.toggle_in_selection(end, size);
                        } else {
                            self.game.select_in_box(start, end, size);
                        }
                        return;
                    }
                    if let Some((origin, mode, may_click)) = self.left_press.take()
                        && !self.left_dragging
                        && may_click
                        && let Some(size) = self.screen_size()
                    {
                        if self.use_imgui {
                            self.game.handle_map_click(origin, size, mode);
                        } else {
                            self.game.handle_click(origin, size, mode);
                        }
                    }
                    self.left_dragging = false;
                }
                (ElementState::Pressed, MouseButton::Right) => {
                    if self.use_imgui
                        && self
                            .imgui
                            .as_ref()
                            .is_some_and(|ctx| ctx.io().want_capture_mouse)
                    {
                        return;
                    }
                    if let (Some(cursor), Some(size)) = (self.cursor_pos, self.screen_size()) {
                        self.game.handle_context_click(
                            cursor,
                            size,
                            self.modifiers.state().control_key(),
                        );
                    }
                }
                (ElementState::Pressed, MouseButton::Middle) => {
                    self.panning = true;
                    if self.left_press.is_some() {
                        self.left_dragging = true;
                    }
                }
                (ElementState::Released, MouseButton::Middle) => self.panning = false,
                _ => {}
            },
            WindowEvent::MouseWheel { delta, .. } => {
                if self.use_imgui
                    && self
                        .imgui
                        .as_ref()
                        .is_some_and(|ctx| ctx.io().want_capture_mouse)
                {
                    return;
                }
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y / 100.0) as f32,
                };
                if !(matches!((self.cursor_pos, self.screen_size()), (Some(cursor), Some(size)) if self.game.scroll_queue_at(cursor, size, steps)))
                {
                    self.game.camera.zoom(steps);
                }
            }
            // Escape closes management first; otherwise holding it quits.
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        state,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                if state == ElementState::Pressed && self.game.exit_structure_menu() {
                    self.quit_held_since = None;
                } else {
                    self.quit_held_since = (state == ElementState::Pressed).then(Instant::now);
                }
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
                KeyCode::Space => {
                    if !self.game.exit_structure_menu() {
                        self.game.hold_or_end_turn();
                    }
                }
                KeyCode::Tab => self.game.select_next_unit(),
                KeyCode::KeyQ => self.game.toggle_selected_ability(),
                KeyCode::KeyC => self.game.select_city(),
                KeyCode::KeyA => self.game.auto_assign_selected_city(),
                KeyCode::KeyM => self.game.choose_move_action(),
                KeyCode::KeyX => self.game.choose_attack_action(),
                KeyCode::KeyR => self.game.build_worker_road_selected(),
                KeyCode::KeyI => self.game.improve_worker_tile_selected(),
                KeyCode::KeyF => self.game.found_city_selected(),
                KeyCode::Digit1 => self
                    .game
                    .queue_selected_city_unit(crate::game::BuildUnit::Melee),
                KeyCode::Digit2 => self
                    .game
                    .queue_selected_city_unit(crate::game::BuildUnit::Ranged),
                KeyCode::Digit3 => self
                    .game
                    .queue_selected_city_unit(crate::game::BuildUnit::Cavalry),
                KeyCode::Digit4 => self
                    .game
                    .queue_selected_city_unit(crate::game::BuildUnit::Siege),
                KeyCode::Digit5 => self
                    .game
                    .queue_selected_city_building(crate::game::Building::Granary),
                KeyCode::Digit6 => self
                    .game
                    .queue_selected_city_building(crate::game::Building::Barracks),
                KeyCode::Digit7 => self
                    .game
                    .queue_selected_city_building(crate::game::Building::Mill),
                KeyCode::Digit8 => self
                    .game
                    .queue_selected_city_building(crate::game::Building::Workshop),
                // Queue management stays compact as the build catalogue grows:
                // Backspace removes the active item; PageDown promotes the
                // second item into production.
                KeyCode::Backspace => self.game.remove_selected_city_queue_head(),
                KeyCode::PageDown => self.game.move_selected_city_queue_head(false),
                KeyCode::F1 => self.game.switch_scenario(Scenario::Combat),
                KeyCode::F2 => self.game.switch_scenario(Scenario::Cities),
                KeyCode::F3 => self.game.switch_scenario(Scenario::Frontier),
                KeyCode::F4 => self.game.switch_scenario(Scenario::World),
                KeyCode::KeyY => self.game.toggle_yields(),
                KeyCode::KeyG => self.game.toggle_guard(),
                KeyCode::F5 => self.toggle_fullscreen(),
                KeyCode::F6 => self.game.save_state(),
                KeyCode::F7 => self.game.load_state(),
                KeyCode::F8 => self.game.toggle_instant_playback(),
                KeyCode::F9 => self.game.debug_complete_current_production(),
                KeyCode::F10 => self.game.toggle_fog(),
                KeyCode::F11 => {
                    self.use_imgui = !self.use_imgui;
                    self.left_press = None;
                    self.game.cancel_queue_drag();
                    self.game.set_ui_notice(if self.use_imgui {
                        "IMGUI UI (F11 TO COMPARE)"
                    } else {
                        "CLASSIC UI (F11 TO COMPARE)"
                    });
                }
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
                if self.use_imgui {
                    self.game.update_hover_imgui(
                        self.cursor_pos.filter(|_| {
                            !self
                                .imgui
                                .as_ref()
                                .is_some_and(|ctx| ctx.io().want_capture_mouse)
                        }),
                        size,
                        dt.as_secs_f32(),
                    );
                } else {
                    self.game
                        .update_hover(self.cursor_pos, size, dt.as_secs_f32());
                }
                let world = self.game.build_vertices();
                let mut ui = if self.use_imgui {
                    Vec::new()
                } else {
                    self.game.build_ui(size, self.cursor_pos)
                };
                if let Some(since) = self.quit_held_since {
                    let progress = since.elapsed().as_secs_f32() / QUIT_HOLD.as_secs_f32();
                    if progress >= 1.0 {
                        if let Some(renderer) = &self.renderer {
                            renderer.wait_idle();
                        }
                        event_loop.exit();
                        return;
                    }
                    ui.extend(quit_prompt(progress, size));
                }
                if let (Some(start), Some(end)) = (self.box_start, self.cursor_pos)
                    && start.distance(end) >= DRAG_THRESHOLD
                {
                    ui.extend(selection_box(start, end, size));
                }
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
                let imgui_data = if let (Some(imgui), Some(platform), Some(window)) =
                    (&mut self.imgui, &mut self.imgui_platform, &self.window)
                {
                    imgui.io_mut().update_delta_time(dt);
                    let _ = platform.prepare_frame(imgui.io_mut(), window);
                    let frame = imgui.frame();
                    if self.use_imgui {
                        self.game.draw_imgui(frame, size, self.cursor_pos);
                    }
                    platform.prepare_render(frame, window);
                    Some(imgui.render())
                } else {
                    None
                };
                if let Some(renderer) = &mut self.renderer
                    && let Err(err) = renderer.draw_frame(&batches, imgui_data)
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
