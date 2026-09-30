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
//! as its name and buttons on one row: OFF / ON for a switch, one per choice
//! (the current one gold), and < > for a slider or a longer list of choices.

use super::builder::{ButtonSpec, PanelBuilder, Row};
use glam::Vec2;

use super::{BODY, GAP, GOLD_TEXT, LABEL_TEXT, Layout, TEXT, TITLE, Target};
use crate::game::GameState;
use crate::game::keys::Command;
use crate::game::settings::{Control, Setting};
use crate::game::strings::{hover_text, text};

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
        if self.net_menu.open {
            return self.network_panel_content();
        }
        let mut panel = PanelBuilder::default();
        panel.text(TITLE, vec![(text!("settings_title").into(), TEXT)]);
        let mut group = None;
        for setting in Setting::ALL {
            if group != Some(setting.group()) {
                group = Some(setting.group());
                panel.gap(GAP);
                panel.heading(&setting.group());
            }
            panel.setting(setting, self.settings.get(setting));
        }
        panel.gap(GAP);
        panel.text(
            BODY,
            vec![(text!("settings_city_overlays").into(), LABEL_TEXT)],
        );
        let (yields, yields_hover) = if self.show_yields {
            (
                text!("settings_city_yields_on"),
                hover_text!("settings_city_yields_on"),
            )
        } else {
            (
                text!("settings_city_yields_off"),
                hover_text!("settings_city_yields_off"),
            )
        };
        panel.compact_buttons(vec![
            ButtonSpec::new(Target::ToggleYields, yields, Command::Yields.key())
                .hover_text(yields_hover)
                .queued(self.show_yields),
        ]);
        panel.gap(GAP);
        panel.compact_buttons(vec![
            ButtonSpec::new(
                Target::OpenMultiplayer,
                text!("settings_multiplayer_button"),
                "",
            )
            .hover_text(hover_text!("settings_multiplayer_button")),
            self.close_settings_button(),
            ButtonSpec::new(Target::Quit, text!("settings_quit_button"), "")
                .hover_text(hover_text!("settings_quit_button")),
        ]);
        panel
    }

    /// The button that closes the menu, on the settings and on the
    /// Multiplayer page.
    pub(super) fn close_settings_button(&self) -> ButtonSpec {
        ButtonSpec::new(
            Target::CloseSettings,
            text!("settings_close_button"),
            Command::Back.key(),
        )
        .hover_text(hover_text!("settings_close_button"))
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

/// How the classic presentation shows `setting` at `value`: one row, its
/// name (and its value, if stepped) and then the buttons that set it.
pub(super) fn classic_setting_rows(setting: Setting, value: i32) -> Vec<Row> {
    let range = setting.range();
    let button =
        |to: i32, label: String| ButtonSpec::new(Target::SetSetting(setting, to), label, "");
    // A step past either end of the range: already there.
    let now = setting.value_text(value);
    let lowest = (value <= *range.start()).then(|| text!("settings_step_at_lowest", value = now));
    let highest = (value >= *range.end()).then(|| text!("settings_step_at_highest", value = now));
    let mut label = vec![(format!("{}  ", setting.name()), LABEL_TEXT)];
    let buttons = if steps_in_classic(setting) {
        label.push((setting.value_text(value), GOLD_TEXT));
        vec![
            button(value - 1, "<".into()).unavailable(lowest),
            button(value + 1, ">".into()).unavailable(highest),
        ]
    } else {
        range
            .map(|to| button(to, setting.value_text(to)).queued(to == value))
            .collect()
    };
    // One row a setting, so the menu fits a short screen.
    vec![Row::LabeledButtons(label, buttons)]
}
