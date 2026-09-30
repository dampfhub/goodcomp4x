//! Animals: aggressive neutral units that roam around dens, to make the
//! early map dangerous. They belong to the wild (`Team::Wild`), which is no
//! side: no cities, stockpile, fog or plan, never a seat, and hostile to
//! every side.
//!
//! `mapgen.rs` places the dens of a world (on forest, jungle and hills, away
//! from every start). A den starts with one animal, a wolf pack or a bear
//! (`UnitType::Wolf`, `UnitType::Bear`), and adds another every
//! `DEN_BREED_TURNS` turns: one of its own while it has fewer than its cap
//! (`den_cap`, by the Animals setting), and past that a stray, which leaves
//! it for good, while the world has fewer strays than its limit
//! (`stray_limit`). An animal of a den never leaves its territory, the hexes
//! within `territory(kind)` of its den (`Unit::home`); a stray has no den
//! and goes anywhere. As its own move step begins an animal goes for the
//! nearest unit or worker out on the map within its hunting range
//! (`hunting_range`, wider for a stray) that it can reach from its
//! territory; with none, it roams to a hex it can reach (of its territory)
//! picked by `roam_key`. As its attack step begins
//! it attacks whoever is in reach. It never attacks or enters a city, and
//! never captures anything (a worker it reaches, it kills). It decides on
//! the real board as the step begins, the same on every machine: its "plan"
//! is a rule of the game, not a side's planning, and draws nothing from the
//! game's RNG. Ties break by hex coordinates.
//!
//! Hunting pays: the side that kills an animal gets food (`bounty`), and a
//! side that stands a unit (not a settler) on a den at a turn's end clears
//! it for food and metal (`DEN_SPOILS`): it adds no more animals, and its
//! land is open to a city (`resolve_dens`). How many dens a world has, how
//! many animals each keeps and how many strays roam, is the Animals setting
//! (`world_animals`).

use super::GameState;
use super::city::{Stock, stock_words};
use super::hex::Hex;
use super::unit::{Team, Unit, UnitType};

/// How far from its den an animal of `kind` goes: it stays within this many
/// hexes of it. Wolves range wider than bears.
pub fn territory(kind: UnitType) -> i32 {
    match kind {
        UnitType::Bear => 3,
        _ => 4,
    }
}

/// The widest territory of any animal (`territory`): an AI side keeps its
/// settlers, city sites and workers this far from any den it knows of.
pub const MAX_TERRITORY: i32 = 4;

/// How far from itself an animal of `kind` notices a unit or worker and
/// goes for it (as far as its territory lets it).
pub fn hunting_range(kind: UnitType) -> i32 {
    match kind {
        UnitType::Bear => 3,
        _ => 4,
    }
}

/// How much farther than an animal of its kind with a den a stray notices
/// a unit or worker and goes for it.
pub const STRAY_EXTRA_HUNTING: i32 = 2;

/// How far from itself `animal` notices a unit or worker: its kind's
/// `hunting_range`, `STRAY_EXTRA_HUNTING` more for a stray.
pub fn hunts_within(animal: &Unit) -> i32 {
    let extra = if animal.is_stray() {
        STRAY_EXTRA_HUNTING
    } else {
        0
    };
    hunting_range(animal.unit_type) + extra
}

/// Turns between a den's new animals: its own while it keeps fewer than
/// its cap, strays past that.
pub const DEN_BREED_TURNS: u32 = 8;

/// Strays the wild keeps at most for each side and each step of the
/// Animals setting (`stray_limit`).
pub const STRAYS_PER_SIDE: usize = 1;

/// How many strays a world of `sides` sides with the Animals setting
/// `world_animals` keeps at most (`STRAYS_PER_SIDE` for each side and each
/// den a side it has): a den at its cap adds one only while fewer roam.
pub fn stray_limit(sides: usize, world_animals: usize) -> usize {
    STRAYS_PER_SIDE * sides * world_animals
}
/// What clearing a den gives the side that clears it.
pub const DEN_SPOILS: Stock = Stock::whole(6, 0, 2);

/// How many animals a den keeps at most in a world with the Animals setting
/// `world_animals` (`Settings::world_animals`): two with few dens, three
/// with many.
pub fn den_cap(world_animals: usize) -> usize {
    world_animals + 1
}

/// What killing an animal of `kind` gives the side that kills it.
pub fn bounty(kind: UnitType) -> Stock {
    match kind {
        UnitType::Bear => Stock::whole(5, 0, 0),
        _ => Stock::whole(3, 0, 0),
    }
}

/// An animal's den (`mapgen.rs` places them).
#[derive(Clone, Debug, PartialEq)]
pub struct Den {
    pub pos: Hex,
    /// The animals it keeps.
    pub kind: UnitType,
    /// The most animals of its own it keeps at once (`den_cap`).
    pub cap: usize,
    /// The turn ends until it adds another animal (at 1, the next; at 0, as
    /// soon as it can: once nothing stands on the den and, at its cap, the
    /// world has room for a stray).
    pub next_in: u32,
}

impl Den {
    /// The den's name, for tooltips and notices.
    pub fn name(&self) -> &'static str {
        den_name(self.kind)
    }
}

/// The name of a den keeping `kind`.
fn den_name(kind: UnitType) -> &'static str {
    match kind {
        UnitType::Bear => "BEAR DEN",
        _ => "WOLF DEN",
    }
}

/// The name of an animal of `kind`.
fn animal_name(kind: UnitType) -> &'static str {
    match kind {
        UnitType::Bear => "BEAR",
        _ => "WOLF PACK",
    }
}

/// The name of several animals of `kind`.
fn animals_name(kind: UnitType) -> &'static str {
    match kind {
        UnitType::Bear => "BEARS",
        _ => "WOLF PACKS",
    }
}

/// A number that looks random, from the turn, animal `id` and `hex`: a
/// roaming animal goes to the hex it can reach with the lowest. Every
/// machine computes the same, and it draws nothing from the game's RNG,
/// which the machines of a network game don't share. (SplitMix64's mixer.)
fn roam_key(turn: u32, id: u32, hex: Hex) -> u64 {
    let mut x = u64::from(turn).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ u64::from(id).wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ u64::from(hex.q as u32).wrapping_mul(0x94D0_49BB_1331_11EB)
        ^ u64::from(hex.r as u32).wrapping_mul(0xD6E8_FEB8_6659_FD93);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

impl GameState {
    /// The den on `hex`, if there is one.
    pub(super) fn den_at(&self, hex: Hex) -> Option<&Den> {
        self.dens.iter().find(|d| d.pos == hex)
    }

    /// The tile tooltip's lines about a den on `hex`: what it keeps, what
    /// clearing it gives, and when it adds another animal. Out of sight
    /// (`remembered`), only that it was there when last seen.
    pub(super) fn den_notes(&self, hex: Hex, remembered: bool) -> Vec<String> {
        if remembered {
            let seen = self.memory.get(&hex).and_then(|seen| seen.den);
            return seen
                .map(|kind| format!("{}, AS LAST SEEN", den_name(kind)))
                .into_iter()
                .collect();
        }
        let Some(den) = self.den_at(hex) else {
            return Vec::new();
        };
        let (kind, animals) = (den.kind, animals_name(den.kind));
        let mut notes = vec![
            format!(
                "{}: UP TO {} {animals}, ROAMING WITHIN {} HEXES OF IT",
                den.name(),
                den.cap,
                territory(kind)
            ),
            format!(
                "THEY HUNT WHOEVER COMES WITHIN {} HEXES OF THEM",
                hunting_range(kind)
            ),
            format!(
                "END A TURN ON IT WITH A UNIT TO CLEAR IT: +{} FOR THE STOCKPILE",
                stock_words(DEN_SPOILS)
            ),
            "CLEARED, ITS LAND IS FREE TO SETTLE".into(),
        ];
        let turns = den.next_in.max(1);
        let full = self.den_animals_at(hex) >= den.cap;
        notes.push(if full {
            format!(
                "FULL: ANOTHER {} LEAVES IT AS A STRAY IN {turns} TURNS",
                animal_name(kind)
            )
        } else {
            format!("ANOTHER {} IN {turns} TURNS", animal_name(kind))
        });
        notes.push(format!(
            "STRAYS ROAM ANYWHERE, HUNTING WITHIN {} HEXES: UP TO {} IN THE WORLD",
            hunting_range(kind) + STRAY_EXTRA_HUNTING,
            self.stray_limit
        ));
        notes
    }

    /// Makes a den on each of `sites`, wolves and bears in turn, each
    /// keeping up to `cap` animals and starting with one at home.
    pub(super) fn make_dens(&mut self, sites: &[Hex], cap: usize) {
        for (i, &pos) in sites.iter().enumerate() {
            let kind = if i % 2 == 0 {
                UnitType::Wolf
            } else {
                UnitType::Bear
            };
            self.dens.push(Den {
                pos,
                kind,
                cap,
                next_in: DEN_BREED_TURNS,
            });
            self.spawn_animal(self.dens.len() - 1, false);
        }
    }

    /// How many animals of the den on `pos` are alive, wherever they are.
    fn den_animals_at(&self, pos: Hex) -> usize {
        self.units.iter().filter(|u| u.home == Some(pos)).count()
    }

    /// How many strays are alive (`Unit::is_stray`).
    pub(super) fn strays(&self) -> usize {
        self.units.iter().filter(|u| u.is_stray()).count()
    }

    /// At a turn's end, before the economy: a den with a side's unit (not a
    /// settler) on it is cleared, and gone, and the side gets
    /// `DEN_SPOILS`; its animals, if alive, keep to its territory, and a
    /// city may stand on its hex from now on. Every other den counts down
    /// `DEN_BREED_TURNS` and then adds another animal once nothing stands
    /// on it (`breed`).
    pub(super) fn resolve_dens(&mut self) {
        let mut d = 0;
        while d < self.dens.len() {
            let pos = self.dens[d].pos;
            let clearer = self
                .units
                .iter()
                .find(|u| u.pos == pos && u.team.is_side() && !self.settlers.contains(&u.id))
                .map(|u| u.team);
            if let Some(team) = clearer {
                let den = self.dens.remove(d);
                *self.stock_mut(team) += DEN_SPOILS;
                let what = format!(
                    "+{} FOR THE STOCKPILE - ITS LAND IS FREE TO SETTLE",
                    stock_words(DEN_SPOILS)
                );
                log::info!("{team:?} clears the {} at {pos:?}: {what}", den.name());
                if team == self.local_team {
                    self.notice = format!("{} CLEARED: {what}", den.name());
                }
                continue;
            }
            let counting = self.dens[d].next_in;
            self.dens[d].next_in = match counting {
                turns if turns > 1 => turns - 1,
                // Due: one more, or wait until it can.
                _ if self.breed(d) => DEN_BREED_TURNS,
                _ => 0,
            };
            d += 1;
        }
    }

    /// After an attack step's damage lands, before the dead are removed
    /// (`resolve_attacks`): each animal the step killed goes to the side
    /// whose blows on it (`hunts`: the animal, the side, the damage) did
    /// the most damage this step, the earliest in `Team::ALL` on a tie, and
    /// that side gets its bounty.
    pub(super) fn reward_hunts(&mut self, hunts: &[(usize, Team, f32)]) {
        let mut kills: Vec<(usize, Team)> = Vec::new();
        for &(idx, ..) in hunts {
            if self.units[idx].is_alive() || kills.iter().any(|&(i, _)| i == idx) {
                continue;
            }
            let mut by_side: Vec<(Team, f32)> = Vec::new();
            for &(_, team, dealt) in hunts.iter().filter(|h| h.0 == idx) {
                match by_side.iter_mut().find(|(t, _)| *t == team) {
                    Some(side) => side.1 += dealt,
                    None => by_side.push((team, dealt)),
                }
            }
            let most = by_side
                .into_iter()
                .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)));
            if let Some((team, _)) = most {
                kills.push((idx, team));
            }
        }
        for (idx, team) in kills {
            let (kind, pos) = (self.units[idx].unit_type, self.units[idx].pos);
            self.reward_kill(team, kind, pos);
        }
    }

    /// An animal of `kind` killed at `pos` by `team`: the side gets its
    /// `bounty`.
    fn reward_kill(&mut self, team: Team, kind: UnitType, pos: Hex) {
        let goods = bounty(kind);
        *self.stock_mut(team) += goods;
        let what = format!("+{} FOR THE STOCKPILE", stock_words(goods));
        log::info!(
            "{team:?} kills the {} at {pos:?}: {what}",
            animal_name(kind)
        );
        if team == self.local_team {
            self.notice = format!("{} KILLED: {what}", animal_name(kind));
        }
    }

    /// Den `den`'s next animal, due now: one of its own below its cap,
    /// else a stray while the world has fewer than `stray_limit`. Whether
    /// it came (`spawn_animal`).
    fn breed(&mut self, den: usize) -> bool {
        let stray = self.den_animals_at(self.dens[den].pos) >= self.dens[den].cap;
        if stray && self.strays() >= self.stray_limit {
            return false;
        }
        self.spawn_animal(den, stray)
    }

    /// A new animal of den `den` appears on it, if nothing stands there:
    /// one of its own, or a `stray` with no den. Whether it came.
    fn spawn_animal(&mut self, den: usize, stray: bool) -> bool {
        let Den { pos, kind, .. } = self.dens[den];
        if self.is_occupied(pos) || self.field_workers.iter().any(|w| w.pos == pos) {
            return false;
        }
        let id = self.next_unit_id;
        self.next_unit_id += 1;
        let mut animal = Unit::new(id, pos, Team::Wild, kind);
        animal.home = (!stray).then_some(pos);
        self.units.push(animal);
        true
    }

    /// Whether `hex` is in the territory of animal `idx` (within
    /// `territory` of its den); anywhere for a stray.
    fn in_territory(&self, idx: usize, hex: Hex) -> bool {
        let unit = &self.units[idx];
        unit.home
            .is_none_or(|home| home.distance(hex) <= territory(unit.unit_type))
    }

    /// Whether an animal may attack whoever stands on `hex`: never on a
    /// city center, nor out on the water.
    fn animal_may_attack(&self, hex: Hex) -> bool {
        !self.cities.iter().any(|c| c.pos == hex)
            && self.grid.contains(hex)
            && !self.grid.terrain(hex).is_water()
    }

    /// The unit or worker animal `idx` hunts as its move step begins: the
    /// nearest it may attack (`animal_may_attack`) within its hunting range
    /// (`hunts_within`), and near enough its den to strike from its
    /// territory (a stray has none to keep to); the lowest (q, r) of those
    /// as near.
    fn animal_prey(&self, idx: usize) -> Option<Hex> {
        let unit = &self.units[idx];
        let (pos, range) = (unit.pos, hunts_within(unit));
        let reach = territory(unit.unit_type) + unit.stats().attack_range;
        let from_territory = |h: Hex| unit.home.is_none_or(|home| h.distance(home) <= reach);
        self.units
            .iter()
            .filter(|u| !u.is_animal())
            .map(|u| u.pos)
            .chain(self.field_workers.iter().map(|w| w.pos))
            .filter(|&h| h.distance(pos) <= range && from_territory(h))
            .filter(|&h| self.animal_may_attack(h))
            .min_by_key(|h| (h.distance(pos), h.q, h.r))
    }

    /// As the move step of `kind` begins: each animal of that kind goes for
    /// its prey (`animal_prey`), as close as it can get within its
    /// territory, staying put on a tie, then the lowest (q, r); with no
    /// prey, it roams: to the hex it can reach with the lowest `roam_key`,
    /// anywhere in its territory but where it stands (a stray: of those as
    /// far from it as it can go). It won't step onto a
    /// worker (it kills them, it doesn't capture them), a city, or a hex
    /// another animal is heading for this step.
    pub(super) fn plan_animal_moves(&mut self, kind: UnitType) {
        for idx in 0..self.units.len() {
            let unit = &self.units[idx];
            if !unit.is_animal() || unit.unit_type != kind || self.rival_of(idx).is_some() {
                continue;
            }
            let (pos, id, stray) = (unit.pos, unit.id, unit.is_stray());
            let claimed = |hex: Hex| {
                self.units
                    .iter()
                    .any(|u| u.is_animal() && u.planned_move == Some(hex))
            };
            let options: Vec<Hex> = self
                .reachable_hexes_by(pos, unit.stats().move_range, |from, to| {
                    self.in_territory(idx, to)
                        && self.can_step(from, to, Team::Wild)
                        && !self.is_occupied(to)
                        && !self.field_workers.iter().any(|w| w.pos == to)
                        && !self.cities.iter().any(|c| c.pos == to)
                })
                .into_iter()
                .filter(|&h| h == pos || !claimed(h))
                .collect();
            let dest = match self.animal_prey(idx) {
                Some(prey) => options
                    .into_iter()
                    .min_by_key(|&h| (h.distance(prey), h != pos, h.q, h.r)),
                None => {
                    // A stray ranges wide: only as far as it can go.
                    let farthest = options.iter().map(|h| h.distance(pos)).max();
                    options
                        .into_iter()
                        .filter(|&h| h != pos)
                        .filter(|h| !stray || Some(h.distance(pos)) == farthest)
                        .min_by_key(|&h| roam_key(self.turn, id, h))
                }
            }
            .unwrap_or(pos);
            self.units[idx].planned_move = (dest != pos).then_some(dest);
        }
    }

    /// As the attack step of `kind` begins: each animal of that kind
    /// attacks what's in its reach (`animal_target`), if anything is.
    pub(super) fn plan_animal_attacks(&mut self, kind: UnitType) {
        for idx in 0..self.units.len() {
            if self.units[idx].is_animal() && self.units[idx].unit_type == kind {
                self.units[idx].planned_attack = self.animal_target(idx);
            }
        }
    }

    /// Where animal `idx` attacks: the nearest unit in its reach it may
    /// attack (`animal_may_attack`, `attack_target_legal`), the weakest
    /// (fewest HP) of those as near, then by (q, r); with none, the nearest
    /// worker out on the map, on a hex with nothing else to hit (no barracks
    /// or battery takes the blow). Anyone in reach, in its territory or not.
    pub(super) fn animal_target(&self, idx: usize) -> Option<Hex> {
        let animal = &self.units[idx];
        let (pos, range) = (animal.pos, animal.stats().attack_range);
        let in_reach = |hex: Hex| hex.distance(pos) <= range && self.animal_may_attack(hex);
        let unit = self
            .units
            .iter()
            .filter(|u| !u.is_animal() && in_reach(u.pos))
            .filter(|u| self.attack_target_legal(idx, u.pos, false))
            .min_by(|a, b| {
                pos.distance(a.pos)
                    .cmp(&pos.distance(b.pos))
                    .then(a.hp.total_cmp(&b.hp))
                    .then((a.pos.q, a.pos.r, a.id).cmp(&(b.pos.q, b.pos.r, b.id)))
            });
        if let Some(unit) = unit {
            return Some(unit.pos);
        }
        self.field_workers
            .iter()
            .map(|w| w.pos)
            .filter(|&h| in_reach(h))
            .filter(|&h| {
                self.enemy_barracks_at(h, Team::Wild).is_none()
                    && self.enemy_coastal_battery_at(h, Team::Wild).is_none()
            })
            .min_by_key(|h| (h.distance(pos), h.q, h.r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::fast_hash::HashSet;
    use crate::game::hex::HexGrid;
    use crate::game::terrain::Tile;
    use crate::game::turn::Phase;
    use crate::game::workers::{FieldWorker, JobKind, WorkerJob};

    /// An open radius-7 map with no cities, a wolf den at (0, 0) keeping
    /// one wolf pack, at home, and nothing else.
    fn wolf_den() -> GameState {
        den_of(1)
    }

    /// `wolf_den`, the den keeping up to `cap`.
    fn den_of(cap: usize) -> GameState {
        let mut game = GameState::new();
        game.units.clear();
        game.grid = HexGrid::new(7, [(Hex::new(0, 0), Tile::HILLS)]);
        game.make_dens(&[Hex::new(0, 0)], cap);
        game.fog_of_war = false;
        game
    }

    fn animal(game: &GameState) -> &Unit {
        game.units
            .iter()
            .find(|u| u.is_animal())
            .expect("an animal")
    }

    fn add(game: &mut GameState, pos: Hex, team: Team, kind: UnitType) -> u32 {
        let id = game.next_unit_id;
        game.next_unit_id += 1;
        game.units.push(Unit::new(id, pos, team, kind));
        id
    }

    /// Resolves a turn with no orders from anyone but the animals.
    fn play(game: &mut GameState) {
        game.selected = None;
        game.resolve_turn();
        game.update(0.0);
    }

    #[test]
    fn a_den_starts_with_its_animal_at_home() {
        let game = wolf_den();
        let wolf = animal(&game);
        assert_eq!(wolf.team, Team::Wild);
        assert_eq!(wolf.unit_type, UnitType::Wolf);
        assert_eq!(
            (wolf.pos, wolf.home),
            (Hex::new(0, 0), Some(Hex::new(0, 0)))
        );
        assert_eq!(game.den_at(Hex::new(0, 0)).map(Den::name), Some("WOLF DEN"));
        assert_eq!(
            game.den_at(Hex::new(0, 0)).unwrap().next_in,
            DEN_BREED_TURNS
        );
        assert!(!wolf.is_stray());
        assert!(!game.ai_teams().contains(&Team::Wild));
    }

    #[test]
    fn the_widest_territory_is_the_one_the_ai_keeps_clear_of() {
        let widest = [UnitType::Wolf, UnitType::Bear]
            .into_iter()
            .map(territory)
            .max();
        assert_eq!(widest, Some(MAX_TERRITORY));
    }

    #[test]
    fn an_animal_hunts_an_intruder_then_roams_again() {
        let mut game = wolf_den();
        let scout = add(&mut game, Hex::new(3, 0), Team::Blue, UnitType::Scout);
        play(&mut game);
        // A wolf moves two: next to the scout, and bites.
        let wolf = animal(&game);
        assert_eq!(wolf.pos.distance(Hex::new(3, 0)), 1, "{:?}", wolf.pos);
        let hurt = game.units.iter().find(|u| u.id == scout);
        assert!(
            hurt.is_none_or(|s| s.hp < s.max_hp()),
            "the scout was bitten"
        );
        // With the scout gone, it roams on: a new hex every turn.
        game.units.retain(|u| u.is_animal());
        for _ in 0..3 {
            let was = animal(&game).pos;
            play(&mut game);
            assert_ne!(animal(&game).pos, was);
        }
    }

    #[test]
    fn with_no_prey_an_animal_roams_its_territory_the_same_on_every_machine() {
        let mut game = wolf_den();
        let mut replay = game.clone();
        let mut seen = HashSet::default();
        for turn in 1..=16 {
            let was = animal(&game).pos;
            play(&mut game);
            play(&mut replay);
            let wolf = animal(&game);
            assert_ne!(wolf.pos, was, "turn {turn}: it moves");
            assert!(wolf.pos.distance(Hex::new(0, 0)) <= territory(UnitType::Wolf));
            assert_eq!(animal(&replay).pos, wolf.pos, "turn {turn}: replayed");
            seen.insert(wolf.pos);
        }
        assert!(seen.len() >= 6, "it gets around: {seen:?}");
    }

    #[test]
    fn an_animal_hunts_whoever_comes_within_its_hunting_range() {
        let mut game = wolf_den();
        // The wolf has roamed east; a troop stands past its territory, 3
        // hexes away. It goes to the edge of its territory, and bites.
        game.units[0].pos = Hex::new(2, 0);
        let troop = add(&mut game, Hex::new(5, 0), Team::Blue, UnitType::Melee);
        play(&mut game);
        assert_eq!(animal(&game).pos, Hex::new(4, 0));
        let troop = game.units.iter().find(|u| u.id == troop).unwrap();
        assert!(troop.hp < troop.max_hp(), "bitten");
    }

    #[test]
    fn a_troop_on_alert_fires_at_an_animal_in_range() {
        let mut game = wolf_den();
        // Within the wolf's hunting range, out of the archer's until it comes.
        add(&mut game, Hex::new(-3, 0), Team::Blue, UnitType::Ranged);
        let archer = game.units.len() - 1;
        game.go_on_alert(archer);
        assert_eq!(game.alert_target(archer), None);
        play(&mut game);
        // Wolves move before archers shoot: it came, and was shot (and
        // maybe finished off, biting the archer).
        let wolf = game.units.iter().find(|u| u.is_animal());
        assert!(wolf.is_none_or(|w| w.hp < w.max_hp()), "shot");
        let archer = game.units.iter().find(|u| !u.is_animal()).unwrap();
        assert_eq!(archer.pos, Hex::new(-3, 0), "and it stayed on alert");
        assert!(archer.alert);
    }

    #[test]
    fn an_animal_stays_in_its_territory() {
        let mut game = wolf_den();
        // Out of its reach from anywhere in its territory: it may come to
        // the edge, but never bites.
        add(&mut game, Hex::new(6, 0), Team::Blue, UnitType::Melee);
        for _ in 0..12 {
            play(&mut game);
            let wolf = animal(&game);
            assert!(wolf.pos.distance(Hex::new(0, 0)) <= territory(UnitType::Wolf));
        }
        let blue = game.units.iter().find(|u| !u.is_animal()).unwrap();
        assert_eq!(blue.hp, blue.max_hp(), "out of reach: never attacked");
    }

    #[test]
    fn an_animal_never_attacks_a_city_or_whoever_is_in_it() {
        let mut game = wolf_den();
        game.cities = vec![crate::game::city::City::new(0, Team::Blue, Hex::new(2, 0))];
        game.auto_assign_city(0);
        let id = add(&mut game, Hex::new(2, 0), Team::Blue, UnitType::Scout);
        let before = game.cities[0].clone();
        for _ in 0..8 {
            play(&mut game);
            assert!(animal(&game).pos != Hex::new(2, 0));
        }
        let scout = game.units.iter().find(|u| u.id == id).unwrap();
        assert_eq!(scout.hp, scout.max_hp(), "safe in its city");
        assert_eq!(game.cities[0].team, before.team);
        assert_eq!(game.cities[0].population, before.population);
        assert!(
            game.cities[0]
                .interior
                .fighters
                .iter()
                .all(|f| f.team != Team::Wild)
        );
    }

    #[test]
    fn an_animal_kills_a_worker_rather_than_capture_it() {
        let mut game = wolf_den();
        game.cities = vec![crate::game::city::City::new(0, Team::Blue, Hex::new(-6, 0))];
        let (worker, hex) = (100, Hex::new(2, 0));
        game.field_workers.push(FieldWorker {
            id: worker,
            team: Team::Blue,
            home: 0,
            base: game.cities[0].pos,
            pos: hex,
            job: Some(WorkerJob::on_tile(hex, JobKind::Fort)),
            work_left: Some(3),
            recalled: false,
        });
        let at_home = game.cities[0].workers;
        play(&mut game);
        assert!(
            game.field_workers.iter().all(|w| w.id != worker),
            "the worker is dead"
        );
        assert_eq!(game.cities[0].workers, at_home, "not captured, nor home");
    }

    #[test]
    fn the_side_that_kills_an_animal_gets_its_bounty() {
        let mut game = wolf_den();
        game.units[0].hp = 1.0;
        add(&mut game, Hex::new(1, 0), Team::Red, UnitType::Melee);
        let red = game.units.len() - 1;
        game.units[red].planned_attack = Some(Hex::new(0, 0));
        let (blue, before) = (game.stock(Team::Blue), game.stock(Team::Red));
        game.resolve_step(UnitType::Melee, Phase::Attack);
        assert!(game.units.iter().all(|u| !u.is_animal()), "killed");
        assert_eq!(game.stock(Team::Red), before + bounty(UnitType::Wolf));
        assert_eq!(game.stock(Team::Blue), blue, "only the hunter gains");
    }

    #[test]
    fn standing_on_a_den_clears_it_for_its_spoils() {
        let mut game = den_of(2);
        // The wolf is out; a settler on the den doesn't clear it.
        game.units[0].pos = Hex::new(3, 0);
        let settler = add(&mut game, Hex::new(0, 0), Team::Blue, UnitType::Melee);
        game.settlers.insert(settler);
        let before = game.stock(Team::Blue);
        game.resolve_dens();
        assert_eq!(game.dens.len(), 1);
        assert_eq!(game.stock(Team::Blue), before);
        assert!(game.founding_issue(Hex::new(0, 0)).is_some(), "nor found");
        // A scout does, and the wolf keeps to its old territory.
        game.settlers.clear();
        game.units.last_mut().unwrap().unit_type = UnitType::Scout;
        game.resolve_dens();
        assert!(game.dens.is_empty(), "cleared");
        assert_eq!(game.stock(Team::Blue), before + DEN_SPOILS);
        assert_eq!(animal(&game).home, Some(Hex::new(0, 0)));
        assert!(game.notice.ends_with("FREE TO SETTLE"), "{}", game.notice);
        // Its land is open to a city at once.
        game.units.pop();
        assert_eq!(game.founding_issue(Hex::new(0, 0)), None);
        // And it adds no more, of its own or strays.
        game.stray_limit = 4;
        for _ in 0..2 * DEN_BREED_TURNS {
            game.resolve_dens();
        }
        assert_eq!(game.units.len(), 1);
    }

    #[test]
    fn a_den_adds_animals_up_to_its_cap() {
        let mut game = den_of(3);
        let den = Hex::new(0, 0);
        let count = |game: &GameState| game.units.iter().filter(|u| u.home == Some(den)).count();
        for born in 2..=3 {
            // Its animals roam off the den, leaving room.
            for (i, unit) in game.units.iter_mut().enumerate() {
                unit.pos = Hex::new(2, i as i32 - 1);
            }
            for turn in 1..DEN_BREED_TURNS {
                game.resolve_dens();
                assert_eq!(count(&game), born - 1, "turn {turn}");
                assert_eq!(game.den_at(den).unwrap().next_in, DEN_BREED_TURNS - turn);
            }
            game.resolve_dens();
            assert_eq!(count(&game), born);
            let new = game.units.last().unwrap();
            assert_eq!((new.pos, new.home), (den, Some(den)));
        }
        // Full, in a world with no room for strays: no more, and the next
        // waits, due.
        for (i, unit) in game.units.iter_mut().enumerate() {
            unit.pos = Hex::new(2, i as i32 - 1);
        }
        for _ in 0..2 * DEN_BREED_TURNS {
            game.resolve_dens();
        }
        assert_eq!(game.units.len(), 3);
        assert_eq!(game.den_at(den).unwrap().next_in, 0);
        // One dies: the den has another at once.
        game.units.pop();
        game.resolve_dens();
        assert_eq!(count(&game), 3);
        assert_eq!(game.den_at(den).unwrap().next_in, DEN_BREED_TURNS);
    }

    #[test]
    fn a_full_den_sends_out_strays_up_to_the_worlds_limit() {
        let mut game = wolf_den();
        game.stray_limit = 2;
        let den = Hex::new(0, 0);
        game.units[0].pos = Hex::new(3, 0);
        let strays = |game: &GameState| game.units.iter().filter(|u| u.is_stray()).count();
        for stray in 1..=2 {
            for turn in 1..DEN_BREED_TURNS {
                game.resolve_dens();
                assert_eq!(strays(&game), stray - 1, "turn {turn}");
            }
            game.resolve_dens();
            assert_eq!(strays(&game), stray);
            let new = game.units.last().unwrap();
            assert_eq!((new.pos, new.home, new.team), (den, None, Team::Wild));
            game.units.last_mut().unwrap().pos = Hex::new(-3, stray as i32);
            assert_eq!(game.den_at(den).unwrap().next_in, DEN_BREED_TURNS);
        }
        // Its own is still one, at its cap.
        assert_eq!(game.units.iter().filter(|u| u.home == Some(den)).count(), 1);
        // At the limit it waits, due...
        for _ in 0..2 * DEN_BREED_TURNS {
            game.resolve_dens();
        }
        assert_eq!(strays(&game), 2);
        assert_eq!(game.den_at(den).unwrap().next_in, 0);
        // ...until a stray dies.
        let gone = game.units.iter().position(Unit::is_stray).unwrap();
        game.units.remove(gone);
        game.resolve_dens();
        assert_eq!(strays(&game), 2);
    }

    #[test]
    fn a_stray_roams_and_hunts_beyond_any_territory_but_never_into_a_city() {
        let mut game = wolf_den();
        game.units.clear();
        // A stray wolf pack at the den, with nobody about: it ranges far,
        // as far as it can go each turn.
        let id = add(&mut game, Hex::new(0, 0), Team::Wild, UnitType::Wolf);
        let stray = |game: &GameState| game.units.iter().find(|u| u.id == id).unwrap().clone();
        assert!(stray(&game).is_stray());
        let mut farthest = 0;
        for _ in 0..12 {
            let was = stray(&game).pos;
            play(&mut game);
            let now = stray(&game).pos;
            assert_eq!(now.distance(was), 2, "{was:?} to {now:?}");
            farthest = farthest.max(now.distance(Hex::new(0, 0)));
        }
        assert!(farthest > territory(UnitType::Wolf), "only {farthest}");
        // It notices prey farther off than a den's wolf pack would.
        let range = hunting_range(UnitType::Wolf) + STRAY_EXTRA_HUNTING;
        assert_eq!(hunts_within(&stray(&game)), range);
        let idx = game.units.iter().position(|u| u.id == id).unwrap();
        game.units[idx].pos = Hex::new(-3, 0);
        let scout = add(
            &mut game,
            Hex::new(-3 + range, 0),
            Team::Blue,
            UnitType::Scout,
        );
        assert_eq!(game.animal_prey(idx), Some(Hex::new(-3 + range, 0)));
        // A scout in a city is safe from it, as from any animal.
        let city = Hex::new(-3 + range, 0);
        game.cities = vec![crate::game::city::City::new(0, Team::Blue, city)];
        game.auto_assign_city(0);
        assert_eq!(game.animal_prey(idx), None);
        game.units[idx].pos = Hex::new(-4 + range, 0);
        for _ in 0..6 {
            play(&mut game);
            let wolf = stray(&game);
            assert_ne!(wolf.pos, city);
        }
        let scout = game.units.iter().find(|u| u.id == scout).unwrap();
        assert_eq!(scout.hp, scout.max_hp(), "safe in its city");
        assert_eq!(game.cities[0].team, Team::Blue);
    }

    #[test]
    fn a_den_has_another_animal_some_turns_after_one_dies() {
        let mut game = wolf_den();
        game.units.clear();
        for turn in 1..DEN_BREED_TURNS {
            game.resolve_dens();
            assert!(game.units.is_empty(), "turn {turn}");
            assert_eq!(
                game.den_at(Hex::new(0, 0)).unwrap().next_in,
                DEN_BREED_TURNS - turn
            );
        }
        // Due, but a settler stands on the den: it waits for it to leave.
        let settler = add(&mut game, Hex::new(0, 0), Team::Blue, UnitType::Melee);
        game.settlers.insert(settler);
        game.resolve_dens();
        assert_eq!(game.den_at(Hex::new(0, 0)).unwrap().next_in, 0);
        game.units.clear();
        game.resolve_dens();
        let wolf = animal(&game);
        assert_eq!((wolf.pos, wolf.unit_type), (Hex::new(0, 0), UnitType::Wolf));
        assert_eq!(
            game.den_at(Hex::new(0, 0)).unwrap().next_in,
            DEN_BREED_TURNS
        );
    }

    #[test]
    fn the_animals_setting_picks_how_many_dens_a_world_has_and_how_many_each_keeps() {
        use crate::game::settings::{ANIMALS_FEW, ANIMALS_MANY, ANIMALS_OFF, Settings};
        let world = |animals| {
            let settings = Settings {
                world_ai: 3,
                world_animals: animals,
                ..Settings::default()
            };
            GameState::world_scenario_with(11, &settings)
        };
        let off = world(ANIMALS_OFF);
        assert!(off.dens.is_empty());
        assert!(off.units.iter().all(|u| !u.is_animal()));
        let few = world(ANIMALS_FEW);
        assert_eq!(few.dens.len(), 4, "one a side");
        assert!(few.dens.iter().all(|d| d.cap == 2), "two animals each");
        let many = world(ANIMALS_MANY);
        assert_eq!(many.dens.len(), 8, "two a side");
        assert!(many.dens.iter().all(|d| d.cap == 3), "three animals each");
        let sites = |game: &GameState| {
            game.dens
                .iter()
                .map(|d| (d.pos, d.kind))
                .collect::<Vec<_>>()
        };
        assert_eq!(sites(&many)[..4], sites(&few)[..], "the same first few");
        for game in [&few, &many] {
            assert_eq!(
                game.units.iter().filter(|u| u.is_animal()).count(),
                game.dens.len(),
                "an animal a den to start with"
            );
            assert!(game.dens.iter().all(|d| d.next_in == DEN_BREED_TURNS));
        }
        // A stray a side for each den a side.
        assert_eq!(off.stray_limit, 0);
        assert_eq!(few.stray_limit, 4 * STRAYS_PER_SIDE);
        assert_eq!(many.stray_limit, 8 * STRAYS_PER_SIDE);
    }

    #[test]
    fn the_tooltip_tells_what_a_den_keeps_and_gives() {
        let mut game = den_of(2);
        let notes = game.den_notes(Hex::new(0, 0), false);
        assert_eq!(
            notes[..2],
            [
                "WOLF DEN: UP TO 2 WOLF PACKS, ROAMING WITHIN 4 HEXES OF IT",
                "THEY HUNT WHOEVER COMES WITHIN 4 HEXES OF THEM",
            ]
        );
        assert!(
            notes[2].contains("CLEAR IT") && notes[2].contains("FOOD"),
            "{notes:?}"
        );
        assert_eq!(notes[3], "CLEARED, ITS LAND IS FREE TO SETTLE");
        assert_eq!(
            notes[4],
            format!("ANOTHER WOLF PACK IN {DEN_BREED_TURNS} TURNS")
        );
        game.resolve_dens();
        let notes = game.den_notes(Hex::new(0, 0), false);
        assert_eq!(
            notes[4],
            format!("ANOTHER WOLF PACK IN {} TURNS", DEN_BREED_TURNS - 1)
        );
        // Full, its next goes out as a stray; the world keeps so many.
        let mut full = wolf_den();
        full.stray_limit = 4;
        assert_eq!(
            full.den_notes(Hex::new(0, 0), false)[4..],
            [
                format!("FULL: ANOTHER WOLF PACK LEAVES IT AS A STRAY IN {DEN_BREED_TURNS} TURNS"),
                "STRAYS ROAM ANYWHERE, HUNTING WITHIN 6 HEXES: UP TO 4 IN THE WORLD".into(),
            ]
        );
        // Out of sight, only what was seen: nothing, before anyone looked.
        assert!(game.den_notes(Hex::new(0, 0), true).is_empty());
        assert!(game.den_notes(Hex::new(1, 0), false).is_empty());
    }
}
