//! What the drawing code notes for the tests (`ui/tests.rs`): where it drew
//! each button, text and icon. Outside tests the notes do nothing.

use super::*;

/// Tests only: `imgui` allows one context at a time in the whole process,
/// so a test holds this while it has one.
#[cfg(test)]
pub(in crate::game::ui) fn one_context_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    // A test that failed holding it leaves nothing behind to protect.
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Tests only: a button ImGui drew, and its screen rectangle's corners.
#[cfg(test)]
pub(in crate::game::ui) type DrawnButton = (Target, [f32; 2], [f32; 2]);

#[cfg(test)]
thread_local! {
    /// Tests only: each panel button ImGui drew this thread, so a test can
    /// move the mouse onto one and click.
    pub(in crate::game::ui) static DRAWN_BUTTONS: std::cell::RefCell<Vec<DrawnButton>> =
        const { std::cell::RefCell::new(Vec::new()) };
}
#[cfg(test)]
thread_local! {
    /// Tests only: the text of each panel button tooltip ImGui showed this
    /// thread, a line per string.
    pub(in crate::game::ui) static SHOWN_TOOLTIPS: std::cell::RefCell<Vec<Vec<String>>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// Tests only: the status bar's notice as ImGui last drew it, its
    /// rectangle, and how far right it may reach (short of End Turn).
    pub(in crate::game::ui) static SHOWN_NOTICE: std::cell::RefCell<ShownNotice> =
        const { std::cell::RefCell::new((String::new(), [0.0; 2], [0.0; 2], 0.0)) };
    /// Tests only: the status bar's second line as ImGui last drew it:
    /// each control's text and rectangle, from Menu on.
    pub(in crate::game::ui) static STATUS_CONTROLS: std::cell::RefCell<Vec<DrawnControl>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Tests only: a notice drawn, its rectangle and its limit.
#[cfg(test)]
pub(in crate::game::ui) type ShownNotice = (String, [f32; 2], [f32; 2], f32);
/// Tests only: a status bar control's text and rectangle.
#[cfg(test)]
pub(in crate::game::ui) type DrawnControl = (String, [f32; 2], [f32; 2]);
/// Tests only: notes the tooltip about to show.
pub(super) fn note_shown_tooltip(_lines: &[(u32, Line)]) {
    #[cfg(test)]
    SHOWN_TOOLTIPS.with_borrow_mut(|shown| {
        shown.push(
            _lines
                .iter()
                .map(|(_, line)| line.iter().map(|(text, _)| text.as_str()).collect())
                .collect(),
        )
    });
}

/// Tests only: notes the notice just drawn.
pub(super) fn note_shown_notice(_notice: &str, _min: [f32; 2], _max: [f32; 2], _limit: f32) {
    #[cfg(test)]
    SHOWN_NOTICE.set((_notice.to_string(), _min, _max, _limit));
}

/// Tests only: the status bar's controls are drawn again from the first
/// (Menu).
pub(super) fn clear_status_controls() {
    #[cfg(test)]
    STATUS_CONTROLS.with_borrow_mut(Vec::clear);
}

/// Tests only: notes the status bar control just drawn.
pub(super) fn note_status_control(_ui: &Ui, _text: &str) {
    #[cfg(test)]
    STATUS_CONTROLS.with_borrow_mut(|drawn| {
        drawn.push((_text.to_string(), _ui.item_rect_min(), _ui.item_rect_max()));
    });
}

#[cfg(test)]
thread_local! {
    /// Tests only: each button ImGui drew short (`short_style`) this
    /// thread.
    pub(in crate::game::ui) static SHORT_BUTTONS: std::cell::RefCell<Vec<Target>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Tests only: notes that the button for `target` is drawn short.
pub(super) fn note_short_button(_target: Target) {
    #[cfg(test)]
    SHORT_BUTTONS.with_borrow_mut(|drawn| drawn.push(_target));
}

/// Tests only: notes where the button for `target` just went.
pub(super) fn note_drawn_button(_ui: &Ui, _target: Target) {
    #[cfg(test)]
    DRAWN_BUTTONS
        .with_borrow_mut(|drawn| drawn.push((_target, _ui.item_rect_min(), _ui.item_rect_max())));
}

/// Tests only: a piece of text or an icon ImGui drew.
#[cfg(test)]
#[derive(Clone, Debug)]
pub(in crate::game::ui) struct DrawnMark {
    /// The panel (root window) it is in.
    pub window: String,
    /// Its text, or "icon".
    pub what: String,
    /// Its rectangle's corners.
    pub min: [f32; 2],
    pub max: [f32; 2],
    /// The corners of what it must stay inside: its window's clip
    /// rectangle, cut down to its button's for a label.
    pub clip_min: [f32; 2],
    pub clip_max: [f32; 2],
}

#[cfg(test)]
thread_local! {
    /// Tests only: each piece of text or icon ImGui drew this thread.
    pub(in crate::game::ui) static DRAWN_MARKS: std::cell::RefCell<Vec<DrawnMark>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Tests only: notes `_what` (text, or an icon), drawn from `_min` to `_max`
/// in the current window, clipped as the window's draw list is now, and
/// within `_inside` too if given (a button's label, its button).
pub(super) fn note_mark(
    _what: &str,
    _min: [f32; 2],
    _max: [f32; 2],
    _inside: Option<([f32; 2], [f32; 2])>,
) {
    #[cfg(test)]
    {
        let (window, mut clip_min, mut clip_max) = current_clip();
        if let Some((min, max)) = _inside {
            clip_min = [clip_min[0].max(min[0]), clip_min[1].max(min[1])];
            clip_max = [clip_max[0].min(max[0]), clip_max[1].min(max[1])];
        }
        DRAWN_MARKS.with_borrow_mut(|drawn| {
            drawn.push(DrawnMark {
                window,
                what: _what.to_string(),
                min: _min,
                max: _max,
                clip_min,
                clip_max,
            })
        });
    }
}

/// Tests only: notes the text item just drawn (`ui.text` and the like),
/// `_what`.
pub(super) fn note_text_item(_ui: &Ui, _what: &str) {
    #[cfg(test)]
    note_mark(_what, _ui.item_rect_min(), _ui.item_rect_max(), None);
}

/// Tests only: notes the label ImGui drew on the button just drawn: `text`,
/// placed as `RenderTextClipped` places it (by the style's alignment, from
/// the left when it doesn't fit), inside the button.
pub(super) fn note_button_label(_ui: &Ui, _text: &str) {
    #[cfg(test)]
    {
        let ui = _ui;
        let text = _text.split("##").next().unwrap_or_default();
        if text.is_empty() {
            return;
        }
        let (min, max) = (ui.item_rect_min(), ui.item_rect_max());
        let style = ui.clone_style();
        let pad = style.frame_padding;
        let align = style.button_text_align;
        let size = ui.calc_text_size(text);
        let x = min[0] + pad[0] + (max[0] - min[0] - 2.0 * pad[0] - size[0]).max(0.0) * align[0];
        let y = min[1] + pad[1] + (max[1] - min[1] - 2.0 * pad[1] - size[1]).max(0.0) * align[1];
        note_mark(text, [x, y], [x + size[0], y + size[1]], Some((min, max)));
    }
}

#[cfg(test)]
impl ImGuiLayoutState {
    /// Tests only: a new outer box, drawn once (`ImGuiScreen::frame`) before
    /// anything is put in it.
    pub(in crate::game::ui) fn add_outer_box(&mut self, viewport: Vec2) {
        let editing = std::mem::replace(&mut self.editing_outer, true);
        self.create_box(viewport);
        self.editing_outer = editing;
    }

    /// Tests only: captures `game`'s selection panel in the newest outer
    /// box, as dragging it there with Ctrl held does.
    pub(in crate::game::ui) fn capture_selection(&mut self, game: &GameState) {
        let box_id = self.outer_boxes.last().expect("an outer box").id;
        let pin = game.selected_pin().expect("something selected");
        if pin.kind == PinnedKind::Group {
            self.pinned_groups.insert(
                pin.id,
                game.group.iter().map(|&i| game.units[i].id).collect(),
            );
        }
        self.capture_panel(pin, box_id, slot_titles()[SELECTION]);
    }

    /// Tests only: how many panels are captured.
    pub(in crate::game::ui) fn captured(&self) -> usize {
        self.pinned.len()
    }

    /// Tests only: the player resizing the panel titled `title` to `size`,
    /// as dragging its grip does. The panel must have been drawn.
    pub(in crate::game::ui) fn resize_panel(&mut self, title: &str, size: Vec2) {
        let slot = slot_titles()
            .iter()
            .position(|slot| *slot == title)
            .expect("a panel's title");
        set_full_size(title, size);
        let window = &mut self.windows[slot];
        window.place_by_player();
        window.size = size;
        window.floating_size = size;
    }

    /// Tests only: whether the panel titled `title` is placed by the player,
    /// and whether it's sized by them.
    pub(in crate::game::ui) fn chosen_by_player(&self, title: &str) -> (bool, bool) {
        let slot = slot_titles()
            .iter()
            .position(|slot| *slot == title)
            .expect("a panel's title");
        (self.windows[slot].manual, self.windows[slot].sized)
    }

    /// Tests only: the side of the map ("left", "right", "up", "down") the
    /// panel titled `title` is docked on in the game window, and how wide
    /// or tall its part of it is, if it's docked there.
    pub(in crate::game::ui) fn window_edge(&self, title: &str) -> Option<(&'static str, f32)> {
        edge_dock(title, self.game_dock_id).map(|edge| (edge.side_text(), edge.extent))
    }

    /// Tests only: the side the panel titled `title` docks on in the game
    /// window in the layout's `view` ("default", "city", "troop"), as saved.
    pub(in crate::game::ui) fn edge_saved(&self, view: &str, title: &str) -> Option<&'static str> {
        let view = view_from_text(view).expect("a view");
        let slot = slot_titles()
            .iter()
            .position(|slot| *slot == title)
            .expect("a panel's title");
        self.docking_for_view(view)
            .edge_of(slot)
            .map(EdgeDock::side_text)
    }

    /// Tests only: whether the layout's `view` has its own docking in the
    /// game window, rather than following Default's.
    pub(in crate::game::ui) fn has_own_docking(&self, view: &str) -> bool {
        self.view_docks
            .contains_key(&view_from_text(view).expect("a view"))
    }
}

/// Tests only: docks the window titled `other` beside the one titled
/// `anchor` ("down": below it), with `share` of the pair's room, as a drop
/// there with the split then dragged would; ImGui does it as the next frame
/// begins. Both must have been drawn, `anchor` floating.
#[cfg(test)]
pub(in crate::game::ui) fn dock_beside(anchor: &str, other: &str, side: &str, share: f32) {
    let direction = match side {
        "left" => ::imgui::sys::ImGuiDir_Left,
        "right" => ::imgui::sys::ImGuiDir_Right,
        "up" => ::imgui::sys::ImGuiDir_Up,
        _ => ::imgui::sys::ImGuiDir_Down,
    };
    let ratio = split_ratio(direction, share);
    assert!(
        queue_dock(anchor, other, None, direction, ratio),
        "both drawn"
    );
}

/// Tests only: where the window titled `title` is, how big, and whether
/// it's collapsed, as ImGui has it.
#[cfg(test)]
pub(in crate::game::ui) fn native_geometry(title: &str) -> Option<(Vec2, Vec2, bool)> {
    native_state(title).map(|window| (window.pos, window.size, window.collapsed))
}
