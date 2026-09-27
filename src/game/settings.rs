//! Player-adjustable options (`Settings`) and the settings menu that Escape
//! opens. The menu's content is `ui/settings_menu.rs`; it has a row for every
//! entry of `Setting::ALL`, so adding a setting touches only this file:
//!
//! 1. a field in `Settings`, and its value in `Settings::default`;
//! 2. a `Setting` variant, listed in `Setting::ALL`;
//! 3. its arms in `Setting::name`, `description`, `range` and `value_text`,
//!    and in `Settings::get` and `Settings::set`.
//!
//! Every setting is an integer in its `range` (a switch is `0..=1`), which
//! the menu's < and > buttons step through, so both UI presentations show and
//! change it without further code. Game code reads the field directly
//! (`self.settings.instant_playback`). Settings, and whether the menu is
//! open, are kept across scenario switches and loads (`scenario.rs`): they
//! belong to the player, not to the game being played. The tests below and
//! in `ui/tests.rs` walk every entry of `Setting::ALL`, so a new setting is
//! covered by them too.

use std::ops::RangeInclusive;

use super::GameState;

/// The player's options. See the module comment for adding one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Play a turn's steps all at once instead of one every
    /// `STEP_INTERVAL` (`turn.rs`). The outcome is the same. F8 toggles it.
    pub instant_playback: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            instant_playback: true,
        }
    }
}

/// One of the `Settings`, as the settings menu shows and changes it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Setting {
    TurnPlayback,
}

impl Setting {
    /// Every setting, in the order the menu lists them.
    pub const ALL: [Setting; 1] = [Setting::TurnPlayback];

    /// Its label in the menu.
    pub fn name(self) -> &'static str {
        match self {
            Setting::TurnPlayback => "TURN PLAYBACK",
        }
    }

    /// What it does, for the tooltip on its buttons.
    pub fn description(self) -> &'static str {
        match self {
            Setting::TurnPlayback => {
                "WHETHER A TURN PLAYS OUT ALL AT ONCE OR ONE STEP AT A TIME. THE OUTCOME IS \
                 THE SAME. F8 SWITCHES IT TOO."
            }
        }
    }

    /// The values it can take, lowest first.
    pub fn range(self) -> RangeInclusive<i32> {
        match self {
            Setting::TurnPlayback => 0..=1,
        }
    }

    /// How the menu shows `value`.
    pub fn value_text(self, value: i32) -> String {
        match self {
            Setting::TurnPlayback => if value == 1 {
                "ALL AT ONCE"
            } else {
                "STEP BY STEP"
            }
            .into(),
        }
    }
}

impl Settings {
    /// `setting`'s current value, within its `range`.
    pub fn get(&self, setting: Setting) -> i32 {
        match setting {
            Setting::TurnPlayback => self.instant_playback as i32,
        }
    }

    /// Sets `setting` to `value`, which must be within its `range`.
    fn set(&mut self, setting: Setting, value: i32) {
        match setting {
            Setting::TurnPlayback => self.instant_playback = value == 1,
        }
    }

    /// Moves `setting` by `delta` steps, stopping at the ends of its range.
    /// Returns whether it changed.
    pub fn step(&mut self, setting: Setting, delta: i32) -> bool {
        let range = setting.range();
        let old = self.get(setting);
        let new = old
            .saturating_add(delta)
            .clamp(*range.start(), *range.end());
        self.set(setting, new);
        new != old
    }
}

impl GameState {
    pub(super) fn close_settings(&mut self) {
        self.settings_open = false;
    }

    /// A press of Escape. It closes one thing, in this order: the settings
    /// menu, then a city view, interior or site being chosen
    /// (`exit_structure_menu`), then wall or gate placement, the selection or
    /// the tile panel (`clear_selection`). With nothing to close it opens the
    /// settings menu and returns true: then holding Escape on quits (`app.rs`).
    pub fn press_escape(&mut self) -> bool {
        if self.settings_open {
            self.settings_open = false;
            return false;
        }
        if self.exit_structure_menu() || self.clear_selection() {
            return false;
        }
        self.settings_open = true;
        true
    }

    /// The menu's < (`delta` -1) and > (+1) buttons for `setting`.
    pub(super) fn step_setting(&mut self, setting: Setting, delta: i32) {
        if self.settings.step(setting, delta) {
            let value = self.settings.get(setting);
            self.notice = format!("{}: {}", setting.name(), setting.value_text(value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::hex::Hex;
    use crate::game::{PLAYER_TEAM, Scenario};

    #[test]
    fn every_setting_starts_in_range_and_steps_within_it() {
        let mut settings = Settings::default();
        for setting in Setting::ALL {
            let range = setting.range();
            assert!(range.contains(&settings.get(setting)), "{setting:?}");
            // Step all the way down, then all the way up: each end holds.
            while settings.step(setting, -1) {}
            assert_eq!(settings.get(setting), *range.start(), "{setting:?}");
            assert!(!settings.step(setting, -1));
            while settings.step(setting, 1) {}
            assert_eq!(settings.get(setting), *range.end(), "{setting:?}");
            assert!(!settings.step(setting, 1));
            for value in range {
                assert!(!setting.value_text(value).is_empty());
            }
        }
    }

    #[test]
    fn turn_playback_is_the_instant_playback_switch() {
        let mut game = GameState::new();
        assert!(game.settings.instant_playback, "on by default");
        game.step_setting(Setting::TurnPlayback, -1);
        assert!(!game.settings.instant_playback);
        assert_eq!(game.notice, "TURN PLAYBACK: STEP BY STEP");
        game.toggle_instant_playback();
        assert_eq!(game.settings.get(Setting::TurnPlayback), 1, "F8 agrees");
    }

    #[test]
    fn settings_survive_scenario_switches_and_loads() {
        let mut game = GameState::city_scenario();
        game.step_setting(Setting::TurnPlayback, -1);
        game.save_state();
        game.step_setting(Setting::TurnPlayback, 1);
        game.step_setting(Setting::TurnPlayback, -1);
        let changed = game.settings.clone();
        assert_ne!(changed, Settings::default());
        game.switch_scenario(Scenario::Combat);
        assert_eq!(game.settings, changed);
        game.step_setting(Setting::TurnPlayback, 1);
        let current = game.settings.clone();
        game.load_state();
        assert_eq!(game.settings, current, "a load keeps the current settings");
    }

    #[test]
    fn escape_closes_whatever_is_open_before_opening_the_menu() {
        let mut game = GameState::city_scenario();
        game.leave_city_view();
        game.clear_selection();
        game.inspected_tile = Some(Hex::new(0, 0));
        game.select_city();
        assert!(game.selected_city.is_some());

        // The city view closes first, then the tile panel.
        assert!(!game.press_escape());
        assert_eq!(game.selected_city, None);
        assert!(!game.settings_open);
        game.inspected_tile = Some(Hex::new(0, 0));
        assert!(!game.press_escape());
        assert_eq!(game.inspected_tile, None);
        assert!(!game.settings_open);

        // With nothing left, Escape opens the menu (and may quit if held),
        // and the next press closes it again.
        assert!(game.press_escape(), "opening the menu starts the quit hold");
        assert!(game.settings_open);
        assert!(!game.press_escape());
        assert!(!game.settings_open);
    }

    #[test]
    fn escape_closes_the_open_menu_before_anything_else() {
        let mut game = GameState::new();
        game.clear_selection();
        assert!(game.press_escape());
        let unit = game.units.iter().position(|u| u.team == PLAYER_TEAM);
        game.selected = unit;
        assert!(!game.press_escape());
        assert!(!game.settings_open, "the menu closed first");
        assert_eq!(game.selected, unit, "the selection stays");
        assert!(!game.press_escape());
        assert_eq!(game.selected, None);
    }

    #[test]
    fn a_scenario_switch_or_load_keeps_the_menu_as_it_was() {
        let mut game = GameState::new();
        game.clear_selection();
        game.save_state();
        game.switch_scenario(Scenario::Cities);
        assert!(!game.settings_open);
        game.clear_selection();
        assert!(game.press_escape());
        game.switch_scenario(Scenario::Combat);
        assert!(game.settings_open, "still open after a switch");
        game.load_state();
        assert!(game.settings_open, "and after a load of a closed-menu save");
    }
}
