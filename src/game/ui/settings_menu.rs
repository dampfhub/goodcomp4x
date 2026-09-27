//! The settings menu Escape opens: a row for every `Setting` (its name, its
//! value and < > buttons that step it), then Close and Quit. It opens in the
//! middle of the screen, over the map and the other panels. The settings themselves,
//! and how to add one, are in `game/settings.rs`; this file needs no change
//! for a new setting.

use super::builder::{ButtonSpec, PanelBuilder};
use glam::Vec2;

use super::{BODY, ButtonState, GAP, GOLD_TEXT, LABEL_TEXT, Layout, TEXT, TITLE, Target};
use crate::game::GameState;
use crate::game::settings::Setting;

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
            panel.place_top_left(top_left, layout);
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
