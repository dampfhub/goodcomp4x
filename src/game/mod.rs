//! Hex combat sandbox: game state and the queries shared across its parts.
//! Player input lives in `orders`, turn resolution in `turn`, the AI in `ai`,
//! world geometry in `draw`, and the screen-space UI in `ui`.

mod ability;
mod ai;
mod animals;
mod camera;
mod city;
mod combat;
mod draw;
mod effects;
mod fast_hash;
mod fog;
mod font;
mod group;
mod hex;
pub mod keys;
mod map_icons;
mod mapgen;
mod mesh;
mod multiplayer;
mod order_queue;
mod orders;
#[cfg(test)]
mod perf;
mod ruins;
mod scenario;
mod settings;
#[cfg(test)]
mod simulation;
mod sprites;
mod strings;
mod terrain;
mod transition;
mod turn;
mod ui;
mod unit;
mod unit_icons;
mod workers;

use fast_hash::{HashMap, HashSet};

use std::collections::VecDeque;

use glam::Vec2;
use rand::SeedableRng;

pub use camera::Camera;
pub use city::{BuildUnit, Building};
pub use font::atlas as font_atlas;
use hex::{HEX_SIZE, Hex, HexGrid};
pub use multiplayer::{
    DEFAULT_PORT, HOST_SEAT, MAX_PLAYERS, Message as NetMessage, PROTOCOL_VERSION,
};
pub use orders::ClickMode;
pub use scenario::Scenario;
pub use settings::Settings;
pub use sprites::atlas as sprite_atlas;
use terrain::Tile;
use turn::Step;
pub use ui::{
    ImGuiLayoutState, MIN_WINDOW_SIZE, NetMenu, NetRequest, selection_box, style_imgui,
    ui_projection,
};
pub use unit::Team;
use unit::{Unit, UnitType};
use unit_icons::UnitIcon;
pub use workers::JobKind;

/// The game's RNG: seedable, the same on every platform, and `Clone` so a
/// savestate can hold it (rand 0.10's `StdRng` isn't).
type GameRng = rand::rngs::Xoshiro256PlusPlus;

const GRID_RADIUS: i32 = 3;
/// The side a single-player game gives the player, and the local side
/// until a multiplayer game says otherwise (`GameState::local_team`).
const PLAYER_TEAM: Team = Team::Blue;

/// Logged at startup. It only says where the controls are: `docs/controls.md`
/// is their one description, so don't list keys here.
const CONTROLS_HELP: &str = "\
Controls: see docs/controls.md (every key, mouse action and map symbol).
Rules: see docs/game-rules.md.";

#[derive(Clone)]
pub struct GameState {
    cities: Vec<city::City>,
    /// Each side's food, wood and metal, indexed by `Team::index`
    /// (`city/economy.rs`).
    stockpiles: [city::Stock; Team::ALL.len()],
    /// Debug setting: a city's production speeds its builds on top of the
    /// fixed time (`work_rate`). Kept across scenario switches and loads.
    production_speedup: bool,
    /// Cavalry and Armored each side's Barracks have trained, by
    /// `Resource::index`: what the cap counts under `lifetime_special_cap`
    /// (`city/barracks.rs`).
    special_trained: [[u32; 2]; Team::ALL.len()],
    /// Debug setting: a deposit's cap counts every Cavalry or Armored ever
    /// trained from it, not those alive. Kept across scenario switches and
    /// loads.
    lifetime_special_cap: bool,
    /// Tests: each time training started past its side's supply
    /// (`city/supply.rs`), which `simulation.rs` checks never happens.
    #[cfg(test)]
    supply_overruns: Vec<String>,
    /// Tests: the cities that gathered by themselves in the last economy,
    /// their queues working nothing (`auto_gather`), which `simulation.rs`
    /// checks against what their queues did.
    #[cfg(test)]
    pub(in crate::game) auto_gathered: Vec<usize>,
    /// Tests: supply each side has on top of its cities' (`fund`), by
    /// `Team::index`.
    #[cfg(test)]
    extra_supply: [u32; Team::ALL.len()],
    sites: crate::game::fast_hash::HashMap<Hex, city::Site>,
    roads: HashSet<Hex>,
    selected_city: Option<usize>,
    /// Barracks have their own production screen, separate from city labor.
    selected_barracks: Option<usize>,
    /// City interior currently being inspected and ordered.
    interior_view: Option<usize>,
    interior_selected: Option<u32>,
    /// The interior tile under the cursor while a city interior is open, for
    /// the attack preview (`hovered_tile` is the map's, and `None` then).
    hovered_interior: Option<Hex>,
    /// Preserve the exterior camera while the tactical city map is open.
    exterior_camera: Option<Camera>,
    /// The city and cluster whose manager has been picked up and awaits a
    /// destination click.
    moving_manager: Option<(usize, usize)>,
    hovered_city: Option<usize>,
    /// Whether the open city shows each tile's yields (Y toggles it).
    show_yields: bool,
    /// Whether Alt is held, showing extra map info: units' turn order and
    /// every tile's yields.
    show_details: bool,
    /// The map hex under the cursor (not over the UI), and how long the
    /// cursor has rested on it, for the tile tooltip.
    hovered_tile: Option<Hex>,
    hover_seconds: f32,
    ui_click_mode: Option<orders::ClickMode>,
    /// What the open city is placing for its workers to build (`workers.rs`):
    /// map clicks and drags place it, on tiles or (a wall or gate) on hex
    /// edges, until Escape, another pick or leaving the city.
    placing_job: Option<workers::JobKind>,
    /// The unit whose Disband was pressed once, waiting for a second press.
    disband_armed: Option<u32>,
    /// A plain click that would replace a selected unit's multi-turn queue,
    /// waiting for the same click again (`orders::confirm_queue_replace`).
    queue_replace_armed: Option<orders::QueueReplace>,
    /// The edge under the cursor while placing one, highlighted.
    hovered_job: Option<(Hex, Option<Hex>)>,
    city_queue_scroll: usize,
    barracks_queue_scroll: usize,
    queue_drag: Option<ui::QueueDrag>,
    notice: String,
    grid: HexGrid,
    /// Living units only: a unit is removed the moment it dies.
    units: Vec<Unit>,
    /// Index into `units` of the player's selected unit.
    selected: Option<usize>,
    /// Several units selected together (see `group.rs`), in place of
    /// `selected`; empty unless at least two are.
    group: Vec<usize>,
    /// The seed of a generated map (the F4 world), shown in the debug panel.
    map_seed: Option<u32>,
    /// Which test scenario this is, for restarting it.
    scenario: Scenario,
    /// A snapshot of the game saved for testing (F6), restored by F7.
    savestate: Option<Box<GameState>>,
    /// Counts the games this one replaced: a scenario switch, a load (F7)
    /// and a network game each take one more than the game before. What
    /// keeps city or unit ids between frames (ImGui's captured panels)
    /// checks it, since every new game reuses them.
    generation: u32,
    /// Attack animations playing out, with how many seconds each has run.
    effects: Vec<(effects::Effect, f32)>,
    /// The player's options (`settings.rs`). Kept across scenario switches
    /// and loads.
    settings: settings::Settings,
    /// Whether the settings menu (Escape) is open. Kept across scenario
    /// switches and loads, like the rest of the UI.
    settings_open: bool,
    /// The settings menu's Multiplayer section: what's typed, and what it
    /// asks the app to do (`ui/network_menu.rs`). Kept like the menu.
    net_menu: ui::NetMenu,
    /// The settings menu's Quit button was clicked; the app closes the window.
    quit_requested: bool,
    /// Debug setting (F10): hide what the player's side can't see (`fog.rs`).
    /// Kept across scenario switches and loads.
    fog_of_war: bool,
    /// Every hex the player's side has seen, as it last saw it: this
    /// machine's view, updated every frame (`fog.rs`).
    memory: fog::Memory,
    /// Every hex each side (by `Team::index`) has seen, as it last saw
    /// it, as of the latest turn the AI planned for it (`side_fog`): game
    /// state, the same on every machine, which the AI plans on.
    side_memory: [std::sync::Arc<fog::Memory>; Team::ALL.len()],
    turn: u32,
    pub camera: Camera,
    /// The F4 world's map seed (combat has no rolls). Seeded from entropy; tests
    /// seed it (`seed_rng`) so a game replays exactly. Kept across scenario
    /// switches (`scenario.rs`).
    rng: GameRng,
    /// The seed the RNG last took (`reseed`): a networked game's host sends
    /// it, so both machines roll the same dice.
    rng_seed: u64,
    /// A networked game's lockstep state (`multiplayer.rs`); `None` alone.
    lockstep: Option<Box<multiplayer::Lockstep>>,
    /// Steps of the turn currently playing out, drained one at a time by `update`.
    pending_steps: VecDeque<Step>,
    step_timer: f32,
    /// Units that acted in the latest step, highlighted until `highlight_timer` runs out.
    recent_actors: Vec<u32>,
    highlight_timer: f32,
    /// Seconds the fog's clouds have drifted (`animate_clouds`, `draw.rs`):
    /// the presentation clock, which also pulses the attack preview.
    cloud_time: f32,
    /// The turn transition playing: units and workers gliding to where a
    /// turn left them, and the new turn's cue (`transition.rs`).
    /// Presentation only: a copy of the game starts without it.
    transition: transition::Transition,
    /// Unit ids that may found a city. They use the melee placeholder body for now.
    settlers: HashSet<u32>,
    /// Sites where an AI side's settler was refused a city by the rules
    /// (a city it hadn't seen stood too near), so its settlers look
    /// elsewhere (`plan_ai_settlers`). Game state, like its memory.
    refused_sites: Vec<(Team, Hex)>,
    /// The turn strip's group whose units it lists one by one, while one of
    /// them is selected (`ui/roster.rs`).
    roster_open: Option<ui::RosterKey>,
    /// Ruins not yet claimed (`ruins.rs`), in the order the map made them.
    ruins: Vec<ruins::Ruin>,
    /// Animal dens not yet cleared (`animals.rs`), in the order the map made
    /// them.
    dens: Vec<animals::Den>,
    /// The most strays (animals of no den, `animals.rs`) the world keeps at
    /// once (`animals::stray_limit`), set with its dens.
    stray_limit: usize,
    /// Workers out on the map; the ones at home are counted by their city
    /// (`workers.rs`).
    field_workers: Vec<workers::FieldWorker>,
    /// Outposts and forts built by workers, by tile.
    structures: HashMap<Hex, workers::Structure>,
    /// Walls and gates built by workers, by the edge they stand on
    /// (`hex::edge`).
    barriers: HashMap<(Hex, Hex), workers::Structure>,
    /// Frontier sandbox units that the player may command despite being Red.
    player_controlled_units: HashSet<u32>,
    /// The side played at this machine: whose orders the input gives, whose
    /// fog, stockpile and notices show. Blue, unless a multiplayer game
    /// seats this player elsewhere (`multiplayer.rs`).
    local_team: Team,
    /// The sides people play, here or across the network; the AI plays
    /// the rest (`ai_teams`).
    humans: Vec<Team>,
    next_unit_id: u32,
}

impl GameState {
    pub fn new() -> Self {
        let layout = [
            (Hex::new(-2, 0), Team::Blue, UnitType::Melee),
            (Hex::new(-2, 1), Team::Blue, UnitType::Ranged),
            (Hex::new(-2, -1), Team::Blue, UnitType::Cavalry),
            (Hex::new(-1, -2), Team::Blue, UnitType::Siege),
            (Hex::new(2, 0), Team::Red, UnitType::Melee),
            (Hex::new(2, -1), Team::Red, UnitType::Ranged),
            (Hex::new(2, 1), Team::Red, UnitType::Cavalry),
            (Hex::new(1, 2), Team::Red, UnitType::Siege),
        ];
        let units = layout
            .into_iter()
            .zip(0..)
            .map(|((pos, team, unit_type), id)| Unit::new(id, pos, team, unit_type))
            .collect();

        // Mountain ridges along the center column leave a three-hex pass
        // (with a hill in the middle) as the only way between the two sides.
        let terrain = [
            (Hex::new(0, -3), Tile::MOUNTAINS),
            (Hex::new(0, -2), Tile::MOUNTAINS),
            (Hex::new(0, 2), Tile::MOUNTAINS),
            (Hex::new(0, 3), Tile::MOUNTAINS),
            (Hex::new(0, 0), Tile::HILLS),
            (Hex::new(-2, 2), Tile::HILLS),
            (Hex::new(2, -2), Tile::HILLS),
        ];

        log::info!(
            "You control {PLAYER_TEAM:?}; every other team is AI-controlled.\n{CONTROLS_HELP}"
        );

        let mut game = Self {
            cities: Vec::new(),
            stockpiles: [city::STARTING_STOCK; Team::ALL.len()],
            production_speedup: false,
            special_trained: [[0; 2]; Team::ALL.len()],
            lifetime_special_cap: false,
            #[cfg(test)]
            supply_overruns: Vec::new(),
            #[cfg(test)]
            auto_gathered: Vec::new(),
            #[cfg(test)]
            extra_supply: [0; Team::ALL.len()],
            sites: crate::game::fast_hash::HashMap::default(),
            roads: HashSet::default(),
            selected_city: None,
            selected_barracks: None,
            interior_view: None,
            interior_selected: None,
            hovered_interior: None,
            exterior_camera: None,
            moving_manager: None,
            hovered_city: None,
            show_yields: true,
            show_details: false,
            hovered_tile: None,
            hover_seconds: 0.0,
            ui_click_mode: None,
            placing_job: None,
            disband_armed: None,
            queue_replace_armed: None,
            hovered_job: None,
            city_queue_scroll: 0,
            barracks_queue_scroll: 0,
            queue_drag: None,
            notice: String::new(),
            grid: HexGrid::new(GRID_RADIUS, terrain),
            units,
            selected: None,
            group: Vec::new(),
            map_seed: None,
            scenario: Scenario::Combat,
            savestate: None,
            generation: 0,
            effects: Vec::new(),
            settings: settings::Settings::default(),
            settings_open: false,
            net_menu: ui::NetMenu::default(),
            quit_requested: false,
            fog_of_war: true,
            memory: fog::Memory::default(),
            side_memory: Default::default(),
            turn: 0,
            camera: Camera::new(Vec2::ZERO, (GRID_RADIUS as f32 + 1.5) * HEX_SIZE),
            rng: GameRng::seed_from_u64(rand::random()),
            rng_seed: 0,
            lockstep: None,
            pending_steps: VecDeque::new(),
            step_timer: 0.0,
            recent_actors: Vec::new(),
            highlight_timer: 0.0,
            cloud_time: 0.0,
            transition: transition::Transition::default(),
            settlers: HashSet::default(),
            refused_sites: Vec::new(),
            ruins: Vec::new(),
            dens: Vec::new(),
            stray_limit: 0,
            roster_open: None,
            field_workers: Vec::new(),
            structures: HashMap::default(),
            barriers: HashMap::default(),
            player_controlled_units: HashSet::default(),
            local_team: PLAYER_TEAM,
            humans: vec![PLAYER_TEAM],
            next_unit_id: 8,
        };
        game.select_next_needing_attention(None);
        game
    }

    #[cfg(test)]
    pub(crate) fn map_seed(&self) -> Option<u32> {
        self.map_seed
    }

    pub fn city_scenario() -> Self {
        let mut game = Self::new();
        game.scenario = Scenario::Cities;
        game.setup_cities();
        game.start_on_whole_map();
        game
    }

    /// A playable siege: Blue surrounds four gates with enough force to
    /// breach the defended command post.
    pub fn siege_scenario() -> Self {
        let mut game = Self::city_scenario();
        game.scenario = Scenario::Siege;
        for (id, pos) in [
            (0, Hex::new(3, 0)),
            (1, Hex::new(3, 1)),
            (4, Hex::new(5, 0)),
            (5, Hex::new(4, -1)),
        ] {
            game.units
                .iter_mut()
                .find(|unit| unit.id == id)
                .unwrap()
                .pos = pos;
        }
        // Keep the practice battle at the gates. The Cities scenario's other
        // troops would require unrelated orders and Red's roamers would
        // arrive mid-siege, changing the intended four-on-two test.
        game.units.retain(|unit| ![2, 3, 6, 7].contains(&unit.id));
        for (pos, kind) in [
            (Hex::new(5, -1), UnitType::Siege),
            (Hex::new(4, 1), UnitType::Melee),
        ] {
            let id = game.next_unit_id;
            game.next_unit_id += 1;
            game.units.push(Unit::new(id, pos, Team::Blue, kind));
        }
        for city in &mut game.cities {
            // Paid for already, as scenario setup: the queue skips the price.
            city.queue.push(city::Queued::prepaid(city::Build::Unit(
                city::BuildUnit::Melee,
            )));
        }
        game.open_city_interior(1);
        game.notice = "SIEGE: FIGHT ON BOTH MAPS (V) - BREACH POST, THEN OCCUPY IT".into();
        game
    }

    /// A narrow strait between two coastal cities, with ships and shore
    /// defenses already deployed for deterministic naval playtesting.
    pub fn naval_scenario() -> Self {
        let mut game = Self::city_scenario();
        game.scenario = Scenario::Naval;
        let water: Vec<_> = game
            .grid
            .all_hexes()
            .filter(|hex| (-1..=1).contains(&hex.q))
            .map(|hex| (hex, terrain::Terrain::Coast))
            .collect();
        game.grid = HexGrid::new(6, water);
        game.roads.retain(|h| game.grid.is_passable(*h));
        game.sites.retain(|h, _| game.grid.is_passable(*h));
        // Place both city centers on the shoreline, not two tiles inland.
        for (city, sign) in [(0, -1), (1, 1)] {
            game.cities[city].pos = Hex::new(sign * 2, 0);
            game.cities[city].clusters.clear();
            game.cities[city].remembered.clear();
        }
        game.units.clear();
        game.next_unit_id = 0;
        for (team, sign) in [(Team::Blue, -1), (Team::Red, 1)] {
            let city = if team == Team::Blue { 0 } else { 1 };
            game.cities[city]
                .extra_buildings
                .insert(city::Building::Harbor, Hex::new(sign * 2, 1));
            game.cities[city]
                .extra_buildings
                .insert(city::Building::CoastalBattery, Hex::new(sign * 2, -1));
            game.cities[city]
                .built
                .extend([city::Building::Harbor, city::Building::CoastalBattery]);
            for (pos, kind) in [
                (Hex::new(sign * 3, 0), UnitType::Melee),
                (Hex::new(sign, -sign), UnitType::LandingCraft),
                (Hex::new(sign, sign), UnitType::PatrolGalley),
                (Hex::new(0, sign * 2), UnitType::BombardShip),
            ] {
                let id = game.next_unit_id;
                game.next_unit_id += 1;
                game.units.push(Unit::new(id, pos, team, kind));
            }
        }
        for city in 0..game.cities.len() {
            game.auto_assign_city(city);
        }
        game.cities[0]
            .queue
            .push(city::Queued::prepaid(city::Build::Unit(
                city::BuildUnit::PatrolGalley,
            )));
        game.start_on_whole_map();
        game.notice =
            "NAVAL TEST: SELECT TROOP THEN CLICK CRAFT TO BOARD; CRAFT THEN SHORE TO LAND".into();
        game
    }

    /// Fresh economy match: each side begins with one settler and no city.
    pub fn frontier_scenario() -> Self {
        let mut game = Self::new();
        game.scenario = Scenario::Frontier;
        game.units.clear();
        game.setup_frontier();
        game.start_on_whole_map();
        game
    }

    /// A generated map (`mapgen.rs`) from `seed`, with the default settings.
    #[cfg(test)]
    pub fn world_scenario(seed: u32) -> Self {
        Self::world_scenario_with(seed, &settings::Settings::default())
    }

    /// A world from `seed` with the player alone on it, with a settler
    /// (first) and a scout: the AI sides' units and cities are taken away.
    #[cfg(test)]
    pub fn solo_world(seed: u32) -> Self {
        let settings = settings::Settings {
            world_start_city: false,
            ..Default::default()
        };
        let mut game = Self::world_scenario_with(seed, &settings);
        game.units.retain(|u| u.team == PLAYER_TEAM);
        game.cities.retain(|c| c.team == PLAYER_TEAM);
        game.selected = Some(0);
        game
    }

    /// A generated map (`mapgen.rs`) from `seed` with the player and as many
    /// AI sides as `settings` ask for (`setup_world`).
    pub fn world_scenario_with(seed: u32, settings: &settings::Settings) -> Self {
        let mut game = Self::new();
        game.scenario = Scenario::World;
        game.units.clear();
        game.setup_world(seed, settings);
        game.start_on_whole_map();
        // The map is too big to take in at once: start on the player's city
        // or settler.
        let home = game
            .cities
            .iter()
            .find(|c| c.team == PLAYER_TEAM)
            .map(|c| c.pos)
            .or_else(|| {
                game.units
                    .iter()
                    .find(|u| u.team == PLAYER_TEAM && game.settlers.contains(&u.id))
                    .map(|u| u.pos)
            })
            .unwrap_or(Hex::new(0, 0));
        game.camera = Camera::new(home.to_world(), game.camera.half_height);
        // What needs seeing to first, as every turn starts: the city's
        // production, or else the settler.
        game.select_next_needing_attention(None);
        game
    }

    /// After a scenario replaces the combat setup: centers the camera on the
    /// map (dropping any glide toward a unit that's gone) and selects the
    /// first unit without moving the camera to it.
    fn start_on_whole_map(&mut self) {
        self.camera = Camera::new(Vec2::ZERO, self.camera.half_height);
        self.selected = self.next_unit_needing_orders(None);
    }

    fn units_at(&self, hex: Hex) -> impl Iterator<Item = usize> + '_ {
        (0..self.units.len()).filter(move |&i| self.units[i].pos == hex)
    }

    /// Whether people play `team` (here or across the network) rather than
    /// the AI.
    pub(super) fn is_human(&self, team: Team) -> bool {
        self.humans.contains(&team)
    }

    fn is_player_controlled(&self, idx: usize) -> bool {
        self.units[idx].team == self.local_team
            || self.player_controlled_units.contains(&self.units[idx].id)
    }

    fn controlled_unit_at(&self, hex: Hex) -> Option<usize> {
        self.units_at(hex).find(|&i| self.is_player_controlled(i))
    }

    /// What the unit is, for display: settlers are marked on top of an
    /// ordinary unit type.
    fn unit_role(&self, unit: &Unit) -> &'static str {
        if self.settlers.contains(&unit.id) {
            "SETTLER"
        } else {
            match unit.unit_type {
                UnitType::Melee => "MELEE",
                UnitType::Ranged => "RANGED",
                UnitType::Cavalry => "CAVALRY",
                UnitType::Siege => "SIEGE",
                UnitType::Scout => "SCOUT",
                UnitType::Armored => "ARMORED",
                UnitType::PatrolGalley => "PATROL GALLEY",
                UnitType::LandingCraft => "LANDING CRAFT",
                UnitType::BombardShip => "BOMBARD SHIP",
                UnitType::Wolf => "WOLF PACK",
                UnitType::Bear => "BEAR",
            }
        }
    }

    /// How to draw `unit`: its pictogram, and hollow if it's a civilian.
    fn unit_look(&self, unit: &Unit) -> draw::UnitLook {
        let (icon, civilian) = if self.settlers.contains(&unit.id) {
            (UnitIcon::Flag, true)
        } else {
            (UnitIcon::of(unit.unit_type), false)
        };
        draw::UnitLook { icon, civilian }
    }

    /// A spawn may not materialize on an enemy city center or an uncaptured
    /// enemy field worker, even though neither counts as a unit occupant.
    fn spawn_clear_of_enemy_civilians(&self, hex: Hex, team: Team) -> bool {
        self.cities
            .iter()
            .all(|city| city.pos != hex || city.team == team)
            && self
                .field_workers
                .iter()
                .all(|worker| worker.pos != hex || worker.team == team)
    }

    fn is_occupied(&self, hex: Hex) -> bool {
        self.units_at(hex).next().is_some()
    }

    fn unit_of_team_at(&self, hex: Hex, team: Team) -> Option<usize> {
        self.units_at(hex).find(|&i| self.units[i].team == team)
    }

    fn enemy_of_team_at(&self, hex: Hex, team: Team) -> Option<usize> {
        self.units_at(hex).find(|&i| self.units[i].team != team)
    }

    fn enemy_barracks_at(&self, hex: Hex, team: Team) -> Option<usize> {
        self.cities.iter().position(|city| {
            city.team != team && city.barracks == Some(hex) && city.barracks_hp > 0.0
        })
    }

    fn enemy_coastal_battery_at(&self, hex: Hex, team: Team) -> Option<usize> {
        self.cities.iter().position(|city| {
            city.team != team
                && city.placed_site(city::Building::CoastalBattery) == Some(hex)
                && city.coastal_battery_hp > 0.0
        })
    }

    fn has_enemy_target_at(&self, hex: Hex, team: Team) -> bool {
        self.enemy_of_team_at(hex, team).is_some()
            || self.enemy_barracks_at(hex, team).is_some()
            || self.enemy_coastal_battery_at(hex, team).is_some()
    }

    /// An empty city center is not an exterior attack target. Units and
    /// workers standing there can still be attacked normally.
    fn empty_city_target(&self, hex: Hex, team: Team) -> bool {
        self.cities.iter().any(|city| city.pos == hex)
            && self.enemy_of_team_at(hex, team).is_none()
            && self.enemy_workers_at(hex, team).is_empty()
    }

    /// Whether two enemies are fighting over this hex.
    fn is_contested(&self, hex: Hex) -> bool {
        self.units_at(hex).count() > 1
    }

    /// The enemy sharing this unit's hex, if it's contested. Units locked in a
    /// contested hex fight each other every turn and can't attack elsewhere.
    fn rival_of(&self, idx: usize) -> Option<usize> {
        let unit = &self.units[idx];
        self.enemy_of_team_at(unit.pos, unit.team)
    }

    /// The ally this unit is swapping places with: the two are queued to move
    /// into each other's hexes.
    fn swap_partner(&self, idx: usize) -> Option<usize> {
        let unit = &self.units[idx];
        let dest = unit.planned_move?;
        self.units_at(dest).find(|&i| {
            let other = &self.units[i];
            other.team == unit.team && other.planned_move == Some(unit.pos)
        })
    }

    /// Hexes a unit of `team` can reach from `start` in at most `move_range`
    /// steps without passing through mountains, walls, others' gates or an
    /// occupied hex, so a line of units blocks the way. Includes `start`
    /// itself. Workers don't block. This is the real board, for tests to
    /// compare with: the player and the AI plan with `known_reachable_hexes`
    /// (`fog.rs`).
    #[cfg(test)]
    fn reachable_hexes(&self, start: Hex, move_range: i32, team: Team) -> HashSet<Hex> {
        self.reachable_hexes_by(start, move_range, |from, to| {
            self.can_step(from, to, team) && !self.is_occupied(to)
        })
    }

    /// Like `reachable_hexes`, with `open` deciding whether a step from one
    /// hex onto an adjacent one is possible.
    fn reachable_hexes_by(
        &self,
        start: Hex,
        move_range: i32,
        open: impl Fn(Hex, Hex) -> bool,
    ) -> HashSet<Hex> {
        let mut visited = HashSet::from_iter([start]);
        let mut frontier = vec![start];

        for _ in 0..move_range {
            let mut next = Vec::new();
            for hex in frontier {
                for neighbor in hex.neighbors() {
                    if open(hex, neighbor) && visited.insert(neighbor) {
                        next.push(neighbor);
                    }
                }
            }
            frontier = next;
        }

        visited
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use terrain::Terrain;
    use turn::Phase;

    /// The starting layout has exactly one unit of each type per team.
    fn find(game: &GameState, team: Team, unit_type: UnitType) -> usize {
        game.units
            .iter()
            .position(|u| u.team == team && u.unit_type == unit_type)
            .unwrap()
    }

    /// The startup log sends players to the docs, so the files it names
    /// must exist.
    #[test]
    fn controls_help_points_at_existing_docs() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for doc in ["docs/controls.md", "docs/game-rules.md"] {
            assert!(
                CONTROLS_HELP.contains(doc),
                "CONTROLS_HELP should name {doc}"
            );
            assert!(root.join(doc).is_file(), "{doc} is missing");
        }
    }

    #[test]
    fn units_cannot_move_through_an_occupied_hex() {
        let mut game = GameState::new();
        let cavalry = find(&game, Team::Blue, UnitType::Cavalry);
        let blocker = find(&game, Team::Red, UnitType::Melee);

        // (2, 0) is 2 steps away, but the only 2-step path runs through (1, 0).
        game.units[cavalry].pos = Hex::new(0, 0);
        game.units[blocker].pos = Hex::new(1, 0);

        game.try_queue_move(cavalry, Hex::new(2, 0));
        assert_eq!(game.units[cavalry].planned_move, None);
    }

    #[test]
    fn units_can_path_around_a_single_blocker() {
        let mut game = GameState::new();
        let cavalry = find(&game, Team::Blue, UnitType::Cavalry);
        let blocker = find(&game, Team::Red, UnitType::Melee);

        // (1, 1) is reachable through either (1, 0) or (0, 1).
        game.units[cavalry].pos = Hex::new(0, 0);
        game.units[blocker].pos = Hex::new(1, 0);

        game.try_queue_move(cavalry, Hex::new(1, 1));
        assert_eq!(game.units[cavalry].planned_move, Some(Hex::new(1, 1)));
    }

    #[test]
    fn cancelling_a_move_clears_an_attack_it_enabled() {
        let mut game = GameState::new();
        let melee = find(&game, Team::Blue, UnitType::Melee);
        let enemy = find(&game, Team::Red, UnitType::Melee);

        // Melee attacks at range 1, so it only reaches (2, 0) after moving to (1, 0).
        game.units[melee].pos = Hex::new(0, 0);
        game.units[enemy].pos = Hex::new(2, 0);

        game.try_queue_move(melee, Hex::new(1, 0));
        game.try_queue_attack(melee, Hex::new(2, 0));
        assert_eq!(game.units[melee].planned_attack, Some(Hex::new(2, 0)));

        game.try_queue_move(melee, Hex::new(1, 0));
        assert_eq!(game.units[melee].planned_move, None);
        assert_eq!(game.units[melee].planned_attack, None);
    }

    #[test]
    fn siege_can_queue_and_land_a_max_range_attack() {
        let mut game = GameState::new();
        let siege = find(&game, Team::Blue, UnitType::Siege);
        let enemy = find(&game, Team::Red, UnitType::Melee);

        game.units[siege].pos = Hex::new(0, 0);
        game.units[enemy].pos = Hex::new(2, 0);
        assert_eq!(UnitType::Siege.stats().attack_range, 2);

        game.try_queue_attack(siege, Hex::new(2, 0));
        assert_eq!(game.units[siege].planned_attack, Some(Hex::new(2, 0)));

        let enemy_id = game.units[enemy].id;
        let hp_before = game.units[enemy].hp;
        game.resolve_step(UnitType::Siege, Phase::Attack);

        // The defender either took damage or died and was removed.
        let hp_after = game
            .units
            .iter()
            .find(|u| u.id == enemy_id)
            .map_or(0.0, |u| u.hp);
        assert!(hp_after < hp_before);
    }

    #[test]
    fn multiple_units_can_target_the_same_hex() {
        let mut game = GameState::new();
        let melee = find(&game, Team::Blue, UnitType::Melee);
        let ranged = find(&game, Team::Blue, UnitType::Ranged);
        game.units[melee].pos = Hex::new(1, 0);
        game.units[ranged].pos = Hex::new(1, -1);

        let target = Hex::new(2, 0);
        game.try_queue_attack(melee, target);
        game.try_queue_attack(ranged, target);

        assert_eq!(game.units[melee].planned_attack, Some(target));
        assert_eq!(game.units[ranged].planned_attack, Some(target));
    }

    #[test]
    fn mountains_block_movement_and_pathing() {
        let mut game = GameState::new();
        let cavalry = find(&game, Team::Blue, UnitType::Cavalry);
        let siege = find(&game, Team::Blue, UnitType::Siege);
        game.units[siege].pos = Hex::new(-3, 0);

        // From (-1, -2), both 2-step paths to (1, -3) cross the mountains at
        // (0, -3) and (0, -2), and the mountain itself can't be entered.
        game.units[cavalry].pos = Hex::new(-1, -2);
        assert_eq!(game.grid.terrain(Hex::new(0, -2)), Terrain::Mountains);
        // Pathing goes by what the player knows: the mountains in sight.
        game.explore();

        game.try_queue_move(cavalry, Hex::new(0, -2));
        assert_eq!(game.units[cavalry].planned_move, None);
        game.try_queue_move(cavalry, Hex::new(1, -3));
        assert_eq!(game.units[cavalry].planned_move, None);
    }

    #[test]
    fn hills_reduce_damage_taken() {
        let hill = Hex::new(1, 0);
        let plain = Hex::new(-1, 0);
        let grid = HexGrid::new(GRID_RADIUS, [(hill, Tile::HILLS)]);

        let damage_taken_at = |pos: Hex| {
            let siege = Unit::new(0, Hex::new(0, 0), Team::Blue, UnitType::Siege);
            let defender = Unit::new(1, pos, Team::Red, UnitType::Melee);
            let multiplier = grid.tile(pos).defense_multiplier();
            combat::damage(&siege, &defender, multiplier)
        };

        assert!(damage_taken_at(hill) < damage_taken_at(plain));
    }

    /// The two cavalry units, placed and given moves for a same-step test.
    fn cavalry_moves(game: &mut GameState, blue: (Hex, Hex), red: (Hex, Hex)) -> (usize, usize) {
        let blue_cav = find(game, Team::Blue, UnitType::Cavalry);
        let red_cav = find(game, Team::Red, UnitType::Cavalry);
        (game.units[blue_cav].pos, game.units[blue_cav].planned_move) = (blue.0, Some(blue.1));
        (game.units[red_cav].pos, game.units[red_cav].planned_move) = (red.0, Some(red.1));
        (blue_cav, red_cav)
    }

    #[test]
    fn enemies_moving_onto_the_same_hex_contest_it_and_fight() {
        let mut game = GameState::new();
        let center = Hex::new(0, 0);
        let (blue, red) = cavalry_moves(
            &mut game,
            (Hex::new(-1, 0), center),
            (Hex::new(1, 0), center),
        );

        game.resolve_step(UnitType::Cavalry, Phase::Move);
        assert_eq!(game.units[blue].pos, center);
        assert_eq!(game.units[red].pos, center);
        assert!(game.is_contested(center));

        // Locked in: it can't be ordered to attack elsewhere...
        game.try_queue_attack(blue, Hex::new(-1, 1));
        assert_eq!(game.units[blue].planned_attack, None);

        // ...but fights its rival in its attack step without any orders.
        game.resolve_step(UnitType::Cavalry, Phase::Attack);
        assert!(
            game.units
                .iter()
                .all(|u| u.unit_type != UnitType::Cavalry || u.hp < u.max_hp())
        );
    }

    #[test]
    fn a_unit_can_withdraw_from_a_contested_hex() {
        let mut game = GameState::new();
        let center = Hex::new(0, 0);
        let (blue, red) = cavalry_moves(
            &mut game,
            (Hex::new(-1, 0), center),
            (Hex::new(1, 0), center),
        );
        game.resolve_step(UnitType::Cavalry, Phase::Move);

        game.try_queue_move(blue, Hex::new(-1, 0));
        game.resolve_step(UnitType::Cavalry, Phase::Move);
        assert_eq!(game.units[blue].pos, Hex::new(-1, 0));
        assert_eq!(game.units[red].pos, center);
        assert!(!game.is_contested(center));
    }

    #[test]
    fn allies_swap_places_when_the_first_of_them_moves() {
        let mut game = GameState::new();
        // Adjacent in the starting layout; melee moves well before ranged.
        let melee = find(&game, Team::Blue, UnitType::Melee);
        let ranged = find(&game, Team::Blue, UnitType::Ranged);
        let (melee_start, ranged_start) = (game.units[melee].pos, game.units[ranged].pos);

        // Queuing the same swap twice cancels it; a third time requeues it.
        game.try_queue_swap(melee, ranged);
        game.try_queue_swap(melee, ranged);
        assert_eq!(game.swap_partner(melee), None);
        game.try_queue_swap(melee, ranged);
        assert_eq!(game.swap_partner(melee), Some(ranged));

        game.resolve_step(UnitType::Melee, Phase::Move);
        assert_eq!(game.units[melee].pos, ranged_start);
        assert_eq!(game.units[ranged].pos, melee_start);
        assert!(game.resolve_step(UnitType::Ranged, Phase::Move).is_empty());
    }

    #[test]
    fn allies_cannot_head_for_the_same_hex() {
        let mut game = GameState::new();
        let melee = find(&game, Team::Blue, UnitType::Melee);
        let cavalry = find(&game, Team::Blue, UnitType::Cavalry);
        let dest = Hex::new(-1, 0);

        game.try_queue_move(melee, dest);
        game.try_queue_move(cavalry, dest);
        assert_eq!(game.units[melee].planned_move, Some(dest));
        assert_eq!(game.units[cavalry].planned_move, None);
    }

    #[test]
    fn a_unit_can_move_into_a_hex_vacated_in_the_same_step() {
        let mut game = GameState::new();
        // Blue has the lower id, so under one-at-a-time resolution it would
        // have been blocked by Red, which hadn't moved yet.
        let (blue, red) = cavalry_moves(
            &mut game,
            (Hex::new(-1, 0), Hex::new(0, 0)),
            (Hex::new(0, 0), Hex::new(1, -1)),
        );
        assert!(game.units[blue].id < game.units[red].id);

        game.resolve_step(UnitType::Cavalry, Phase::Move);
        assert_eq!(game.units[blue].pos, Hex::new(0, 0));
        assert_eq!(game.units[red].pos, Hex::new(1, -1));
    }

    #[test]
    fn enemies_cannot_swap_places() {
        let mut game = GameState::new();
        let (blue, red) = cavalry_moves(
            &mut game,
            (Hex::new(-1, 0), Hex::new(0, 0)),
            (Hex::new(0, 0), Hex::new(-1, 0)),
        );

        game.resolve_step(UnitType::Cavalry, Phase::Move);
        assert_eq!(game.units[blue].pos, Hex::new(-1, 0));
        assert_eq!(game.units[red].pos, Hex::new(0, 0));
    }

    #[test]
    fn simultaneous_attacks_all_land_before_anyone_is_removed() {
        let mut game = GameState::new();
        let blue = find(&game, Team::Blue, UnitType::Melee);
        let red = find(&game, Team::Red, UnitType::Melee);
        for (idx, pos, target) in [
            (blue, Hex::new(-1, 0), Hex::new(0, 0)),
            (red, Hex::new(0, 0), Hex::new(-1, 0)),
        ] {
            let unit = &mut game.units[idx];
            (unit.pos, unit.planned_attack, unit.hp) = (pos, Some(target), 1.0);
        }

        // Either blow alone is lethal; both units still get theirs in.
        game.resolve_step(UnitType::Melee, Phase::Attack);
        assert!(game.units.iter().all(|u| u.unit_type != UnitType::Melee));
    }

    #[test]
    fn order_badges_follow_the_resolution_order() {
        use turn::step_rank;
        assert_eq!(step_rank(UnitType::Scout, Phase::Move), 1);
        assert_eq!(step_rank(UnitType::Cavalry, Phase::Move), 2);
        assert_eq!(step_rank(UnitType::Siege, Phase::Move), 7);
        assert_eq!(step_rank(UnitType::Ranged, Phase::Attack), 1);
        assert_eq!(step_rank(UnitType::Scout, Phase::Attack), 2);
        assert_eq!(step_rank(UnitType::Siege, Phase::Attack), 7);
        assert_eq!(step_rank(UnitType::Wolf, Phase::Move), 3);
        assert_eq!(step_rank(UnitType::Bear, Phase::Attack), 6);
    }

    /// Selects `idx` and toggles its ability, as the button would.
    fn toggle_ability(game: &mut GameState, idx: usize) {
        game.selected = Some(idx);
        game.toggle_selected_ability();
    }

    #[test]
    fn shield_wall_raises_defense_and_cancels_movement() {
        let mut game = GameState::new();
        let melee = find(&game, Team::Blue, UnitType::Melee);
        game.try_queue_move(melee, Hex::new(-1, 0));

        toggle_ability(&mut game, melee);
        let unit = &game.units[melee];
        assert_eq!(unit.planned_move, None);
        assert_eq!(unit.stats().defense, UnitType::Melee.stats().defense * 1.5);

        game.try_queue_move(melee, Hex::new(-1, 0));
        assert_eq!(game.units[melee].planned_move, None);
    }

    #[test]
    fn charge_extends_movement_until_it_is_turned_off() {
        let mut game = GameState::new();
        let cavalry = find(&game, Team::Blue, UnitType::Cavalry);
        // 3 steps away through the pass: out of reach without the charge bonus.
        let far = Hex::new(1, -1);

        toggle_ability(&mut game, cavalry);
        game.try_queue_move(cavalry, far);
        assert_eq!(game.units[cavalry].planned_move, Some(far));

        toggle_ability(&mut game, cavalry);
        assert_eq!(game.units[cavalry].planned_move, None);
    }

    #[test]
    fn volley_also_hits_enemies_next_to_the_target() {
        let mut game = GameState::new();
        let ranged = find(&game, Team::Blue, UnitType::Ranged);
        let (target, neighbor) = (
            find(&game, Team::Red, UnitType::Melee),
            find(&game, Team::Red, UnitType::Cavalry),
        );
        game.units[ranged].pos = Hex::new(-1, 0);
        game.units[target].pos = Hex::new(1, 0);
        game.units[neighbor].pos = Hex::new(1, 1);
        let ids = [game.units[target].id, game.units[neighbor].id];

        toggle_ability(&mut game, ranged);
        game.try_queue_attack(ranged, Hex::new(1, 0));
        game.resolve_step(UnitType::Ranged, Phase::Attack);

        for id in ids {
            let unit = game.units.iter().find(|u| u.id == id).unwrap();
            assert!(unit.hp < unit.max_hp(), "{unit} should have been hit");
        }
    }

    #[test]
    fn siege_spends_a_turn_deploying_then_gains_range() {
        let mut game = GameState::new();
        let siege = find(&game, Team::Blue, UnitType::Siege);

        toggle_ability(&mut game, siege);
        assert!(!game.units[siege].can_attack());

        game.units[siege].end_turn();
        let unit = &game.units[siege];
        assert!(unit.deployed && unit.can_attack());
        assert_eq!(
            unit.stats().attack_range,
            UnitType::Siege.stats().attack_range + 1
        );
        assert_eq!(unit.stats().move_range, 0);
    }

    #[test]
    fn abilities_go_on_cooldown_after_use() {
        let mut game = GameState::new();
        let ranged = find(&game, Team::Blue, UnitType::Ranged);

        toggle_ability(&mut game, ranged);
        game.units[ranged].end_turn();
        assert_eq!(game.units[ranged].ability_cooldown, 2);

        toggle_ability(&mut game, ranged);
        assert!(
            !game.units[ranged].ability_queued,
            "can't queue while cooling down"
        );

        game.units[ranged].end_turn();
        game.units[ranged].end_turn();
        toggle_ability(&mut game, ranged);
        assert!(game.units[ranged].ability_queued);
    }

    #[test]
    fn moving_and_attacking_selects_the_next_unit() {
        let mut game = GameState::new();
        let melee = find(&game, Team::Blue, UnitType::Melee);
        let ranged = find(&game, Team::Blue, UnitType::Ranged);
        assert_eq!(game.selected, Some(melee), "the first unit starts selected");

        // With no enemy nearby it stays selected after moving, so it can
        // still attack a square...
        game.try_queue_move(melee, Hex::new(-1, 0));
        game.advance_selection_if_done();
        assert_eq!(game.selected, Some(melee));

        // ...and moves on once it does.
        game.try_queue_attack(melee, Hex::new(0, 0));
        game.advance_selection_if_done();
        assert_eq!(game.selected, Some(ranged));
    }

    #[test]
    fn skipping_selects_the_next_unit_and_focuses_the_camera() {
        let mut game = GameState::new();
        let ranged = find(&game, Team::Blue, UnitType::Ranged);

        game.select_next_unit();
        assert_eq!(game.selected, Some(ranged));
        game.update(10.0);
        assert_eq!(game.camera.center, game.units[ranged].pos.to_world());
    }

    #[test]
    fn holding_keeps_orders_and_moves_on() {
        let mut game = GameState::new();
        let melee = find(&game, Team::Blue, UnitType::Melee);
        let ranged = find(&game, Team::Blue, UnitType::Ranged);

        game.try_queue_move(melee, Hex::new(-1, 0));
        game.hold_selected_unit();
        assert_eq!(game.selected, Some(ranged));
        assert_eq!(game.units[melee].planned_move, Some(Hex::new(-1, 0)));
        assert!(!game.is_resolving());
    }

    #[test]
    fn turn_waits_for_explicit_end_after_every_unit_has_acted() {
        let mut game = GameState::new();
        let blue_count = game.units.iter().filter(|u| u.team == Team::Blue).count();

        for _ in 1..blue_count {
            game.hold_selected_unit();
        }
        assert!(!game.is_resolving(), "one unit still to act");

        // The last unit finishes by queuing orders rather than holding.
        let last = game.selected.expect("last unit selected");
        let from = game.units[last].pos;
        let dest = from
            .neighbors()
            .into_iter()
            .find(|&h| game.grid.is_passable(h) && !game.is_occupied(h));
        game.try_queue_move(last, dest.expect("room to move"));
        game.try_queue_attack(last, from);
        game.advance_selection_if_done();
        assert!(!game.is_resolving());
        game.end_planning();
        assert!(game.is_resolving());

        // Playing the turn out starts the next one with the first unit selected.
        while game.is_resolving() {
            game.update(1.0);
        }
        assert!(game.units.iter().all(|u| !u.holding));
        assert!(game.selected.is_some());
    }

    #[test]
    fn space_holds_each_unit_then_ends_the_turn() {
        let mut game = GameState::new();
        let blue_count = game.units.iter().filter(|u| u.team == Team::Blue).count();
        for _ in 0..blue_count {
            game.hold_or_end_turn();
        }
        assert!(
            !game.is_resolving(),
            "holding the last unit doesn't end the turn"
        );
        assert_eq!(game.pending(), (0, 0));
        game.hold_or_end_turn();
        assert!(game.is_resolving());
    }

    #[test]
    fn guarding_lasts_across_turns_until_an_order() {
        let mut game = GameState::new();
        let melee = find(&game, Team::Blue, UnitType::Melee);
        assert_eq!(game.selected, Some(melee));
        game.toggle_guard();
        assert_ne!(game.selected, Some(melee), "guarding moves on");

        while game.pending() != (0, 0) {
            game.hold_or_end_turn();
        }
        game.hold_or_end_turn();
        while game.is_resolving() {
            game.update(1.0);
        }
        let melee = find(&game, Team::Blue, UnitType::Melee);
        assert!(game.units[melee].guarding, "still guarding next turn");
        assert!(!game.needs_orders(melee));

        // Giving it an order wakes it.
        game.selected = Some(melee);
        let open = game.units[melee]
            .pos
            .neighbors()
            .into_iter()
            .find(|&h| game.grid.is_passable(h) && !game.is_occupied(h))
            .unwrap();
        game.try_queue_move(melee, open);
        assert!(!game.units[melee].guarding);
    }

    #[test]
    fn a_queued_attack_draws_an_arc() {
        let mut game = GameState::new();
        let ranged = find(&game, Team::Blue, UnitType::Ranged);
        let before = game.build_vertices().len();
        game.try_queue_attack(ranged, Hex::new(0, 1));
        assert!(game.units[ranged].planned_attack.is_some());
        assert!(game.build_vertices().len() > before);

        // The AI's attacks aren't drawn.
        let with_ours = game.build_vertices().len();
        let enemy = find(&game, Team::Red, UnitType::Ranged);
        game.units[enemy].planned_attack = Some(Hex::new(0, 1));
        assert_eq!(game.build_vertices().len(), with_ours);
    }

    #[test]
    fn a_resolved_attack_animates_and_clears_its_arrow() {
        let mut game = GameState::new();
        let ranged = find(&game, Team::Blue, UnitType::Ranged);
        // An empty hex: the shot plays out as a miss.
        game.try_queue_attack(ranged, Hex::new(0, 1));
        game.resolve_step(UnitType::Ranged, Phase::Attack);
        assert_eq!(game.units[ranged].planned_attack, None);
        assert!(matches!(
            game.effects[..],
            [(
                effects::Effect::Shot {
                    outcome: effects::Outcome::Miss,
                    ..
                },
                _
            )]
        ));
    }

    #[test]
    fn turn_waits_for_city_builds_but_not_citizens() {
        let mut game = GameState::city_scenario();
        game.units.retain(|u| u.team != Team::Blue);
        game.selected = None;
        // A city with nothing to build; its worker idle at home doesn't
        // hold the turn up.
        assert_eq!(game.pending(), (0, 1));

        // Space opens the city that needs a build instead of ending the turn.
        game.hold_or_end_turn();
        assert!(!game.is_resolving());
        assert_eq!(game.selected_city, Some(0));

        game.queue_selected_city_unit(city::BuildUnit::Melee);
        assert_eq!(game.pending(), (0, 0));
        // Space closes the city, and then ends the turn.
        game.hold_or_end_turn();
        game.hold_or_end_turn();
        assert!(game.is_resolving());
    }

    #[test]
    fn empty_city_center_rejects_exterior_attacks() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        let target = game
            .cities
            .iter()
            .position(|city| city.team == Team::Red)
            .unwrap();
        let pos = game.cities[target].pos;
        game.units.push(Unit::new(
            900,
            pos.neighbors()[0],
            Team::Blue,
            UnitType::Ranged,
        ));
        game.try_queue_attack(0, pos);
        assert_eq!(game.units[0].planned_attack, None);
        assert!(!game.queue_attack(pos));
        game.group.push(0);
        game.group_order(pos, ClickMode::Attack);
        assert_eq!(game.units[0].planned_attack, None);
        game.group.clear();

        // An old or externally supplied order cannot damage the city either.
        game.units[0].planned_attack = Some(pos);
        game.resolve_step(UnitType::Ranged, Phase::Attack);
        assert!(game.effects.iter().any(|(effect, _)| matches!(
            effect,
            effects::Effect::Shot {
                outcome: effects::Outcome::Miss,
                ..
            }
        )));
        assert_eq!(game.units[0].hp, game.units[0].max_hp());
    }

    #[test]
    fn unit_on_city_center_remains_attackable() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        let city = game
            .cities
            .iter()
            .find(|city| city.team == Team::Red)
            .unwrap()
            .pos;
        game.units
            .push(Unit::new(901, city, Team::Red, UnitType::Melee));
        game.units.push(Unit::new(
            902,
            city.neighbors()[0],
            Team::Blue,
            UnitType::Ranged,
        ));
        game.try_queue_attack(1, city);
        assert_eq!(game.units[1].planned_attack, Some(city));
        game.resolve_step(UnitType::Ranged, Phase::Attack);
        assert!(game.units[0].hp < game.units[0].max_hp());
    }

    #[test]
    fn hitting_an_empty_enemy_barracks_is_a_hit_not_a_miss() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        // This test checks the attack outcome, so keep its distant shot visible.
        game.fog_of_war = false;
        let target = game
            .cities
            .iter()
            .position(|city| city.team == Team::Red)
            .unwrap();
        let city = game.cities[target].pos;
        let barracks = city.neighbors()[0];
        game.cities[target].barracks = Some(barracks);
        game.units.push(Unit::new(
            902,
            Hex::new(barracks.q, barracks.r + 2),
            Team::Blue,
            UnitType::Ranged,
        ));
        game.units[0].planned_attack = Some(barracks);
        game.resolve_step(UnitType::Ranged, Phase::Attack);
        let shots: Vec<_> = game
            .effects
            .iter()
            .filter_map(|(effect, _)| match effect {
                effects::Effect::Shot { outcome, .. } => Some(*outcome),
                _ => None,
            })
            .collect();
        assert_eq!(shots, [effects::Outcome::Hit]);
        assert!(game.cities[target].barracks_hp < city::BARRACKS_MAX_HP);
    }

    #[test]
    fn barracks_is_tanky_but_does_not_return_fire() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        let target = game
            .cities
            .iter()
            .position(|city| city.team == Team::Red)
            .unwrap();
        let barracks = game.cities[target].pos.neighbors()[0];
        game.cities[target].barracks = Some(barracks);
        game.units.push(Unit::new(
            901,
            barracks.neighbors()[0],
            Team::Blue,
            UnitType::Ranged,
        ));
        let barracks_hp = game.cities[target].barracks_hp;
        let unit_hp = game.units[0].hp;
        game.try_queue_attack(0, barracks);
        game.resolve_step(UnitType::Ranged, Phase::Attack);
        assert!(game.cities[target].barracks_hp < barracks_hp);
        assert_eq!(game.units[0].hp, unit_hp);
    }
}
