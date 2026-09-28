//! The settings menu Escape opens: the settings under a heading per group
//! (`Setting::group`), each a `Row::Setting` showing its name and the control
//! its `Setting::control` names, then Close and Quit. It opens in the middle
//! of the screen, over the map and the other panels. The settings themselves,
//! and how to add one, are in `game/settings.rs`; this file needs no change
//! for a new setting.
//!
//! ImGui draws each control as a widget (`imgui.rs`): a checkbox, a slider,
//! or a button per choice (a drop-down list past `Control::MAX_BUTTONS`).
//! Classic has no such widgets, so `classic_setting_rows` lays a setting out
//! as buttons: OFF / ON for a switch, one per choice (the current one gold),
//! and < > for a slider or a longer list of choices.

use super::builder::{ButtonSpec, PanelBuilder, Row};
use glam::Vec2;

use super::{BODY, ButtonState, GAP, GOLD_TEXT, LABEL_TEXT, Layout, TEXT, TITLE, Target};
use crate::game::GameState;
use crate::game::settings::{Control, Setting};

impl GameState {
    /// The classic presentation's settings menu, while it's open: centered
    /// on the screen and placed last, so it draws over the other panels and
    /// its buttons take clicks before theirs (`Layout::button_at`).
    pub(super) fn place_settings(&self, screen_size: Vec2, layout: &mut Layout) {
        if self.settings_open {
            let panel = self.settings_panel_content();
            let size = panel.size();
            let top_left = Vec2::new(
                (screen_size.x - size.x) / 2.0,
                (screen_size.y + size.y) / 2.0,
            );
            layout.overlay = Some((layout.shapes.len(), layout.buttons.len()));
            panel.place_top_left(top_left, layout);
        }
    }

    /// The settings menu's content, shared by both presentations.
    pub(super) fn settings_panel_content(&self) -> PanelBuilder {
        let mut panel = PanelBuilder::default();
        panel.text(TITLE, vec![("SETTINGS".into(), TEXT)]);
        let mut group = None;
        for setting in Setting::ALL {
            if group != Some(setting.group()) {
                group = Some(setting.group());
                panel.gap(GAP);
                panel.heading(setting.group());
            }
            panel.setting(setting, self.settings.get(setting));
        }
        panel.gap(GAP);
        panel.text(BODY, vec![("CITY OVERLAYS".into(), LABEL_TEXT)]);
        panel.compact_buttons(vec![ButtonSpec {
            target: Target::ToggleYields,
            label: if self.show_yields {
                "CITY YIELDS: ON"
            } else {
                "CITY YIELDS: OFF"
            }
            .into(),
            hint: "Y".into(),
            state: ButtonState::new(self.show_yields, false),
            armed: false,
        }]);
        panel.gap(GAP);
        panel.compact_buttons(vec![
            ButtonSpec {
                target: Target::CloseSettings,
                label: "CLOSE".into(),
                hint: "ESC".into(),
                state: ButtonState::Ready,
                armed: false,
            },
            ButtonSpec {
                target: Target::Quit,
                label: "QUIT".into(),
                hint: String::new(),
                state: ButtonState::Ready,
                armed: false,
            },
        ]);
        panel
    }
}

/// Whether the classic menu steps `setting` with < and > rather than
/// showing a button for each value: a slider, or more choices than fit.
pub(super) fn steps_in_classic(setting: Setting) -> bool {
    match setting.control() {
        Control::Toggle => false,
        Control::Slider => true,
        Control::Choice => setting.range().count() > Control::MAX_BUTTONS,
    }
}

/// How the classic presentation shows `setting` at `value`: a line with its
/// name (and its value, if stepped), then a row of buttons that set it.
pub(super) fn classic_setting_rows(setting: Setting, value: i32) -> Vec<Row> {
    let range = setting.range();
    let button = |to: i32, label: String, state| ButtonSpec {
        target: Target::SetSetting(setting, to),
        label,
        hint: String::new(),
        state,
        armed: false,
    };
    let mut label = vec![(format!("{}  ", setting.name()), LABEL_TEXT)];
    let buttons = if steps_in_classic(setting) {
        label.push((setting.value_text(value), GOLD_TEXT));
        vec![
            button(
                value - 1,
                "<".into(),
                ButtonState::new(false, value <= *range.start()),
            ),
            button(
                value + 1,
                ">".into(),
                ButtonState::new(false, value >= *range.end()),
            ),
        ]
    } else {
        range
            .map(|to| {
                button(
                    to,
                    setting.value_text(to),
                    ButtonState::new(to == value, false),
                )
            })
            .collect()
    };
    // The gap keeps the name clear of the buttons above it.
    vec![
        Row::Gap(GAP / 2.0),
        Row::Text(BODY, label),
        Row::Buttons(buttons, true),
    ]
}
