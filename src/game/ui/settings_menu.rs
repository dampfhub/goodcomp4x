//! The settings menu Escape opens: a row for every `Setting` (its name, its
//! value and < > buttons that step it), then Close. The settings themselves,
//! and how to add one, are in `game/settings.rs`; this file needs no change
//! for a new setting.

use super::builder::{ButtonSpec, PanelBuilder};
use super::dock::Zone;
use super::{
    BODY, ButtonState, DIM_TEXT, GAP, GOLD_TEXT, LABEL_TEXT, Layout, SMALL, TEXT, TITLE, Target,
};
use crate::game::GameState;
use crate::game::settings::Setting;

impl GameState {
    /// The classic presentation's settings menu, while it's open.
    pub(super) fn dock_settings(&self, layout: &mut Layout) {
        if self.settings_open {
            layout.dock_panel(self.settings_panel_content(), Zone::TopRight);
        }
    }

    /// The settings menu's content, shared by both presentations.
    pub(super) fn settings_panel_content(&self) -> PanelBuilder {
        let mut panel = PanelBuilder::default();
        panel.text(TITLE, vec![("SETTINGS".into(), TEXT)]);
        for setting in Setting::ALL {
            let value = self.settings.get(setting);
            let range = setting.range();
            panel.gap(GAP);
            panel.text(
                BODY,
                vec![
                    (format!("{}  ", setting.name()), LABEL_TEXT),
                    (setting.value_text(value), GOLD_TEXT),
                ],
            );
            let step = |delta: i32, label: &str, at_end: bool| ButtonSpec {
                target: Target::StepSetting(setting, delta),
                label: label.into(),
                hint: String::new(),
                state: ButtonState::new(false, at_end),
                armed: false,
            };
            panel.compact_buttons(vec![
                step(-1, "<", value <= *range.start()),
                step(1, ">", value >= *range.end()),
            ]);
        }
        panel.gap(GAP);
        panel.compact_buttons(vec![ButtonSpec {
            target: Target::CloseSettings,
            label: "CLOSE".into(),
            hint: "ESC".into(),
            state: ButtonState::Ready,
            armed: false,
        }]);
        panel.text(
            SMALL,
            vec![("HOLD ESC WITH NOTHING OPEN TO QUIT".into(), DIM_TEXT)],
        );
        panel
    }
}
