//! Testing aids: switching between the scenario pages (F1-F3; pressing the
//! current one's key again restarts it), and a savestate (F6 saves, F7 loads)
//! holding a snapshot of the whole game. The savestate survives switching
//! pages, and loading it can be repeated to retry the same situation.

use super::GameState;

/// The test scenarios F1-F3 switch between.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scenario {
    /// The original small combat map.
    Combat,
    /// Two cities with their economies set up.
    Cities,
    /// A settler each and no cities yet.
    Frontier,
}

impl Scenario {
    pub const ALL: [Scenario; 3] = [Scenario::Combat, Scenario::Cities, Scenario::Frontier];

    pub fn name(self) -> &'static str {
        match self {
            Scenario::Combat => "COMBAT",
            Scenario::Cities => "CITIES",
            Scenario::Frontier => "FRONTIER",
        }
    }

    /// The function key that switches to it.
    pub fn key(self) -> &'static str {
        match self {
            Scenario::Combat => "F1",
            Scenario::Cities => "F2",
            Scenario::Frontier => "F3",
        }
    }

    fn start(self) -> GameState {
        match self {
            Scenario::Combat => GameState::new(),
            Scenario::Cities => GameState::city_scenario(),
            Scenario::Frontier => GameState::frontier_scenario(),
        }
    }
}

impl GameState {
    /// F1-F3: starts `scenario` afresh (restarting it, if it's the current
    /// one), keeping the savestate and debug settings.
    pub fn switch_scenario(&mut self, scenario: Scenario) {
        let savestate = self.savestate.take();
        let instant_playback = self.instant_playback;
        *self = scenario.start();
        self.savestate = savestate;
        self.instant_playback = instant_playback;
    }

    /// F8: switches turn playback between one step at a time and all at once.
    pub fn toggle_instant_playback(&mut self) {
        self.instant_playback = !self.instant_playback;
        self.notice = if self.instant_playback {
            "TURNS NOW PLAY OUT ALL AT ONCE - F8 FOR STEP BY STEP".into()
        } else {
            "TURNS NOW PLAY OUT STEP BY STEP - F8 FOR ALL AT ONCE".into()
        };
    }

    /// F6: saves a snapshot of the whole game, replacing any earlier one.
    pub fn save_state(&mut self) {
        if self.is_resolving() {
            self.notice = "CAN'T SAVE WHILE A TURN PLAYS OUT".into();
            return;
        }
        // Drop the old snapshot first so it isn't copied into the new one.
        self.savestate = None;
        let snapshot = self.clone();
        self.savestate = Some(Box::new(snapshot));
        self.notice = format!(
            "SAVED {} - F7 LOADS IT",
            self.saved_summary().unwrap_or_default()
        );
    }

    /// F7: restores the saved snapshot, keeping it to load again. The camera
    /// stays where it is, unless the snapshot is from another scenario.
    pub fn load_state(&mut self) {
        let Some(saved) = self.savestate.take() else {
            self.notice = "NOTHING SAVED YET - F6 SAVES".into();
            return;
        };
        let mut restored = (*saved).clone();
        if restored.scenario == self.scenario {
            restored.camera = self.camera.clone();
        }
        restored.instant_playback = self.instant_playback;
        restored.savestate = Some(saved);
        restored.notice = format!("LOADED {}", restored.saved_summary().unwrap_or_default());
        *self = restored;
    }

    /// What's saved, like "TURN 3 OF CITIES", if anything is.
    pub(super) fn saved_summary(&self) -> Option<String> {
        self.savestate
            .as_ref()
            .map(|saved| format!("TURN {} OF {}", saved.turn + 1, saved.scenario.name()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::hex::Hex;

    #[test]
    fn loading_restores_the_snapshot_and_keeps_it() {
        let mut game = GameState::new();
        let unit = game.selected.unwrap();
        let start = game.units[unit].pos;
        game.save_state();

        game.units[unit].pos = Hex::new(0, 1);
        game.units[unit].hp = 1.0;
        game.load_state();
        assert_eq!(game.units[unit].pos, start);
        assert_eq!(game.units[unit].hp, game.units[unit].max_hp());

        // It can be loaded again, even from another scenario.
        game.units[unit].hp = 1.0;
        game.switch_scenario(Scenario::Cities);
        assert_eq!(game.scenario, Scenario::Cities);
        game.load_state();
        assert_eq!(game.scenario, Scenario::Combat);
        assert_eq!(game.units[unit].hp, game.units[unit].max_hp());
    }

    #[test]
    fn switching_to_the_current_scenario_restarts_it() {
        let mut game = GameState::new();
        game.switch_scenario(Scenario::Frontier);
        game.units.clear();
        game.switch_scenario(Scenario::Frontier);
        assert_eq!(game.scenario, Scenario::Frontier);
        assert!(!game.units.is_empty());
    }

    #[test]
    fn instant_playback_resolves_the_whole_turn_at_once() {
        let mut game = GameState::new();
        game.toggle_instant_playback();
        game.switch_scenario(Scenario::Combat);
        assert!(game.instant_playback, "kept across a scenario switch");

        while game.pending() != (0, 0) {
            game.hold_or_end_turn();
        }
        game.hold_or_end_turn();
        assert!(game.is_resolving());
        game.update(0.0);
        assert!(!game.is_resolving(), "every step played in one update");
    }
}
