//! Player-adjustable options (`Settings`) and the settings menu that Escape
//! opens. The menu's content is `ui/settings_menu.rs`; it has a row for every
//! entry of `Setting::ALL`, so adding a setting touches only this file:
//!
//! 1. a field in `Settings`, and its value in `Settings::default`;
//! 2. a `Setting` variant, listed in `Setting::ALL`;
//! 3. its arms in `Setting::key` (its name in the saved file), `name`,
//!    `description`, `range` and `value_text`,
//!    and in `Settings::get` and `Settings::set`.
//!
//! Every setting is an integer in its `range` (a switch is `0..=1`), which
//! the menu's < and > buttons step through, so both UI presentations show and
//! change it without further code. Game code reads the field directly
//! (`self.settings.instant_playback`). Settings, and whether the menu is
//! open, are kept across scenario switches and loads (`scenario.rs`), and the
//! settings between sessions (`to_text`, saved by `app.rs`): they
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
    /// The most turns a unit's plan holds, this one included: Shift-clicks
    /// (`order_queue.rs`) queue no turns past it. A hex farther away is
    /// queued as far along the way as the limit allows.
    pub max_queued_turns: usize,
    /// Draw unexplored hexes under clouds (`push_cloud_banks`, `draw.rs`)
    /// rather than a flat grey.
    pub cloud_fog: bool,
    /// AI players in the next world (F4, `setup_world`): 1 to 6, or 0 for
    /// 4 to 6 picked by the map's seed.
    pub world_ai: usize,
    /// Each side in the next world starts with its city already founded,
    /// rather than a settler to found it with.
    pub world_start_city: bool,
}

/// `Settings::world_ai` for 4 to 6 AI players, picked by the map's seed.
pub const WORLD_AI_BY_SEED: usize = 0;

impl Default for Settings {
    fn default() -> Self {
        Self {
            instant_playback: true,
            max_queued_turns: 6,
            cloud_fog: true,
            world_ai: WORLD_AI_BY_SEED,
            world_start_city: true,
        }
    }
}

impl Settings {
    /// How many AI players a world from `seed` gets.
    pub fn world_ai_for(&self, seed: u32) -> usize {
        if self.world_ai == WORLD_AI_BY_SEED {
            4 + (seed % 3) as usize
        } else {
            self.world_ai
        }
    }
}

/// One of the `Settings`, as the settings menu shows and changes it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Setting {
    TurnPlayback,
    MaxQueuedTurns,
    FogStyle,
    WorldAi,
    WorldStart,
}

impl Setting {
    /// Every setting, in the order the menu lists them.
    pub const ALL: [Setting; 5] = [
        Setting::TurnPlayback,
        Setting::MaxQueuedTurns,
        Setting::FogStyle,
        Setting::WorldAi,
        Setting::WorldStart,
    ];

    /// Its name in the saved settings file (`to_text`). Old files use
    /// these, so a setting keeps its key once it has one.
    pub fn key(self) -> &'static str {
        match self {
            Setting::TurnPlayback => "turn_playback",
            Setting::MaxQueuedTurns => "queue_limit",
            Setting::FogStyle => "fog",
            Setting::WorldAi => "world_ai",
            Setting::WorldStart => "world_start",
        }
    }

    /// Its label in the menu.
    pub fn name(self) -> &'static str {
        match self {
            Setting::TurnPlayback => "TURN PLAYBACK",
            Setting::MaxQueuedTurns => "QUEUE LIMIT",
            Setting::FogStyle => "FOG",
            Setting::WorldAi => "WORLD AI",
            Setting::WorldStart => "WORLD START",
        }
    }

    /// What it does, for the tooltip on its buttons.
    pub fn description(self) -> &'static str {
        match self {
            Setting::TurnPlayback => {
                "WHETHER A TURN PLAYS OUT ALL AT ONCE OR ONE STEP AT A TIME. THE OUTCOME IS \
                 THE SAME. F8 SWITCHES IT TOO."
            }
            Setting::MaxQueuedTurns => {
                "THE MOST TURNS A UNIT CAN HAVE QUEUED, THIS ONE INCLUDED. SHIFT-CLICKING A HEX \
                 FARTHER AWAY QUEUES THE MOVE AS FAR AS THE LIMIT GOES."
            }
            Setting::FogStyle => "HOW UNEXPLORED LAND IS HIDDEN: UNDER CLOUDS, OR A FLAT GREY.",
            Setting::WorldAi => "AI PLAYERS IN THE NEXT WORLD (F4). THE MAP GROWS WITH THEM.",
            Setting::WorldStart => {
                "WHETHER EVERY SIDE IN THE NEXT WORLD (F4) STARTS WITH ITS CITY, OR A SETTLER."
            }
        }
    }

    /// The values it can take, lowest first.
    pub fn range(self) -> RangeInclusive<i32> {
        match self {
            Setting::TurnPlayback => 0..=1,
            Setting::MaxQueuedTurns => 1..=20,
            Setting::FogStyle => 0..=1,
            Setting::WorldAi => 0..=6,
            Setting::WorldStart => 0..=1,
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
            Setting::MaxQueuedTurns if value == 1 => "1 TURN".into(),
            Setting::MaxQueuedTurns => format!("{value} TURNS"),
            Setting::FogStyle => if value == 1 { "CLOUDS" } else { "SOLID GREY" }.into(),
            Setting::WorldAi if value == WORLD_AI_BY_SEED as i32 => "4-6 BY MAP".into(),
            Setting::WorldAi => value.to_string(),
            Setting::WorldStart => if value == 1 { "CITY" } else { "SETTLER" }.into(),
        }
    }
}

impl Settings {
    /// `setting`'s current value, within its `range`.
    pub fn get(&self, setting: Setting) -> i32 {
        match setting {
            Setting::TurnPlayback => self.instant_playback as i32,
            Setting::MaxQueuedTurns => self.max_queued_turns as i32,
            Setting::FogStyle => self.cloud_fog as i32,
            Setting::WorldAi => self.world_ai as i32,
            Setting::WorldStart => self.world_start_city as i32,
        }
    }

    /// Sets `setting` to `value`, which must be within its `range`.
    fn set(&mut self, setting: Setting, value: i32) {
        match setting {
            Setting::TurnPlayback => self.instant_playback = value == 1,
            Setting::MaxQueuedTurns => self.max_queued_turns = value as usize,
            Setting::FogStyle => self.cloud_fog = value == 1,
            Setting::WorldAi => self.world_ai = value as usize,
            Setting::WorldStart => self.world_start_city = value == 1,
        }
    }

    /// The settings as text to save between sessions (`persist.rs`): a line
    /// per setting, its `key` and value.
    pub fn to_text(&self) -> String {
        Setting::ALL
            .iter()
            .map(|&setting| format!("{} {}\n", setting.key(), self.get(setting)))
            .collect()
    }

    /// Settings read back from `to_text`'s text. Anything missing, unknown or
    /// out of its setting's range keeps its default, so an old file (or a
    /// damaged one) still loads.
    pub fn from_text(text: &str) -> Self {
        let mut settings = Settings::default();
        for (key, values) in text.lines().filter_map(crate::persist::key_and_values) {
            let Some(setting) = Setting::ALL.into_iter().find(|s| s.key() == key) else {
                continue;
            };
            if let Some(value) = values.first().and_then(|v| v.parse::<i32>().ok())
                && setting.range().contains(&value)
            {
                settings.set(setting, value);
            }
        }
        settings
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
    /// The player's settings as text to save (`Settings::to_text`).
    pub fn settings_text(&self) -> String {
        self.settings.to_text()
    }

    /// Takes on the player's settings, e.g. saved in an earlier session.
    pub fn set_settings(&mut self, settings: Settings) {
        self.settings = settings;
    }

    pub(super) fn close_settings(&mut self) {
        self.settings_open = false;
    }

    /// Whether the settings menu's Quit button was clicked: the app then
    /// closes the window.
    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    /// A press of Escape. It closes one thing, in this order: the settings
    /// menu, then a city view, interior or site being chosen
    /// (`exit_structure_menu`), then a worker job being placed, then the
    /// worker menu, then the selection or the tile picked (`clear_selection`).
    /// With nothing to close it opens the settings menu, which has the Quit
    /// button.
    pub fn press_escape(&mut self) {
        if self.settings_open {
            self.settings_open = false;
        } else if self.exit_structure_menu() {
        } else if self.worker_mode && self.placing_job.is_none() {
            self.set_worker_mode(false);
        } else if !self.clear_selection() {
            self.settings_open = true;
        }
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
    fn settings_save_as_text_and_read_back() {
        let mut settings = Settings::default();
        for setting in Setting::ALL {
            // Every setting away from its default, one step.
            if !settings.step(setting, 1) {
                settings.step(setting, -1);
            }
        }
        assert_ne!(settings, Settings::default());
        let text = settings.to_text();
        assert_eq!(text.lines().count(), Setting::ALL.len());
        assert_eq!(Settings::from_text(&text), settings);

        // Unknown keys, bad values, comments and out-of-range values are
        // skipped; what's missing keeps its default.
        let damaged = "# saved\nqueue_limit 9\nfog maybe\nworld_ai 99\nsomething 3\n";
        let loaded = Settings::from_text(damaged);
        assert_eq!(loaded.max_queued_turns, 9);
        assert_eq!(loaded.cloud_fog, Settings::default().cloud_fog);
        assert_eq!(loaded.world_ai, Settings::default().world_ai);
        assert_eq!(Settings::from_text(""), Settings::default());
    }

    #[test]
    fn every_setting_has_its_own_key() {
        for (i, a) in Setting::ALL.iter().enumerate() {
            for b in &Setting::ALL[i + 1..] {
                assert_ne!(a.key(), b.key());
            }
            assert!(!a.key().contains(char::is_whitespace));
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

        // The city view closes first, then the tile picked.
        game.press_escape();
        assert_eq!(game.selected_city, None);
        assert!(!game.settings_open);
        game.inspected_tile = Some(Hex::new(0, 0));
        game.press_escape();
        assert_eq!(game.inspected_tile, None);
        assert!(!game.settings_open);

        // With nothing left, Escape opens the menu, and the next press
        // closes it again.
        game.press_escape();
        assert!(game.settings_open);
        game.press_escape();
        assert!(!game.settings_open);
    }

    #[test]
    fn escape_closes_the_open_menu_before_anything_else() {
        let mut game = GameState::new();
        game.clear_selection();
        game.press_escape();
        assert!(game.settings_open);
        let unit = game.units.iter().position(|u| u.team == PLAYER_TEAM);
        game.selected = unit;
        game.press_escape();
        assert!(!game.settings_open, "the menu closed first");
        assert_eq!(game.selected, unit, "the selection stays");
        game.press_escape();
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
        game.press_escape();
        game.switch_scenario(Scenario::Combat);
        assert!(game.settings_open, "still open after a switch");
        game.load_state();
        assert!(game.settings_open, "and after a load of a closed-menu save");
    }
}
