//! A deliberately minimal AI, playing every team but the player's.
//!
//! It plays under the same fog as the player (`fog.rs`): each side plans on
//! what it sees and remembers (`side_fog`), never on the real board. Enemy
//! units and workers count only in sight; cities, ruins and terrain as last
//! seen; ground never seen as open.

use std::cmp::Reverse;
use std::collections::VecDeque;

use super::GameState;
use super::ability::{Ability, CHARGE_EXTRA_MOVE, DEPLOYED_EXTRA_RANGE};
use super::animals::{DEN_BREED_TURNS, DEN_SPOILS, MAX_TERRITORY, bounty, den_cap, territory};
use super::city::{Build, BuildUnit, Building, Lane, MIN_CITY_DISTANCE, Stock};
use super::combat;
use super::fast_hash::{HashMap, HashSet};
use super::fog::{Fog, Sighting};
use super::hex::Hex;
use super::settings::ANIMALS_MANY;
use super::unit::{Team, Unit, UnitType};
use super::workers::{FARM_FOOD, JobKind, WorkerJob};

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
    /// Animal dens (`animals.rs`) in sight, or remembered where they were
    /// last seen: worth clearing, and their territory worth keeping
    /// settlers and workers out of.
    dens: HashSet<Hex>,
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
    /// What it makes of each den it knows of, by hex coordinates
    /// (`den_plans`). The reach of a guarded den it sends no force to is
    /// `closed` too, so its units walk around it.
    den_plans: Vec<DenPlan>,
    /// City centers it knows of, any side's: no animal strikes there.
    city_centers: Vec<Hex>,
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

impl Knowledge {
    /// Whether `hex` is within the territory of a den the side knows of
    /// (`animals.rs`), the widest any animal roams (`MAX_TERRITORY`).
    fn in_den_territory(&self, hex: Hex) -> bool {
        self.dens
            .iter()
            .any(|den| den.distance(hex) <= MAX_TERRITORY)
    }

    /// The den troop `id` is sent to clear (`den_force`), if any.
    fn den_of(&self, id: u32) -> Option<&DenPlan> {
        self.den_plans.iter().find(|p| p.force.contains(&id))
    }

    /// Whether an animal of a guarded den the side knows of could strike
    /// `hex` (`DenPlan::threatens`).
    fn in_danger(&self, hex: Hex) -> bool {
        self.den_plans
            .iter()
            .any(|p| p.threatens(hex, &self.city_centers))
    }

    /// How far out of the reach of the guarded dens its side leaves alone
    /// (no force sent), and of `also`, `hex` is: the fewest hexes past
    /// any one's reach, 0 or less inside one, `i32::MAX` with none near.
    fn margin(&self, hex: Hex, also: Option<&DenPlan>) -> i32 {
        self.den_plans
            .iter()
            .filter(|p| p.force.is_empty() || also.is_some_and(|a| a.pos == p.pos))
            .filter(|p| p.threatens(hex, &self.city_centers))
            .map(|p| hex.distance(p.pos) - p.reach)
            .min()
            .unwrap_or(i32::MAX)
    }

    /// Of `hexes`, those out of the reach of the guarded dens its side
    /// leaves alone, and of `also` (`margin`); if none are, those farthest
    /// out.
    fn out_of_reach<C: FromIterator<Hex>>(
        &self,
        hexes: impl IntoIterator<Item = Hex>,
        also: Option<&DenPlan>,
    ) -> C {
        let margins: Vec<(Hex, i32)> = hexes
            .into_iter()
            .map(|h| (h, self.margin(h, also).min(1)))
            .collect();
        let best = margins.iter().map(|&(_, m)| m).max();
        margins
            .into_iter()
            .filter(|&(_, m)| Some(m) == best)
            .map(|(h, _)| h)
            .collect()
    }
}

/// A hex a `Flood` hasn't reached.
const NOT_FOUND: i32 = i32::MAX;

/// Turns after which a hex its side has seen is worth a scout's look again.
const SCOUT_STALE_TURNS: u32 = 10;

/// How far from where it stands `enemy` could attack next turn: its move
/// (and a Charge's extra hex), then its range (and a deployed siege
/// engine's extra one). By its type, not its orders, which are its own
/// side's. `None` for one that can't attack.
fn threat_reach(enemy: &Unit) -> Option<i32> {
    if enemy.unit_type == UnitType::LandingCraft {
        return None;
    }
    let stats = enemy.unit_type.stats();
    let extra = match Ability::of(enemy.unit_type) {
        Ability::Charge => CHARGE_EXTRA_MOVE,
        Ability::Deploy => DEPLOYED_EXTRA_RANGE,
        _ => 0,
    };
    Some(stats.move_range + stats.attack_range + extra)
}

/// The hexes within `radius` of `center`, on the map or not.
pub(super) fn within(center: Hex, radius: i32) -> impl Iterator<Item = Hex> {
    (-radius..=radius).flat_map(move |dq| {
        ((-radius).max(-dq - radius)..=radius.min(-dq + radius))
            .map(move |dr| Hex::new(center.q + dq, center.r + dr))
    })
}

/// How far from a den or band of animals (as the crow flies) the AI looks
/// for troops to send against it (`den_force`).
const DEN_FORCE_RANGE: i32 = 12;
/// The most troops the AI sends against one den or band.
const DEN_MAX_FORCE: usize = 4;
/// How far past a den's reach (`DenPlan::reach`) a force's troops gather
/// before they go in together.
const DEN_GATHER: i32 = 2;
/// Rounds of blows the AI plays a fight with animals out to (`den_fight`);
/// animals still standing after them aren't beaten.
const DEN_FIGHT_ROUNDS: usize = 6;
/// How near one another stray animals (of no den the AI knows of) are
/// taken as one band (`den_plans`).
const STRAY_BAND: i32 = 3;
/// How many turns of a den left alone the AI counts against it
/// (`den_riddance`): the strays it would send out in that time.
const DEN_HORIZON_TURNS: u32 = 16;
/// What one stray is expected to cost the sides about it, in workers'
/// prices (`Build::Worker`).
const STRAY_HARM_WORKERS: f32 = 1.0;
/// What a den's land, open to a city and to workers once the den is
/// cleared, is worth to a side with a city within `AI_SITE_MAX_DISTANCE`
/// of it, in settlers' prices (`Build::Settler`).
const DEN_LAND_SETTLERS: f32 = 0.25;

/// What `stock` is worth to the AI, in whole units of food, wood and metal
/// alike.
fn worth(stock: Stock) -> f32 {
    (stock.food + stock.wood + stock.metal) as f32 / 4.0
}

/// What a troop of `kind` at full health is worth to the AI: its price
/// (`BuildUnit`).
fn troop_worth(kind: UnitType) -> f32 {
    let build = BuildUnit::ALL
        .into_iter()
        .find(|b| b.unit_type() == kind)
        .unwrap_or(BuildUnit::Melee);
    worth(build.price())
}

/// How the AI expects `force` to fare against the animals `guard`, under
/// `cover` (their terrain's defense bonus; its troops' counted as open
/// ground), by the combat formula (`combat::damage`, which has no random
/// spread). Each round is two attack steps: the animals strike, each at
/// the weakest of the force (as `animal_target` picks among those beside
/// it), then each troop left at the weakest animal its fellows haven't
/// already finished. A melee blow draws one back from a defender it doesn't
/// kill, and a step's blows land at its end. If the force kills them all
/// within `DEN_FIGHT_ROUNDS` rounds, what it loses: the HP its troops
/// lose, a dead one's all, by what it's worth (`troop_worth`); `None` if
/// it doesn't.
fn den_fight(force: &[Unit], guard: &[Unit], cover: f32) -> Option<f32> {
    let mut ours: Vec<Unit> = force.to_vec();
    let mut theirs: Vec<Unit> = guard.to_vec();
    // The one of `units` with the fewest HP left after `hurt`, if any has.
    let weakest = |units: &[Unit], hurt: &[f32]| {
        (0..units.len())
            .filter(|&i| units[i].hp > hurt[i])
            .min_by(|&a, &b| (units[a].hp - hurt[a]).total_cmp(&(units[b].hp - hurt[b])))
    };
    let land = |units: &mut Vec<Unit>, hurt: Vec<f32>| {
        for (unit, hurt) in units.iter_mut().zip(hurt) {
            unit.hp -= hurt;
        }
    };
    for _ in 0..DEN_FIGHT_ROUNDS {
        let (mut to_ours, mut to_theirs) = (vec![0.0; ours.len()], vec![0.0; theirs.len()]);
        let none = vec![0.0; ours.len()];
        for (a, animal) in theirs.iter().enumerate() {
            let Some(t) = weakest(&ours, &none) else {
                break;
            };
            let hit = combat::damage(animal, &ours[t], 1.0);
            to_ours[t] += hit;
            if combat::retaliates(animal, &ours[t], hit) {
                to_theirs[a] += combat::damage(&ours[t], animal, cover);
            }
        }
        land(&mut ours, to_ours);
        land(&mut theirs, to_theirs);
        theirs.retain(Unit::is_alive);
        let (mut to_ours, mut to_theirs) = (vec![0.0; ours.len()], vec![0.0; theirs.len()]);
        for (i, troop) in ours.iter().enumerate().filter(|(_, u)| u.is_alive()) {
            let Some(t) = weakest(&theirs, &to_theirs) else {
                break;
            };
            let hit = combat::damage(troop, &theirs[t], cover);
            to_theirs[t] += hit;
            if combat::retaliates(troop, &theirs[t], hit) {
                to_ours[i] += combat::damage(&theirs[t], troop, 1.0);
            }
        }
        land(&mut ours, to_ours);
        land(&mut theirs, to_theirs);
        theirs.retain(Unit::is_alive);
        if theirs.is_empty() || ours.iter().all(|u| !u.is_alive()) {
            break;
        }
    }
    let lost = |(was, now): (&Unit, &Unit)| {
        (was.hp - now.hp.max(0.0)) / was.max_hp() * troop_worth(was.unit_type)
    };
    (theirs.is_empty() && ours.iter().any(Unit::is_alive))
        .then(|| force.iter().zip(&ours).map(lost).sum())
}

/// What an AI side makes of an animal den it knows of, or of a band of
/// animals it sees that belong to none (`den_plans`).
struct DenPlan {
    pos: Hex,
    /// How far from `pos` the animals can strike: a den's, their
    /// territory and a hex more; a band's, as far as each could move and
    /// attack next turn (`threat_reach`).
    reach: i32,
    /// Whether it expects any animals there (`den_guard`).
    guarded: bool,
    /// Whether it's a den; else a band of stray animals, around the first
    /// of them (by hex coordinates).
    den: bool,
    /// The ids of the troops it sends to clear the den or kill the band
    /// (`den_force`); none if it leaves it alone.
    force: Vec<u32>,
    /// Whether the force goes in: all of it has gathered near the reach
    /// (within `DEN_GATHER` of it), or some of it is already inside.
    advancing: bool,
}

impl DenPlan {
    /// Whether one of its animals could strike `hex`: within its reach, if
    /// any are expected, and not a city center (`cities`), where none
    /// attacks.
    fn threatens(&self, hex: Hex, cities: &[Hex]) -> bool {
        self.guarded && hex.distance(self.pos) <= self.reach && !cities.contains(&hex)
    }
}

/// Units (scouts and settlers aside) the AI wants for each of its cities
/// before it spends on growth.
const AI_ARMY_PER_CITY: usize = 2;

/// The population from which an AI city trains a Settler to expand.
const AI_SETTLE_POPULATION: usize = 4;
/// How far from its side's nearest city an AI settler founds a new one: at
/// most this many hexes (and at least `MIN_CITY_DISTANCE`, as the rules
/// say).
const AI_SITE_MAX_DISTANCE: i32 = 10;
/// How many steps on foot from a settler (or its city) the AI looks for a
/// site.
const AI_SITE_STEPS: i32 = 14;
/// What each step on foot to a site takes off its value (`site_value`).
const AI_SITE_STEP_COST: i32 = 2;
/// How near a site the rules refused (a city its side hadn't seen was too
/// close) the AI won't try again.
const AI_REFUSED_RADIUS: i32 = 2;
/// How far around a site its land counts toward its value.
const AI_SITE_RADIUS: i32 = 2;

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
    /// stockpile or for supply (the income it counted on didn't come, or a
    /// city or citizens were lost) is emptied, which costs nothing, as
    /// they're unpaid, and planned again. Nothing is queued past the side's
    /// supply (`city/supply.rs`): at the cap the Barracks stays idle, and
    /// the city grows or gathers instead. The Barracks is
    /// the military building (`city/barracks.rs`): an idle one trains a
    /// Pikeman, paid in food alone, while its side sees more enemy mounted
    /// troops (Cavalry) than it has Pikemen alive and queued; else
    /// Cavalry or Armored when its deposits allow and the side can pay, else
    /// Melee, or Ranged for one in three (the other when the side can pay
    /// only for it). A city's own queue trains a worker
    /// first if it has none left; its workers then build a Barracks (sited
    /// by `ai_barracks_site`, paid when placed), and the queue grows; a city
    /// without a Barracks, built or going up, trains Melee itself, slowly, until the side has
    /// `AI_ARMY_PER_CITY` units per city, falling back on growth, and
    /// gathering when it can't pay for anything. A side with no scout, alive
    /// or queued, trains one after the worker. A city of
    /// `AI_SETTLE_POPULATION` or more expands, before it grows: it trains a
    /// Settler when its side has none out or queued and knows a site for a
    /// city (`ai_city_site`).
    fn plan_ai_cities(&mut self, team: Team, known: &Knowledge) {
        let fog = &known.fog;
        let queued_anywhere = |game: &GameState, build: Build| {
            game.cities
                .iter()
                .any(|c| c.team == team && c.queue.iter().any(|q| q.build == build))
        };
        let mut scouting = queued_anywhere(self, Build::Scout)
            || self
                .units
                .iter()
                .any(|u| u.team == team && u.unit_type == UnitType::Scout);
        let mut settling = queued_anywhere(self, Build::Settler)
            || self
                .units
                .iter()
                .any(|u| u.team == team && self.settlers.contains(&u.id));
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
        // Enemy mounted troops in sight, against the Pikemen it has or is
        // training: while there are more of them, a Barracks trains Pikemen.
        let mounted = self
            .units
            .iter()
            .filter(|u| u.team != team && u.unit_type.is_mounted() && fog.sees(u.pos))
            .count();
        let pike = BuildUnit::Pikeman;
        let mut pikes = soldiers(self, Some(UnitType::Pikeman))
            + cities
                .iter()
                .map(|&c| {
                    let c = &self.cities[c];
                    c.barracks_queue.iter().filter(|q| q.build == pike).count()
                        + c.queue
                            .iter()
                            .filter(|q| q.build == Build::Unit(pike))
                            .count()
                })
                .sum::<usize>();
        for &city in &cities {
            let stock = self.stock(team);
            self.place_ai_barracks(city, fog);
            // A job is paid when placed.
            spare -= stock - self.stock(team);
            if self.cities[city].barracks.is_some() && self.cities[city].barracks_queue.is_empty() {
                // Melee, or Ranged for one in three; they're paid in
                // different goods (food or wood, and metal), so when the
                // side can pay for only one, that one.
                let basics = if soldiers(self, Some(UnitType::Ranged)) * 2
                    < soldiers(self, Some(UnitType::Melee))
                {
                    [BuildUnit::Ranged, BuildUnit::Melee]
                } else {
                    [BuildUnit::Melee, BuildUnit::Ranged]
                };
                let against_mounted = (pikes < mounted).then_some(pike);
                let builds = against_mounted
                    .into_iter()
                    .chain([BuildUnit::Cavalry, BuildUnit::Armored]);
                for build in builds.chain(basics) {
                    if self.barracks_lock(city, build).is_none() && spare.covers(build.price()) {
                        self.queue_barracks(city, build);
                        spare -= build.price();
                        army += 1;
                        pikes += usize::from(build == pike);
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
            if !scouting {
                choices.push(Build::Scout);
            }
            // Not while its workers build a Barracks, which trains one
            // twice as fast.
            if c.barracks.is_none()
                && !self.building_job_queued(city, Building::Barracks)
                && army < AI_ARMY_PER_CITY * cities.len()
            {
                choices.push(melee);
            }
            if !settling
                && c.population >= AI_SETTLE_POPULATION
                && spare.covers(Build::Settler.price())
                && self.ai_city_site(team, c.pos, known, false).is_some()
            {
                choices.push(Build::Settler);
            }
            choices.push(Build::Grow);
            // With nothing it can pay for, it gathers.
            choices.push(Build::Gather);
            for build in choices {
                // Not past the population cap or the side's supply
                // (`city_build_issue`): a troop or scout waits for room.
                if (build == Build::Grow && !self.can_grow(city))
                    || self.city_build_issue(city, build).is_some()
                {
                    continue;
                }
                let price = self.queue_price(city, build);
                // A free Gather fits even when the Barracks job took more
                // than was spare.
                if price == Stock::default() || spare.covers(price) {
                    self.queue_build(city, build);
                    spare -= price;
                    army += usize::from(build == melee);
                    scouting |= build == Build::Scout;
                    settling |= build == Build::Settler;
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
                near.iter()
                    .find(|h| h.distance(c.pos) <= 2 && !self.cities.iter().any(|o| o.works(**h)))
            })
            .or_else(|| near.first())
            .copied()
    }

    /// `team`'s settlers found cities, or head for where they will. A
    /// settler founds where it stands when that's its site (`ai_city_site`)
    /// and the rules allow it (`founding_issue`); with no city yet, its side
    /// founds wherever the rules allow, at once, as a starting city. Else it
    /// walks toward its site, keeping out of the reach of enemies in sight
    /// where it can. A site the rules refuse (a city its side hadn't seen
    /// is too near) goes on its side's list (`refused_sites`) and it looks
    /// again. A settler with no site known waits for its side to see more:
    /// never founding closer than the rules allow, even a side's first city
    /// (one whose city was taken, its settler left beside it).
    fn plan_ai_settlers(
        &mut self,
        team: Team,
        known: &Knowledge,
        floods: &mut HashMap<Hex, Flood>,
    ) {
        let settlers: Vec<u32> = self
            .units
            .iter()
            .filter(|u| u.team == team && self.settlers.contains(&u.id))
            .filter(|u| !self.player_controlled_units.contains(&u.id))
            .map(|u| u.id)
            .collect();
        for id in settlers {
            let Some(idx) = self.units.iter().position(|u| u.id == id) else {
                continue;
            };
            let pos = self.units[idx].pos;
            let first = !self.cities.iter().any(|c| c.team == team);
            if first && self.founding_issue(pos).is_none() {
                self.found_city(id, team, pos);
                continue;
            }
            let mut site = self.ai_city_site(team, pos, known, first);
            if site == Some(pos) {
                if self.founding_issue(pos).is_none() {
                    self.found_city(id, team, pos);
                    continue;
                }
                self.refused_sites.push((team, pos));
                site = self.ai_city_site(team, pos, known, first);
            }
            // With no site known, it waits for its side to see more.
            let Some(site) = site else {
                continue;
            };
            let dest = self.settler_step(idx, site, known, floods);
            self.units[idx].planned_move = (dest != pos).then_some(dest);
            self.units[idx].planned_attack = None;
        }
    }

    /// Where settler `idx` steps toward `site`: of the hexes it can reach
    /// that no ally has claimed, those no enemy in sight could attack next
    /// turn (`threat_reach`) if there are any, the nearest `site` on foot.
    fn settler_step(
        &self,
        idx: usize,
        site: Hex,
        known: &Knowledge,
        floods: &mut HashMap<Hex, Flood>,
    ) -> Hex {
        let unit = &self.units[idx];
        let (team, pos) = (unit.team, unit.pos);
        let fog = &known.fog;
        let threats: Vec<(Hex, i32)> = self
            .units
            .iter()
            .filter(|enemy| enemy.team != team && fog.sees(enemy.pos))
            .filter_map(|enemy| Some((enemy.pos, threat_reach(enemy)?)))
            .collect();
        let safe = |hex: &Hex| {
            threats.iter().all(|&(at, reach)| at.distance(*hex) > reach)
                && !known.in_den_territory(*hex)
        };
        let mut reachable: Vec<Hex> = self
            .known_reachable_for_domain(pos, unit.stats().move_range, team, fog, false)
            .into_iter()
            .filter(|&hex| {
                hex == pos
                    || !self
                        .units
                        .iter()
                        .any(|u| u.team == team && u.planned_move == Some(hex))
            })
            .collect();
        reachable.sort_by_key(|h| (h.q, h.r));
        let safe_hexes: Vec<Hex> = reachable.iter().copied().filter(safe).collect();
        let options = if safe_hexes.is_empty() {
            &reachable
        } else {
            &safe_hexes
        };
        self.step_toward(site, pos, options, known, floods)
    }

    /// Where a settler of `team` at `from` would found a city, as its side
    /// knows the map: of the hexes within `AI_SITE_STEPS` steps on foot,
    /// one it has seen that the rules would allow as far as it knows
    /// (`known_site`), within `AI_SITE_MAX_DISTANCE` of its nearest city
    /// unless it has none. With a city, the best by its land
    /// (`site_value`) less `AI_SITE_STEP_COST` a step; with none (`first`),
    /// the nearest. Ties by hex coordinates.
    fn ai_city_site(&self, team: Team, from: Hex, known: &Knowledge, first: bool) -> Option<Hex> {
        let own: Vec<Hex> = self
            .cities
            .iter()
            .filter(|c| c.team == team)
            .map(|c| c.pos)
            .collect();
        let mut best: Option<(i32, Hex)> = None;
        let mut steps = 0;
        self.search_rings(from, known, |ring, _| {
            for &hex in ring {
                let near_home = own.iter().any(|c| c.distance(hex) <= AI_SITE_MAX_DISTANCE);
                if !(first || near_home) || !self.known_site(team, hex, known, &own) {
                    continue;
                }
                let score = if first {
                    -steps
                } else {
                    self.site_value(hex, &known.fog) - AI_SITE_STEP_COST * steps
                };
                let better = best.is_none_or(|(top, at)| {
                    (score, Reverse((hex.q, hex.r))) > (top, Reverse((at.q, at.r)))
                });
                if better {
                    best = Some((score, hex));
                }
            }
            steps += 1;
            // Stop: far enough, or (for a first city) the nearest found.
            (steps > AI_SITE_STEPS || (first && best.is_some())).then_some(from)
        });
        best.map(|(_, hex)| hex)
    }

    /// Whether `team` knows of nothing against founding a city on `hex`
    /// (`founding_issue`, as its side knows the map): seen, open land, no
    /// ruins or enemy in sight on it, `MIN_CITY_DISTANCE` from its cities
    /// (`own`) and the enemy cities it knows of, and not near a site the
    /// rules refused it (`AI_REFUSED_RADIUS`).
    fn known_site(&self, team: Team, hex: Hex, known: &Knowledge, own: &[Hex]) -> bool {
        let far = |city: &Hex| city.distance(hex) >= MIN_CITY_DISTANCE;
        self.explored_by(hex, &known.fog)
            && self.grid.is_passable(hex)
            && !self.grid.terrain(hex).is_water()
            && !known.ruins.contains(&hex)
            && !known.enemies.contains(&hex)
            && !known.in_den_territory(hex)
            && own.iter().all(far)
            && known.cities.iter().all(far)
            && !self
                .refused_sites
                .iter()
                .any(|&(side, at)| side == team && at.distance(hex) <= AI_REFUSED_RADIUS)
    }

    /// What a city on `hex` would have around it, as `fog`'s side knows the
    /// land: each seen hex within `AI_SITE_RADIUS`'s food twice, and its
    /// wood and metal once, and on ground a farm could go (fresh water,
    /// `HexGrid::farmable`) a farm's food twice too. Ground never seen
    /// counts nothing.
    fn site_value(&self, hex: Hex, fog: &Fog) -> i32 {
        within(hex, AI_SITE_RADIUS)
            .filter(|&h| self.grid.contains(h) && self.explored_by(h, fog))
            .map(|h| {
                let (food, wood, metal) = self.known_yield(h, fog);
                let farm = FARM_FOOD * i32::from(self.grid.farmable(h));
                2 * (food + farm) + wood + metal
            })
            .sum()
    }

    /// Each unit goes for the nearest on foot of the enemy units and workers
    /// its side sees, the ruins nobody on its side holds (`ruins.rs`) and
    /// the enemy cities its side has seen (`nearest_ai_target`); with none
    /// of those, it explores, heading for the nearest ground its side has
    /// never seen. An enemy it attacks if already in range, otherwise it
    /// moves as close as it can and attacks if that brings it into range;
    /// ruins it steps onto, and then holds until they're claimed; next to
    /// an enemy city, its fighters go in through the gates
    /// (`city/interior.rs`). A unit going elsewhere than an enemy attacks
    /// any enemy in range of where it ends up. Units in a contested hex stay
    /// and fight. Animals (`animals.rs`) are the exception: its side sends a
    /// force against a den or band of them only if it expects to win, and
    /// for more than it would lose (`den_plans`); the force gathers outside
    /// their reach, goes in together, fights the animals it sees and steps
    /// onto the den to clear it. Every other land unit keeps out of the
    /// reach of animals its side leaves alone, walking around it, and gets
    /// out if it's in it. Otherwise no coordination beyond not sending two
    /// units to the same hex, and no retreating, but for scouts, which scout
    /// and keep out of harm's way instead (`plan_ai_scout`). It all goes by
    /// what the side knows (`side_fog`), never the real board: an enemy out
    /// of sight isn't there, and ground never seen is open.
    pub(super) fn plan_ai_turn(&mut self, team: Team) {
        let fog = self.side_fog(team);
        let known = self.knowledge(fog);
        let mut floods: HashMap<Hex, Flood> = HashMap::default();
        // Settlers found their cities, or head for a site, before the cities
        // plan: a city founded now plans its first build this turn.
        self.plan_ai_settlers(team, &known, &mut floods);
        self.plan_ai_cities(team, &known);
        self.plan_ai_workers(team, |hex| {
            known.in_den_territory(hex) || known.in_danger(hex)
        });
        let mut watched = HashSet::default();
        for idx in 0..self.units.len() {
            if self.units[idx].team != team
                || self.player_controlled_units.contains(&self.units[idx].id)
                || self.settlers.contains(&self.units[idx].id)
            {
                continue;
            }
            // A scout even slips out of a contested hex.
            if self.units[idx].unit_type == UnitType::Scout {
                self.plan_ai_scout(idx, &known, &mut floods, &mut watched);
                continue;
            }
            if self.rival_of(idx).is_some() {
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
            let den = known.den_of(self.units[idx].id);
            // Sent to clear a den, it gathers with the rest of its force
            // outside the reach of the den's animals, then goes in with them.
            let target = match den {
                Some(den) if !den.advancing => {
                    self.gather_at_den(idx, den, &known, &mut floods);
                    continue;
                }
                Some(den) => Some(self.den_target(idx, den, &known)),
                None => self.nearest_ai_target(idx, &known),
            };
            // With nowhere to go, it still gets out of a den's reach.
            let Some(target) = target.or_else(|| (known.margin(pos, None) <= 0).then_some(pos))
            else {
                continue;
            };

            let fog = &known.fog;
            let unit = &self.units[idx];
            let stats = unit.stats();
            // An enemy it knows is there is fought. Anything else (a worker,
            // ruins, ground never seen) is stepped onto, and a city closed on.
            let enemy_there = fog.sees(target) && self.enemy_of_team_at(target, team).is_some();
            // Ships keep their water-only movement domain while land units use
            // roads and gates. A land unit keeps out of the reach of the dens
            // its side leaves alone if it can.
            let reachable = self.known_reachable_for_domain(
                unit.pos,
                stats.move_range,
                team,
                fog,
                unit.is_naval(),
            );
            let reachable: HashSet<Hex> = if unit.is_naval() {
                reachable
            } else {
                known.out_of_reach(reachable, None)
            };
            let claimed_by_ally = |hex: &Hex| {
                self.units
                    .iter()
                    .any(|u| u.team == team && u.id != unit.id && u.planned_move == Some(*hex))
            };
            // One unit steps onto a hex; a second would bounce off the first.
            let dest = if !enemy_there && reachable.contains(&target) && !claimed_by_ally(&target) {
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
    /// remembered; and what it makes of the animals (`den_plans`).
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
        let dens = self
            .dens
            .iter()
            .map(|den| den.pos)
            .filter(|&hex| fog.sees(hex))
            .chain(remembered(|seen, _| seen.den.is_some()))
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
        let mut city_centers: Vec<Hex> = self
            .cities
            .iter()
            .filter(|c| c.team == team)
            .map(|c| c.pos)
            .chain(cities.iter().copied())
            .collect();
        city_centers.sort_by_key(|h| (h.q, h.r));
        let den_plans = self.den_plans(&fog, &dens);
        for plan in den_plans.iter().filter(|p| p.force.is_empty()) {
            for hex in within(plan.pos, plan.reach) {
                if let Some(i) = self.grid.index(hex)
                    && plan.threatens(hex, &city_centers)
                {
                    closed[i] = true;
                }
            }
        }
        Knowledge {
            fog,
            enemies,
            ruins,
            dens,
            cities,
            gates,
            closed,
            walls,
            den_plans,
            city_centers,
        }
    }

    /// What `fog`'s side makes of each den it knows of (`dens`), by hex
    /// coordinates, and then of each band of animals it sees that belongs
    /// to none of them (strays: their den unknown, or cleared): the
    /// animals it expects guard it (`den_guard`; a band's are the band),
    /// and the troops it sends to clear it or kill them (`den_force`),
    /// each troop to one at most. One it sends none to it leaves alone,
    /// keeping its units out of the animals' reach.
    fn den_plans(&self, fog: &Fog, dens: &HashSet<Hex>) -> Vec<DenPlan> {
        let mut dens: Vec<Hex> = dens.iter().copied().collect();
        dens.sort_by_key(|h| (h.q, h.r));
        let mut taken = HashSet::default();
        let mut plans = Vec::new();
        let mut plan =
            |pos: Hex, reach: i32, guard: Vec<Unit>, den: bool, taken: &mut HashSet<u32>| {
                let force = self.den_force(fog.team(), pos, &guard, den, taken);
                taken.extend(force.iter().copied());
                let near = |id: &u32, radius: i32| {
                    self.units
                        .iter()
                        .find(|u| u.id == *id)
                        .is_some_and(|u| u.pos.distance(pos) <= radius)
                };
                let advancing = guard.is_empty()
                    || force.iter().all(|id| near(id, reach + DEN_GATHER))
                    || force.iter().any(|id| near(id, reach));
                plans.push(DenPlan {
                    pos,
                    reach,
                    guarded: !guard.is_empty(),
                    den,
                    force,
                    advancing,
                });
            };
        let mut kinds = Vec::new();
        for pos in dens {
            let kind = self.known_den_kind(pos, fog);
            kinds.push((pos, kind));
            let reach = territory(kind) + kind.stats().attack_range;
            plan(pos, reach, self.den_guard(pos, kind, fog), true, &mut taken);
        }
        // Strays, by hex coordinates, each band those within
        // `STRAY_BAND` of one another.
        let mut strays: Vec<&Unit> = self
            .units
            .iter()
            .filter(|u| u.is_animal() && fog.sees(u.pos))
            .filter(|u| {
                !kinds.iter().any(|&(den, kind)| {
                    kind == u.unit_type && den.distance(u.pos) <= territory(kind)
                })
            })
            .collect();
        strays.sort_by_key(|u| (u.pos.q, u.pos.r));
        while let Some(first) = strays.first() {
            let pos = first.pos;
            let (band, rest): (Vec<&Unit>, Vec<&Unit>) = strays
                .iter()
                .partition(|u| u.pos.distance(pos) <= STRAY_BAND);
            strays = rest;
            let reach = band
                .iter()
                .filter_map(|u| Some(u.pos.distance(pos) + threat_reach(u)?))
                .max()
                .unwrap_or(0);
            let guard = band.into_iter().cloned().collect();
            plan(pos, reach, guard, false, &mut taken);
        }
        plans
    }

    /// The animal a den on `hex` keeps, as `fog`'s side knows: in sight,
    /// the den's; else as last seen.
    fn known_den_kind(&self, hex: Hex, fog: &Fog) -> UnitType {
        let seen = self.den_at(hex).filter(|_| fog.sees(hex)).map(|d| d.kind);
        seen.or_else(|| self.memory_of(fog).get(&hex).and_then(|s| s.den))
            .unwrap_or(UnitType::Wolf)
    }

    /// The animals `fog`'s side expects guard the den on `den`, keeping
    /// `kind`: those of its kind it sees within their territory of it, as
    /// they are; and unless it sees the whole territory, more at full
    /// health up to as many as the den can have by now. A den starts with
    /// one and adds one every `DEN_BREED_TURNS` turns up to its cap
    /// (`den_cap`), the same for every den of a world: the Animals setting,
    /// which every side knows.
    fn den_guard(&self, den: Hex, kind: UnitType, fog: &Fog) -> Vec<Unit> {
        let range = territory(kind);
        let mut guard: Vec<Unit> = self
            .units
            .iter()
            .filter(|u| u.is_animal() && u.unit_type == kind)
            .filter(|u| fog.sees(u.pos) && u.pos.distance(den) <= range)
            .cloned()
            .collect();
        let all_seen = within(den, range)
            .filter(|&h| self.grid.contains(h))
            .all(|h| fog.sees(h));
        if !all_seen {
            let cap = self
                .dens
                .iter()
                .map(|d| d.cap)
                .max()
                .unwrap_or(den_cap(ANIMALS_MANY));
            let born = 1 + (self.turn / DEN_BREED_TURNS) as usize;
            while guard.len() < cap.min(born) {
                guard.push(Unit::new(0, den, Team::Wild, kind));
            }
        }
        guard
    }

    /// The troops `team` sends to clear the den on `den` against its
    /// `guard` (`den_guard`): of its troops within `DEN_FORCE_RANGE` of it
    /// and not `taken` for another den, the nearest few (at most
    /// `DEN_MAX_FORCE`, ties to the lowest id) expected to kill the whole
    /// guard (`den_fight`) for the most the den is worth over what they'd
    /// lose: its spoils, what else clearing it gains (`den_riddance`) and
    /// the animals' bounties (`animals.rs`), less the HP they'd lose, by
    /// what it's worth. The
    /// fewest on a tie; none if no such force comes out ahead. Ships,
    /// scouts, settlers, player-controlled units, units in a contested hex
    /// and those holding ruins aren't sent.
    fn den_force(
        &self,
        team: Team,
        pos: Hex,
        guard: &[Unit],
        den: bool,
        taken: &HashSet<u32>,
    ) -> Vec<u32> {
        let mut near: Vec<usize> = (0..self.units.len())
            .filter(|&i| {
                let u = &self.units[i];
                u.team == team
                    && u.pos.distance(pos) <= DEN_FORCE_RANGE
                    && !u.is_naval()
                    && u.unit_type != UnitType::Scout
                    && !self.settlers.contains(&u.id)
                    && !self.player_controlled_units.contains(&u.id)
                    && !taken.contains(&u.id)
                    && self.rival_of(i).is_none()
                    && self.ruin_at(u.pos).is_none()
            })
            .collect();
        near.sort_by_key(|&i| (self.units[i].pos.distance(pos), self.units[i].id));
        let spoils = if den {
            worth(DEN_SPOILS) + self.den_riddance(team, pos)
        } else {
            0.0
        };
        let reward = spoils
            + guard
                .iter()
                .map(|a| worth(bounty(a.unit_type)))
                .sum::<f32>();
        let cover = self.grid.tile(pos).defense_multiplier();
        let mut best: Option<(f32, usize)> = None;
        for size in 1..=near.len().min(DEN_MAX_FORCE) {
            let force: Vec<Unit> = near[..size]
                .iter()
                .map(|&i| self.units[i].clone())
                .collect();
            let Some(loss) = den_fight(&force, guard, cover) else {
                continue;
            };
            let value = reward - loss;
            if value > 0.0 && best.is_none_or(|(top, _)| value > top) {
                best = Some((value, size));
            }
        }
        best.map_or_else(Vec::new, |(_, size)| {
            near[..size].iter().map(|&i| self.units[i].id).collect()
        })
    }

    /// What `team` gains by clearing the den on `den` beyond its spoils,
    /// in `worth`: the strays it won't send out over `DEN_HORIZON_TURNS`
    /// (one every `DEN_BREED_TURNS`, in a world that keeps any: the
    /// Animals setting, which every side knows), each `STRAY_HARM_WORKERS`
    /// workers' prices; and with a city of its own within
    /// `AI_SITE_MAX_DISTANCE`, its land, `DEN_LAND_SETTLERS` of a settler's.
    fn den_riddance(&self, team: Team, den: Hex) -> f32 {
        let strays = if self.stray_limit > 0 {
            let harm = STRAY_HARM_WORKERS * worth(Build::Worker.price());
            (DEN_HORIZON_TURNS / DEN_BREED_TURNS) as f32 * harm
        } else {
            0.0
        };
        let near = self
            .cities
            .iter()
            .any(|c| c.team == team && c.pos.distance(den) <= AI_SITE_MAX_DISTANCE);
        let land = DEN_LAND_SETTLERS * worth(Build::Settler.price());
        strays + if near { land } else { 0.0 }
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

    /// Where unit `idx` heads (see `plan_ai_turn`): of the enemy units in
    /// sight (animals aside, which are for the forces sent against them),
    /// the enemy workers alone in sight, and ruins (and dens no animal is
    /// expected at and no force is sent to, `den_plans`) that no ally holds
    /// or is already heading for, and the enemy cities seen, the nearest on
    /// foot over the ground as its side knows it (around the reach of
    /// animals it leaves alone; a city counting from its gates, and an
    /// enemy before a city as near), ties to the lowest coordinates. With
    /// none reachable, the
    /// nearest ground its side has never seen; with none left, the enemy in
    /// sight nearest as the crow flies that it could hit (a ship only for a
    /// unit that can shoot at water). A ship goes by the crow's flight: to
    /// the nearest enemy in sight, else the nearest ground never seen.
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
        let claimed: HashSet<Hex> = self
            .units
            .iter()
            .filter(|u| u.team == team && u.id != unit.id)
            .filter_map(|u| u.planned_move)
            .collect();
        let held = |pos: Hex| self.units.iter().any(|u| u.team == team && u.pos == pos);
        // An enemy troop in sight is fought, however many allies go for it;
        // ruins, a den to clear and an enemy worker alone (captured by
        // stepping onto it) take one unit, so none an ally holds or already
        // heads for.
        let troop =
            |pos: Hex| known.enemies.contains(&pos) && self.enemy_of_team_at(pos, team).is_some();
        // Animals, and dens, are for the forces sent against them
        // (`den_force`); but a den no animal is expected at and no troop is
        // near enough to be sent to, for anyone.
        let lone_den = |pos: &Hex| {
            known
                .den_plans
                .iter()
                .any(|p| p.den && p.pos == *pos && !p.guarded && p.force.is_empty())
        };
        let animal = |pos: Hex| {
            known.in_danger(pos) && self.units.iter().any(|u| u.pos == pos && u.is_animal())
        };
        let targets: HashSet<Hex> = known
            .ruins
            .iter()
            .chain(known.dens.iter().filter(|pos| lone_den(pos)))
            .copied()
            .filter(|&pos| !held(pos))
            .chain(known.enemies.iter().copied().filter(|&pos| !animal(pos)))
            .filter(|&pos| troop(pos) || !claimed.contains(&pos))
            .collect();
        let is_target = |hex: &Hex| targets.contains(hex);
        let lowest = |hexes: &mut dyn Iterator<Item = Hex>| hexes.min_by_key(|hex| (hex.q, hex.r));
        // With nothing known to go for, the nearest ground never seen is it.
        let exploring = targets.is_empty() && known.cities.is_empty();
        let mut unexplored = None;
        // A city is as far as its gates, plus one.
        let found = self.search_rings(unit.pos, known, |ring, last_ring| {
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
                if exploring {
                    return unexplored;
                }
            }
            None
        });
        // A ship is only worth chasing along the shore for one that can hit
        // it there.
        let can_hit = |hex: &Hex| {
            unit.attacks_water()
                || !(self.grid.contains(*hex) && self.grid.terrain(*hex).is_water())
        };
        found.or(unexplored).or_else(|| {
            let enemies = targets.iter().copied();
            nearest(&mut enemies.filter(|hex| known.enemies.contains(hex) && can_hit(hex)))
        })
    }

    /// Where troop `idx`, going in to clear `den` with its force, heads: the
    /// nearest of the den's animals in sight (as the crow flies, ties to
    /// the lowest coordinates), to fight it, else the den itself.
    fn den_target(&self, idx: usize, den: &DenPlan, known: &Knowledge) -> Hex {
        let pos = self.units[idx].pos;
        self.units
            .iter()
            .filter(|u| u.is_animal() && known.fog.sees(u.pos))
            .map(|u| u.pos)
            .filter(|&h| den.threatens(h, &known.city_centers))
            .min_by_key(|h| (h.distance(pos), h.q, h.r))
            .unwrap_or(den.pos)
    }

    /// Troop `idx`, sent to clear `den` with a force not all gathered yet,
    /// steps as near the den on foot as it can while staying out of the
    /// reach of its animals (and of any den its side leaves alone), and
    /// attacks any enemy in range of where it ends up.
    fn gather_at_den(
        &mut self,
        idx: usize,
        den: &DenPlan,
        known: &Knowledge,
        floods: &mut HashMap<Hex, Flood>,
    ) {
        let unit = &self.units[idx];
        let (team, pos) = (unit.team, unit.pos);
        let claimed = |hex: Hex| {
            self.units
                .iter()
                .any(|u| u.team == team && u.id != unit.id && u.planned_move == Some(hex))
        };
        let reachable = self
            .known_reachable_for_domain(pos, unit.stats().move_range, team, &known.fog, false)
            .into_iter()
            .filter(|&hex| hex == pos || !claimed(hex));
        let mut options: Vec<Hex> = known.out_of_reach(reachable, Some(den));
        options.sort_by_key(|h| (h.q, h.r));
        let dest = self.step_toward(den.pos, pos, &options, known, floods);
        self.units[idx].planned_move = (dest != pos).then_some(dest);
        self.units[idx].planned_attack = self.ai_attack_from(idx, dest, &known.fog);
    }

    /// Searches outward from `start` ring by ring (each the hexes one more
    /// step on foot away) over the ground as `known`'s side knows it,
    /// handing `pick` each ring and the one before it until it picks a hex.
    fn search_rings(
        &self,
        start: Hex,
        known: &Knowledge,
        mut pick: impl FnMut(&[Hex], &[Hex]) -> Option<Hex>,
    ) -> Option<Hex> {
        let mut seen = vec![false; self.grid.cells()];
        if let Some(i) = self.grid.index(start) {
            seen[i] = true;
        }
        let mut ring = vec![start];
        let mut last_ring: Vec<Hex> = Vec::new();
        while !ring.is_empty() || !last_ring.is_empty() {
            if let Some(hex) = pick(&ring, &last_ring) {
                return Some(hex);
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
        None
    }

    /// Scout `idx` of the AI gathers what its side knows and stays alive,
    /// rather than fight. It never ends a move where an enemy in sight could
    /// reach and attack it next turn (`threat_reach`) if it can help it;
    /// hurt (under half health), it keeps a hex farther off still, and heads
    /// home to its side's nearest city. Of the safe hexes it can reach it
    /// takes, first, the nearest with an enemy worker alone (captured, since
    /// a worker can't hit back) or ruins (held while safe); else the one
    /// that brings the most into sight that its side has never seen or not
    /// seen for a while (`scouting_value`; hills see farther), farthest from
    /// its side's cities on a tie, so scouts fan out; with nothing new
    /// within reach, it heads for the nearest ground that has
    /// (`nearest_unscouted`). With no safe hex it's cornered: it takes the
    /// least threatened, and fights back from there. `watched` is what the
    /// side's scouts planned so far this turn will see, which the next one
    /// doesn't count again.
    fn plan_ai_scout(
        &mut self,
        idx: usize,
        known: &Knowledge,
        floods: &mut HashMap<Hex, Flood>,
        watched: &mut HashSet<Hex>,
    ) {
        let unit = &self.units[idx];
        let (team, pos) = (unit.team, unit.pos);
        let fog = &known.fog;
        let hurt = unit.hp < unit.max_hp() / 2.0;
        let threats: Vec<(Hex, i32)> = self
            .units
            .iter()
            .filter(|enemy| enemy.team != team && fog.sees(enemy.pos))
            .filter_map(|enemy| Some((enemy.pos, threat_reach(enemy)? + i32::from(hurt))))
            .collect();
        let threatened = |hex: Hex| {
            threats
                .iter()
                .filter(|&&(at, reach)| at.distance(hex) <= reach)
                .count()
                + usize::from(known.in_danger(hex))
        };
        let claimed = |hex: Hex| {
            self.units
                .iter()
                .any(|u| u.team == team && u.planned_move == Some(hex))
        };
        let mut reachable: Vec<Hex> = self
            .known_reachable_for_domain(pos, unit.stats().move_range, team, fog, false)
            .into_iter()
            .filter(|&hex| hex == pos || !claimed(hex))
            .collect();
        reachable.sort_by_key(|h| (h.q, h.r));
        let safe: Vec<Hex> = reachable
            .iter()
            .copied()
            .filter(|&hex| threatened(hex) == 0)
            .collect();
        let homes: Vec<Hex> = self
            .cities
            .iter()
            .filter(|c| c.team == team)
            .map(|c| c.pos)
            .collect();
        let prey = |hex: Hex| {
            let worker = known.enemies.contains(&hex) && self.enemy_of_team_at(hex, team).is_none();
            let den = known.dens.contains(&hex) && !known.in_danger(hex);
            let ruins = (known.ruins.contains(&hex) || den) && !known.enemies.contains(&hex);
            worker || ruins
        };

        let mut attack = None;
        let dest = if safe.is_empty() {
            // Out of reach of as many as it can, as far as it can get, and
            // toward home: two scouts fleeing each other from one hex would
            // otherwise pick the same way, and meet again.
            let nearest_threat = |hex: Hex| {
                threats
                    .iter()
                    .map(|&(at, _)| at.distance(hex))
                    .min()
                    .unwrap_or(i32::MAX)
            };
            let to_home = |hex: Hex| homes.iter().map(|&c| c.distance(hex)).min().unwrap_or(0);
            let dest = reachable
                .iter()
                .copied()
                .min_by_key(|&hex| {
                    let far = Reverse(nearest_threat(hex));
                    (threatened(hex), far, to_home(hex), hex != pos, hex.q, hex.r)
                })
                .unwrap_or(pos);
            // Cornered, with an enemy still beside it: it fights back.
            if nearest_threat(dest) <= 1 {
                attack = self.ai_attack_from(idx, dest, fog);
            }
            dest
        } else if let Some(hex) = safe
            .iter()
            .copied()
            .filter(|&hex| prey(hex))
            .min_by_key(|&hex| (hex.distance(pos), hex.q, hex.r))
        {
            hex
        } else if let Some(home) = homes
            .iter()
            .copied()
            .filter(|_| hurt)
            .min_by_key(|&c| (c.distance(pos), c.q, c.r))
        {
            self.step_toward(home, pos, &safe, known, floods)
        } else {
            let from_home = |hex: Hex| homes.iter().map(|&c| c.distance(hex)).min().unwrap_or(0);
            let best = safe
                .iter()
                .copied()
                .map(|hex| (self.scouting_value(hex, known, watched), hex))
                .max_by_key(|&(value, hex)| (value, from_home(hex), Reverse((hex.q, hex.r))));
            match best {
                Some((value, hex)) if value > 0 => hex,
                _ => match self.nearest_unscouted(pos, known, watched) {
                    Some(goal) => self.step_toward(goal, pos, &safe, known, floods),
                    // Nothing left to see: the nearest safe hex, which is
                    // where it stands if that's safe.
                    None => safe
                        .iter()
                        .copied()
                        .min_by_key(|&hex| (hex.distance(pos), hex.q, hex.r))
                        .unwrap_or(pos),
                },
            }
        };
        watched.extend(within(dest, self.sight_at(UnitType::Scout, dest)));
        let unit = &mut self.units[idx];
        unit.planned_move = (dest != pos).then_some(dest);
        unit.planned_attack = attack;
    }

    /// How much a scout standing on `hex` would see that `known`'s side
    /// wants seen (`unscouted`), and no other scout of its sees this turn
    /// (`watched`): what's in range and not behind a mountain.
    fn scouting_value(&self, hex: Hex, known: &Knowledge, watched: &HashSet<Hex>) -> u32 {
        within(hex, self.sight_at(UnitType::Scout, hex))
            .filter(|h| self.grid.contains(*h) && !watched.contains(h))
            .filter(|&h| self.in_line_of_sight(hex, h))
            .map(|h| self.unscouted(h, known))
            .sum()
    }

    /// How much `known`'s side wants `hex` seen: 2 if it never has been, 1 if
    /// not for `SCOUT_STALE_TURNS`, else 0.
    fn unscouted(&self, hex: Hex, known: &Knowledge) -> u32 {
        match self.recalled(hex, &known.fog) {
            None => 2,
            Some(seen) => u32::from(self.turn.saturating_sub(seen.turn) >= SCOUT_STALE_TURNS),
        }
    }

    /// The nearest ground on foot from `from` that `known`'s side wants seen
    /// (`unscouted`) and no scout of its sees this turn, ties to the lowest
    /// coordinates.
    fn nearest_unscouted(
        &self,
        from: Hex,
        known: &Knowledge,
        watched: &HashSet<Hex>,
    ) -> Option<Hex> {
        self.search_rings(from, known, |ring, _| {
            ring.iter()
                .copied()
                .filter(|h| !watched.contains(h) && self.unscouted(*h, known) > 0)
                .min_by_key(|h| (h.q, h.r))
        })
    }

    /// Of `options`, where a unit at `pos` gets nearest `goal` on foot
    /// (`steps_to`), staying put on a tie, then by coordinates.
    fn step_toward(
        &self,
        goal: Hex,
        pos: Hex,
        options: &[Hex],
        known: &Knowledge,
        floods: &mut HashMap<Hex, Flood>,
    ) -> Hex {
        let flood = floods.entry(goal).or_insert_with(|| self.flood_from(goal));
        self.steps_to(flood, known, &options.iter().copied().collect());
        let steps = |hex: Hex| self.grid.index(hex).map_or(NOT_FOUND, |i| flood.steps[i]);
        options
            .iter()
            .copied()
            .min_by_key(|&hex| (steps(hex), hex != pos, hex.q, hex.r))
            .unwrap_or(pos)
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
    use crate::game::city::City;
    use crate::game::hex::HexGrid;
    use crate::game::terrain::{Terrain, Tile};
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
        lone_red_unit(UnitType::Melee)
    }

    /// `lone_red`, with a unit of `unit_type`.
    fn lone_red_unit(unit_type: UnitType) -> GameState {
        let mut game = GameState::new();
        game.grid = HexGrid::new(8, [(Hex::new(0, 0), Tile::default())]);
        game.units = vec![Unit::new(1, Hex::new(0, 0), Team::Red, unit_type)];
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

    /// `lone_red`, with a wolf den at (6, 0) seen by Red and its wolf dead,
    /// so the den stands empty.
    fn red_knows_an_empty_den() -> GameState {
        let mut game = lone_red();
        red_explores_the_rest(&mut game);
        game.make_dens(&[Hex::new(6, 0)], 1);
        game.units.retain(|u| !u.is_animal());
        red_glances_from(&mut game, Hex::new(4, 0));
        game
    }

    #[test]
    fn an_ai_unit_goes_to_clear_a_den_its_side_knows_of() {
        let mut game = red_knows_an_empty_den();
        let (dest, _) = replan(&mut game);
        assert_eq!(dest, Some(Hex::new(1, 0)), "toward it");
        // Once there, it stands on the den, and its side clears it.
        game.units[0].pos = Hex::new(5, 0);
        let (dest, _) = replan(&mut game);
        assert_eq!(dest, Some(Hex::new(6, 0)));
        game.units[0].pos = Hex::new(6, 0);
        game.resolve_dens();
        assert!(game.dens.is_empty());
    }

    #[test]
    fn the_ai_expects_the_fight_the_combat_formula_gives() {
        let unit = |kind| Unit::new(1, Hex::new(0, 0), Team::Red, kind);
        let animals = |kind, n| vec![Unit::new(2, Hex::new(1, 0), Team::Wild, kind); n];
        let melee = |n| vec![unit(UnitType::Melee); n];
        // On open ground a melee troop beats a wolf pack, hurt: it's
        // bitten, bites back, then finishes it.
        let wolf = animals(UnitType::Wolf, 1);
        let bite = combat::damage(&wolf[0], &unit(UnitType::Melee), 1.0);
        let loss = den_fight(&melee(1), &wolf, 1.0).expect("won");
        let expected = bite / 100.0 * troop_worth(UnitType::Melee);
        assert!((loss - expected).abs() < 1e-4, "{loss} {expected}");
        // Two wolves kill it, and so does a bear; three wolves take one of
        // four with them.
        assert_eq!(den_fight(&melee(1), &animals(UnitType::Wolf, 2), 1.0), None);
        assert_eq!(den_fight(&melee(1), &animals(UnitType::Bear, 1), 1.0), None);
        let three = den_fight(&melee(4), &animals(UnitType::Wolf, 3), 1.0).expect("won");
        assert!(three >= troop_worth(UnitType::Melee), "{three}");
        // Cover makes the animals harder to kill.
        let wolves = animals(UnitType::Wolf, 2);
        let open = den_fight(&melee(2), &wolves, 1.0).expect("won");
        let hills = den_fight(&melee(2), &wolves, 1.25);
        assert!(hills.is_none_or(|hills| hills > open));
    }

    /// `lone_red` on turn 40, its side having seen the whole map, with a
    /// wolf den at (6, 0) that keeps up to three wolf packs (one at home,
    /// out of Red's sight) and that Red has seen.
    fn red_knows_a_full_den() -> GameState {
        let mut game = lone_red();
        red_explores_the_rest(&mut game);
        game.turn = 40;
        game.make_dens(&[Hex::new(6, 0)], 3);
        red_glances_from(&mut game, Hex::new(4, 0));
        game
    }

    /// Red's plans for the animals it knows of (`den_plans`).
    fn red_den_plans(game: &mut GameState) -> Vec<DenPlan> {
        let fog = game.side_fog(Team::Red);
        game.knowledge(fog).den_plans
    }

    /// Plans Red's turn afresh: each of its units' planned move, by id.
    fn replan_red(game: &mut GameState) -> Vec<(u32, Option<Hex>)> {
        for unit in &mut game.units {
            unit.planned_move = None;
            unit.planned_attack = None;
        }
        game.plan_ai_turn(Team::Red);
        game.units
            .iter()
            .filter(|u| u.team == Team::Red)
            .map(|u| (u.id, u.planned_move))
            .collect()
    }

    #[test]
    fn an_ai_side_leaves_a_guarded_den_alone_unless_a_force_it_has_can_clear_it() {
        let mut game = red_knows_a_full_den();
        let den = Hex::new(6, 0);
        // Red sees none of the den's animals, but by turn 40 a den may have
        // its three: too many for a lone melee troop, so it stays away.
        let plan = &red_den_plans(&mut game)[0];
        assert!(plan.den && plan.guarded && plan.force.is_empty());
        assert_eq!(plan.reach, 5);
        assert_eq!(replan(&mut game), (None, None), "not toward it");
        // Inside their reach, it gets out.
        game.units[0].pos = Hex::new(3, 0);
        let (dest, _) = replan(&mut game);
        assert_eq!(dest, Some(Hex::new(2, 0)));
        // What it expects comes only from what its side knows: with the
        // wolf out of its sight dead, it plans the same.
        let mut wolfless = game.clone();
        wolfless.units.retain(|u| !u.is_animal());
        assert_eq!(replan(&mut wolfless), replan(&mut game));
        game.units[0].pos = Hex::new(0, 0);

        // Four melee troops can kill three wolf packs, for more than the
        // one they'd lose: it sends them, nearest first, and gathered as
        // they are, they go in together.
        for (id, pos) in [
            (2, Hex::new(0, 2)),
            (3, Hex::new(2, -2)),
            (4, Hex::new(-1, 3)),
        ] {
            game.units
                .push(Unit::new(id, pos, Team::Red, UnitType::Melee));
        }
        let plan = &red_den_plans(&mut game)[0];
        assert_eq!(plan.force, [1, 2, 3, 4]);
        assert!(plan.advancing);
        let moves = replan_red(&mut game);
        for &(id, dest) in &moves {
            let unit = game.units.iter().find(|u| u.id == id).unwrap();
            let dest = dest.unwrap_or_else(|| panic!("each goes: {moves:?}"));
            assert_eq!(dest.distance(den), unit.pos.distance(den) - 1, "unit {id}");
        }
        // With one far behind, the others wait for it at the edge of the
        // animals' reach.
        let far = game.units.iter().position(|u| u.id == 2).unwrap();
        game.units[far].pos = Hex::new(-5, 0);
        assert!(!red_den_plans(&mut game)[0].advancing);
        let moves = replan_red(&mut game);
        assert!(moves.contains(&(1, None)), "{moves:?}");
        assert!(moves.contains(&(3, None)), "{moves:?}");
        assert!(moves.contains(&(2, Some(Hex::new(-4, 0)))), "{moves:?}");
    }

    #[test]
    fn clearing_a_den_is_worth_the_strays_it_would_send_out_and_its_land() {
        // Red, three cavalry, and a bear den at (6, 0) with its bear, which
        // Red has seen.
        let mut game = lone_red_unit(UnitType::Cavalry);
        red_explores_the_rest(&mut game);
        game.make_dens(&[Hex::new(-8, 4), Hex::new(6, 0)], 1);
        game.dens.remove(0);
        game.units.retain(|u| u.home != Some(Hex::new(-8, 4)));
        for (id, pos) in [(2, Hex::new(0, 1)), (3, Hex::new(1, -1))] {
            game.units
                .push(Unit::new(id, pos, Team::Red, UnitType::Cavalry));
        }
        red_glances_from(&mut game, Hex::new(4, 0));
        let den = Hex::new(6, 0);
        // Three troops kill a bear, but for more than its bounty and the
        // den's spoils: in a world with no strays and no city of its own
        // near, it leaves the den alone.
        let fight = [(); 3].map(|_| Unit::new(1, Hex::new(0, 0), Team::Red, UnitType::Cavalry));
        let bear = [Unit::new(9, den, Team::Wild, UnitType::Bear)];
        let loss = den_fight(&fight, &bear, 1.0).expect("won");
        let spoils = worth(DEN_SPOILS) + worth(bounty(UnitType::Bear));
        assert!(spoils < loss, "{spoils} {loss}");
        assert_eq!(game.den_riddance(Team::Red, den), 0.0);
        assert!(red_den_plans(&mut game)[0].force.is_empty());
        // Left alone, it sends out strays: clearing it is worth them too.
        let mut strays = game.clone();
        strays.stray_limit = 4;
        let riddance = 2.0 * worth(Build::Worker.price());
        assert_eq!(strays.den_riddance(Team::Red, den), riddance);
        assert_eq!(red_den_plans(&mut strays)[0].force, [1, 2, 3]);
        // With a city of its own near, so is its land, open to settle.
        game.cities = vec![City::new(0, Team::Red, Hex::new(-4, 0))];
        let land = worth(Build::Settler.price()) / 4.0;
        assert_eq!(game.den_riddance(Team::Red, den), land);
        assert_eq!(red_den_plans(&mut game)[0].force, [1, 2, 3]);
        // Not with its city farther than it would found another from.
        game.cities[0].pos = Hex::new(-5, 0);
        assert_eq!(game.den_riddance(Team::Red, den), 0.0);
    }

    #[test]
    fn an_ai_side_forgets_a_cleared_den_once_it_sees_it_gone() {
        let mut game = red_knows_an_empty_den();
        let den = Hex::new(6, 0);
        let own = [];
        // Cleared by someone else out of Red's sight: Red still remembers
        // it there, and keeps its sites out of its territory.
        game.dens.clear();
        assert_eq!(game.founding_issue(den), None, "free to settle");
        let fog = game.side_fog(Team::Red);
        let known = game.knowledge(fog);
        assert!(known.dens.contains(&den));
        assert!(!game.known_site(Team::Red, den, &known, &own));
        // Once it sees the den gone, it forgets it: the land is a site.
        red_glances_from(&mut game, Hex::new(4, 0));
        let fog = game.side_fog(Team::Red);
        let known = game.knowledge(fog);
        assert!(known.dens.is_empty());
        assert!(!known.in_den_territory(Hex::new(4, 0)));
        assert!(game.known_site(Team::Red, den, &known, &own));
    }

    #[test]
    fn an_ai_side_keeps_settlers_sites_and_workers_out_of_a_dens_territory() {
        let mut game = red_knows_an_empty_den();
        let fog = game.side_fog(Team::Red);
        let known = game.knowledge(fog);
        assert!(known.in_den_territory(Hex::new(2, 0)));
        assert!(!known.in_den_territory(Hex::new(1, 0)));
        // No city site within its reach.
        let own = [];
        assert!(!game.known_site(Team::Red, Hex::new(3, 1), &known, &own));
        assert!(game.known_site(Team::Red, Hex::new(-3, 1), &known, &own));
        // A settler heading east steps around it: its step stays out.
        game.settlers.insert(1);
        game.units[0].pos = Hex::new(1, 0);
        let mut floods = HashMap::default();
        let step = game.settler_step(0, Hex::new(8, 0), &known, &mut floods);
        assert_ne!(step, Hex::new(2, 0), "the straight way is through it");
        assert!(!known.in_den_territory(step), "{step:?}");
        // A city working tiles on both sides improves only the safe one.
        game.settlers.clear();
        game.units.clear();
        game.cities = vec![City::new(0, Team::Red, Hex::new(-1, 0))];
        let (inside, outside) = (Hex::new(2, 0), Hex::new(-3, 0));
        game.cities[0].clusters = crate::game::city::one_cluster(&[inside, outside]);
        game.cities[0].population = 2;
        game.plan_ai_workers(Team::Red, |hex| known.in_den_territory(hex));
        let jobs: Vec<Hex> = game.cities[0].worker_jobs.iter().map(|j| j.hex).collect();
        assert_eq!(jobs, [outside]);
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

    /// A lone Red scout at the origin, its side having seen the whole map.
    fn red_scout() -> GameState {
        let mut game = lone_red_unit(UnitType::Scout);
        red_explores_the_rest(&mut game);
        game
    }

    #[test]
    fn a_scout_keeps_out_of_reach_of_an_enemy_it_sees() {
        let mut game = red_scout();
        // A Blue melee two hexes off could step in and hit it next turn.
        // The AI's scouts used to go for it; now it's kept away from.
        let blue = Hex::new(2, 0);
        game.units
            .push(Unit::new(2, blue, Team::Blue, UnitType::Melee));
        let (dest, attack) = replan(&mut game);
        let reach = threat_reach(&game.units[1]).unwrap();
        assert_eq!(reach, 2);
        assert!(dest.is_some_and(|d| d.distance(blue) > reach), "{dest:?}");
        assert_eq!(attack, None);
    }

    #[test]
    fn a_scout_heads_for_ground_its_side_has_never_seen() {
        let mut game = red_scout();
        // Nothing east of q = 4 has been seen.
        Arc::make_mut(&mut game.side_memory[Team::Red.index()]).retain(|h, _| h.q < 5);
        let (dest, _) = replan(&mut game);
        assert!(dest.is_some_and(|d| d.q == 3), "{dest:?}");
    }

    #[test]
    fn a_scout_looks_again_where_its_side_has_not_for_a_while() {
        let mut game = red_scout();
        // All seen, but the west long ago.
        game.turn = 30;
        let memory = Arc::make_mut(&mut game.side_memory[Team::Red.index()]);
        for (hex, seen) in memory.iter_mut() {
            seen.turn = if hex.q < -4 { 1 } else { 30 };
        }
        let (dest, _) = replan(&mut game);
        assert!(dest.is_some_and(|d| d.q == -3), "{dest:?}");
        // With everything freshly seen, there's nothing to go for.
        for seen in Arc::make_mut(&mut game.side_memory[Team::Red.index()]).values_mut() {
            seen.turn = 30;
        }
        assert_eq!(replan(&mut game), (None, None));
    }

    #[test]
    fn a_scout_captures_a_lone_worker_but_attacks_no_troop() {
        use crate::game::workers::FieldWorker;
        let mut game = red_scout();
        let at = Hex::new(2, 0);
        game.field_workers.push(FieldWorker {
            id: 5,
            team: Team::Blue,
            home: 0,
            base: at,
            pos: at,
            job: None,
            work_left: None,
            recalled: false,
        });
        // A worker can't hit back: the scout steps onto it.
        assert_eq!(replan(&mut game), (Some(at), None));
        // With a Blue troop guarding it, the scout keeps away.
        game.units
            .push(Unit::new(2, at, Team::Blue, UnitType::Melee));
        let (dest, attack) = replan(&mut game);
        assert!(dest.is_some_and(|d| d.distance(at) > 2), "{dest:?}");
        assert_eq!(attack, None);
    }

    /// A Blue worker out on `at`, alone.
    fn blue_worker_at(game: &mut GameState, at: Hex) {
        use crate::game::workers::FieldWorker;
        game.field_workers.push(FieldWorker {
            id: 5,
            team: Team::Blue,
            home: 0,
            base: at,
            pos: at,
            job: None,
            work_left: None,
            recalled: false,
        });
    }

    #[test]
    fn one_ai_unit_goes_to_capture_a_worker_not_two() {
        let mut game = lone_red();
        red_explores_the_rest(&mut game);
        // Two Red melee either side of a lone Blue worker. Both used to
        // plan onto it, bounce off each other and leave it free.
        let worker = Hex::new(1, 0);
        blue_worker_at(&mut game, worker);
        game.units
            .push(Unit::new(2, Hex::new(2, 0), Team::Red, UnitType::Melee));
        game.plan_ai_turn(Team::Red);
        let onto = |game: &GameState| {
            game.units
                .iter()
                .filter(|u| u.planned_move == Some(worker))
                .map(|u| u.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(onto(&game), vec![1]);
        assert_eq!(game.units[1].planned_move, None, "nothing else to go for");
        // Played out, the move step captures it.
        let mut played = game.clone();
        played.settings.instant_playback = true;
        played.resolve_turn();
        played.update(0.0);
        assert!(played.field_workers.is_empty(), "captured");
        assert_eq!(played.units[0].pos, worker);

        // With an enemy troop there instead, both go for it.
        game.field_workers.clear();
        let blue = Hex::new(2, -2);
        game.units
            .push(Unit::new(3, blue, Team::Blue, UnitType::Melee));
        for unit in &mut game.units {
            unit.planned_move = None;
        }
        game.plan_ai_turn(Team::Red);
        assert_eq!(game.units[0].planned_attack, Some(blue));
        assert_eq!(game.units[1].planned_attack, Some(blue));
        assert_ne!(game.units[0].planned_move, game.units[1].planned_move);
    }

    #[test]
    fn an_ai_unit_leaves_ruins_an_ally_claimed() {
        // Two Red melee beside the same ruins: one steps on, the other
        // doesn't follow.
        use crate::game::ruins::{Ruin, RuinReward};
        let mut game = lone_red();
        let ruin = Hex::new(1, 0);
        game.ruins = vec![Ruin::new(ruin, RuinReward::Harvest)];
        red_explores_the_rest(&mut game);
        game.units
            .push(Unit::new(2, Hex::new(2, 0), Team::Red, UnitType::Melee));
        game.plan_ai_turn(Team::Red);
        assert_eq!(game.units[0].planned_move, Some(ruin));
        assert_ne!(game.units[1].planned_move, Some(ruin));
    }

    #[test]
    fn a_unit_on_ruins_holds_them_and_fights_from_there() {
        use crate::game::ruins::{Ruin, RuinReward};
        let mut game = lone_red();
        game.ruins = vec![Ruin::new(Hex::new(0, 0), RuinReward::Harvest)];
        red_explores_the_rest(&mut game);
        // An enemy two hexes off isn't chased off the ruins.
        game.units
            .push(Unit::new(2, Hex::new(2, 0), Team::Blue, UnitType::Melee));
        assert_eq!(replan(&mut game), (None, None));
        // One beside them is fought from there.
        game.units[1].pos = Hex::new(1, 0);
        assert_eq!(replan(&mut game), (None, Some(Hex::new(1, 0))));
    }

    #[test]
    fn a_unit_at_its_citys_gates_holds_them_while_an_enemy_is_at_them() {
        let mut game = lone_red();
        game.cities = vec![City::new(0, Team::Red, Hex::new(0, 0))];
        game.units[0].pos = Hex::new(1, 0);
        red_explores_the_rest(&mut game);
        // A Blue melee at another of the gates, out of the defender's
        // reach: it stays where it is rather than go after it.
        let blue = Hex::new(0, -1);
        game.units
            .push(Unit::new(2, blue, Team::Blue, UnitType::Melee));
        assert_eq!(replan(&mut game), (None, None));
        // Away from the gates, the same enemy is gone after.
        game.units[1].pos = Hex::new(1, -3);
        let (dest, _) = replan(&mut game);
        assert!(dest.is_some(), "{dest:?}");
    }

    #[test]
    fn targets_as_near_break_ties_by_coordinates_whatever_the_order() {
        let mut game = lone_red();
        red_explores_the_rest(&mut game);
        // Two lone Blue workers, each two steps off: the lower coordinates,
        // (0, -2), win, whichever came first.
        blue_worker_at(&mut game, Hex::new(0, 2));
        blue_worker_at(&mut game, Hex::new(0, -2));
        game.field_workers[1].id = 6;
        let plan = replan(&mut game);
        assert_eq!(plan, (Some(Hex::new(0, -1)), Some(Hex::new(0, -2))));
        game.field_workers.reverse();
        assert_eq!(replan(&mut game), plan);
    }

    #[test]
    fn a_land_unit_chases_no_ship_it_cannot_hit() {
        // A Blue galley on a lake two hexes off, in sight, and nothing else
        // to go for. A melee used to follow it along the shore.
        let mut game = lone_red();
        let lake = Hex::new(2, 0);
        game.grid = HexGrid::new(8, [(lake, Tile::from(Terrain::Lake))]);
        red_explores_the_rest(&mut game);
        game.units
            .push(Unit::new(2, lake, Team::Blue, UnitType::PatrolGalley));
        assert_eq!(replan(&mut game), (None, None));
        // An archer can shoot it from the shore, and does.
        game.units[0].unit_type = UnitType::Ranged;
        assert_eq!(replan(&mut game), (None, Some(lake)));
    }

    #[test]
    fn a_cornered_scout_fights_back() {
        // Mountains all around the scout but for one hex, where a Blue melee
        // stands: nowhere to go.
        let mut game = lone_red_unit(UnitType::Scout);
        let blue = Hex::new(1, 0);
        let walls = Hex::new(0, 0)
            .neighbors()
            .into_iter()
            .filter(|&h| h != blue)
            .map(|h| (h, Tile::MOUNTAINS));
        game.grid = HexGrid::new(8, walls);
        game.units
            .push(Unit::new(2, blue, Team::Blue, UnitType::Melee));
        assert_eq!(replan(&mut game), (None, Some(blue)));
    }

    #[test]
    fn a_scout_flees_an_enemy_scout_on_its_hex_toward_home() {
        // Two scouts met on one hex. Each flees as far as it can, toward its
        // own city, so they part rather than run the same way together.
        let mut game = red_scout();
        let home = Hex::new(6, 0);
        game.cities = vec![City::new(0, Team::Red, home)];
        game.units
            .push(Unit::new(2, Hex::new(0, 0), Team::Blue, UnitType::Scout));
        assert_eq!(replan(&mut game), (Some(Hex::new(3, 0)), None));
    }

    #[test]
    fn a_hurt_scout_heads_home() {
        let mut game = red_scout();
        let home = Hex::new(-6, 0);
        game.cities = vec![City::new(0, Team::Red, home)];
        // East has never been seen, but the scout is down to a third.
        Arc::make_mut(&mut game.side_memory[Team::Red.index()]).retain(|h, _| h.q < 3);
        game.units[0].hp = 20.0;
        let (dest, _) = replan(&mut game);
        assert!(dest.is_some_and(|d| d.distance(home) == 3), "{dest:?}");
        // Well again, it's off east.
        game.units[0].hp = game.units[0].max_hp();
        let (dest, _) = replan(&mut game);
        assert!(dest.is_some_and(|d| d.q > 0), "{dest:?}");
    }
}

#[cfg(test)]
mod expansion_tests {
    use std::sync::Arc;

    use super::*;
    use crate::game::city::City;
    use crate::game::hex::HexGrid;
    use crate::game::terrain::Tile;

    /// Open plains of radius 8, all seen by Red, with a Red city of
    /// population 4 at (-5, 0) that has a worker and a Barracks (so it
    /// wants no more troops of its own queue), a funded stockpile and a
    /// scout: a side that can afford to expand.
    fn red_ready_to_expand() -> GameState {
        let mut game = GameState::new();
        game.grid = HexGrid::new(8, [(Hex::new(0, 0), Tile::default())]);
        game.units = vec![Unit::new(1, Hex::new(-5, 2), Team::Red, UnitType::Scout)];
        game.ruins.clear();
        game.cities = vec![City {
            population: 4,
            barracks: Some(Hex::new(-5, 1)),
            ..City::new(0, Team::Red, Hex::new(-5, 0))
        }];
        game.auto_assign_city(0);
        game.fund(Team::Red);
        let hexes: Vec<Hex> = game.grid.all_hexes().collect();
        let memory = Arc::make_mut(&mut game.side_memory[Team::Red.index()]);
        for hex in hexes {
            memory.entry(hex).or_default();
        }
        game
    }

    fn add_red_settler(game: &mut GameState, pos: Hex) -> u32 {
        let id = 50;
        game.units
            .push(Unit::new(id, pos, Team::Red, UnitType::Melee));
        game.settlers.insert(id);
        id
    }

    fn unit(game: &GameState, id: u32) -> &Unit {
        game.units.iter().find(|u| u.id == id).expect("still there")
    }

    #[test]
    fn a_city_of_four_trains_a_settler_to_expand() {
        let mut game = red_ready_to_expand();
        game.plan_ai_turn(Team::Red);
        assert_eq!(
            game.cities[0].queue.first().map(|q| q.build),
            Some(Build::Settler)
        );
        // Smaller, it grows first.
        let mut game = red_ready_to_expand();
        game.cities[0].population = AI_SETTLE_POPULATION - 1;
        game.plan_ai_turn(Team::Red);
        assert_eq!(
            game.cities[0].queue.first().map(|q| q.build),
            Some(Build::Grow)
        );
        // With a settler already out, it doesn't train another.
        let mut game = red_ready_to_expand();
        add_red_settler(&mut game, Hex::new(-4, 0));
        game.plan_ai_turn(Team::Red);
        assert_ne!(
            game.cities[0].queue.first().map(|q| q.build),
            Some(Build::Settler)
        );
    }

    /// #375: an idle Barracks trains Pikemen while its side sees more enemy
    /// Cavalry than it has Pikemen, and not otherwise.
    #[test]
    fn a_barracks_trains_pikemen_against_cavalry_it_sees() {
        let trains = |game: &mut GameState| {
            game.plan_ai_turn(Team::Red);
            game.cities[0].barracks_queue.first().map(|q| q.build)
        };
        // No enemy in sight: no Pikeman.
        let mut game = red_ready_to_expand();
        assert_eq!(trains(&mut game), Some(BuildUnit::Melee));
        // A Blue Cavalry by its city: a Pikeman.
        let mut game = red_ready_to_expand();
        game.units
            .push(Unit::new(7, Hex::new(-3, 0), Team::Blue, UnitType::Cavalry));
        assert_eq!(trains(&mut game), Some(BuildUnit::Pikeman));
        // With a Pikeman for it already, back to its usual troops.
        let mut game = red_ready_to_expand();
        game.units
            .push(Unit::new(7, Hex::new(-3, 0), Team::Blue, UnitType::Cavalry));
        game.units
            .push(Unit::new(8, Hex::new(-6, 1), Team::Red, UnitType::Pikeman));
        assert_eq!(trains(&mut game), Some(BuildUnit::Melee));
        // Cavalry out of sight counts for nothing.
        let mut game = red_ready_to_expand();
        game.units
            .push(Unit::new(7, Hex::new(6, 0), Team::Blue, UnitType::Cavalry));
        assert_eq!(trains(&mut game), Some(BuildUnit::Melee));
    }

    #[test]
    fn a_side_without_a_scout_trains_one() {
        let mut game = red_ready_to_expand();
        game.units.clear();
        game.plan_ai_turn(Team::Red);
        assert_eq!(
            game.cities[0].queue.first().map(|q| q.build),
            Some(Build::Scout)
        );
    }

    #[test]
    fn a_settler_walks_to_a_site_and_founds_a_city_there() {
        let mut game = red_ready_to_expand();
        let home = game.cities[0].pos;
        let from = Hex::new(-4, 0);
        let id = add_red_settler(&mut game, from);
        let fog = game.side_fog(Team::Red);
        let known = game.knowledge(fog);
        let site = game
            .ai_city_site(Team::Red, from, &known, false)
            .expect("open plains all round");
        assert!(
            (MIN_CITY_DISTANCE..=AI_SITE_MAX_DISTANCE).contains(&site.distance(home)),
            "{site:?}"
        );
        game.plan_ai_turn(Team::Red);
        let step = unit(&game, id).planned_move.expect("it heads off");
        assert!(step.distance(site) < from.distance(site), "{step:?}");
        assert_eq!(game.cities.len(), 1);

        // There, it founds the city: one without a worker.
        let at = game.units.iter().position(|u| u.id == id).unwrap();
        game.units[at].pos = site;
        game.units[at].planned_move = None;
        game.plan_ai_turn(Team::Red);
        assert_eq!(game.cities.len(), 2);
        assert_eq!(game.cities[1].pos, site);
        assert_eq!(game.cities[1].workers, 0);
        assert!(game.settlers.is_empty());
    }

    #[test]
    fn a_site_refused_for_a_city_unseen_is_given_up() {
        let mut game = red_ready_to_expand();
        let home = game.cities[0].pos;
        let from = Hex::new(-4, 0);
        let id = add_red_settler(&mut game, from);
        let fog = game.side_fog(Team::Red);
        let known = game.knowledge(fog);
        let site = game.ai_city_site(Team::Red, from, &known, false).unwrap();
        // A Blue city five hexes past the site, out of Red's sight.
        let blue = game
            .grid
            .all_hexes()
            .filter(|h| h.distance(site) == MIN_CITY_DISTANCE - 1 && h.distance(home) > 8)
            .min_by_key(|h| (h.q, h.r))
            .expect("room on the map");
        game.cities.push(City::new(1, Team::Blue, blue));
        let at = game.units.iter().position(|u| u.id == id).unwrap();
        game.units[at].pos = site;
        game.plan_ai_turn(Team::Red);
        assert_eq!(game.cities.len(), 2, "not founded");
        assert!(game.refused_sites.contains(&(Team::Red, site)));
        assert!(game.settlers.contains(&id));
        // It looks elsewhere, not near there.
        let fog = game.side_fog(Team::Red);
        let known = game.knowledge(fog);
        if let Some(next) = game.ai_city_site(Team::Red, site, &known, false) {
            assert!(next.distance(site) > AI_REFUSED_RADIUS, "{next:?}");
        }
    }

    #[test]
    fn a_first_city_keeps_its_distance_from_another_sides() {
        let mut game = red_ready_to_expand();
        // Red has no city yet, and its settler stands 3 hexes from Blue's.
        game.cities = vec![City::new(0, Team::Blue, Hex::new(3, 0))];
        let from = Hex::new(0, 0);
        let id = add_red_settler(&mut game, from);
        game.plan_ai_turn(Team::Red);
        assert!(game.cities.iter().all(|c| c.team != Team::Red));
        let step = unit(&game, id).planned_move.expect("it heads away");
        assert!(step.distance(Hex::new(3, 0)) > from.distance(Hex::new(3, 0)));
        // Where the rules allow it, it founds its first city at once, with
        // a worker, as a starting city.
        let at = game.units.iter().position(|u| u.id == id).unwrap();
        game.units[at].pos = Hex::new(-3, 0);
        game.plan_ai_turn(Team::Red);
        let city = game.cities.iter().find(|c| c.team == Team::Red).unwrap();
        assert_eq!((city.pos, city.workers), (Hex::new(-3, 0), 1));

        // Knowing of nowhere it could found (it has seen only the hexes
        // near Blue's city), it waits: it never founds too close.
        let mut game = red_ready_to_expand();
        game.cities = vec![City::new(0, Team::Blue, Hex::new(3, 0))];
        let near: Vec<Hex> = game.grid.all_hexes().collect();
        let memory = Arc::make_mut(&mut game.side_memory[Team::Red.index()]);
        memory.retain(|hex, _| hex.distance(Hex::new(3, 0)) < MIN_CITY_DISTANCE);
        assert!(near.len() > memory.len());
        game.units.clear();
        let id = add_red_settler(&mut game, Hex::new(1, 0));
        let fog = game.side_fog(Team::Red);
        let known = game.knowledge(fog);
        assert_eq!(
            game.ai_city_site(Team::Red, Hex::new(1, 0), &known, true),
            None
        );
        game.plan_ai_turn(Team::Red);
        assert!(game.cities.iter().all(|c| c.team != Team::Red));
        assert!(game.settlers.contains(&id));
    }
}
