//! Tests only: the ImGui presentation without a window (`ImGuiScreen`),
//! which draws the real panels with `draw_imgui` and clicks, drags and
//! hovers where ImGui drew them. `ui/tests.rs` and the frame tests
//! (`tests/frames.rs`) use it.

use super::*;

/// The ImGui presentation without a window: a context with a font, and the
/// layout it keeps from frame to frame, as `App` has them.
pub(in crate::game::ui) struct ImGuiScreen {
    pub(in crate::game::ui) context: ::imgui::Context,
    pub(in crate::game::ui) fonts: [::imgui::FontId; 3],
    pub(in crate::game::ui) layout: ImGuiLayoutState,
    /// The window's size.
    pub(in crate::game::ui) size: Vec2,
    /// Last, to drop after the context.
    _one: std::sync::MutexGuard<'static, ()>,
}

impl ImGuiScreen {
    /// The window's size `new` gives it, as `ui/tests.rs`'s `SCREEN`.
    pub(in crate::game::ui) const SIZE: Vec2 = Vec2::new(1600.0, 900.0);

    pub(in crate::game::ui) fn new() -> Self {
        Self::with_size(Self::SIZE)
    }

    /// The same, in a window of `size`.
    pub(in crate::game::ui) fn with_size(size: Vec2) -> Self {
        Self::build(size, false)
    }

    /// The same with the game's fonts and style (`style_imgui`), so text
    /// measures as it does in play: for tests of where text goes.
    pub(in crate::game::ui) fn styled(size: Vec2) -> Self {
        Self::build(size, true)
    }

    fn build(size: Vec2, styled: bool) -> Self {
        let one = one_context_at_a_time();
        let mut context = ::imgui::Context::create();
        context.set_ini_filename(None);
        context.io_mut().display_size = size.to_array();
        context
            .io_mut()
            .config_flags
            .insert(::imgui::ConfigFlags::DOCKING_ENABLE);
        let fonts = if styled {
            context.io_mut().config_windows_resize_from_edges = false;
            style_imgui(&mut context)
        } else {
            let font = context
                .fonts()
                .add_font(&[::imgui::FontSource::DefaultFontData { config: None }]);
            [font; 3]
        };
        context.fonts().build_rgba32_texture();
        Self {
            context,
            fonts,
            layout: ImGuiLayoutState::default(),
            size,
            _one: one,
        }
    }

    /// Draws a frame with the mouse at `mouse` (window pixels; `None`: off
    /// the window) and its left button `down`, carrying out what the panels
    /// were asked to do, as `App` does.
    pub(in crate::game::ui) fn frame(
        &mut self,
        game: &mut GameState,
        mouse: Option<Vec2>,
        down: bool,
    ) {
        let io = self.context.io_mut();
        io.delta_time = 1.0 / 60.0;
        io.add_mouse_pos_event(mouse.map_or([f32::MIN; 2], |m| m.to_array()));
        io.add_mouse_button_event(::imgui::MouseButton::Left, down);
        DRAWN_BUTTONS.with_borrow_mut(Vec::clear);
        DRAWN_MARKS.with_borrow_mut(Vec::clear);
        SHORT_BUTTONS.with_borrow_mut(Vec::clear);
        let ui = self.context.frame();
        game.draw_imgui(ui, self.size, mouse, &self.fonts, &mut self.layout);
        self.context.render();
    }

    /// A few frames with the mouse away, for the panels to settle where
    /// the dock puts them.
    pub(in crate::game::ui) fn settle(&mut self, game: &mut GameState) {
        for _ in 0..4 {
            self.frame(game, None, false);
        }
    }

    /// The middle of the button ImGui drew for `target` last frame.
    pub(in crate::game::ui) fn button(&self, target: Target) -> Option<Vec2> {
        DRAWN_BUTTONS.with_borrow(|drawn| {
            drawn
                .iter()
                .find(|(drawn, ..)| *drawn == target)
                .map(|&(_, min, max)| (Vec2::from(min) + Vec2::from(max)) / 2.0)
        })
    }

    /// Moves the mouse onto `target`'s button and clicks it. Panics if
    /// ImGui doesn't show that button.
    pub(in crate::game::ui) fn click(&mut self, game: &mut GameState, target: Target) {
        self.settle(game);
        let at = self
            .button(target)
            .unwrap_or_else(|| panic!("ImGui shows no {target:?} button"));
        self.frame(game, Some(at), false);
        self.frame(game, Some(at), true);
        self.frame(game, Some(at), false);
    }

    /// Presses Ctrl, or lets it go, with the mouse away: held, the panels
    /// show their title bars and grips to be arranged.
    pub(in crate::game::ui) fn hold_ctrl(&mut self, game: &mut GameState, down: bool) {
        let io = self.context.io_mut();
        io.add_key_event(::imgui::Key::ModCtrl, down);
        io.add_key_event(::imgui::Key::LeftCtrl, down);
        self.frame(game, None, false);
    }

    /// Drags with the left button from `from` to `to`, a few frames on the
    /// way, and lets go.
    pub(in crate::game::ui) fn drag(&mut self, game: &mut GameState, from: Vec2, to: Vec2) {
        self.frame(game, Some(from), false);
        self.frame(game, Some(from), true);
        for step in 1..=4 {
            self.frame(game, Some(from.lerp(to, step as f32 / 4.0)), true);
        }
        self.frame(game, Some(to), false);
    }

    /// Double-clicks at `at`, then moves the mouse away and lets the
    /// panels settle.
    pub(in crate::game::ui) fn double_click(&mut self, game: &mut GameState, at: Vec2) {
        self.frame(game, Some(at), false);
        for down in [true, false, true, false] {
            self.frame(game, Some(at), down);
        }
        self.settle(game);
    }

    /// Presses and lets go of Ctrl+`key`, a frame each, with the mouse
    /// away: a shortcut for the text box that has the keys.
    pub(in crate::game::ui) fn ctrl_key(&mut self, game: &mut GameState, key: ::imgui::Key) {
        for down in [true, false] {
            let io = self.context.io_mut();
            io.add_key_event(::imgui::Key::ModCtrl, down);
            io.add_key_event(::imgui::Key::LeftCtrl, down);
            io.add_key_event(key, down);
            self.frame(game, None, false);
        }
    }

    /// Captures `game`'s selection panel in a new outer box, as dragging it
    /// there with Ctrl held does, and lets it dock.
    pub(in crate::game::ui) fn capture_selection(&mut self, game: &mut GameState) {
        self.layout.add_outer_box(self.size);
        self.settle(game);
        self.layout.capture_selection(game);
        self.settle(game);
        assert_eq!(self.layout.captured(), 1, "the panel stays in its box");
    }

    /// The tooltip of each button ImGui draws for `target`, as a line of
    /// text, with the mouse on it.
    pub(in crate::game::ui) fn tooltips(
        &mut self,
        game: &mut GameState,
        target: Target,
    ) -> Vec<String> {
        self.settle(game);
        let drawn: Vec<Vec2> = DRAWN_BUTTONS.with_borrow(|drawn| {
            drawn
                .iter()
                .filter(|(drawn, ..)| *drawn == target)
                .map(|&(_, min, max)| (Vec2::from(min) + Vec2::from(max)) / 2.0)
                .collect()
        });
        drawn
            .into_iter()
            .map(|at| {
                self.frame(game, Some(at), false);
                SHOWN_TOOLTIPS.with_borrow_mut(Vec::clear);
                self.frame(game, Some(at), false);
                SHOWN_TOOLTIPS
                    .with_borrow_mut(std::mem::take)
                    .concat()
                    .join(" ")
            })
            .collect()
    }

    /// Whether ImGui keeps a mouse press at `at` from the map (`App` then
    /// doesn't pass it on).
    pub(in crate::game::ui) fn captures_mouse_at(
        &mut self,
        game: &mut GameState,
        at: Vec2,
    ) -> bool {
        self.frame(game, Some(at), false);
        self.frame(game, Some(at), false);
        self.context.io().want_capture_mouse
    }
}
