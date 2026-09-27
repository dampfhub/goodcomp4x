//! Testing aids: switching between the scenario pages (F1-F4; pressing the
//! current one's key again restarts it, on a new map for the F4 world), and a
//! savestate (F6 saves, F7 loads) holding a snapshot of the whole game. The
//! savestate survives switching pages, and loading it can be repeated to
//! retry the same situation.

use rand::RngExt;

use super::{GameRng, GameState};

/// The test scenarios F1-F4 switch between.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scenario {
    /// The original small combat map.
    Combat,
    /// Two cities with their economies set up.
    Cities,
    /// A settler each and no cities yet.
    Frontier,
    /// A settler each on a randomly generated map (`mapgen.rs`).
    World,
}

impl Scenario {
    pub const ALL: [Scenario; 4] = [
        Scenario::Combat,
        Scenario::Cities,
        Scenario::Frontier,
        Scenario::World,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Scenario::Combat => "COMBAT",
            Scenario::Cities => "CITIES",
            Scenario::Frontier => "FRONTIER",
            Scenario::World => "WORLD",
        }
    }

    /// The function key that switches to it.
    pub fn key(self) -> &'static str {
        match self {
            Scenario::Combat => "F1",
            Scenario::Cities => "F2",
            Scenario::Frontier => "F3",
            Scenario::World => "F4",
        }
    }

    /// A fresh game of this scenario, drawing the world's map seed from `rng`.
    fn start(self, rng: &mut GameRng) -> GameState {
        match self {
            Scenario::Combat => GameState::new(),
            Scenario::Cities => GameState::city_scenario(),
            Scenario::Frontier => GameState::frontier_scenario(),
            Scenario::World => GameState::world_scenario(rng.random()),
        }
    }
}

impl GameState {
    /// Seeds the game's RNG, which rolls damage and picks the map of every
    /// F4 world started from this game, so the same seed and orders replay
    /// the same game.
    #[cfg(test)]
    pub(super) fn seed_rng(&mut self, seed: u64) {
        self.rng = rand::SeedableRng::seed_from_u64(seed);
    }

    /// F1-F4: starts `scenario` afresh (restarting it, if it's the current
    /// one; the world gets a new random map), keeping the savestate, debug
    /// settings and the RNG (so a seeded game stays reproducible).
    pub fn switch_scenario(&mut self, scenario: Scenario) {
        let savestate = self.savestate.take();
        let (instant_playback, fog_of_war) = (self.instant_playback, self.fog_of_war);
        let mut rng = self.rng.clone();
        *self = scenario.start(&mut rng);
        self.rng = rng;
        self.savestate = savestate;
        self.instant_playback = instant_playback;
        self.fog_of_war = fog_of_war;
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
        restored.fog_of_war = self.fog_of_war;
        // Rolls carry on from the current game rather than replaying the
        // saved ones, so retrying a save can go differently.
        restored.rng = self.rng.clone();
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
    fn the_seed_carries_across_scenario_switches_but_not_into_loads() {
        // (`simulation.rs` checks that the seed also picks the F4 world.)
        let next_roll_after_switch = |seed| {
            let mut game = GameState::new();
            game.seed_rng(seed);
            game.switch_scenario(Scenario::Cities);
            game.rng.random::<u64>()
        };
        assert_eq!(next_roll_after_switch(3), next_roll_after_switch(3));
        assert_ne!(next_roll_after_switch(3), next_roll_after_switch(4));

        // Loading a save keeps the current RNG, so a retry rolls afresh.
        let mut game = GameState::new();
        game.seed_rng(3);
        game.save_state();
        let first: u64 = game.rng.random();
        game.load_state();
        assert_ne!(game.rng.random::<u64>(), first);
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
    fn the_world_starts_each_side_with_a_settler_worker_and_warrior() {
        let game = GameState::world_scenario(5);
        assert_eq!(game.scenario, Scenario::World);
        assert_eq!(game.map_seed, Some(5));
        assert_eq!(game.units.len(), 6);
        assert_eq!(game.settlers.len(), 2);
        assert_eq!(game.workers.len(), 2);
        assert!(game.units.iter().all(|u| game.grid.is_passable(u.pos)));
        let selected = &game.units[game.selected.unwrap()];
        assert!(game.settlers.contains(&selected.id), "the player's settler");
    }

    #[test]
    fn cities_work_water_but_goods_never_cross_it() {
        // Find a world whose Blue start has water within logistics reach.
        for seed in 0..40 {
            let mut game = GameState::world_scenario(seed);
            game.found_city_selected();
            let routes = game.routes(0);
            let water: Vec<Hex> = routes
                .costs
                .keys()
                .copied()
                .filter(|h| game.grid.terrain(*h).is_water())
                .collect();
            if water.is_empty() {
                continue;
            }
            for hex in water {
                let land_beside = hex
                    .neighbors()
                    .into_iter()
                    .any(|n| !game.grid.terrain(n).is_water() && routes.costs.contains_key(&n));
                assert!(land_beside, "seed {seed}: {hex:?} reached over water");
            }
            let manager = game.cities[0].worked[0];
            assert!(!game.grid.terrain(manager).is_water(), "manager on land");
            return;
        }
        panic!("no start near water in 40 worlds");
    }

    #[test]
    fn instant_playback_resolves_the_whole_turn_at_once() {
        let mut game = GameState::new();
        assert!(game.instant_playback, "on by default");
        game.toggle_instant_playback();
        game.switch_scenario(Scenario::Combat);
        assert!(!game.instant_playback, "kept across a scenario switch");
        game.toggle_instant_playback();

        while game.pending() != (0, 0) {
            game.hold_or_end_turn();
        }
        game.hold_or_end_turn();
        assert!(game.is_resolving());
        game.update(0.0);
        assert!(!game.is_resolving(), "every step played in one update");
    }
}
