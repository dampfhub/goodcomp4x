//! Hex combat sandbox: game state and the queries shared across its parts.
//! Player input lives in `orders`, turn resolution in `turn`, the AI in `ai`,
//! world geometry in `draw`, and the screen-space UI in `ui`.

mod ability;
mod ai;
mod camera;
mod city;
mod combat;
mod draw;
mod effects;
mod fog;
mod font;
mod group;
mod hex;
mod mapgen;
mod mesh;
mod orders;
mod scenario;
#[cfg(test)]
mod simulation;
mod terrain;
mod turn;
mod ui;
mod unit;

use std::collections::{HashSet, VecDeque};

use glam::Vec2;
use rand::rngs::ThreadRng;

pub use camera::Camera;
pub use city::{BuildUnit, Building};
pub use font::atlas as font_atlas;
use hex::{HEX_SIZE, Hex, HexGrid};
pub use orders::ClickMode;
pub use scenario::Scenario;
use terrain::Tile;
use turn::Phase;
pub use ui::{quit_prompt, selection_box, ui_projection};
use unit::{Team, Unit, UnitType};

const GRID_RADIUS: i32 = 3;
const PLAYER_TEAM: Team = Team::Blue;
const AI_TEAM: Team = Team::Red;

const CONTROLS_HELP: &str = "\
Controls:
  Click a unit to select it.
  Click a green hex to queue a move; click it again to cancel.
  Click an enemy in range to queue an attack on it.
  Shift-click any hex in range to attack that square instead: whoever stands there when the attack resolves gets hit.
  A unit can queue a move and an attack; it attacks from its new hex.
  Ctrl-click an adjacent ally to swap places with it.
  Alt-drag a box (or Alt-click units) to select a group: clicking a hex sends each member as
  close to it as it can get, and clicking an enemy has every member in range attack it.
  Queued attacks are drawn as arrows from the attacker (or its ghost) to the target.
  Units can't move through occupied hexes, and two allies can't head for the same hex.
  Space holds the selected unit: it keeps any orders already queued and skips the rest.
  G guards it instead: it stays put and is skipped every turn until given an order.
  Ctrl-right-click clears the selected unit's orders (and a hold or guard).
  Once a unit has queued a move and an attack (or can't do one of them), the next unit is
  selected automatically and the camera glides to it. Tab looks at the next unit without
  holding this one.
  The End Turn button holds unfinished units and ends the turn. Cities still need a build queued.
  C selects your city. Click tiles to assign or release citizens. A auto-assigns. Y shows yields.
  Rest the cursor on any hex for a moment to see what it is and yields.
  1-4 queue city units; 5-8 queue buildings. Drag queue rows to reorder or click X to remove;
  Backspace removes the active city build and PageDown promotes the next item. F founds with a settler.
  F1 combat, F2 cities, F3 settler frontier, F4 random world (again to restart; F4 makes a new map). F6 saves a snapshot, F7 loads it, F8 changes playback, F9 completes production, F10 toggles fog of war.
  The faded DEBUG panel at the top-left has buttons for these.
  Scroll to zoom, left-drag or middle-drag to pan. Clicks act on release; dragging does not issue orders.
  F5 toggles fullscreen.
Turn order:
  Each unit's blue number is when it moves and its red number when it attacks (1 = first).
  Units of the same type act simultaneously; simultaneous attacks all land before anyone is removed.
  A swap happens when whichever of the two allies moves first acts.
Abilities (button at the bottom of the screen, or Q, for the selected unit):
  Melee - Shield Wall: +50% defense this turn, but no moving.
  Ranged - Volley: the attack also hits enemies next to the target, all at 60% damage.
  Cavalry - Charge: +1 move and +50% attack this turn.
  Siege - Deploy: spend a turn setting up, then +1 range but no moving until packed up.
  Scout - Lookout: stay put this turn, then see 2 hexes farther through the next.
  A gold ring marks a queued ability; a steel ring marks deployed siege.
Contested hexes:
  Two enemies moving onto the same hex both take it, turning it orange. Every turn they
  fight there instead of attacking anything else, until one dies or moves out.
Terrain:
  Tiles are a ground (grassland, plains, desert, tundra, snow, marsh), optionally with hills
  (small peaks: +25% defense) and forest or jungle (pines or round canopies: +15%).
  Mountains (large snowy peak): impassable. Water (waves): units can't enter, cities can work it.
  Each yields differently; rest the cursor on a hex to see. Blue lines between hexes are
  rivers: land beside a river or lake has fresh water, +1 food.
Fog of war:
  Hexes you have never seen are blank. Hexes seen before but out of sight now are greyed and
  show what was there when you last saw them: enemy units, cities, improvements and roads. Units see 2 hexes (scouts and cavalry 3, +1 on hills), cities 3. F10 toggles it.";

#[derive(Clone)]
pub struct GameState {
    cities: Vec<city::City>,
    sites: std::collections::HashMap<Hex, city::Site>,
    roads: HashSet<Hex>,
    selected_city: Option<usize>,
    /// Barracks have their own production screen, separate from city labor.
    selected_barracks: Option<usize>,
    /// City whose manager has been picked up and awaits a destination click.
    moving_manager: Option<usize>,
    /// City and building whose site is being chosen.
    placing_building: Option<(usize, city::Building)>,
    hovered_city: Option<usize>,
    /// Whether the open city shows each tile's yields (Y toggles it).
    show_yields: bool,
    /// The map hex under the cursor (not over the UI), and how long the
    /// cursor has rested on it, for the tile tooltip.
    hovered_tile: Option<Hex>,
    hover_seconds: f32,
    ui_click_mode: Option<orders::ClickMode>,
    inspected_tile: Option<Hex>,
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
    /// Attack animations playing out, with how many seconds each has run.
    effects: Vec<(effects::Effect, f32)>,
    /// Debug setting (F8): play a turn's steps all at once instead of one
    /// every `STEP_INTERVAL`. Kept across scenario switches and loads.
    instant_playback: bool,
    /// Debug setting (F10): hide what the player's side can't see (`fog.rs`).
    /// Kept across scenario switches and loads.
    fog_of_war: bool,
    /// Every hex the player's side has seen, as it last saw it.
    memory: fog::Memory,
    turn: u32,
    pub camera: Camera,
    rng: ThreadRng,
    /// Steps of the turn currently playing out, drained one at a time by `update`.
    pending_steps: VecDeque<(UnitType, Phase)>,
    step_timer: f32,
    /// Units that acted in the latest step, highlighted until `highlight_timer` runs out.
    recent_actors: Vec<u32>,
    highlight_timer: f32,
    /// Unit ids that may found a city. They use the melee placeholder body for now.
    settlers: HashSet<u32>,
    workers: HashSet<u32>,
    /// Frontier sandbox units that the player may command despite being Red.
    player_controlled_units: HashSet<u32>,
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

        log::info!("You control {PLAYER_TEAM:?}; {AI_TEAM:?} is AI-controlled.\n{CONTROLS_HELP}");

        let mut game = Self {
            cities: Vec::new(),
            sites: std::collections::HashMap::new(),
            roads: HashSet::new(),
            selected_city: None,
            selected_barracks: None,
            moving_manager: None,
            placing_building: None,
            hovered_city: None,
            show_yields: true,
            hovered_tile: None,
            hover_seconds: 0.0,
            ui_click_mode: None,
            inspected_tile: None,
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
            effects: Vec::new(),
            instant_playback: true,
            fog_of_war: true,
            memory: fog::Memory::new(),
            turn: 0,
            camera: Camera::new(Vec2::ZERO, (GRID_RADIUS as f32 + 1.5) * HEX_SIZE),
            rng: rand::rng(),
            pending_steps: VecDeque::new(),
            step_timer: 0.0,
            recent_actors: Vec::new(),
            highlight_timer: 0.0,
            settlers: HashSet::new(),
            workers: HashSet::new(),
            player_controlled_units: HashSet::new(),
            next_unit_id: 8,
        };
        game.select_next_or_end_turn(None);
        game
    }

    pub fn city_scenario() -> Self {
        let mut game = Self::new();
        game.scenario = Scenario::Cities;
        game.setup_cities();
        game.start_on_whole_map();
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

    /// A generated map (`mapgen.rs`) from `seed`, with a settler, a worker
    /// and a scout per side.
    pub fn world_scenario(seed: u32) -> Self {
        let mut game = Self::new();
        game.scenario = Scenario::World;
        game.units.clear();
        game.setup_world(seed);
        game.start_on_whole_map();
        game.selected = game.unit_of_team_at(game.units[0].pos, PLAYER_TEAM);
        // The map is too big to take in at once: start on the settler.
        game.camera = Camera::new(game.units[0].pos.to_world(), game.camera.half_height);
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

    fn is_player_controlled(&self, idx: usize) -> bool {
        self.units[idx].team == PLAYER_TEAM
            || self.player_controlled_units.contains(&self.units[idx].id)
    }

    fn controlled_unit_at(&self, hex: Hex) -> Option<usize> {
        self.units_at(hex).find(|&i| self.is_player_controlled(i))
    }

    /// What the unit is, for display: settlers and workers are marked on
    /// top of an ordinary unit type.
    fn unit_role(&self, unit: &Unit) -> (&'static str, char) {
        if self.settlers.contains(&unit.id) {
            ("SETTLER", 'T')
        } else if self.workers.contains(&unit.id) {
            ("WORKER", 'W')
        } else {
            let name = match unit.unit_type {
                UnitType::Melee => "MELEE",
                UnitType::Ranged => "RANGED",
                UnitType::Cavalry => "CAVALRY",
                UnitType::Siege => "SIEGE",
                UnitType::Scout => "SCOUT",
                UnitType::Horse => "HORSE",
                UnitType::Armored => "ARMORED",
            };
            (name, unit.unit_type.letter())
        }
    }

    /// How to draw `unit`: its letter, and hollow if it's a civilian.
    fn unit_look(&self, unit: &Unit) -> draw::UnitLook {
        draw::UnitLook {
            letter: self.unit_role(unit).1,
            civilian: self.settlers.contains(&unit.id) || self.workers.contains(&unit.id),
        }
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

    fn enemy_city_at(&self, hex: Hex, team: Team) -> Option<usize> {
        self.cities
            .iter()
            .position(|city| city.team != team && city.pos == hex && city.hp > 0.0)
    }

    fn enemy_barracks_at(&self, hex: Hex, team: Team) -> Option<usize> {
        self.cities.iter().position(|city| {
            city.team != team && city.barracks == Some(hex) && city.barracks_hp > 0.0
        })
    }

    fn has_enemy_target_at(&self, hex: Hex, team: Team) -> bool {
        self.enemy_of_team_at(hex, team).is_some()
            || self.enemy_city_at(hex, team).is_some()
            || self.enemy_barracks_at(hex, team).is_some()
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

    /// Hexes reachable from `start` in at most `move_range` steps without
    /// passing through mountains or an occupied hex, so a line of units
    /// blocks the way. Includes `start` itself.
    fn reachable_hexes(&self, start: Hex, move_range: i32) -> HashSet<Hex> {
        let mut visited = HashSet::from([start]);
        let mut frontier = vec![start];

        for _ in 0..move_range {
            let mut next = Vec::new();
            for hex in frontier {
                for neighbor in hex.neighbors() {
                    let open = self.grid.is_passable(neighbor) && !self.is_occupied(neighbor);
                    if open && visited.insert(neighbor) {
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

    /// The starting layout has exactly one unit of each type per team.
    fn find(game: &GameState, team: Team, unit_type: UnitType) -> usize {
        game.units
            .iter()
            .position(|u| u.team == team && u.unit_type == unit_type)
            .unwrap()
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

        game.try_queue_move(cavalry, Hex::new(0, -2));
        assert_eq!(game.units[cavalry].planned_move, None);
        game.try_queue_move(cavalry, Hex::new(1, -3));
        assert_eq!(game.units[cavalry].planned_move, None);
    }

    #[test]
    fn hills_reduce_damage_taken() {
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        let hill = Hex::new(1, 0);
        let plain = Hex::new(-1, 0);
        let grid = HexGrid::new(GRID_RADIUS, [(hill, Tile::HILLS)]);

        // Same seed on both sides so both attacks roll the same variance.
        let damage_taken_at = |pos: Hex| {
            let siege = Unit::new(0, Hex::new(0, 0), Team::Blue, UnitType::Siege);
            let defender = Unit::new(1, pos, Team::Red, UnitType::Melee);
            combat::roll_damage(&siege, &defender, &grid, &mut StdRng::seed_from_u64(7))
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
        assert_eq!(step_rank(UnitType::Siege, Phase::Move), 5);
        assert_eq!(step_rank(UnitType::Ranged, Phase::Attack), 1);
        assert_eq!(step_rank(UnitType::Scout, Phase::Attack), 2);
        assert_eq!(step_rank(UnitType::Siege, Phase::Attack), 5);
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
        assert_eq!(game.pending(), (0, 1));

        // Space opens the city that needs a build instead of ending the turn.
        game.hold_or_end_turn();
        assert!(!game.is_resolving());
        assert_eq!(game.selected_city, Some(0));

        game.queue_selected_city_unit(city::BuildUnit::Melee);
        assert_eq!(game.pending(), (0, 0));
        game.hold_or_end_turn();
        assert!(game.is_resolving());
    }

    #[test]
    fn city_is_a_tanky_ranged_target_that_returns_fire() {
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
        let city_hp = game.cities[target].hp;
        let unit_hp = game.units[0].hp;
        game.try_queue_attack(0, pos);
        game.resolve_step(UnitType::Ranged, Phase::Attack);
        assert!(game.cities[target].hp < city_hp);
        assert!(
            game.units[0].hp < unit_hp,
            "the city should return ranged fire"
        );
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
