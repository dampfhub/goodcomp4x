use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use glam::Vec2;
use imgui::{ConfigFlags, Context as ImGuiContext, FontConfig, FontId, FontSource, StyleColor};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{
    ElementState, Event, KeyEvent, Modifiers, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::cli::Options;
use crate::game::Settings;
use crate::game::{
    ClickMode, GameState, ImGuiLayoutState, Scenario, font_atlas, selection_box, ui_projection,
};
use crate::icon;
use crate::persist;
use crate::renderer::{DrawBatch, Renderer};
use crate::screenshot::{self, Screenshot};

/// Cap on the render loop's frame rate, so it doesn't load the GPU with
/// frames the display can't show.
const TARGET_FPS: u64 = 165;
const FRAME_DURATION: Duration = Duration::from_micros(1_000_000 / TARGET_FPS);
const DRAG_THRESHOLD: f32 = 6.0;
/// The window opens at this fraction of the primary monitor's size.
const WINDOW_SCREEN_FRACTION: f32 = 0.8;

/// The files the session is kept in between runs (`persist.rs`): the
/// player's settings, saved whenever they change; the window's size, the UI
/// presentation and the ImGui panels' layout, and ImGui's own docking data,
/// saved on quitting.
const SETTINGS_FILE: &str = "settings.txt";
const LAYOUT_FILE: &str = "layout.txt";
const IMGUI_FILE: &str = "imgui.ini";
/// Window size when the monitor's size can't be found.
const DEFAULT_WINDOW_SIZE: PhysicalSize<u32> = PhysicalSize::new(1600, 900);

/// Window icon sizes, in pixels; Windows scales them to fit.
const WINDOW_ICON_SIZE: u32 = 64;
#[cfg(windows)]
const TASKBAR_ICON_SIZE: u32 = 256;
/// How long after the window first shows its icons are set again.
const ICON_REFRESH_DELAY: Duration = Duration::from_secs(1);

pub struct App {
    // Declared before `window` so it's dropped first: the Vulkan surface
    // must be destroyed before the window it was created from.
    renderer: Option<Renderer>,
    window: Option<Window>,
    game: GameState,
    imgui: Option<ImGuiContext>,
    imgui_platform: Option<WinitPlatform>,
    imgui_fonts: Option<[FontId; 3]>,
    imgui_layout: ImGuiLayoutState,
    use_imgui: bool,
    last_frame: Option<Instant>,
    minimized: bool,
    /// When to set the window's icons again (`ICON_REFRESH_DELAY` after it
    /// first shows), so the taskbar button picks them up.
    icon_refresh_at: Option<Instant>,
    cursor_pos: Option<Vec2>,
    panning: bool,
    left_press: Option<(Vec2, ClickMode, bool)>,
    left_dragging: bool,
    /// The left button is down placing walls or gates on hex edges.
    painting_barriers: bool,
    queue_scroll_dragging: bool,
    queue_item_dragging: bool,
    modifiers: Modifiers,
    /// Where a left press on the map started, while the button is down: once
    /// the cursor moves `DRAG_THRESHOLD` away it's a selection box, not a
    /// click.
    box_start: Option<Vec2>,
    /// The window's inner size, if set on the command line (or by screenshot
    /// mode); otherwise it's a fraction of the monitor.
    requested_size: Option<PhysicalSize<u32>>,
    /// Screenshot mode (`--screenshot`): the window stays hidden, ignores
    /// input, and the app quits once a frame is written.
    screenshot: Option<Screenshot>,
    /// Why the app quit, if something failed; `main` returns it.
    failure: Option<anyhow::Error>,
    /// Keep settings and layout between sessions: everywhere but screenshot
    /// mode, which always starts from the defaults so shots are repeatable.
    remember: bool,
    /// The settings as last saved, to save again only when they change.
    saved_settings: String,
    /// ImGui's docking data from the last session, for the new context.
    imgui_ini: Option<String>,
    /// Start maximized, as the window was when the last session ended.
    start_maximized: bool,
    /// The window's size when last neither maximized nor fullscreen, to
    /// open at next time.
    normal_size: Option<PhysicalSize<u32>>,
}

/// What `LAYOUT_FILE` says about the window, beside the ImGui layout
/// (`ImGuiLayoutState::from_text` skips these lines): the presentation, the
/// window's size and whether it's maximized.
struct SavedWindow {
    imgui: bool,
    size: Option<PhysicalSize<u32>>,
    maximized: bool,
}

impl SavedWindow {
    fn from_text(text: &str) -> Self {
        let mut saved = SavedWindow {
            imgui: true,
            size: None,
            maximized: false,
        };
        for (key, values) in text.lines().filter_map(persist::key_and_values) {
            let number = |i: usize| values.get(i).and_then(|v| v.parse::<u32>().ok());
            match key {
                "presentation" => saved.imgui = values.first() != Some(&"classic"),
                "window_size" => {
                    if let (Some(width), Some(height)) = (number(0), number(1))
                        && width >= 320
                        && height >= 240
                    {
                        saved.size = Some(PhysicalSize::new(width, height));
                    }
                }
                "maximized" => saved.maximized = number(0) == Some(1),
                _ => {}
            }
        }
        saved
    }
}

impl App {
    pub fn new(options: Options) -> Self {
        let screenshot = options.screenshot.map(Screenshot::new);
        let remember = screenshot.is_none();
        let load = |name| if remember { persist::read(name) } else { None };
        let settings =
            load(SETTINGS_FILE).map_or_else(Settings::default, |text| Settings::from_text(&text));
        let layout = load(LAYOUT_FILE).unwrap_or_default();
        let saved = SavedWindow::from_text(&layout);
        let requested_size = options
            .size
            .or(screenshot.as_ref().map(|_| screenshot::DEFAULT_SIZE))
            .map(|(width, height)| PhysicalSize::new(width, height))
            .or(saved.size);
        let mut game = match options.seed {
            Some(seed) => GameState::world_scenario_with(seed, &settings),
            None => options.scenario.new_game(&settings),
        };
        game.set_settings(settings);
        Self {
            renderer: None,
            window: None,
            saved_settings: game.settings_text(),
            game,
            imgui: None,
            imgui_platform: None,
            imgui_fonts: None,
            imgui_layout: ImGuiLayoutState::from_text(&layout),
            use_imgui: saved.imgui,

            last_frame: None,
            minimized: false,
            icon_refresh_at: None,
            cursor_pos: None,
            panning: false,
            left_press: None,
            left_dragging: false,
            painting_barriers: false,
            queue_scroll_dragging: false,
            queue_item_dragging: false,
            modifiers: Modifiers::default(),
            box_start: None,
            requested_size,
            screenshot,
            failure: None,
            remember,
            imgui_ini: load(IMGUI_FILE),
            start_maximized: remember && saved.maximized,
            normal_size: requested_size,
        }
    }

    /// Saves the settings if they changed since last saved.
    fn save_settings_if_changed(&mut self) {
        if !self.remember {
            return;
        }
        let text = self.game.settings_text();
        if text != self.saved_settings {
            persist::write(SETTINGS_FILE, &text);
            self.saved_settings = text;
        }
    }

    /// On quitting: saves the settings, the window, the presentation and the
    /// layout of the panels, ImGui's docking included, for the next session.
    fn save_session(&mut self) {
        if !self.remember {
            return;
        }
        self.save_settings_if_changed();
        let mut layout = format!(
            "presentation {}\n",
            if self.use_imgui { "imgui" } else { "classic" }
        );
        if let Some(size) = self.normal_size {
            layout += &format!("window_size {} {}\n", size.width, size.height);
        }
        let maximized = self.window.as_ref().is_some_and(Window::is_maximized);
        layout += &format!("maximized {}\n", u8::from(maximized));
        layout += &self.imgui_layout.to_text();
        persist::write(LAYOUT_FILE, &layout);
        if let Some(imgui) = &mut self.imgui {
            let mut ini = String::new();
            imgui.save_ini_settings(&mut ini);
            persist::write(IMGUI_FILE, &ini);
        }
    }

    /// How the run ended, once the event loop has returned: an error if
    /// something failed, or if screenshot mode quit without writing one.
    pub fn into_result(self) -> anyhow::Result<()> {
        if let Some(err) = self.failure {
            return Err(err);
        }
        if self.screenshot.is_some_and(|shot| !shot.is_written()) {
            bail!("the window closed before the screenshot was written");
        }
        Ok(())
    }

    /// Quits, with `err` as the reason `main` reports.
    fn fail(&mut self, event_loop: &ActiveEventLoop, err: anyhow::Error) {
        if let Some(renderer) = &self.renderer {
            renderer.wait_idle();
        }
        self.failure.get_or_insert(err);
        event_loop.exit();
    }

    /// Advances the game by the time since the last frame and draws it.
    /// Sets the window's icons again, as new icon handles, so Windows sees
    /// them change and redraws the taskbar button with them (it doesn't
    /// always pick up the icons set as the window is created).
    fn refresh_icons(&self) {
        let Some(window) = &self.window else {
            return;
        };
        window.set_window_icon(Some(icon::icon(WINDOW_ICON_SIZE)));
        #[cfg(windows)]
        {
            use winit::platform::windows::WindowExtWindows;
            window.set_taskbar_icon(Some(icon::icon(TASKBAR_ICON_SIZE)));
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        if self.minimized {
            return;
        }

        let now = Instant::now();
        let dt = now - self.last_frame.unwrap_or(now);
        self.last_frame = Some(now);
        self.game.update(dt.as_secs_f32());
        // A screenshot shows the clouds still, so the same arguments give the
        // same image.
        if self.screenshot.is_none() {
            self.game.animate_clouds(dt.as_secs_f32());
        }
        if self.icon_refresh_at.is_some_and(|at| now >= at) {
            self.icon_refresh_at = None;
            self.refresh_icons();
        }

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
        self.save_settings_if_changed();
        // The settings menu's Quit button.
        if self.game.quit_requested() {
            self.save_session();
            if let Some(renderer) = &self.renderer {
                renderer.wait_idle();
            }
            event_loop.exit();
            return;
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
            if self.use_imgui
                && let Some(fonts) = self.imgui_fonts
            {
                self.game
                    .draw_imgui(frame, size, self.cursor_pos, &fonts, &mut self.imgui_layout);
            }
            platform.prepare_render(frame, window);
            Some(imgui.render())
        } else {
            None
        };
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        let finished = renderer
            .draw_frame(&batches, imgui_data)
            .context("draw_frame failed")
            .and_then(|()| match &mut self.screenshot {
                Some(shot) => shot.after_frame(renderer),
                None => Ok(false),
            });
        match finished {
            Ok(false) => {}
            Ok(true) => {
                renderer.wait_idle();
                event_loop.exit();
            }
            Err(err) => self.fail(event_loop, err),
        }
    }

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

        // Most of the screen (unless a size was asked for), centered, so
        // there's room for the map and the UI.
        let mut attributes = Window::default_attributes()
            .with_title("Hex Combat Sandbox")
            .with_inner_size(self.requested_size.unwrap_or(DEFAULT_WINDOW_SIZE))
            .with_window_icon(Some(icon::icon(WINDOW_ICON_SIZE)))
            // Created hidden and shown once it exists (below), so its icons are
            // set before the taskbar button is made. A screenshot needs no
            // window on screen, or taking focus, so it stays hidden.
            .with_visible(false)
            .with_maximized(self.start_maximized);
        // Windows shows a separate, larger icon on the taskbar.
        #[cfg(windows)]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attributes = attributes.with_taskbar_icon(Some(icon::icon(TASKBAR_ICON_SIZE)));
        }
        if let Some(monitor) = event_loop.primary_monitor() {
            let (screen, origin) = (monitor.size(), monitor.position());
            let size = self.requested_size.unwrap_or(PhysicalSize::new(
                (screen.width as f32 * WINDOW_SCREEN_FRACTION) as u32,
                (screen.height as f32 * WINDOW_SCREEN_FRACTION) as u32,
            ));
            let position = PhysicalPosition::new(
                origin.x + (screen.width as i32 - size.width as i32) / 2,
                origin.y + (screen.height as i32 - size.height as i32) / 2,
            );
            attributes = attributes.with_inner_size(size).with_position(position);
        }
        let window = event_loop
            .create_window(attributes)
            .expect("failed to create window");
        // Shown once winit has attached the icons. Windows can still make the
        // taskbar button with the blank default icon and not update it until
        // the window is minimized and restored, so the icons are set again
        // shortly after (`refresh_icons`).
        if self.screenshot.is_none() {
            window.set_visible(true);
            self.icon_refresh_at = Some(Instant::now() + ICON_REFRESH_DELAY);
        }

        let mut imgui = ImGuiContext::create();
        // No file of its own: its docking data is kept with the rest of the
        // session (`save_session`) and loaded before the first frame.
        imgui.set_ini_filename(None);
        if let Some(ini) = &self.imgui_ini {
            imgui.load_ini_settings(ini);
        }
        imgui
            .io_mut()
            .config_flags
            .insert(ConfigFlags::DOCKING_ENABLE);
        imgui.io_mut().config_docking_transparent_payload = true;
        // The corner grip is reliable here; edge resizing conflicts with the
        // game's panel placement and offers no useful cursor feedback.
        imgui.io_mut().config_windows_resize_from_edges = false;
        // Use the host UI font when available. ImGui copies the bytes into its atlas.
        let system_font = std::fs::read("C:\\Windows\\Fonts\\segoeui.ttf").ok();
        let mut add_font = |size| {
            if let Some(font) = &system_font {
                imgui.fonts().add_font(&[FontSource::TtfData {
                    data: font,
                    size_pixels: size,
                    config: None,
                }])
            } else {
                imgui.fonts().add_font(&[FontSource::DefaultFontData {
                    config: Some(FontConfig {
                        size_pixels: size,
                        ..FontConfig::default()
                    }),
                }])
            }
        };
        let body_font = add_font(18.0);
        let small_font = add_font(15.0);
        let title_font = add_font(22.0);
        let style = imgui.style_mut();
        style.window_padding = [12.0, 10.0];
        style.frame_padding = [10.0, 6.0];
        style.item_spacing = [7.0, 6.0];
        style.window_rounding = 0.0;
        style.frame_rounding = 0.0;
        style.scrollbar_rounding = 0.0;
        style.popup_rounding = 0.0;
        style.child_rounding = 0.0;
        style.grab_rounding = 0.0;
        style.tab_rounding = 0.0;
        style.window_border_size = 1.0;
        style.frame_border_size = 1.0;
        style.window_title_align = [0.0, 0.5];
        style.button_text_align = [0.5, 0.5];
        style.colors[StyleColor::Text as usize] = [0.91, 0.92, 0.91, 1.0];
        style.colors[StyleColor::TextDisabled as usize] = [0.46, 0.48, 0.50, 1.0];
        style.colors[StyleColor::WindowBg as usize] = [0.018, 0.022, 0.030, 0.96];
        style.colors[StyleColor::PopupBg as usize] = [0.025, 0.030, 0.041, 0.98];
        style.colors[StyleColor::Border as usize] = [0.29, 0.32, 0.38, 0.95];
        style.colors[StyleColor::TitleBg as usize] = [0.030, 0.036, 0.050, 1.0];
        style.colors[StyleColor::TitleBgActive as usize] = [0.055, 0.065, 0.086, 1.0];
        style.colors[StyleColor::TitleBgCollapsed as usize] = [0.030, 0.036, 0.050, 0.96];
        style.colors[StyleColor::FrameBg as usize] = [0.032, 0.039, 0.052, 1.0];
        style.colors[StyleColor::FrameBgHovered as usize] = [0.073, 0.084, 0.108, 1.0];
        style.colors[StyleColor::FrameBgActive as usize] = [0.12, 0.14, 0.18, 1.0];
        style.colors[StyleColor::Button as usize] = [0.045, 0.053, 0.070, 1.0];
        style.colors[StyleColor::ButtonHovered as usize] = [0.085, 0.10, 0.13, 1.0];
        style.colors[StyleColor::ButtonActive as usize] = [0.13, 0.15, 0.19, 1.0];
        style.colors[StyleColor::Header as usize] = [0.075, 0.090, 0.12, 1.0];
        style.colors[StyleColor::HeaderHovered as usize] = [0.12, 0.15, 0.19, 1.0];
        style.colors[StyleColor::ScrollbarBg as usize] = [0.024, 0.029, 0.039, 1.0];
        style.colors[StyleColor::ScrollbarGrab as usize] = [0.21, 0.24, 0.28, 1.0];
        style.colors[StyleColor::ScrollbarGrabHovered as usize] = [0.31, 0.35, 0.39, 1.0];
        style.colors[StyleColor::PlotHistogram as usize] = [0.80, 0.69, 0.35, 1.0];
        style.colors[StyleColor::DragDropTarget as usize] = [0.91, 0.77, 0.38, 1.0];
        let mut imgui_platform = WinitPlatform::new(&mut imgui);
        imgui_platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Default);

        // Safety: `App` drops the renderer before the window (see field order).
        match unsafe { Renderer::new(&window, font_atlas(), &mut imgui) } {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(err) => {
                self.fail(event_loop, err.context("failed to initialize the renderer"));
                return;
            }
        }
        self.window = Some(window);
        self.imgui = Some(imgui);
        self.imgui_platform = Some(imgui_platform);
        self.imgui_fonts = Some([small_font, body_font, title_font]);
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
        // A screenshot shows the scenario as it starts, whatever the mouse and keyboard do.
        if self.screenshot.is_some() && is_input(&event) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                self.save_session();
                if let Some(renderer) = &self.renderer {
                    renderer.wait_idle();
                }
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.minimized = size.width == 0 || size.height == 0;
                let normal = self
                    .window
                    .as_ref()
                    .is_some_and(|w| !w.is_maximized() && w.fullscreen().is_none());
                if normal && !self.minimized {
                    self.normal_size = Some(size);
                }
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
                if self.painting_barriers {
                    if let Some(size) = self.screen_size() {
                        self.game.paint_barrier_at(pos, size, false);
                    }
                    self.cursor_pos = Some(pos);
                    return;
                }
                if self.use_imgui
                    && self
                        .imgui
                        .as_ref()
                        .is_some_and(|ctx| ctx.io().want_capture_mouse)
                {
                    self.cursor_pos = Some(pos);
                    return;
                }
                // A left press that moves far enough is a selection box (drawn
                // from `box_start`), not a click; only the middle button pans.
                if let Some((origin, _, _)) = self.left_press
                    && !self.left_dragging
                    && pos.distance(origin) >= DRAG_THRESHOLD
                {
                    self.left_dragging = true;
                }
                if self.panning
                    && let (Some(last), Some(size)) = (self.cursor_pos, self.screen_size())
                {
                    self.game.camera.pan(pos - last, size);
                }
                self.cursor_pos = Some(pos);
            }
            WindowEvent::Focused(false) | WindowEvent::CursorLeft { .. } => {
                self.painting_barriers = false;
                self.left_press = None;
                self.left_dragging = false;
                self.queue_scroll_dragging = false;
                self.queue_item_dragging = false;
                self.game.cancel_queue_drag();
                self.panning = false;
                self.cursor_pos = None;
                self.box_start = None;
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
                self.game.set_details(modifiers.state().alt_key());
            }
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
                        if !self.use_imgui
                            && !keys.shift_key()
                            && !keys.control_key()
                            && let Some(size) = self.screen_size()
                            && self.game.start_queue_drag_at(cursor, size)
                        {
                            self.queue_item_dragging = true;
                            return;
                        }
                        // With a wall or gate armed, the press (and any drag)
                        // places it on hex edges instead of clicking or panning.
                        if let Some(size) = self.screen_size()
                            && self.game.paint_barrier_at(cursor, size, !self.use_imgui)
                        {
                            self.painting_barriers = true;
                            return;
                        }
                        // Left-click moves; Shift adds the move to the queue.
                        let mode = if keys.shift_key() {
                            ClickMode::QueueMove
                        } else if keys.control_key() {
                            ClickMode::Swap
                        } else {
                            ClickMode::Normal
                        };
                        self.left_press = Some((cursor, mode, !self.game.is_resolving()));
                        self.left_dragging = self.panning;
                        // A drag from the map (not from a classic panel) selects
                        // the units inside its box.
                        let on_panel = !self.use_imgui
                            && self
                                .screen_size()
                                .is_some_and(|size| self.game.ui_covers(cursor, size));
                        self.box_start = (!on_panel && !self.panning).then_some(cursor);
                    }
                }
                (ElementState::Released, MouseButton::Left) => {
                    if self.painting_barriers {
                        self.painting_barriers = false;
                        return;
                    }
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
                        && self.left_dragging
                        && start.distance(end) >= DRAG_THRESHOLD
                    {
                        // Shift adds the boxed units to the selection.
                        let add = self.modifiers.state().shift_key();
                        self.game.select_in_box(start, end, size, add);
                        self.left_press = None;
                        self.left_dragging = false;
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
                    // Right-click attacks; Shift adds the attack to the
                    // queue, Ctrl clears orders.
                    if let (Some(cursor), Some(size)) = (self.cursor_pos, self.screen_size()) {
                        let keys = self.modifiers.state();
                        self.game.handle_context_click(
                            cursor,
                            size,
                            keys.control_key(),
                            keys.shift_key(),
                        );
                    }
                }
                (ElementState::Pressed, MouseButton::Middle) => {
                    self.panning = true;
                    // Panning mid-press cancels both the click and the box.
                    self.box_start = None;
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
            // Escape closes the settings menu, a view or the selection first;
            // with nothing to close it opens the settings menu.
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => self.game.press_escape(),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(key),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                let modifiers = self.modifiers.state();
                if self.use_imgui
                    && key == KeyCode::KeyR
                    && modifiers.control_key()
                    && modifiers.shift_key()
                {
                    if self.imgui_layout.request_reset_active_view() {
                        self.game
                            .set_ui_notice("VIEW DEBUG LAYOUT RESET TO DEFAULT");
                    }
                    return;
                }
                match key {
                    KeyCode::Space => {
                        let closed_menu = self.game.exit_structure_menu();
                        if !closed_menu {
                            self.game.hold_or_end_turn();
                        }
                    }
                    KeyCode::Tab => self.game.select_next_unit(),
                    KeyCode::KeyQ => self.game.toggle_selected_ability(),
                    KeyCode::KeyC => self.game.select_city(),
                    KeyCode::KeyV => self.game.toggle_city_interior(),
                    KeyCode::KeyA => self.game.auto_assign_selected_city(),
                    KeyCode::KeyM => self.game.choose_move_action(),
                    KeyCode::KeyX => self.game.choose_attack_action(),
                    KeyCode::KeyR => self.game.queue_worker_job(crate::game::JobKind::Road),
                    KeyCode::KeyI => self.game.queue_worker_job(crate::game::JobKind::Improve),
                    KeyCode::KeyW => self.game.toggle_worker_mode(),
                    KeyCode::KeyF => self.game.found_city_selected(),
                    KeyCode::Digit1 => self
                        .game
                        .queue_selected_city_unit(crate::game::BuildUnit::Melee),
                    KeyCode::Digit2 => self
                        .game
                        .queue_selected_city_unit(crate::game::BuildUnit::Ranged),
                    KeyCode::Digit3 => self
                        .game
                        .queue_selected_city_unit(crate::game::BuildUnit::Siege),
                    KeyCode::Digit4 => self
                        .game
                        .queue_selected_city_building(crate::game::Building::Granary),
                    KeyCode::Digit5 => self
                        .game
                        .queue_selected_city_building(crate::game::Building::Barracks),
                    KeyCode::Digit6 => self
                        .game
                        .queue_selected_city_building(crate::game::Building::Mill),
                    KeyCode::Digit7 => self
                        .game
                        .queue_selected_city_building(crate::game::Building::Workshop),
                    KeyCode::Backspace if self.game.is_in_city_interior() => {
                        self.game.clear_selected_interior_orders()
                    }
                    KeyCode::Digit8 => self.game.queue_selected_city_worker(),
                    KeyCode::Backspace => self.game.remove_selected_city_queue_head(),
                    KeyCode::Delete => self.game.disband_selected(),
                    KeyCode::PageDown => self.game.move_selected_city_queue_head(false),
                    KeyCode::F1 => self.game.switch_scenario(Scenario::Combat),
                    KeyCode::F2 => self.game.switch_scenario(Scenario::Cities),
                    KeyCode::F3 => self.game.switch_scenario(Scenario::Frontier),
                    KeyCode::F4 => self.game.switch_scenario(Scenario::World),
                    KeyCode::F12 => self.game.switch_scenario(Scenario::Siege),
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
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(Err(err)) = self.screenshot.as_ref().map(Screenshot::check_timeout) {
            self.fail(event_loop, err);
            return;
        }
        let Some(window) = &self.window else { return };
        if self.minimized {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }

        let next_frame_at = self
            .last_frame
            .map_or_else(Instant::now, |t| t + FRAME_DURATION);
        if Instant::now() >= next_frame_at {
            if self.screenshot.is_some() {
                // A hidden window gets no redraw events, so draw right away.
                self.redraw(event_loop);
            } else {
                window.request_redraw();
            }
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(next_frame_at));
        }
    }
}

/// Whether `event` comes from the mouse or keyboard.
fn is_input(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::CursorMoved { .. }
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::CursorLeft { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::KeyboardInput { .. }
            | WindowEvent::ModifiersChanged(_)
    )
}
