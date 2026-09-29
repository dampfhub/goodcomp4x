//! Testing aids: switching between the scenario pages (F1-F4; pressing the
//! current one's key again restarts it, on a new map for the F4 world), and a
//! savestate (F6 saves, F7 loads) holding a snapshot of the whole game. The
//! savestate survives switching pages, and loading it can be repeated to
//! retry the same situation.

use rand::RngExt;

use super::settings::Settings;
use super::{GameRng, GameState};

/// The test scenarios F1-F4 switch between.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Scenario {
    /// The original small combat map.
    Combat,
    /// Two cities with their economies set up.
    Cities,
    /// A settler each and no cities yet.
    Frontier,
    /// The player and 4-6 AI sides on a randomly generated map (`mapgen.rs`).
    World,
    /// A ready-to-play attack on the Red city's interior.
    Siege,
    Naval,
}

impl Scenario {
    pub const ALL: [Scenario; 6] = [
        Scenario::Combat,
        Scenario::Cities,
        Scenario::Frontier,
        Scenario::World,
        Scenario::Siege,
        Scenario::Naval,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Scenario::Combat => "COMBAT",
            Scenario::Cities => "CITIES",
            Scenario::Frontier => "FRONTIER",
            Scenario::World => "WORLD",
            Scenario::Siege => "SIEGE",
            Scenario::Naval => "NAVAL",
        }
    }

    /// The function key that switches to it.
    pub fn key(self) -> &'static str {
        match self {
            Scenario::Combat => "F1",
            Scenario::Cities => "F2",
            Scenario::Frontier => "F3",
            Scenario::World => "F4",
            Scenario::Siege => "F12",
            Scenario::Naval => "DEBUG",
        }
    }

    /// The scenario called `name` (its `name()` in any case, e.g. `cities`),
    /// as the `--scenario` command-line flag spells it.
    pub fn from_name(name: &str) -> Option<Scenario> {
        Self::ALL
            .into_iter()
            .find(|scenario| scenario.name().eq_ignore_ascii_case(name))
    }

    /// A fresh game of this scenario with the player's `settings`, on a
    /// random map for the world.
    pub fn new_game(self, settings: &Settings) -> GameState {
        let mut rng = rand::SeedableRng::seed_from_u64(rand::random());
        let mut game = self.start(&mut rng, settings);
        game.settings = settings.clone();
        game
    }

    /// A fresh game of this scenario, drawing the world's map seed from `rng`
    /// and building it as `settings` say (`Settings::world_ai`,
    /// `Settings::world_start_city`).
    fn start(self, rng: &mut GameRng, settings: &Settings) -> GameState {
        match self {
            Scenario::Combat => GameState::new(),
            Scenario::Cities => GameState::city_scenario(),
            Scenario::Frontier => GameState::frontier_scenario(),
            Scenario::World => GameState::world_scenario_with(rng.random(), settings),
            Scenario::Siege => GameState::siege_scenario(),
            Scenario::Naval => GameState::naval_scenario(),
        }
    }
}

impl GameState {
    /// Seeds the game's RNG, which rolls damage and picks the map of every
    /// F4 world started from this game, so the same seed and orders replay
    /// the same game.
    #[cfg(test)]
    pub(super) fn seed_rng(&mut self, seed: u64) {
        self.reseed(seed);
    }

    /// Seeds the RNG with `seed`, remembering it (`rng_seed`).
    pub(super) fn reseed(&mut self, seed: u64) {
        self.rng = rand::SeedableRng::seed_from_u64(seed);
        self.rng_seed = seed;
    }

    /// Whether a debug action that changes the game on this machine alone is
    /// off, as it is in a networked game (it would desync it); says so.
    pub(super) fn refuses_debug(&mut self) -> bool {
        if self.is_networked() {
            self.notice = "NOT IN A NETWORK GAME".into();
        }
        self.is_networked()
    }

    /// F1-F4: starts `scenario` afresh (restarting it, if it's the current
    /// one; the world gets a new random map), keeping the savestate, the
    /// player's settings (and whether their menu is open), whether Alt is
    /// held, the debug settings and the RNG (so a seeded game stays
    /// reproducible).
    pub fn switch_scenario(&mut self, scenario: Scenario) {
        if self.refuses_debug() {
            return;
        }
        let savestate = self.savestate.take();
        let settings = std::mem::take(&mut self.settings);
        let (settings_open, fog_of_war) = (self.settings_open, self.fog_of_war);
        // Whether Alt is held doesn't change with the scenario.
        let show_details = self.show_details;
        let net_menu = std::mem::take(&mut self.net_menu);
        let production_speedup = self.production_speedup;
        let lifetime_special_cap = self.lifetime_special_cap;
        let generation = self.generation.wrapping_add(1);
        let mut rng = self.rng.clone();
        *self = scenario.start(&mut rng, &settings);
        self.rng = rng;
        self.generation = generation;
        self.savestate = savestate;
        self.settings = settings;
        self.settings_open = settings_open;
        self.show_details = show_details;
        self.net_menu = net_menu;
        self.fog_of_war = fog_of_war;
        self.production_speedup = production_speedup;
        self.lifetime_special_cap = lifetime_special_cap;
    }

    /// F8: switches turn playback between one step at a time and all at once.
    pub fn toggle_instant_playback(&mut self) {
        self.settings.instant_playback = !self.settings.instant_playback;
        self.notice = if self.settings.instant_playback {
            "TURNS NOW PLAY OUT ALL AT ONCE - F8 FOR STEP BY STEP".into()
        } else {
            "TURNS NOW PLAY OUT STEP BY STEP - F8 FOR ALL AT ONCE".into()
        };
    }

    /// F6: saves a snapshot of the whole game, replacing any earlier one.
    pub fn save_state(&mut self) {
        if self.refuses_debug() {
            return;
        }
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

    /// F7: restores the saved snapshot, keeping it to load again. On the
    /// same map (`same_map`) the camera on the map stays where it is, and so
    /// does the camera inside a city if the snapshot and the game both have
    /// one open; whichever view the snapshot is in, it opens in it. On
    /// another map the snapshot's cameras come back with it.
    pub fn load_state(&mut self) {
        if self.refuses_debug() {
            return;
        }
        let Some(saved) = self.savestate.take() else {
            self.notice = "NOTHING SAVED YET - F6 SAVES".into();
            return;
        };
        let mut restored = (*saved).clone();
        if self.same_map(&restored) {
            let (map_camera, interior_camera) = match (&self.exterior_camera, self.interior_view) {
                (Some(exterior), Some(_)) => (exterior.clone(), Some(self.camera.clone())),
                _ => (self.camera.clone(), None),
            };
            if restored.interior_view.is_some() {
                restored.exterior_camera = Some(map_camera);
                if let Some(camera) = interior_camera {
                    restored.camera = camera;
                }
            } else {
                restored.camera = map_camera;
            }
        }
        restored.show_details = self.show_details;
        restored.settings = self.settings.clone();
        restored.settings_open = self.settings_open;
        restored.net_menu = self.net_menu.clone();
        restored.fog_of_war = self.fog_of_war;
        restored.production_speedup = self.production_speedup;
        restored.lifetime_special_cap = self.lifetime_special_cap;
        // Rolls carry on from the current game rather than replaying the
        // saved ones, so retrying a save can go differently.
        restored.rng = self.rng.clone();
        restored.generation = self.generation.wrapping_add(1);
        restored.savestate = Some(saved);
        restored.notice = format!("LOADED {}", restored.saved_summary().unwrap_or_default());
        *self = restored;
    }

    /// Whether `other` is on the same map as this game: the same scenario
    /// and, for a generated world, the same seed and size.
    fn same_map(&self, other: &GameState) -> bool {
        self.scenario == other.scenario
            && self.map_seed == other.map_seed
            && self.grid.bounds() == other.grid.bounds()
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
    use crate::game::PLAYER_TEAM;
    use crate::game::camera::Camera;
    use crate::game::hex::Hex;
    use crate::game::unit::{Team, UnitType};
    use glam::Vec2;

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

    fn camera_at(x: f32, y: f32) -> Camera {
        Camera::new(Vec2::new(x, y), 9.0)
    }

    #[test]
    fn loading_on_the_same_map_keeps_the_camera() {
        for mut game in [GameState::city_scenario(), GameState::world_scenario(5)] {
            game.save_state();
            game.camera = camera_at(7.0, -3.0);
            game.load_state();
            assert_eq!(game.camera.center, Vec2::new(7.0, -3.0));
            assert_eq!(game.camera.half_height, 9.0);
            assert!(game.exterior_camera.is_none());
        }
    }

    #[test]
    fn loading_a_snapshot_of_another_world_brings_its_camera() {
        // Another seed (F4 again).
        let mut game = GameState::world_scenario(5);
        game.camera = camera_at(7.0, -3.0);
        game.save_state();
        game.switch_scenario(Scenario::World);
        assert_ne!(game.map_seed, Some(5));
        game.camera = camera_at(-20.0, 4.0);
        game.load_state();
        assert_eq!(game.map_seed, Some(5));
        assert_eq!(game.camera.center, Vec2::new(7.0, -3.0));

        // The same seed at another size (more sides).
        let settings = |world_ai| Settings {
            world_ai,
            ..Settings::default()
        };
        let mut small = GameState::world_scenario_with(5, &settings(1));
        let mut large = GameState::world_scenario_with(5, &settings(5));
        assert_ne!(small.grid.bounds(), large.grid.bounds());
        small.camera = camera_at(7.0, -3.0);
        small.save_state();
        large.savestate = small.savestate.take();
        large.camera = camera_at(-20.0, 4.0);
        large.load_state();
        assert_eq!(large.camera.center, Vec2::new(7.0, -3.0));
    }

    #[test]
    fn loading_opens_the_snapshots_view_with_the_current_map_camera() {
        // Saved inside the city (Siege starts there), loaded outside it:
        // the interior's camera inside, and the map's as it was on leaving.
        let mut game = GameState::siege_scenario();
        let city = game.interior_view.expect("Siege starts inside a city");
        let inside = game.camera.center;
        game.save_state();
        game.close_city_interior();
        game.camera = camera_at(7.0, -3.0);
        game.load_state();
        assert_eq!(game.interior_view, Some(city));
        assert_eq!(game.camera.center, inside);
        game.close_city_interior();
        assert_eq!(game.camera.center, Vec2::new(7.0, -3.0));

        // Saved outside, loaded inside: the map's camera from before
        // entering, with no interior camera left over.
        let mut game = GameState::siege_scenario();
        game.close_city_interior();
        game.save_state();
        game.camera = camera_at(7.0, -3.0);
        game.open_city_interior(city);
        game.load_state();
        assert_eq!(game.interior_view, None);
        assert_eq!(game.camera.center, Vec2::new(7.0, -3.0));
        assert!(game.exterior_camera.is_none());

        // Saved and loaded inside: both cameras stay where they are.
        let mut game = GameState::siege_scenario();
        game.save_state();
        game.camera = camera_at(1.0, 2.0);
        game.exterior_camera = Some(camera_at(7.0, -3.0));
        game.load_state();
        assert_eq!(game.camera.center, Vec2::new(1.0, 2.0));
        game.close_city_interior();
        assert_eq!(game.camera.center, Vec2::new(7.0, -3.0));
    }

    #[test]
    fn holding_alt_carries_across_loads_and_scenario_switches() {
        let mut game = GameState::new();
        game.save_state();
        game.set_details(true);
        game.load_state();
        assert!(game.show_details);
        game.switch_scenario(Scenario::Cities);
        assert!(game.show_details);
        game.set_details(false);
        game.load_state();
        assert!(!game.show_details);
    }

    #[test]
    fn every_scenario_is_found_by_its_name_in_any_case() {
        for scenario in Scenario::ALL {
            let name = scenario.name();
            assert_eq!(Scenario::from_name(name), Some(scenario));
            assert_eq!(
                Scenario::from_name(&name.to_ascii_lowercase()),
                Some(scenario)
            );
        }
        assert_eq!(Scenario::from_name("city"), None);
        assert_eq!(Scenario::from_name(""), None);
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
    fn the_world_starts_every_side_with_a_city_and_a_scout() {
        for seed in [5, 6, 7] {
            let game = GameState::world_scenario(seed);
            assert_eq!(game.scenario, Scenario::World);
            assert_eq!(game.map_seed, Some(seed));
            // 4 to 6 AI sides, by the seed, and the player.
            let sides = 1 + 4 + (seed % 3) as usize;
            assert_eq!(game.cities.len(), sides, "seed {seed}");
            let sides_units = game.units.iter().filter(|u| u.team.is_side()).count();
            assert_eq!(sides_units, sides, "a scout each");
            assert_eq!(game.dens.len(), sides, "a den each, by default");
            assert!(game.settlers.is_empty());
            for team in &Team::ALL[..sides] {
                assert_eq!(game.cities.iter().filter(|c| c.team == *team).count(), 1);
                assert!(
                    game.units
                        .iter()
                        .any(|u| u.team == *team && u.unit_type == UnitType::Scout)
                );
            }
            assert_eq!(game.ai_teams().len(), sides - 1);
            assert!(game.units.iter().all(|u| game.grid.is_passable(u.pos)));
            assert!(!game.ruins.is_empty(), "ruins to fight over");
            // It opens on the city, which has nothing to build yet.
            let open = game
                .selected_city
                .expect("the city's production comes first");
            assert_eq!(game.cities[open].team, PLAYER_TEAM);
            assert_eq!(game.selected, None);
            let scout = game
                .units
                .iter()
                .find(|u| u.team == PLAYER_TEAM)
                .expect("the player's scout");
            assert_eq!(scout.pos.distance(game.cities[open].pos), 1, "by the city");
        }
    }

    #[test]
    fn the_world_can_start_with_settlers_and_a_chosen_number_of_ai() {
        let settings = Settings {
            world_ai: 2,
            world_start_city: false,
            ..Settings::default()
        };
        let game = GameState::world_scenario_with(9, &settings);
        assert!(game.cities.is_empty());
        assert_eq!(game.settlers.len(), 3, "the player's and two AI settlers");
        let sides_units = game.units.iter().filter(|u| u.team.is_side()).count();
        assert_eq!(sides_units, 6, "and a scout each");
        let selected = &game.units[game.selected.unwrap()];
        assert!(game.settlers.contains(&selected.id), "the player's settler");
        assert_eq!(selected.team, PLAYER_TEAM);
    }

    #[test]
    fn world_settings_carry_into_the_next_world() {
        let mut game = GameState::new();
        game.settings.world_ai = 1;
        game.settings.world_start_city = false;
        game.switch_scenario(Scenario::World);
        assert_eq!(game.settlers.len(), 2);
        assert_eq!(game.ai_teams(), vec![Team::Red]);
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
            let manager = game.cities[0].clusters[0].manager;
            assert!(!game.grid.terrain(manager).is_water(), "manager on land");
            return;
        }
        panic!("no start near water in 40 worlds");
    }

    #[test]
    fn instant_playback_resolves_the_whole_turn_at_once() {
        let mut game = GameState::new();
        assert!(game.settings.instant_playback, "on by default");
        game.toggle_instant_playback();
        game.switch_scenario(Scenario::Combat);
        assert!(
            !game.settings.instant_playback,
            "kept across a scenario switch"
        );
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
