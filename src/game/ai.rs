//! A deliberately minimal AI, playing every team but the player's.
//!
//! It plays under the same fog as the player (`fog.rs`): each side plans on
//! what it sees and remembers (`side_fog`), never on the real board. Enemy
//! units and workers count only in sight; cities, ruins and terrain as last
//! seen; ground never seen as open.

use std::collections::VecDeque;

use super::GameState;
use super::city::{Build, BuildUnit, Building, City, Lane, Stock};
use super::fast_hash::{HashMap, HashSet};
use super::fog::{Fog, Sighting};
use super::hex::Hex;
use super::unit::{Team, UnitType};
use super::workers::{JobKind, WorkerJob};

/// What an AI side knows as it plans its units: its fog, and what's worth
/// going for in it. Sets, not lists, as searches ask of every hex they
/// pass; whatever picks from one breaks ties by hex coordinates, so their
/// order never counts.
struct Knowledge {
    fog: Fog,
    /// Enemy units and workers in sight.
    enemies: HashSet<Hex>,
    /// Ruins in sight, or remembered where they were last seen.
    ruins: HashSet<Hex>,
    /// Enemy city centers in sight or remembered.
    cities: HashSet<Hex>,
    /// The hexes around those cities.
    gates: HashSet<Hex>,
    /// Whether its land units know they can't step onto each hex
    /// (`known_step`), by `HexGrid::index`: ground seen to be impassable,
    /// and enemy cities. An array, as searches ask of every
    /// hex they pass.
    closed: Vec<bool>,
    /// Whether it may know of a wall or gate anywhere, so steps must be
    /// checked for them.
    walls: bool,
}

/// Steps on foot to one target from the hexes around it (`steps_to`), found
/// outward from the target as far as asked so far: units of a side heading
/// for the same target share it, each searching on only as far as it needs.
struct Flood {
    /// Steps to the target by `HexGrid::index`, `NOT_FOUND` where not
    /// found (yet).
    steps: Vec<i32>,
    frontier: VecDeque<Hex>,
}

/// A hex a `Flood` hasn't reached.
const NOT_FOUND: i32 = i32::MAX;

/// Units (scouts and settlers aside) the AI wants for each of its cities
/// before it spends on growth.
const AI_ARMY_PER_CITY: usize = 2;

impl GameState {
    /// The teams the AI plays: every team but the player's that still has a
    /// unit or a city, in `Team::ALL` order.
    pub(super) fn ai_teams(&self) -> Vec<Team> {
        Team::ALL
            .into_iter()
            .filter(|&team| !self.is_human(team))
            .filter(|&team| {
                self.units.iter().any(|u| u.team == team)
                    || self.cities.iter().any(|c| c.team == team)
            })
            .collect()
    }

    /// What `team`'s cities and Barracks queue: one item in each empty
    /// queue, and only what the side can pay for this turn, with what its
    /// other queues start (`forecast`'s `spare`), so everything it queues is
    /// paid and started this turn. A queue whose items all wait for the
    /// stockpile (the income it counted on didn't come) is emptied, which
    /// costs nothing, as they're unpaid, and planned again. The Barracks is
    /// the military building (`city/barracks.rs`): an idle one trains
    /// Cavalry or Armored when its deposits allow and the side can pay, else
    /// Melee, or Ranged for one in three. A city's own queue trains a worker
    /// first if it has none left; its workers then build a Barracks (sited
    /// by `ai_barracks_site`, paid when placed), and the queue grows; a city
    /// without a Barracks trains Melee itself, slowly, until the side has
    /// `AI_ARMY_PER_CITY` units per city, falling back on growth, and
    /// gathering when it can't pay for anything.
    fn plan_ai_cities(&mut self, team: Team, fog: &Fog) {
        let soldiers = |game: &GameState, kind: Option<UnitType>| {
            game.units
                .iter()
                .filter(|u| {
                    u.team == team
                        && u.unit_type != UnitType::Scout
                        && !game.settlers.contains(&u.id)
                        && kind.is_none_or(|kind| u.unit_type == kind)
                })
                .count()
        };
        let mut army = soldiers(self, None);
        let cities: Vec<usize> = (0..self.cities.len())
            .filter(|&i| self.cities[i].team == team)
            .collect();
        let forecast = self.forecast(team);
        for lane in &forecast.lanes {
            if lane.worked.is_some() {
                continue;
            }
            while let Some(last) = self.lane_len(lane.city, lane.lane).checked_sub(1) {
                match lane.lane {
                    Lane::City => {
                        self.take_queue_item(lane.city, last);
                    }
                    Lane::Barracks => {
                        self.take_barracks_item(lane.city, last);
                    }
                }
            }
        }
        // Emptying queues of unpaid items leaves what they start unchanged.
        let mut spare = forecast.spare;
        for &city in &cities {
            let stock = self.stock(team);
            self.place_ai_barracks(city, fog);
            // A job is paid when placed.
            spare -= stock - self.stock(team);
            if self.cities[city].barracks.is_some() && self.cities[city].barracks_queue.is_empty() {
                let basic = if soldiers(self, Some(UnitType::Ranged)) * 2
                    < soldiers(self, Some(UnitType::Melee))
                {
                    BuildUnit::Ranged
                } else {
                    BuildUnit::Melee
                };
                for build in [BuildUnit::Cavalry, BuildUnit::Armored, basic] {
                    if self.barracks_lock(city, build).is_none() && spare.covers(build.price()) {
                        self.queue_barracks(city, build);
                        spare -= build.price();
                        army += 1;
                        break;
                    }
                }
            }
            let c = &self.cities[city];
            if !c.queue.is_empty() {
                continue;
            }
            let melee = Build::Unit(BuildUnit::Melee);
            let mut choices = if c.workers == 0 && self.workers_out(city) == 0 {
                vec![Build::Worker]
            } else {
                Vec::new()
            };
            if c.barracks.is_none() && army < AI_ARMY_PER_CITY * cities.len() {
                choices.extend([melee, Build::Grow]);
            } else {
                choices.push(Build::Grow);
            }
            // With nothing it can pay for, it gathers.
            choices.push(Build::Gather);
            for build in choices {
                if build == Build::Grow && !self.can_grow(city) {
                    continue;
                }
                let price = self.queue_price(city, build);
                // A free Gather fits even when the Barracks job took more
                // than was spare.
                if price == Stock::default() || spare.covers(price) {
                    self.queue_build(city, build);
                    spare -= price;
                    army += usize::from(build == melee);
                    break;
                }
            }
        }
    }

    /// City `city` of the AI, with a worker and no Barracks built or placed,
    /// places one for its workers to build (`ai_barracks_site`), if its
    /// side can pay.
    fn place_ai_barracks(&mut self, city: usize, fog: &Fog) {
        let c = &self.cities[city];
        if c.barracks.is_some()
            || self.building_job_queued(city, Building::Barracks)
            || !self.has_workers(city)
        {
            return;
        }
        if let Some(site) = self.ai_barracks_site(city, fog) {
            let job = WorkerJob::on_tile(site, JobKind::Build(Building::Barracks));
            let team = self.cities[city].team;
            if self.job_problem(city, job).is_none() && self.tile_job_at(team, site).is_none() {
                let _ = self.try_queue_job(city, job);
            }
        }
    }

    /// Where city `city` of the AI puts its Barracks: on a Horses or Iron
    /// deposit within 3 hexes, of a kind its side has none of yet if it can,
    /// else on the nearest open, unworked tile within 2; ties by distance,
    /// then hex coordinates. Open means no enemy in sight (`fog`) stands on it.
    fn ai_barracks_site(&self, city: usize, fog: &Fog) -> Option<Hex> {
        let c = &self.cities[city];
        let team = c.team;
        let open = |hex: Hex| {
            self.grid.contains(hex)
                && self.ai_site_issue(city, Building::Barracks, hex).is_none()
                && !(fog.sees(hex) && self.enemy_of_team_at(hex, team).is_some())
        };
        let mut near: Vec<Hex> = self
            .grid
            .all_hexes()
            .filter(|h| (1..=3).contains(&h.distance(c.pos)) && open(*h))
            .collect();
        near.sort_by_key(|h| (h.distance(c.pos), h.q, h.r));
        let deposit = |hex: &Hex| self.grid.resource(*hex);
        let lacking = |hex: &Hex| {
            deposit(hex).is_some_and(|resource| self.side_deposits(team, resource).is_empty())
        };
        near.iter()
            .find(|h| lacking(h))
            .or_else(|| near.iter().find(|h| deposit(h).is_some()))
            .or_else(|| {
                near.iter().find(|h| {
                    h.distance(c.pos) <= 2 && !self.cities.iter().any(|o| o.worked.contains(h))
                })
            })
            .or_else(|| near.first())
            .copied()
    }

    /// Each unit goes for the nearest on foot of the enemy units and workers
    /// its side sees, the ruins nobody on its side holds (`ruins.rs`) and the
    /// enemy cities its side has seen (`nearest_ai_target`); with none of
    /// those, it explores, heading for the nearest ground its side has never
    /// seen. An enemy it attacks if already in range, otherwise it moves as
    /// close as it can and attacks if that brings it into range; ruins it
    /// steps onto, and then holds until they're claimed; next to an enemy
    /// city, its fighters go in through the gates (`city/interior.rs`). A
    /// unit going elsewhere than an enemy attacks any enemy in range of
    /// where it ends up. Units in a contested hex stay and fight. No
    /// coordination beyond not sending two units to the same hex, and no
    /// retreating. It all goes by what the side knows (`side_fog`), never
    /// the real board: an enemy out of sight isn't there, and ground never
    /// seen is open.
    pub(super) fn plan_ai_turn(&mut self, team: Team) {
        // The computer founds its first city immediately, before combat orders.
        if self.cities.iter().all(|c| c.team != team)
            && let Some(i) = self
                .units
                .iter()
                .position(|u| u.team == team && self.settlers.contains(&u.id))
        {
            let unit = self.units.remove(i);
            self.settlers.remove(&unit.id);
            let id = self.cities.len() as u32;
            self.cities.push(City::new(id, team, unit.pos));
            self.auto_assign_city(self.cities.len() - 1);
        }
        let fog = self.side_fog(team);
        self.plan_ai_cities(team, &fog);
        self.plan_ai_workers(team);
        let known = self.knowledge(fog);
        let mut floods: HashMap<Hex, Flood> = HashMap::default();
        for idx in 0..self.units.len() {
            if self.units[idx].team != team
                || self.rival_of(idx).is_some()
                || self.player_controlled_units.contains(&self.units[idx].id)
                || self.settlers.contains(&self.units[idx].id)
            {
                continue;
            }
            let pos = self.units[idx].pos;
            // A defender already holding a city gate should keep projecting its
            // interior copy while enemy troops contest the surrounding ring.
            let gate_under_attack = self.cities.iter().any(|city| {
                city.team == team
                    && pos.distance(city.pos) == 1
                    && self.units.iter().any(|enemy| {
                        enemy.team != team
                            && known.fog.sees(enemy.pos)
                            && enemy.pos.distance(city.pos) == 1
                    })
            });
            // Holding ruins until they're claimed: stay, and fight from there.
            if gate_under_attack || self.ruin_at(pos).is_some() {
                self.units[idx].planned_attack = self.ai_attack_from(idx, pos, &known.fog);
                continue;
            }
            let Some(target) = self.nearest_ai_target(idx, &known) else {
                continue;
            };

            let fog = &known.fog;
            let unit = &self.units[idx];
            let stats = unit.stats();
            // An enemy it knows is there is fought. Anything else (a worker,
            // ruins, ground never seen) is stepped onto, and a city closed on.
            let enemy_there = fog.sees(target) && self.enemy_of_team_at(target, team).is_some();
            // Ships keep their water-only movement domain while land units use
            // roads and gates.
            let reachable = self.known_reachable_for_domain(
                unit.pos,
                stats.move_range,
                team,
                fog,
                unit.is_naval(),
            );
            let dest = if !enemy_there && reachable.contains(&target) {
                target
            } else if enemy_there && unit.pos.distance(target) <= stats.attack_range {
                unit.pos
            } else {
                let to_target = (!unit.is_naval()).then(|| {
                    let flood = floods
                        .entry(target)
                        .or_insert_with(|| self.flood_from(target));
                    self.steps_to(flood, &known, &reachable);
                    &flood.steps
                });
                let steps_left = |hex: &Hex| match to_target {
                    Some(steps) => self.grid.index(*hex).map_or(NOT_FOUND, |i| steps[i]),
                    None => hex.distance(target),
                };
                let claimed_by_ally = |hex: &Hex| {
                    self.units
                        .iter()
                        .any(|u| u.team == team && u.planned_move == Some(*hex))
                };
                // Staying put wins a tie, so a unit at a city's gates holds it.
                reachable
                    .into_iter()
                    .filter(|hex| !claimed_by_ally(hex))
                    .min_by_key(|hex| (steps_left(hex), *hex != unit.pos, hex.q, hex.r))
                    .unwrap_or(unit.pos)
            };

            if dest != pos {
                self.units[idx].planned_move = Some(dest);
            }
            let attack = if known.enemies.contains(&target) {
                let in_reach = dest != target
                    && dest.distance(target) <= stats.attack_range
                    && self.known_attack_target_legal(idx, target, false, fog);
                in_reach.then_some(target)
            } else {
                self.ai_attack_from(idx, dest, fog)
            };
            self.units[idx].planned_attack = attack;
        }
    }

    /// What `fog`'s side knows is worth going for: the enemy units and
    /// workers in sight, and the ruins and enemy cities in sight or
    /// remembered.
    fn knowledge(&self, fog: Fog) -> Knowledge {
        let team = fog.team();
        let enemies = self
            .units
            .iter()
            .filter(|u| u.team != team)
            .map(|u| u.pos)
            .chain(
                self.field_workers
                    .iter()
                    .filter(|w| w.team != team)
                    .map(|w| w.pos),
            )
            .filter(|&hex| fog.sees(hex))
            .collect();
        // Out of sight, as last seen; the memory's order doesn't count.
        let (fog_ref, memory) = (&fog, self.memory_of(&fog));
        let remembered = |seen: fn(&Sighting, Team) -> bool| {
            memory
                .iter()
                .filter(move |&(&hex, sighting)| !fog_ref.sees(hex) && seen(sighting, team))
                .map(|(&hex, _)| hex)
        };
        let ruins = self
            .ruins
            .iter()
            .map(|ruin| ruin.pos)
            .filter(|&hex| fog.sees(hex))
            .chain(remembered(|seen, _| seen.ruin))
            .collect();
        let cities: HashSet<Hex> = self
            .cities
            .iter()
            .filter(|c| c.team != team && fog.sees(c.pos))
            .map(|c| c.pos)
            .chain(remembered(|seen, team| {
                seen.city.is_some_and(|city| city.team != team)
            }))
            .collect();
        let gates = cities.iter().flat_map(|city| city.neighbors()).collect();
        // Everything in sight is in the memory too (`side_fog`), and ground
        // never seen is open.
        let mut closed = vec![false; self.grid.cells()];
        for &hex in memory.keys() {
            if !self.known_step(hex, hex, team, &fog, false) {
                closed[self.grid.index(hex).expect("on the grid")] = true;
            }
        }
        let walls = !self.barriers.is_empty() || memory.values().any(|s| !s.barriers.is_empty());
        Knowledge {
            fog,
            enemies,
            ruins,
            cities,
            gates,
            closed,
            walls,
        }
    }

    /// `known_step` for a land unit of `known`'s side, from what it knows
    /// gathered up front, as a search takes many steps.
    fn ai_step(&self, from: Hex, to: Hex, known: &Knowledge) -> bool {
        self.grid.index(to).is_some_and(|i| !known.closed[i])
            && self.grid.contains(to)
            && (!known.walls || self.known_can_cross(from, to, known.fog.team(), &known.fog))
    }

    /// The enemy in `fog`'s sight that unit `idx` would attack from `from`:
    /// the nearest in range it may attack, ties to the lowest id.
    fn ai_attack_from(&self, idx: usize, from: Hex, fog: &Fog) -> Option<Hex> {
        let unit = &self.units[idx];
        self.units
            .iter()
            .filter(|enemy| enemy.team != unit.team && fog.sees(enemy.pos))
            .filter(|enemy| {
                from.distance(enemy.pos) <= unit.stats().attack_range
                    && self.known_attack_target_legal(idx, enemy.pos, false, fog)
            })
            .min_by_key(|enemy| (from.distance(enemy.pos), enemy.id))
            .map(|enemy| enemy.pos)
    }

    /// Where unit `idx` heads (see `plan_ai_turn`): of the enemy units and
    /// workers in sight, the ruins no ally holds or is already heading for
    /// and the enemy cities seen, the nearest on foot over the ground as
    /// its side knows it (a city counting from its gates, and an enemy
    /// before a city as near), ties to the lowest coordinates. With none
    /// reachable, the nearest ground its side has never seen; with none
    /// left, the enemy in sight nearest as the crow flies. A ship goes by
    /// the crow's flight: to the nearest enemy in sight, else the nearest
    /// ground never seen.
    fn nearest_ai_target(&self, idx: usize, known: &Knowledge) -> Option<Hex> {
        let unit = &self.units[idx];
        let team = unit.team;
        let fog = &known.fog;
        let nearest = |hexes: &mut dyn Iterator<Item = Hex>| {
            hexes.min_by_key(|pos| (unit.pos.distance(*pos), pos.q, pos.r))
        };
        if unit.is_naval() {
            return nearest(&mut known.enemies.iter().copied()).or_else(|| {
                nearest(
                    &mut self
                        .grid
                        .all_hexes()
                        .filter(|&hex| !self.explored_by(hex, fog)),
                )
            });
        }
        let open_ruins: HashSet<Hex> = known
            .ruins
            .iter()
            .copied()
            .filter(|&pos| {
                !self
                    .units
                    .iter()
                    .any(|u| u.team == team && (u.pos == pos || u.planned_move == Some(pos)))
            })
            .collect();
        let is_target = |hex: &Hex| known.enemies.contains(hex) || open_ruins.contains(hex);
        let lowest = |hexes: &mut dyn Iterator<Item = Hex>| hexes.min_by_key(|hex| (hex.q, hex.r));
        // With nothing known to go for, the nearest ground never seen is it.
        let exploring =
            known.enemies.is_empty() && open_ruins.is_empty() && known.cities.is_empty();
        let mut unexplored = None;
        // Search outward ring by ring, stopping at the first ring with a
        // target. A city is as far as its gates, plus one.
        let mut seen = vec![false; self.grid.cells()];
        if let Some(i) = self.grid.index(unit.pos) {
            seen[i] = true;
        }
        let mut ring = vec![unit.pos];
        let mut last_ring: Vec<Hex> = Vec::new();
        while !ring.is_empty() || !last_ring.is_empty() {
            if let Some(hex) = lowest(&mut ring.iter().copied().filter(is_target)) {
                return Some(hex);
            }
            let cities = last_ring
                .iter()
                .filter(|hex| known.gates.contains(hex))
                .flat_map(|gate| gate.neighbors())
                .filter(|hex| known.cities.contains(hex));
            if let Some(city) = lowest(&mut cities.into_iter()) {
                return Some(city);
            }
            if unexplored.is_none() {
                unexplored =
                    lowest(&mut ring.iter().copied().filter(|&h| !self.explored_by(h, fog)));
                if exploring && unexplored.is_some() {
                    return unexplored;
                }
            }
            let mut next = Vec::new();
            for &hex in &ring {
                for neighbor in hex.neighbors() {
                    if self.ai_step(hex, neighbor, known) {
                        let i = self.grid.index(neighbor).expect("on the grid");
                        if !seen[i] {
                            seen[i] = true;
                            next.push(neighbor);
                        }
                    }
                }
            }
            last_ring = std::mem::replace(&mut ring, next);
        }
        unexplored.or_else(|| nearest(&mut known.enemies.iter().copied()))
    }

    /// Carries `flood` on until it has the steps on foot from each of
    /// `wanted` to its target, for a unit of `known`'s side: around the
    /// terrain, walls and others' gates and cities it knows of, ground never
    /// seen counting as open, but ignoring units, since those will have
    /// moved by the time anyone gets there. Any of `wanted` it can't reach
    /// are left out.
    fn steps_to(&self, flood: &mut Flood, known: &Knowledge, wanted: &HashSet<Hex>) {
        let found = |flood: &Flood, hex: Hex| {
            self.grid
                .index(hex)
                .is_some_and(|i| flood.steps[i] != NOT_FOUND)
        };
        let mut left = wanted.iter().filter(|&&hex| !found(flood, hex)).count();
        while left > 0
            && let Some(hex) = flood.frontier.pop_front()
        {
            let next = flood.steps[self.grid.index(hex).expect("on the grid")] + 1;
            for neighbor in hex.neighbors() {
                if self.ai_step(hex, neighbor, known) {
                    let i = self.grid.index(neighbor).expect("on the grid");
                    if flood.steps[i] == NOT_FOUND {
                        flood.steps[i] = next;
                        flood.frontier.push_back(neighbor);
                        if wanted.contains(&neighbor) {
                            left -= 1;
                        }
                    }
                }
            }
        }
    }

    /// A flood from `target` that has found nothing but the target.
    fn flood_from(&self, target: Hex) -> Flood {
        let mut steps = vec![NOT_FOUND; self.grid.cells()];
        if let Some(i) = self.grid.index(target) {
            steps[i] = 0;
        }
        Flood {
            steps,
            frontier: VecDeque::from([target]),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::game::hex::HexGrid;
    use crate::game::terrain::Tile;
    use crate::game::unit::{Unit, UnitType};

    #[test]
    fn gate_defense_never_orders_a_landing_craft_to_attack() {
        let mut game = GameState::new();
        game.units.clear();
        game.cities = vec![City::new(0, Team::Red, Hex::new(0, 0))];
        game.ruins.clear();
        game.units.push(Unit::new(
            90,
            Hex::new(1, 0),
            Team::Red,
            UnitType::LandingCraft,
        ));
        game.units
            .push(Unit::new(91, Hex::new(0, 1), Team::Blue, UnitType::Melee));
        game.plan_ai_turn(Team::Red);
        assert!(game.units[0].planned_attack.is_none());
    }
    /// A lone Red melee at the origin of open plains of radius 8, with no
    /// cities, ruins or memory.
    fn lone_red() -> GameState {
        let mut game = GameState::new();
        game.grid = HexGrid::new(8, [(Hex::new(0, 0), Tile::default())]);
        game.units = vec![Unit::new(1, Hex::new(0, 0), Team::Red, UnitType::Melee)];
        game.cities.clear();
        game.ruins.clear();
        game
    }

    /// Red has seen every hex of the map it hasn't already, as open ground
    /// with nothing on it.
    fn red_explores_the_rest(game: &mut GameState) {
        let hexes: Vec<Hex> = game.grid.all_hexes().collect();
        let memory = Arc::make_mut(&mut game.side_memory[Team::Red.index()]);
        for hex in hexes {
            memory.entry(hex).or_default();
        }
    }

    /// A Red unit at `at` looks around for a moment, and leaves.
    fn red_glances_from(game: &mut GameState, at: Hex) {
        game.units
            .push(Unit::new(99, at, Team::Red, UnitType::Melee));
        game.side_fog(Team::Red);
        game.units.pop();
    }

    fn replan(game: &mut GameState) -> (Option<Hex>, Option<Hex>) {
        game.units[0].planned_move = None;
        game.units[0].planned_attack = None;
        game.plan_ai_turn(Team::Red);
        (game.units[0].planned_move, game.units[0].planned_attack)
    }

    #[test]
    fn an_ai_unit_ignores_an_enemy_out_of_sight() {
        let mut game = lone_red();
        red_explores_the_rest(&mut game);
        // Blue stands five hexes off, out of a melee's sight of 2. The AI
        // used to go for it anyway; now it knows of nothing to go for.
        game.units
            .push(Unit::new(2, Hex::new(5, 0), Team::Blue, UnitType::Melee));
        assert_eq!(replan(&mut game), (None, None));

        // In sight, it closes and attacks.
        let blue = Hex::new(2, 0);
        game.units[1].pos = blue;
        let (dest, attack) = replan(&mut game);
        assert_eq!(dest.map(|d| d.distance(blue)), Some(1));
        assert_eq!(attack, Some(blue));
    }

    #[test]
    fn an_ai_unit_heads_for_an_enemy_city_only_once_its_side_has_seen_it() {
        let mut game = lone_red();
        let city = Hex::new(6, 0);
        game.cities = vec![City::new(0, Team::Blue, city)];
        // Knowing of nothing, it explores: the nearest ground never seen,
        // ties to the lowest coordinates, is away west.
        let (dest, _) = replan(&mut game);
        assert_eq!(dest, Some(Hex::new(-1, 0)));
        red_explores_the_rest(&mut game);
        assert_eq!(replan(&mut game), (None, None), "seen as empty ground");

        // Once seen, it goes for it, and keeps going by its memory even
        // after the city is gone out of its sight.
        red_glances_from(&mut game, Hex::new(4, 0));
        let (dest, _) = replan(&mut game);
        assert_eq!(dest.map(|d| d.distance(city)), Some(5));
        game.cities.clear();
        let (dest, _) = replan(&mut game);
        assert_eq!(dest.map(|d| d.distance(city)), Some(5));
    }

    #[test]
    fn an_ai_unit_counts_ground_its_side_has_never_seen_as_open() {
        // Mountains down the column q = 3 wall Red off from a Blue city at
        // (6, 0), but for a gap at the map's southern edge, (3, 5).
        let mut game = lone_red();
        let wall = (-8..=4).map(|r| (Hex::new(3, r), Tile::MOUNTAINS));
        game.grid = HexGrid::new(8, wall);
        let city = Hex::new(6, 0);
        game.cities = vec![City::new(0, Team::Blue, city)];
        // Red has seen the city (from the far side), but not the wall.
        red_glances_from(&mut game, Hex::new(7, -1));
        let wall_seen =
            |game: &GameState| game.side_memory[Team::Red.index()].contains_key(&Hex::new(3, 0));
        assert!(!wall_seen(&game));
        // Straight at it, as far as it knows.
        let (dest, _) = replan(&mut game);
        assert_eq!(dest.map(|d| d.distance(city)), Some(5));
        assert!(!wall_seen(&game), "still out of sight");

        // Once it has seen the wall, it goes around, south for the gap.
        red_explores_the_rest(&mut game);
        let (dest, _) = replan(&mut game);
        assert_eq!(dest, Some(Hex::new(0, 1)));
    }

    #[test]
    fn the_ai_plans_the_same_whatever_this_machine_sees() {
        // What a side knows is game state: the player's view (F10, which
        // side this machine plays, what it has seen) changes nothing.
        let mut game = GameState::world_scenario(2);
        game.settings.instant_playback = true;
        for _ in 0..6 {
            game.resolve_turn();
            game.update(0.0);
        }
        let mut other = game.clone();
        other.toggle_fog();
        other.memory.clear();
        other.local_team = Team::Green;
        let orders = |game: &mut GameState| {
            for team in game.ai_teams() {
                game.plan_ai_turn(team);
            }
            let orders: Vec<_> = game
                .units
                .iter()
                .map(|u| (u.id, u.planned_move, u.planned_attack))
                .collect();
            (orders, game.checksum())
        };
        let (mine, theirs) = (orders(&mut game), orders(&mut other));
        assert!(mine.0.iter().any(|(_, m, _)| m.is_some()), "the AI moves");
        assert_eq!(mine, theirs);
    }
}
