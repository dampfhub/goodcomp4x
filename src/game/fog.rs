//! Fog of war, in three layers:
//! - in sight: hexes a side's units, cities, barracks, outposts and
//!   workers out on the map see now, and the tiles its cities work,
//!   shown as they are;
//! - remembered: hexes seen before but out of sight now, shown under a dark
//!   tint as they were when last seen (`Sighting`): cities, barracks,
//!   improvements, roads, structures, and other sides' construction
//!   (`SeenJob`). Units and workers move, so they aren't remembered: out of
//!   sight, none is known to be anywhere;
//! - unexplored: never seen, covered by clouds (`push_cloud_banks`, `draw.rs`).
//!
//! Every side sees and remembers by the same rules, in one of two memories:
//! - the player's (`fog()`, `GameState::memory`): this machine's view of
//!   its side, updated every frame (`explore`), and a debug setting (F10)
//!   turns the fog off for it;
//! - each side's own (`side_fog`, `GameState::side_memory`): game state, the
//!   same on every machine, brought up to date only as the AI plans that
//!   side's turn, and untouched by F10 or by which side this machine plays.
//!   The AI plans on it and nothing else, so it plays under the same fog as
//!   the player.
//!
//! A `Fog` says which memory it recalls from, so every `known_*` query below
//! answers for either.

use std::sync::Arc;

use super::GameState;
use super::city::{BARRACKS_MAX_HP, Building, Routes};
use super::fast_hash::{HashMap, HashSet};
use super::hex::{Hex, edge};
use super::terrain::Terrain;
use super::unit::{Team, Unit};
use super::workers::{
    FieldWorker, OUTPOST_SIGHT, Structure, StructureKind, WORKER_SIGHT, WorkerJob,
};

/// How far a city sees, and a barracks.
const CITY_SIGHT: i32 = 3;
const BARRACKS_SIGHT: i32 = 1;
const WATCHPOST_SIGHT: i32 = 4;
/// Extra sight for a unit standing on hills.
const HILLS_SIGHT: i32 = 1;
/// Extra sight for a scout after a turn on lookout.
pub(super) const LOOKOUT_SIGHT: i32 = 2;
/// How far a line of sight is shifted to settle which side of an edge it
/// passes (see `Hex::line_between`).
const SIGHT_NUDGE: f32 = 1e-4;

/// What one side can see now, and where it recalls what it saw before.
pub(super) struct Fog {
    /// Hexes in sight, or `None` with the fog turned off.
    visible: Option<HashSet<Hex>>,
    /// Whose sight it is: the side whose own units always show.
    team: Team,
    /// Which memory holds the hexes out of sight.
    recall: Recall,
}

/// Where a `Fog` recalls hexes out of sight from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Recall {
    /// The player's memory at this machine (`GameState::memory`).
    Player,
    /// The side's own memory in the game state (`GameState::side_memory`).
    Side,
}

impl Fog {
    /// Whether `hex` is in sight (always, with the fog off).
    pub fn sees(&self, hex: Hex) -> bool {
        self.visible.as_ref().is_none_or(|v| v.contains(&hex))
    }

    /// Whether to show `unit`: the local side's own always, others in sight.
    pub fn shows(&self, unit: &Unit) -> bool {
        unit.team == self.team || self.sees(unit.pos)
    }

    /// Whether to show a worker out on the map, by the same rule.
    pub fn shows_worker(&self, worker: &FieldWorker) -> bool {
        worker.team == self.team || self.sees(worker.pos)
    }

    /// Whose sight it is.
    pub fn team(&self) -> Team {
        self.team
    }
}

/// A hex as the player last saw it. Only what can change is kept; the
/// ground itself never does. Units aren't kept: a sighting of one says
/// nothing about where it is now.
#[derive(Clone, Default)]
pub(super) struct Sighting {
    pub city: Option<SeenBuilding>,
    pub barracks: Option<SeenBuilding>,
    /// An improvement's label and owner.
    pub site: Option<(&'static str, Team)>,
    pub road: bool,
    /// An outpost or fort.
    pub structure: Option<Structure>,
    /// Walls and gates on the hex's edges, by the neighbor across each.
    pub barriers: Vec<(Hex, Structure)>,
    /// The tile's food, wood and metal, with any improvement or city.
    pub yields: (i32, i32, i32),
    /// Unclaimed ruins (`ruins.rs`).
    pub ruin: bool,
    /// What other sides' workers standing here were building: on the
    /// tile, or a wall or gate on one of its edges.
    pub construction: Vec<SeenJob>,
}

/// Another side's construction: a job one of its workers is at work on
/// (standing at it, with work left). Only what's on the ground: not the jobs
/// it has queued or is walking to, nor how long the work will take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SeenJob {
    pub job: WorkerJob,
    pub team: Team,
    /// The job's name, like "BARRACKS" or "FARM" (`job_name`).
    pub name: &'static str,
}

#[derive(Clone, Copy)]
pub(super) struct SeenBuilding {
    pub team: Team,
    /// The city's number, from 0.
    pub id: u32,
    /// Share of full health, 0 to 1.
    pub health: f32,
    /// For a city: its population.
    pub population: usize,
}

impl GameState {
    /// What the player sees this frame, recalling from the player's memory.
    pub(super) fn fog(&self) -> Fog {
        Fog {
            visible: self.fog_of_war.then(|| self.visible_hexes()),
            team: self.local_team,
            recall: Recall::Player,
        }
    }

    /// What `team` sees now, recalling from its own memory, which this
    /// first brings up to date with everything in sight. It depends on
    /// nothing but the game (not on F10, nor on the side this machine
    /// plays), so every machine builds the same one. The AI plans each
    /// side's turn on it.
    pub(super) fn side_fog(&mut self, team: Team) -> Fog {
        let visible = self.side_sight(team);
        let sightings: Vec<(Hex, Sighting)> = visible
            .iter()
            .map(|&hex| (hex, self.sighting(hex, team)))
            .collect();
        // Shared with any copy of the game (a savestate, a network turn's
        // start) until it changes.
        Arc::make_mut(&mut self.side_memory[team.index()]).extend(sightings);
        Fog {
            visible: Some(visible),
            team,
            recall: Recall::Side,
        }
    }

    /// Whether the player has ever seen `hex` (always, with the fog off).
    pub(super) fn is_explored(&self, hex: Hex) -> bool {
        !self.fog_of_war || self.memory.contains_key(&hex)
    }

    /// How `hex` looked when the player last saw it, if they ever did.
    pub(super) fn remembered(&self, hex: Hex) -> Option<&Sighting> {
        self.memory.get(&hex)
    }

    /// The memory `fog` recalls from.
    pub(super) fn memory_of(&self, fog: &Fog) -> &Memory {
        match fog.recall {
            Recall::Player => &self.memory,
            Recall::Side => &self.side_memory[fog.team.index()],
        }
    }

    /// How `hex` looked when `fog`'s side last saw it, if it ever did.
    pub(super) fn recalled(&self, hex: Hex, fog: &Fog) -> Option<&Sighting> {
        self.memory_of(fog).get(&hex)
    }

    /// `is_explored` for `fog`'s side: whether it has seen `hex` (always,
    /// with the fog off). Its memory holds what's in sight too, as seen
    /// when it was last brought up to date (`explore`, `side_fog`).
    pub(super) fn explored_by(&self, hex: Hex, fog: &Fog) -> bool {
        fog.visible.is_none() || self.memory_of(fog).contains_key(&hex)
    }

    /// How far `unit` sees: its type's sight, more from hills or after a
    /// turn on lookout.
    pub(super) fn sight(&self, unit: &Unit) -> i32 {
        let hills = if self.grid.tile(unit.pos).hills {
            HILLS_SIGHT
        } else {
            0
        };
        let lookout = if unit.lookout { LOOKOUT_SIGHT } else { 0 };
        unit.unit_type.sight() + hills + lookout
    }

    /// Whether `from` can see `to`: no mountain stands on the line between
    /// them. A mountain itself can be seen, just not past. Where the line
    /// runs along the edge between two hexes, either side being clear will do.
    fn in_line_of_sight(&self, from: Hex, to: Hex) -> bool {
        [SIGHT_NUDGE, -SIGHT_NUDGE].into_iter().any(|nudge| {
            from.line_between(to, nudge)
                .all(|h| self.grid.terrain(h) != Terrain::Mountains)
        })
    }

    /// Every hex the player's side sees now (`sight_of`), with what the
    /// units the player commands of another side (the Frontier sandbox's) see.
    fn visible_hexes(&self) -> HashSet<Hex> {
        self.sight_of(self.local_team, |idx| self.is_player_controlled(idx))
    }

    /// Every hex `team` sees now (`sight_of`), from its own units.
    fn side_sight(&self, team: Team) -> HashSet<Hex> {
        self.sight_of(team, |idx| self.units[idx].team == team)
    }

    /// Every hex seen now by the units `looks` picks and by `team`'s
    /// cities, barracks, watchposts, outposts and workers, and every tile a
    /// city of `team` works, whatever the range or mountains: an enemy
    /// standing on one of a side's tiles is always seen. That includes a
    /// tile the city was working until a cut route took its citizen off it
    /// (`remembered_worked`), so an enemy that ends a turn on a worked tile
    /// doesn't vanish when the city reassigns the citizen.
    fn sight_of(&self, team: Team, looks: impl Fn(usize) -> bool) -> HashSet<Hex> {
        let mut seen = HashSet::default();
        let mut look = |from: Hex, range: i32| {
            for dq in -range..=range {
                for dr in (-range).max(-dq - range)..=range.min(-dq + range) {
                    let hex = Hex::new(from.q + dq, from.r + dr);
                    if self.grid.contains(hex) && self.in_line_of_sight(from, hex) {
                        seen.insert(hex);
                    }
                }
            }
        };
        for (idx, unit) in self.units.iter().enumerate() {
            if looks(idx) {
                look(unit.pos, self.sight(unit));
            }
        }
        for city in self.cities.iter().filter(|c| c.team == team) {
            look(city.pos, CITY_SIGHT);
            if let Some(barracks) = city.barracks {
                look(barracks, BARRACKS_SIGHT);
            }
            if let Some(post) = city.placed_site(Building::Watchpost) {
                look(
                    post,
                    WATCHPOST_SIGHT + i32::from(self.grid.tile(post).hills),
                );
            }
        }
        for (&hex, structure) in &self.structures {
            if structure.team == team && structure.kind == StructureKind::Outpost {
                look(hex, OUTPOST_SIGHT);
            }
        }
        for worker in self.field_workers.iter().filter(|w| w.team == team) {
            look(worker.pos, WORKER_SIGHT);
        }
        for city in self.cities.iter().filter(|c| c.team == team) {
            seen.extend(city.worked.iter().chain(&city.remembered_worked).copied());
        }
        seen
    }

    /// `hex` as it is right now, for `viewer`'s memory.
    fn sighting(&self, hex: Hex, viewer: Team) -> Sighting {
        let city = self
            .cities
            .iter()
            .find(|c| c.pos == hex)
            .map(|c| SeenBuilding {
                team: c.team,
                id: c.id,
                health: 1.0,
                population: c.population,
            });
        let barracks = self
            .cities
            .iter()
            .find(|c| c.barracks == Some(hex))
            .map(|c| SeenBuilding {
                team: c.team,
                id: c.id,
                health: c.barracks_hp / BARRACKS_MAX_HP,
                population: 0,
            });
        Sighting {
            city,
            barracks,
            site: self.sites.get(&hex).map(|s| (s.label, s.team)),
            road: self.roads.contains(&hex),
            structure: self.structures.get(&hex).copied(),
            barriers: hex
                .neighbors()
                .into_iter()
                .filter_map(|n| Some((n, *self.barriers.get(&edge(hex, n))?)))
                .collect(),
            yields: self.raw_yield(hex),
            ruin: self.ruin_at(hex).is_some(),
            construction: self
                .field_workers
                .iter()
                .filter(|w| w.pos == hex)
                .filter_map(|w| self.others_construction(w, viewer))
                .collect(),
        }
    }

    /// The job `worker` is at work on, if it belongs to another side than
    /// `viewer`: what `viewer` may see of it.
    pub(super) fn others_construction(
        &self,
        worker: &FieldWorker,
        viewer: Team,
    ) -> Option<SeenJob> {
        if worker.team == viewer || worker.recalled || worker.work_left.is_none() {
            return None;
        }
        let job = worker.job.filter(|job| job.hex == worker.pos)?;
        Some(SeenJob {
            job,
            team: worker.team,
            name: self.job_name(job),
        })
    }

    // What the player knows: a hex in sight as it is, one out of sight as last
    // seen. Planning and drawing for the player, and the AI's planning, go
    // through these, so nothing out of sight gives away what's really there.
    // "The player" below is whichever side `fog` is for.

    /// Whether the player knows of a unit on `hex`: only in sight, since
    /// units aren't remembered.
    pub(super) fn known_occupied(&self, hex: Hex, fog: &Fog) -> bool {
        fog.sees(hex) && self.is_occupied(hex)
    }

    /// Whether the player knows of something on `hex` that `team` can attack:
    /// an enemy unit in sight, or an enemy barracks in sight or remembered.
    pub(super) fn known_enemy_target_at(&self, hex: Hex, team: Team, fog: &Fog) -> bool {
        if fog.sees(hex) {
            return self.has_enemy_target_at(hex, team);
        }
        self.recalled(hex, fog)
            .is_some_and(|seen| seen.barracks.is_some_and(|barracks| barracks.team != team))
    }

    /// `empty_city_target` as the player knows the board: a city center in
    /// sight with nobody on it to attack. One out of sight never counts, since
    /// the player can't know that nobody stands there (units aren't
    /// remembered), and refusing an attack on it would say so.
    pub(super) fn known_empty_city_target(&self, hex: Hex, team: Team, fog: &Fog) -> bool {
        fog.sees(hex) && self.empty_city_target(hex, team)
    }

    /// A tile's food, wood and metal as the player knows them.
    pub(super) fn known_yield(&self, hex: Hex, fog: &Fog) -> (i32, i32, i32) {
        match self.recalled(hex, fog) {
            Some(seen) if !fog.sees(hex) => seen.yields,
            _ => self.raw_yield(hex),
        }
    }

    /// Whether the player knows of nothing on `hex` that stops a unit
    /// entering it: on the map, and never seen (the player can't know what's
    /// there), or seen to be land a unit can walk (for a ship, water). Terrain
    /// never changes, so a hex seen once is known for good.
    pub(super) fn known_passable(&self, hex: Hex, naval: bool, fog: &Fog) -> bool {
        self.grid.contains(hex)
            && (!self.explored_by(hex, fog)
                || if naval {
                    self.grid.terrain(hex).is_water()
                } else {
                    self.can_enter(hex)
                })
    }

    /// The hexes a player-controlled unit of `team` at `start` can plan to
    /// reach, going around the terrain, units, walls and gates the player
    /// knows of.
    pub(super) fn known_reachable_hexes(
        &self,
        start: Hex,
        move_range: i32,
        team: Team,
        fog: &Fog,
    ) -> HashSet<Hex> {
        self.known_reachable_for_domain(start, move_range, team, fog, false)
    }

    pub(super) fn known_reachable_for_domain(
        &self,
        start: Hex,
        move_range: i32,
        team: Team,
        fog: &Fog,
        naval: bool,
    ) -> HashSet<Hex> {
        self.known_reachable_past(start, move_range, team, fog, naval, |hex| {
            self.known_occupied(hex, fog)
        })
    }

    /// `known_reachable_for_domain`, with `blocked` saying which hexes
    /// the units on them close.
    pub(super) fn known_reachable_past(
        &self,
        start: Hex,
        move_range: i32,
        team: Team,
        fog: &Fog,
        naval: bool,
        blocked: impl Fn(Hex) -> bool,
    ) -> HashSet<Hex> {
        let mut reachable = self.reachable_hexes_by(start, move_range, |from, to| {
            self.known_step(from, to, team, fog, naval) && !blocked(to)
        });
        if move_range > 0 && !naval {
            for city in self.cities.iter().filter(|city| city.team == team) {
                if let Some(dest) = city.placed_site(Building::Railhead)
                    && !blocked(dest)
                    && self.rail_transfer_available(start, dest, team, Some(fog))
                {
                    reachable.insert(dest);
                }
            }
        }
        reachable
    }

    /// `can_step` as the player knows the board: whether a unit of `team`
    /// (a ship, if `naval`) may step from `from` onto the adjacent `to`,
    /// going by the terrain, enemy city centers, walls and gates the player
    /// knows of. Units aside, as they'll have moved by the time anyone
    /// gets there.
    pub(super) fn known_step(
        &self,
        from: Hex,
        to: Hex,
        team: Team,
        fog: &Fog,
        naval: bool,
    ) -> bool {
        self.known_passable(to, naval, fog)
            && !self.known_enemy_city_at(to, team, fog)
            && self.known_can_cross(from, to, team, fog)
    }

    /// Whether the player knows of a city of another side than `team` on
    /// `hex`: in sight, or remembered there.
    pub(super) fn known_enemy_city_at(&self, hex: Hex, team: Team, fog: &Fog) -> bool {
        if fog.sees(hex) {
            return self
                .cities
                .iter()
                .any(|city| city.pos == hex && city.team != team);
        }
        self.recalled(hex, fog)
            .is_some_and(|seen| seen.city.is_some_and(|city| city.team != team))
    }

    /// The wall or gate the player knows of on the edge between adjacent
    /// `a` and `b`: the real one when either side is in sight, else as
    /// last seen.
    pub(super) fn known_barrier(&self, a: Hex, b: Hex, fog: &Fog) -> Option<Structure> {
        if fog.sees(a) || fog.sees(b) {
            return self.barriers.get(&edge(a, b)).copied();
        }
        let remembered = |from: Hex, across: Hex| {
            self.recalled(from, fog).and_then(|seen| {
                seen.barriers
                    .iter()
                    .find(|&&(n, _)| n == across)
                    .map(|&(_, barrier)| barrier)
            })
        };
        remembered(a, b).or_else(|| remembered(b, a))
    }

    /// Whether the player knows of nothing stopping `team` crossing from
    /// `from` to the adjacent `to`.
    pub(super) fn known_can_cross(&self, from: Hex, to: Hex, team: Team, fog: &Fog) -> bool {
        self.known_barrier(from, to, fog)
            .is_none_or(|barrier| barrier.admits(team))
    }

    /// City `city`'s delivery routes as the player knows the board: what
    /// the yield badges, tooltips and city panel show. Income still follows
    /// the real routes (`routes`).
    pub(super) fn known_routes(&self, city: usize, fog: &Fog) -> Routes {
        self.known_routes_from(self.cities[city].team, self.cities[city].pos, fog)
    }

    /// `routes_from` as the player knows the board.
    pub(super) fn known_routes_from(&self, team: Team, origin: Hex, fog: &Fog) -> Routes {
        // Out of sight, the memory (none for a hex never seen); in sight, the board.
        let memory = |hex: Hex| (!fog.sees(hex)).then(|| self.recalled(hex, fog));
        let river_banks = self.navigable_river_banks(team);
        self.routes_from_by(
            origin,
            |hex| match memory(hex) {
                Some(seen) => seen.is_some_and(|seen| seen.city.is_some_and(|c| c.team != team)),
                None => {
                    self.enemy_of_team_at(hex, team).is_some()
                        || self.cities.iter().any(|c| c.pos == hex && c.team != team)
                }
            },
            |from, to| self.known_can_cross(from, to, team, fog),
            |from, to| {
                river_banks.contains(&from) && river_banks.contains(&to)
                    || match memory(to) {
                        Some(seen) => seen.is_some_and(|seen| seen.road || seen.city.is_some()),
                        None => self.is_road_hex(to),
                    }
            },
        )
    }

    /// Records everything in sight as the player's latest memory of it.
    /// Called every frame.
    pub(super) fn explore(&mut self) {
        if !self.fog_of_war {
            return;
        }
        for hex in self.visible_hexes() {
            let sighting = self.sighting(hex, self.local_team);
            self.memory.insert(hex, sighting);
        }
    }

    /// F10: turns the fog of war on or off.
    pub fn toggle_fog(&mut self) {
        self.fog_of_war = !self.fog_of_war;
        self.explore();
        self.notice = if self.fog_of_war {
            "FOG OF WAR ON - F10 TO LIFT IT".into()
        } else {
            "FOG OF WAR OFF - F10 TO BRING IT BACK".into()
        };
    }
}

/// A side's memory of the map: every hex ever seen, as last seen.
pub(super) type Memory = HashMap<Hex, Sighting>;

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::game::PLAYER_TEAM;
    use crate::game::city::Site;
    use crate::game::hex::HexGrid;
    use crate::game::orders::ClickMode;
    use crate::game::terrain::Tile;
    use crate::game::unit::{Team, UnitType};

    #[test]
    fn units_see_around_them_and_explored_hexes_stay_explored() {
        let mut game = GameState::solo_world(3);
        game.explore();
        let scout = game
            .units
            .iter()
            .find(|u| u.team == Team::Blue && u.unit_type == UnitType::Scout)
            .unwrap();
        let (start, sight) = (scout.pos, game.sight(scout));
        assert!(sight >= 3, "scouts see farther");
        let far = Hex::new(start.q + sight, start.r);
        let fog = game.fog();
        assert!(fog.sees(start) && game.is_explored(start));
        assert!(fog.sees(far) || !game.grid.contains(far));
        let distant = game
            .grid
            .all_hexes()
            .find(|h| {
                game.units
                    .iter()
                    .all(|u| u.team != Team::Blue || u.pos.distance(*h) > 8)
            })
            .unwrap();
        assert!(!fog.sees(distant) && !game.is_explored(distant));

        // Every Blue unit leaves; what they saw stays explored but unseen.
        for unit in game.units.iter_mut().filter(|u| u.team == Team::Blue) {
            unit.pos = distant;
        }
        game.explore();
        assert!(game.is_explored(start));
        assert!(!game.fog().sees(start));

        game.toggle_fog();
        assert!(game.fog().sees(start) && game.is_explored(Hex::new(0, 0)));
    }

    /// A lone Blue cavalry at the origin with Charge queued (move 3, sight 3)
    /// and a mountain at (1, 0) hiding (2, 0), which is still three steps away
    /// around the mountain.
    pub(in crate::game) fn behind_the_mountain() -> (GameState, usize, Hex) {
        let mut game = GameState::frontier_scenario();
        game.grid = HexGrid::new(6, [(Hex::new(1, 0), Tile::MOUNTAINS)]);
        game.units.clear();
        game.cities.clear();
        game.sites.clear();
        game.roads.clear();
        game.memory.clear();
        game.player_controlled_units.clear();
        game.units
            .push(Unit::new(1, Hex::new(0, 0), Team::Blue, UnitType::Cavalry));
        game.units[0].ability_queued = true;
        let hidden = Hex::new(2, 0);
        game.explore();
        assert!(game.fog_of_war && !game.fog().sees(hidden));
        assert_eq!(game.units[0].stats().move_range, 3);
        (game, 0, hidden)
    }

    /// Walks unit `idx` next to `hex` to look at it, then back out of sight.
    pub(in crate::game) fn glance_at(game: &mut GameState, idx: usize, hex: Hex) {
        let home = game.units[idx].pos;
        game.units[idx].pos = Hex::new(hex.q, hex.r - 1);
        game.explore();
        game.units[idx].pos = home;
        game.explore();
        assert!(!game.fog().sees(hex) && game.is_explored(hex));
    }

    /// The Cities scenario with no units and Blue's city open, plus a hex
    /// its goods reach that its own sight doesn't, seen once while empty.
    pub(in crate::game) fn remembered_route_hex() -> (GameState, usize, Hex) {
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.selected = None;
        let city = game
            .cities
            .iter()
            .position(|c| c.team == PLAYER_TEAM)
            .unwrap();
        game.selected_city = Some(city);
        let fog = game.fog();
        let far = game
            .routes(city)
            .costs
            .keys()
            .copied()
            .filter(|&h| !fog.sees(h) && game.grid.terrain(h).is_workable())
            .min_by_key(|h| (h.q, h.r))
            .expect("a route hex out of the city's sight");
        game.units
            .push(Unit::new(50, far, PLAYER_TEAM, UnitType::Scout));
        game.explore();
        game.units.clear();
        assert!(!game.fog().sees(far) && game.is_explored(far));
        (game, city, far)
    }

    #[test]
    fn unseen_enemies_do_not_cut_the_routes_the_player_is_shown() {
        let (mut game, city, far) = remembered_route_hex();
        game.units
            .push(Unit::new(51, far, Team::Red, UnitType::Melee));
        assert!(
            !game.routes(city).costs.contains_key(&far),
            "goods really are cut"
        );
        let fog = game.fog();
        assert!(game.known_routes(city, &fog).costs.contains_key(&far));
    }

    /// A lone Blue city at the origin in marsh, working one tile five hexes
    /// east that a mountain at (3, 0) hides from it. Only a road around the
    /// mountain, through (4, -1), brings its goods home.
    fn worked_tile_behind_the_mountain() -> (GameState, usize, Hex) {
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.sites.clear();
        game.memory.clear();
        game.player_controlled_units.clear();
        game.selected = None;
        game.selected_city = None;
        game.cities.retain(|c| c.team == PLAYER_TEAM);
        let origin = Hex::new(0, 0);
        let mountain = Hex::new(3, 0);
        let road = [(1, 0), (2, 0), (3, -1), (4, -1), (4, 0), (5, 0)].map(|(q, r)| Hex::new(q, r));
        let tile = |h: Hex| -> Tile {
            if h == mountain {
                Tile::MOUNTAINS
            } else if h == origin || road.contains(&h) {
                Terrain::Plains.into()
            } else {
                Terrain::Marsh.into()
            }
        };
        let hexes: Vec<Hex> = HexGrid::new(7, [(origin, Tile::default())])
            .all_hexes()
            .collect();
        game.grid = HexGrid::new(7, hexes.into_iter().map(|h| (h, tile(h))));
        game.roads = road.into_iter().collect();
        let worked = Hex::new(5, 0);
        let city = &mut game.cities[0];
        city.pos = origin;
        city.population = 1;
        city.worked = vec![worked];
        city.remembered_worked = vec![worked];
        assert_eq!(game.routes(0).costs.get(&worked), Some(&6));
        (game, 0, worked)
    }

    #[test]
    fn a_worked_tile_is_in_sight_past_range_and_mountains() {
        let (mut game, city, worked) = worked_tile_behind_the_mountain();
        let pos = game.cities[city].pos;
        assert!(pos.distance(worked) > CITY_SIGHT && !game.in_line_of_sight(pos, worked));
        game.explore();
        let fog = game.fog();
        assert!(fog.sees(worked) && game.is_explored(worked));
        assert!(
            !fog.sees(Hex::new(4, 0)),
            "only the tile itself, not around it"
        );

        // A Red unit steps onto it: the player sees it there.
        game.units
            .push(Unit::new(51, worked, Team::Red, UnitType::Melee));
        assert!(game.fog().shows(&game.units[0]));
        assert!(game.known_occupied(worked, &game.fog()));

        // The turn ends, and the cut-off citizen moves elsewhere; the enemy
        // stays in sight on the tile it took.
        game.resolve_economy();
        assert!(!game.cities[city].worked.contains(&worked));
        let fog = game.fog();
        assert!(fog.sees(worked) && fog.shows(&game.units[0]));

        // Once the player assigns citizens elsewhere, which forgets that
        // tile, it's out of sight again.
        game.cities[city].remembered_worked = game.cities[city].worked.clone();
        assert!(!game.fog().shows(&game.units[0]));
    }

    #[test]
    fn an_unseen_enemy_on_a_route_hex_still_cuts_goods() {
        // Accepted: an enemy on a route hex that isn't a worked tile stays
        // hidden, but the city's real income, rings and route notice (all
        // from `routes`) show the goods it cuts off.
        let (mut game, city, worked) = worked_tile_behind_the_mountain();
        let full = game.income(city);
        let blocker = Hex::new(4, -1);
        game.units
            .push(Unit::new(51, blocker, Team::Red, UnitType::Melee));
        game.explore();
        let fog = game.fog();
        assert!(fog.sees(worked) && !fog.sees(blocker));
        assert!(!fog.shows(&game.units[0]));
        // The goods detour south of the mountain, through the marsh at
        // (2, 1) and (3, 1): two hexes off the road, so a quarter arrives.
        assert_eq!(
            game.routes(city).path_from(worked),
            [(5, 0), (4, 0), (3, 1), (2, 1), (2, 0), (1, 0), (0, 0)].map(|(q, r)| Hex::new(q, r))
        );
        assert_eq!(game.routes(city).costs.get(&worked), Some(&8));
        assert!(
            game.income(city).food + game.income(city).production() < full.food + full.production(),
            "{:?} vs {full:?}",
            game.income(city)
        );
    }

    #[test]
    fn a_group_click_on_an_unseen_enemy_moves_there() {
        let (mut game, cavalry, hidden) = behind_the_mountain();
        game.units
            .push(Unit::new(2, Hex::new(-1, 0), Team::Blue, UnitType::Melee));
        game.units
            .push(Unit::new(3, hidden, Team::Red, UnitType::Melee));
        assert!(!game.fog().sees(hidden));
        game.group = vec![cavalry, 1];
        game.group_order(hidden, ClickMode::Normal);
        assert_eq!(game.units[cavalry].planned_move, Some(hidden));
        assert!(game.units.iter().all(|u| u.planned_attack.is_none()));
    }

    #[test]
    fn a_move_toward_an_unseen_enemy_survives_an_order_check() {
        let (mut game, cavalry, hidden) = behind_the_mountain();
        game.units
            .push(Unit::new(2, hidden, Team::Red, UnitType::Melee));
        game.queue_order_at(cavalry, hidden);
        game.drop_orders_now_impossible(cavalry);
        assert_eq!(game.units[cavalry].planned_move, Some(hidden));
    }

    #[test]
    fn unseen_units_do_not_shape_the_move_range() {
        let (mut game, cavalry, hidden) = behind_the_mountain();
        let start = game.units[cavalry].pos;
        game.units
            .push(Unit::new(2, hidden, Team::Red, UnitType::Melee));
        assert!(!game.reachable_hexes(start, 3, Team::Blue).contains(&hidden));
        let fog = game.fog();
        assert!(
            game.known_reachable_hexes(start, 3, Team::Blue, &fog)
                .contains(&hidden)
        );
        assert!(!game.known_occupied(hidden, &fog));

        // Having seen it there doesn't change that once it's out of sight.
        glance_at(&mut game, cavalry, hidden);
        let fog = game.fog();
        assert!(
            game.known_reachable_hexes(start, 3, Team::Blue, &fog)
                .contains(&hidden)
        );
    }

    #[test]
    fn clicking_an_unseen_enemys_hex_plans_a_move_not_an_attack() {
        let (mut game, cavalry, hidden) = behind_the_mountain();
        game.units
            .push(Unit::new(2, hidden, Team::Red, UnitType::Melee));
        game.queue_order_at(cavalry, hidden);
        assert_eq!(game.units[cavalry].planned_move, Some(hidden));
        assert_eq!(game.units[cavalry].planned_attack, None);
    }

    #[test]
    fn an_enemy_seen_then_seen_elsewhere_leaves_no_ghost() {
        // Red is seen at `hidden`, then walks into sight next to Blue.
        let (mut game, cavalry, hidden) = behind_the_mountain();
        game.units
            .push(Unit::new(2, hidden, Team::Red, UnitType::Melee));
        glance_at(&mut game, cavalry, hidden);
        let red = game.units.len() - 1;
        game.units[red].pos = Hex::new(0, 1);
        game.explore();
        let fog = game.fog();
        assert!(fog.sees(Hex::new(0, 1)) && !fog.sees(hidden));
        assert!(!game.known_occupied(hidden, &fog));
        assert!(!game.known_enemy_target_at(hidden, Team::Blue, &fog));
        // A click there plans a move, not an attack on the empty hex.
        game.queue_order_at(cavalry, hidden);
        assert_eq!(game.units[cavalry].planned_move, Some(hidden));
        assert_eq!(game.units[cavalry].planned_attack, None);
    }

    #[test]
    fn an_enemy_out_of_sight_is_not_a_target_even_if_seen_there() {
        let (mut game, cavalry, hidden) = behind_the_mountain();
        game.units
            .push(Unit::new(2, hidden, Team::Red, UnitType::Melee));
        glance_at(&mut game, cavalry, hidden);
        let fog = game.fog();
        assert!(!game.known_enemy_target_at(hidden, Team::Blue, &fog));
    }

    #[test]
    fn the_memory_keeps_other_sides_construction_but_not_their_plans() {
        use crate::game::city::Building;
        use crate::game::workers::{FieldWorker, JobKind};
        let (mut game, cavalry, hidden) = behind_the_mountain();
        let job = WorkerJob::on_tile(hidden, JobKind::Build(Building::Barracks));
        let worker = |team: Team, work_left: Option<u32>| FieldWorker {
            id: 7,
            team,
            home: 0,
            base: hidden,
            pos: hidden,
            job: Some(job),
            work_left,
            recalled: false,
        };
        let construction = |game: &GameState| game.remembered(hidden).unwrap().construction.clone();
        // Just arrived, not yet at work: nothing on the ground.
        game.field_workers = vec![worker(Team::Red, None)];
        glance_at(&mut game, cavalry, hidden);
        assert!(construction(&game).is_empty());
        // At work: remembered with its side and name, and nothing else.
        game.field_workers = vec![worker(Team::Red, Some(3))];
        glance_at(&mut game, cavalry, hidden);
        let seen = SeenJob {
            job,
            team: Team::Red,
            name: "BARRACKS",
        };
        assert_eq!(construction(&game), vec![seen]);
        // Finished out of sight: still as last seen.
        game.field_workers.clear();
        game.explore();
        assert_eq!(construction(&game), vec![seen]);
        // The player's own jobs are drawn from the board, not remembered
        // (and a worker of theirs sees its tile).
        game.field_workers = vec![worker(PLAYER_TEAM, Some(3))];
        game.explore();
        assert!(game.fog().sees(hidden));
        assert!(construction(&game).is_empty());
    }

    #[test]
    fn yields_out_of_sight_are_as_last_seen() {
        let (mut game, cavalry, hidden) = behind_the_mountain();
        glance_at(&mut game, cavalry, hidden);
        let seen = game.remembered(hidden).unwrap().yields;
        // Red farms the hex while nobody's looking.
        game.sites.insert(
            hidden,
            Site {
                team: Team::Red,
                food: 9,
                production: 9,
                label: "FARM",
            },
        );
        let fog = game.fog();
        assert_ne!(game.raw_yield(hidden), seen);
        assert_eq!(game.known_yield(hidden, &fog), seen);
        let near = Hex::new(0, 1);
        assert_eq!(game.known_yield(near, &fog), game.raw_yield(near));
    }

    #[test]
    fn a_side_sees_by_the_players_rules_whatever_this_machine_shows() {
        let mut game = GameState::world_scenario(1);
        let side = game.side_fog(game.local_team);
        assert_eq!(game.fog().visible, side.visible);
        // What the side has seen is its memory now, and only that.
        let memory = &game.side_memory[game.local_team.index()];
        assert_eq!(memory.len(), side.visible.as_ref().unwrap().len());
        // F10 lifts the player's fog, not a side's.
        game.toggle_fog();
        assert!(game.fog().visible.is_none());
        assert_eq!(game.side_fog(game.local_team).visible, side.visible);
        // Nor does the side this machine plays count: every side's is its own.
        let red = game.side_fog(Team::Red);
        game.local_team = Team::Red;
        assert_eq!(game.side_fog(Team::Red).visible, red.visible);
        assert_ne!(red.visible, side.visible);
    }

    #[test]
    fn the_fog_notice_names_its_key() {
        let mut game = GameState::world_scenario(3);
        game.toggle_fog();
        assert!(game.notice.contains("F10"), "{}", game.notice);
        game.toggle_fog();
        assert!(game.notice.contains("F10"), "{}", game.notice);
    }

    /// World 3, which has no Red side, with a Red scout added far from
    /// Blue's units.
    fn world_with_a_distant_red_scout() -> (GameState, usize) {
        let mut game = GameState::solo_world(3);
        let far = game
            .grid
            .all_hexes()
            .filter(|&h| game.grid.is_passable(h))
            .find(|&h| game.units.iter().all(|u| u.pos.distance(h) > 10))
            .expect("land far from Blue");
        game.units
            .push(Unit::new(100, far, Team::Red, UnitType::Scout));
        let red = game.units.len() - 1;
        (game, red)
    }

    #[test]
    fn enemies_out_of_sight_are_hidden() {
        let (game, red) = world_with_a_distant_red_scout();
        let fog = game.fog();
        assert!(!fog.shows(&game.units[red]), "Red is far out of sight");
        let blue = game.units.iter().find(|u| u.team == Team::Blue).unwrap();
        assert!(fog.shows(blue));
    }

    #[test]
    fn units_are_forgotten_once_out_of_sight() {
        let (mut game, red) = world_with_a_distant_red_scout();
        let settler = game.units[0].pos;
        let near = settler
            .neighbors()
            .into_iter()
            .find(|h| game.grid.is_passable(*h) && !game.is_occupied(*h))
            .unwrap();
        // The Red scout wanders into sight next to Blue's settler.
        let red_start = game.units[red].pos;
        game.units[red].pos = near;
        game.explore();
        assert!(game.known_occupied(near, &game.fog()));

        // Every Blue unit leaves, with the scout still standing there.
        let far = red_start;
        for unit in game.units.iter_mut().filter(|u| u.team == Team::Blue) {
            unit.pos = far;
        }
        game.explore();
        let fog = game.fog();
        assert!(!fog.sees(near) && game.is_explored(near));
        assert!(!game.known_occupied(near, &fog));
    }

    #[test]
    fn mountains_block_sight_beyond_them() {
        use crate::game::hex::HexGrid;
        use crate::game::terrain::Tile;
        let mut game = GameState::new();
        game.units.clear();
        game.grid = HexGrid::new(4, [(Hex::new(1, 0), Tile::MOUNTAINS)]);
        game.units
            .push(Unit::new(0, Hex::new(0, 0), Team::Blue, UnitType::Scout));
        let fog = game.fog();
        assert!(fog.sees(Hex::new(1, 0)), "the mountain itself shows");
        assert!(!fog.sees(Hex::new(2, 0)) && !fog.sees(Hex::new(3, 0)));
        // A line along the mountain's edge still passes on its clear side.
        assert!(fog.sees(Hex::new(2, -1)));
        assert!(fog.sees(Hex::new(-3, 0)), "other directions are open");
    }

    #[test]
    fn every_world_starts_its_scouts_on_hills() {
        for seed in 0..6 {
            let game = GameState::world_scenario(seed);
            for unit in &game.units {
                let hills = game.grid.tile(unit.pos).hills;
                let scout = unit.unit_type == UnitType::Scout;
                assert_eq!(hills, scout, "seed {seed}: {unit}");
            }
        }
    }
}
