use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use glam::Vec2;
use imgui::{ConfigFlags, Context as ImGuiContext, FontId};
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
use crate::clipboard;
use crate::game::Settings;
use crate::game::{
    ClickMode, GameState, ImGuiLayoutState, NetMenu, NetRequest, Scenario, font_atlas,
    selection_box, style_imgui, ui_projection,
};
use crate::icon;
use crate::net::{self, Session};
use crate::persist;
use crate::renderer::{DrawBatch, Renderer, Vertex};
use crate::screenshot::{self, Screenshot};

/// Cap on the render loop's frame rate, so it doesn't load the GPU with
/// frames the display can't show. Below it, frames come at the refresh rate
/// of the monitor the window is on (`frame_duration`).
const TARGET_FPS: u64 = 165;
const FRAME_DURATION: Duration = Duration::from_micros(1_000_000 / TARGET_FPS);
/// Never slower than this, whatever a monitor reports.
const MIN_FPS: u32 = 30;

/// The time between frames for `window`: its monitor's refresh rate, capped
/// at `TARGET_FPS`.
fn frame_duration(window: &Window) -> Duration {
    let hertz = window
        .current_monitor()
        .and_then(|monitor| monitor.refresh_rate_millihertz())
        .map_or(TARGET_FPS as u32, |millihertz| millihertz.div_ceil(1000))
        .clamp(MIN_FPS, TARGET_FPS as u32);
    FRAME_DURATION.max(Duration::from_micros(1_000_000 / u64::from(hertz)))
}
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
/// What the Multiplayer section last had typed in it (`NetMenu::to_text`),
/// saved on hosting, joining and quitting.
const NETWORK_FILE: &str = "network.txt";
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
    /// A multiplayer game's connection (`net/`), pumped every frame.
    network: Option<Session>,
    /// A join the Multiplayer section started, connecting on its own
    /// thread so the window keeps drawing.
    joining: Option<Receiver<anyhow::Result<(Session, GameState)>>>,
    imgui: Option<ImGuiContext>,
    imgui_platform: Option<WinitPlatform>,
    imgui_fonts: Option<[FontId; 3]>,
    imgui_layout: ImGuiLayoutState,
    use_imgui: bool,
    /// The world's and the classic UI's vertices, kept between frames for
    /// their memory (`redraw`).
    world_vertices: Vec<Vertex>,
    ui_vertices: Vec<Vertex>,
    last_frame: Option<Instant>,
    /// The time between frames (`frame_duration`), found again when the
    /// window moves, as it may have moved to another monitor.
    frame_duration: Duration,
    minimized: bool,
    /// When to set the window's icons again (`ICON_REFRESH_DELAY` after it
    /// first shows), so the taskbar button picks them up.
    icon_refresh_at: Option<Instant>,
    cursor_pos: Option<Vec2>,
    panning: bool,
    /// A left press on the map, waiting for its release to be a click: where,
    /// its modifiers, and whether the plan was out of the player's hands
    /// (`is_resolving`) when it went down (`None`: a turn was playing out, so
    /// no click). A release only clicks in the same phase, so a press while
    /// a network game waits for the others' plans can't become an order in
    /// the next turn's planning.
    left_press: Option<(Vec2, ClickMode, Option<bool>)>,
    left_dragging: bool,
    /// The left button is down placing walls or gates on hex edges.
    painting_jobs: bool,
    queue_scroll_dragging: bool,
    building_scroll_dragging: bool,
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
    /// A command-line size applies for this run without changing saved window preferences.
    explicit_size: bool,
    saved_maximized: bool,
}

/// What `LAYOUT_FILE` says about the window, beside the ImGui layout
/// (`ImGuiLayoutState::from_text` skips these lines): the presentation, the
/// window's size and whether it's maximized.
#[derive(Debug, PartialEq, Eq)]
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

    fn to_text(&self) -> String {
        let mut text = format!(
            "presentation {}\n",
            if self.imgui { "imgui" } else { "classic" }
        );
        if let Some(size) = self.size {
            text += &format!("window_size {} {}\n", size.width, size.height);
        }
        text += &format!("maximized {}\n", u8::from(self.maximized));
        text
    }
}

/// Choose a size and centered position. Saved sizes are bounded by the current
/// monitor; an explicit --size stays exact, including for screenshots.
fn initial_window_rect(
    screen: PhysicalSize<u32>,
    origin: PhysicalPosition<i32>,
    requested: Option<PhysicalSize<u32>>,
    saved: Option<PhysicalSize<u32>>,
) -> (PhysicalSize<u32>, PhysicalPosition<i32>) {
    let size = requested.unwrap_or_else(|| {
        saved.map_or_else(
            || {
                PhysicalSize::new(
                    (screen.width as f32 * WINDOW_SCREEN_FRACTION) as u32,
                    (screen.height as f32 * WINDOW_SCREEN_FRACTION) as u32,
                )
            },
            |size| PhysicalSize::new(size.width.min(screen.width), size.height.min(screen.height)),
        )
    });
    let offset_x = i32::try_from(screen.width.saturating_sub(size.width) / 2).unwrap_or(i32::MAX);
    let offset_y = i32::try_from(screen.height.saturating_sub(size.height) / 2).unwrap_or(i32::MAX);
    let position = PhysicalPosition::new(
        origin.x.saturating_add(offset_x),
        origin.y.saturating_add(offset_y),
    );
    (size, position)
}

/// `game` is hosted on `port`: its Multiplayer page shows the address
/// players on the local network join at, if this machine has one.
fn show_hosting(game: &mut GameState, port: u16) {
    game.set_net_status(String::new(), false);
    game.set_host_address(port, net::lan_address());
}

/// The settings saved from the last session, or the defaults.
pub fn saved_settings() -> Settings {
    persist::read(SETTINGS_FILE).map_or_else(Settings::default, |text| Settings::from_text(&text))
}

impl App {
    pub fn new(options: Options, network: Option<(Session, GameState)>) -> Self {
        Self::new_with_load(options, network, persist::read)
    }

    fn new_with_load(
        options: Options,
        network: Option<(Session, GameState)>,
        read: impl Fn(&str) -> Option<String>,
    ) -> Self {
        let screenshot = options.screenshot.map(Screenshot::new);
        let remember = screenshot.is_none();
        let load = |name| if remember { read(name) } else { None };
        let settings =
            load(SETTINGS_FILE).map_or_else(Settings::default, |text| Settings::from_text(&text));
        let layout = load(LAYOUT_FILE).unwrap_or_default();
        let saved = SavedWindow::from_text(&layout);
        let explicit_size = options.size.is_some();
        let requested_size = options
            .size
            .or(screenshot.as_ref().map(|_| screenshot::DEFAULT_SIZE))
            .map(|(width, height)| PhysicalSize::new(width, height));
        let (network, game) = match network {
            Some((session, game)) => (Some(session), Some(game)),
            None => (None, None),
        };
        let mut game = game.unwrap_or_else(|| match options.seed {
            Some(seed) => GameState::world_scenario_with(seed, &settings),
            None => options.scenario.new_game(&settings),
        });
        game.set_settings(settings);
        if let Some(text) = load(NETWORK_FILE) {
            game.set_net_menu(NetMenu::from_text(&text));
        }
        if let Some(port) = network.as_ref().and_then(Session::port) {
            show_hosting(&mut game, port);
        }
        Self {
            renderer: None,
            window: None,
            saved_settings: game.settings_text(),
            game,
            network,
            joining: None,
            imgui: None,
            imgui_platform: None,
            imgui_fonts: None,
            imgui_layout: ImGuiLayoutState::from_text(&layout),
            world_vertices: Vec::new(),
            ui_vertices: Vec::new(),
            use_imgui: saved.imgui,

            last_frame: None,
            frame_duration: FRAME_DURATION,
            minimized: false,
            icon_refresh_at: None,
            cursor_pos: None,
            panning: false,
            left_press: None,
            left_dragging: false,
            painting_jobs: false,
            queue_scroll_dragging: false,
            building_scroll_dragging: false,
            queue_item_dragging: false,
            modifiers: Modifiers::default(),
            box_start: None,
            requested_size,
            screenshot,
            failure: None,
            remember,
            imgui_ini: load(IMGUI_FILE),
            start_maximized: remember && !explicit_size && saved.maximized,
            normal_size: saved.size,
            explicit_size,
            saved_maximized: saved.maximized,
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
        self.save_net_menu();
        let saved = SavedWindow {
            imgui: self.use_imgui,
            size: self.normal_size,
            maximized: if self.explicit_size {
                self.saved_maximized
            } else {
                self.window.as_ref().is_some_and(Window::is_maximized)
            },
        };
        let mut layout = saved.to_text();
        layout += &self.imgui_layout.to_text();
        persist::write(LAYOUT_FILE, &layout);
        if let Some(imgui) = &mut self.imgui {
            let mut ini = String::new();
            imgui.save_ini_settings(&mut ini);
            persist::write(IMGUI_FILE, &ini);
        }
    }

    /// Keeps what the Multiplayer section has typed for next time.
    fn save_net_menu(&self) {
        if self.remember {
            persist::write(NETWORK_FILE, &self.game.net_menu().to_text());
        }
    }

    /// Carries out what the settings menu's Multiplayer section asked for:
    /// hosting a new world, starting a join, or leaving a network game.
    fn handle_net_request(&mut self) {
        let Some(request) = self.game.take_net_request() else {
            return;
        };
        match request {
            NetRequest::Host { port, players } => {
                match Session::host(port, players, self.game.settings()) {
                    Ok((session, game)) => {
                        self.start_network_game(session, game);
                        show_hosting(&mut self.game, port);
                    }
                    Err(err) => {
                        let status = format!("CAN'T HOST: {err:#}").to_uppercase();
                        self.game.set_net_status(status, false);
                    }
                }
            }
            NetRequest::Join { address, code } => {
                let (send, receive) = mpsc::channel();
                self.game
                    .set_net_status(format!("JOINING {address}..."), true);
                thread::spawn(move || {
                    let _ = send.send(Session::join(&address, &code));
                });
                self.joining = Some(receive);
            }
            NetRequest::Leave => {
                self.network = None;
                let mut game = Scenario::World.new_game(self.game.settings());
                game.keep_menus_of(&self.game);
                self.game = game;
                self.game.set_net_status(String::new(), false);
                self.game
                    .set_ui_notice("LEFT THE NETWORK GAME - A NEW WORLD");
            }
            NetRequest::Copy(text) => self.copy(&text),
        }
    }

    /// Puts `text` on the clipboard, with a notice saying so.
    fn copy(&mut self, text: &str) {
        let notice = if text.is_empty() {
            "NOTHING TO COPY".to_string()
        } else if clipboard::set_text(text) {
            format!("COPIED {text}")
        } else {
            "CAN'T COPY: NO CLIPBOARD".to_string()
        };
        self.game.set_ui_notice(&notice);
    }

    /// Takes the game a join brought back, once it has.
    fn poll_join(&mut self) {
        let Some(joining) = &self.joining else {
            return;
        };
        let result = match joining.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(anyhow::anyhow!("the join stopped")),
        };
        self.joining = None;
        match result {
            Ok((session, game)) => {
                self.start_network_game(session, game);
                self.game.set_net_status(String::new(), false);
            }
            Err(err) => {
                let status = format!("CAN'T JOIN: {err:#}").to_uppercase();
                self.game.set_net_status(status, false);
            }
        }
    }

    /// Plays `game` over `session` from now on, with the menus as they were.
    fn start_network_game(&mut self, session: Session, mut game: GameState) {
        game.keep_menus_of(&self.game);
        self.game = game;
        self.network = Some(session);
        self.save_net_menu();
    }

    /// Whether keys go into a text field rather than to the game: ImGui's
    /// text box has them, or classic is typing into a Multiplayer field.
    fn typing(&self) -> bool {
        if self.use_imgui {
            self.imgui
                .as_ref()
                .is_some_and(|ctx| ctx.io().want_text_input)
        } else {
            self.game.net_field_editing().is_some()
        }
    }

    /// A key pressed while typing: ImGui has it already; classic types it
    /// into the field, pastes into it (Ctrl+V) or copies it (Ctrl+C, and
    /// Ctrl+X, which empties it), and Enter, Tab or Escape ends typing.
    fn type_key(&mut self, event: &KeyEvent) {
        if self.use_imgui {
            return;
        }
        let ctrl = self.modifiers.state().control_key();
        match event.physical_key {
            PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                if let Some(text) = clipboard::get_text() {
                    self.game.paste_net_text(&text);
                }
            }
            PhysicalKey::Code(key @ (KeyCode::KeyC | KeyCode::KeyX)) if ctrl => {
                if let Some(text) = self.game.copy_net_field(key == KeyCode::KeyX) {
                    self.copy(&text);
                }
            }
            PhysicalKey::Code(KeyCode::Backspace) => self.game.net_field_backspace(),
            PhysicalKey::Code(
                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Tab | KeyCode::Escape,
            ) => self.game.stop_typing(),
            _ => {
                if let Some(text) = &event.text {
                    self.game.type_net_text(text);
                }
            }
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
        log::error!("{err:#}");
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
        // Failed and on the way out: winit may ask for one more frame.
        if self.minimized || self.failure.is_some() {
            return;
        }

        let now = Instant::now();
        let dt = now - self.last_frame.unwrap_or(now);
        self.last_frame = Some(now);
        self.handle_net_request();
        self.poll_join();
        if let Some(network) = self.network.as_mut() {
            network.pump(&mut self.game);
        }
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
        // The vertex buffers are kept from frame to frame: a scene is a few
        // megabytes, and filling fresh memory each frame costs as much as
        // building it.
        let mut world = std::mem::take(&mut self.world_vertices);
        self.game.build_vertices_into(&mut world);
        let mut ui = std::mem::take(&mut self.ui_vertices);
        if self.use_imgui {
            ui.clear();
        } else {
            self.game.build_ui_into(size, self.cursor_pos, &mut ui);
        }
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
        (self.world_vertices, self.ui_vertices) = (world, ui);
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

    fn cancel_drags(&mut self) {
        self.painting_jobs = false;
        self.left_press = None;
        self.left_dragging = false;
        self.queue_scroll_dragging = false;
        self.building_scroll_dragging = false;
        self.queue_item_dragging = false;
        self.game.cancel_queue_drag();
        self.panning = false;
        self.box_start = None;
    }

    fn switch_presentation(&mut self) {
        self.use_imgui = !self.use_imgui;
        self.cancel_drags();
        self.game.set_ui_notice(if self.use_imgui {
            "IMGUI UI (F11 TO COMPARE)"
        } else {
            "CLASSIC UI (F11 TO COMPARE)"
        });
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
            .with_inner_size(
                self.requested_size
                    .or(self.normal_size)
                    .unwrap_or(DEFAULT_WINDOW_SIZE),
            )
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
            let (size, position) =
                initial_window_rect(screen, origin, self.requested_size, self.normal_size);
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
        // Ctrl+C, X and V in its text boxes (the Multiplayer fields).
        imgui.set_clipboard_backend(clipboard::ImGuiClipboard);
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
        let [small_font, body_font, title_font] = style_imgui(&mut imgui);
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
        self.frame_duration = frame_duration(&window);
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
            WindowEvent::Moved(_) => {
                if let Some(window) = &self.window {
                    self.frame_duration = frame_duration(window);
                }
            }
            WindowEvent::Resized(size) => {
                self.minimized = size.width == 0 || size.height == 0;
                let normal = self
                    .window
                    .as_ref()
                    .is_some_and(|w| !w.is_maximized() && w.fullscreen().is_none());
                if normal && !self.minimized && !self.explicit_size {
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
                if !self.use_imgui && self.building_scroll_dragging {
                    if let Some(size) = self.screen_size() {
                        self.game.drag_building_scrollbar_at(pos, size, true);
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
                if self.painting_jobs {
                    if let Some(size) = self.screen_size() {
                        self.game.paint_job_at(pos, size, false);
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
            WindowEvent::Focused(false) => self.cancel_drags(),
            WindowEvent::CursorLeft { .. } => {
                self.cancel_drags();
                self.cursor_pos = None;
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
                        if !self.use_imgui
                            && let Some(size) = self.screen_size()
                            && self.game.drag_building_scrollbar_at(cursor, size, false)
                        {
                            self.building_scroll_dragging = true;
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
                            && self.game.paint_job_at(cursor, size, !self.use_imgui)
                        {
                            self.painting_jobs = true;
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
                        let phase = (!self.game.is_playing_out()).then(|| self.game.is_resolving());
                        self.left_press = Some((cursor, mode, phase));
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
                    if self.painting_jobs {
                        self.painting_jobs = false;
                        return;
                    }
                    if self.queue_scroll_dragging || self.building_scroll_dragging {
                        self.queue_scroll_dragging = false;
                        self.building_scroll_dragging = false;
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
                    if let Some((origin, mode, phase)) = self.left_press.take()
                        && !self.left_dragging
                        && phase.is_some_and(|frozen| frozen == self.game.is_resolving())
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
                if !(matches!((self.cursor_pos, self.screen_size()), (Some(cursor), Some(size)) if self.game.scroll_queue_at(cursor, size, steps) || self.game.scroll_buildings_at(cursor, size, steps)))
                {
                    self.game.camera.zoom(steps);
                }
            }
            // While typing into a text field, keys are the field's.
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && self.typing() =>
            {
                self.type_key(&event)
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
                    KeyCode::KeyR => self.game.arm_worker_job(crate::game::JobKind::Road),
                    KeyCode::KeyI => self.game.arm_worker_job(crate::game::JobKind::Improve),
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
                    KeyCode::Digit4 => self.game.queue_selected_city_scout(),
                    KeyCode::KeyS => self.game.queue_selected_city_settler(),
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
                    KeyCode::Digit9 => self.game.queue_selected_city_growth(),
                    KeyCode::Digit0 => self.game.queue_selected_city_gather(),
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
                    KeyCode::KeyE => self.game.toggle_alert(),
                    KeyCode::F5 => self.toggle_fullscreen(),
                    KeyCode::F6 => self.game.save_state(),
                    KeyCode::F7 => self.game.load_state(),
                    KeyCode::F8 => self.game.toggle_instant_playback(),
                    KeyCode::F9 => self.game.debug_complete_current_production(),
                    KeyCode::F10 => self.game.toggle_fog(),
                    KeyCode::F11 => self.switch_presentation(),
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
            .map_or_else(Instant::now, |t| t + self.frame_duration);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_loss_keeps_the_cursor_and_switching_ui_cancels_every_drag() {
        fn arm_drags(app: &mut App, cursor: Vec2) {
            app.left_press = Some((cursor, ClickMode::Normal, Some(false)));
            app.left_dragging = true;
            app.painting_jobs = true;
            app.queue_scroll_dragging = true;
            app.building_scroll_dragging = true;
            app.queue_item_dragging = true;
            app.panning = true;
            app.box_start = Some(cursor);
        }

        fn assert_no_drags(app: &App) {
            assert!(app.left_press.is_none());
            assert!(!app.left_dragging);
            assert!(!app.painting_jobs);
            assert!(!app.queue_scroll_dragging);
            assert!(!app.building_scroll_dragging);
            assert!(!app.queue_item_dragging);
            assert!(!app.panning);
            assert!(app.box_start.is_none());
        }

        let mut app = App::new_with_load(Options::default(), None, |_| None);
        let cursor = Vec2::new(120.0, 240.0);
        app.cursor_pos = Some(cursor);
        arm_drags(&mut app, cursor);
        app.cancel_drags(); // Focused(false), with no later CursorMoved.
        assert_eq!(app.cursor_pos, Some(cursor));
        assert_no_drags(&app);

        arm_drags(&mut app, cursor);
        let old_presentation = app.use_imgui;
        app.switch_presentation(); // F11.
        assert_ne!(app.use_imgui, old_presentation);
        assert_eq!(app.cursor_pos, Some(cursor));
        assert_no_drags(&app);
    }

    #[test]
    fn saved_window_round_trips() {
        let saved = SavedWindow {
            imgui: false,
            size: Some(PhysicalSize::new(1280, 720)),
            maximized: true,
        };
        assert_eq!(SavedWindow::from_text(&saved.to_text()), saved);
    }

    #[test]
    fn oversized_saved_window_stays_inside_monitor() {
        let screen = PhysicalSize::new(800, 600);
        let origin = PhysicalPosition::new(100, 50);
        let (size, position) =
            initial_window_rect(screen, origin, None, Some(PhysicalSize::new(2000, 1200)));
        assert_eq!(size, screen);
        assert!(position.x >= origin.x && position.y >= origin.y);
        assert!(position.x + size.width as i32 <= origin.x + screen.width as i32);
        assert!(position.y + size.height as i32 <= origin.y + screen.height as i32);
    }

    #[test]
    fn explicit_size_ignores_saved_maximized_window_without_replacing_preferences() {
        let options = Options {
            size: Some((1280, 720)),
            ..Options::default()
        };
        let app = App::new_with_load(options, None, |name| {
            (name == LAYOUT_FILE).then(|| "window_size 1024 768\nmaximized 1\n".into())
        });
        assert_eq!(app.requested_size, Some(PhysicalSize::new(1280, 720)));
        assert_eq!(app.normal_size, Some(PhysicalSize::new(1024, 768)));
        assert!(app.explicit_size);
        assert!(app.saved_maximized);
        assert!(!app.start_maximized);
    }

    #[test]
    fn screenshot_defaults_to_1600_by_900_without_loading_persisted_state() {
        let options = Options {
            scenario: Scenario::World,
            seed: Some(42),
            screenshot: Some("test.png".into()),
            ..Options::default()
        };
        let app = App::new_with_load(options, None, |_| panic!("screenshot must not load state"));
        assert_eq!(app.requested_size, Some(PhysicalSize::new(1600, 900)));
        assert!(!app.remember);
        assert_eq!(app.game.map_seed(), Some(42));
    }
}
