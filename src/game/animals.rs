//! Animals: aggressive neutral units that guard dens, to make the early map
//! dangerous. They belong to the wild (`Team::Wild`), which is no side: no
//! cities, stockpile, fog or plan, never a seat, and hostile to every side.
//!
//! `mapgen.rs` places the dens of a world (on forest, jungle and hills, away
//! from every start), and each den keeps one animal: a wolf pack or a bear
//! (`UnitType::Wolf`, `UnitType::Bear`). An animal is territorial: it never
//! leaves the hexes within `TERRITORY_RADIUS` of its den (`Unit::home`). As
//! its own move step begins it goes for the nearest unit or worker out on
//! the map within its territory, and with none there heads home; as its
//! attack step begins it attacks whoever is in reach. It never attacks or
//! enters a city, and never captures anything (a worker it reaches, it
//! kills). It decides on the real board as the step begins, the same on
//! every machine: its "plan" is a rule of the game, not a side's planning.
//! Ties break by hex coordinates.

use super::GameState;
use super::hex::Hex;
use super::unit::{Team, Unit, UnitType};

/// How far from its den an animal goes: it stays within this many hexes.
pub const TERRITORY_RADIUS: i32 = 3;

/// An animal's den (`mapgen.rs` places them).
#[derive(Clone, Debug, PartialEq)]
pub struct Den {
    pub pos: Hex,
    /// The animal it keeps.
    pub kind: UnitType,
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

impl GameState {
    /// The den on `hex`, if there is one.
    pub(super) fn den_at(&self, hex: Hex) -> Option<&Den> {
        self.dens.iter().find(|d| d.pos == hex)
    }

    /// The tile tooltip's lines about a den on `hex`. Out of sight
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
        vec![format!(
            "{}: ITS ANIMAL ATTACKS WHOEVER COMES WITHIN {TERRITORY_RADIUS} HEXES",
            den.name()
        )]
    }

    /// Makes a den on each of `sites`, wolves and bears in turn, each with
    /// its animal at home.
    pub(super) fn make_dens(&mut self, sites: &[Hex]) {
        for (i, &pos) in sites.iter().enumerate() {
            let kind = if i % 2 == 0 {
                UnitType::Wolf
            } else {
                UnitType::Bear
            };
            self.dens.push(Den { pos, kind });
            self.spawn_animal(self.dens.len() - 1);
        }
    }

    /// Den `den`'s animal appears on it, if nothing stands there.
    fn spawn_animal(&mut self, den: usize) -> bool {
        let Den { pos, kind } = self.dens[den];
        if self.is_occupied(pos) || self.field_workers.iter().any(|w| w.pos == pos) {
            return false;
        }
        let id = self.next_unit_id;
        self.next_unit_id += 1;
        let mut animal = Unit::new(id, pos, Team::Wild, kind);
        animal.home = Some(pos);
        self.units.push(animal);
        true
    }

    /// Whether `hex` is in the territory of animal `idx` (within
    /// `TERRITORY_RADIUS` of its den).
    fn in_territory(&self, idx: usize, hex: Hex) -> bool {
        let unit = &self.units[idx];
        unit.home.unwrap_or(unit.pos).distance(hex) <= TERRITORY_RADIUS
    }

    /// Whether an animal may attack whoever stands on `hex`: never on a
    /// city center, nor out on the water.
    fn animal_may_attack(&self, hex: Hex) -> bool {
        !self.cities.iter().any(|c| c.pos == hex)
            && self.grid.contains(hex)
            && !self.grid.terrain(hex).is_water()
    }

    /// As the move step of `kind` begins: each animal of that kind goes for
    /// the nearest unit or worker in its territory it may attack
    /// (`animal_may_attack`), as close as it can get within its territory;
    /// with none there, it heads home. It won't step onto a worker (it
    /// kills them, it doesn't capture them), a city, or a hex another
    /// animal is heading for this step; staying put wins a tie, then the
    /// lowest (q, r).
    pub(super) fn plan_animal_moves(&mut self, kind: UnitType) {
        for idx in 0..self.units.len() {
            let unit = &self.units[idx];
            if !unit.is_animal() || unit.unit_type != kind || self.rival_of(idx).is_some() {
                continue;
            }
            let (pos, home) = (unit.pos, unit.home.unwrap_or(unit.pos));
            let prey = self
                .units
                .iter()
                .filter(|u| !u.is_animal())
                .map(|u| u.pos)
                .chain(self.field_workers.iter().map(|w| w.pos))
                .filter(|&h| self.in_territory(idx, h) && self.animal_may_attack(h))
                .min_by_key(|h| (h.distance(pos), h.q, h.r));
            let goal = prey.unwrap_or(home);
            let claimed = |hex: Hex| {
                self.units
                    .iter()
                    .any(|u| u.is_animal() && u.planned_move == Some(hex))
            };
            let options = self.reachable_hexes_by(pos, unit.stats().move_range, |from, to| {
                self.in_territory(idx, to)
                    && self.can_step(from, to, Team::Wild)
                    && !self.is_occupied(to)
                    && !self.field_workers.iter().any(|w| w.pos == to)
            });
            let dest = options
                .into_iter()
                .filter(|&h| h == pos || !claimed(h))
                .min_by_key(|&h| (h.distance(goal), h != pos, h.q, h.r))
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
    use crate::game::hex::HexGrid;
    use crate::game::terrain::Tile;
    use crate::game::workers::{FieldWorker, JobKind, WorkerJob};

    /// An open radius-6 map with no cities, a wolf den at (0, 0) with its
    /// wolf at home, and nothing else.
    fn wolf_den() -> GameState {
        let mut game = GameState::new();
        game.units.clear();
        game.grid = HexGrid::new(6, [(Hex::new(0, 0), Tile::HILLS)]);
        game.make_dens(&[Hex::new(0, 0)]);
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
    fn a_den_keeps_its_animal_at_home() {
        let game = wolf_den();
        let wolf = animal(&game);
        assert_eq!(wolf.team, Team::Wild);
        assert_eq!(wolf.unit_type, UnitType::Wolf);
        assert_eq!(
            (wolf.pos, wolf.home),
            (Hex::new(0, 0), Some(Hex::new(0, 0)))
        );
        assert_eq!(game.den_at(Hex::new(0, 0)).map(Den::name), Some("WOLF DEN"));
        assert!(!game.ai_teams().contains(&Team::Wild));
    }

    #[test]
    fn an_animal_attacks_an_intruder_and_goes_home_after() {
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
        // With the scout gone, it heads home.
        game.units.retain(|u| u.is_animal());
        play(&mut game);
        play(&mut game);
        assert_eq!(animal(&game).pos, Hex::new(0, 0));
    }

    #[test]
    fn a_troop_on_alert_fires_at_an_animal_in_range() {
        let mut game = wolf_den();
        // At the edge of the wolf's territory, out of range until it comes.
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
        // Just past its territory: it goes to the edge and waits there.
        add(&mut game, Hex::new(5, 0), Team::Blue, UnitType::Melee);
        for _ in 0..3 {
            play(&mut game);
            let wolf = animal(&game);
            assert!(wolf.pos.distance(Hex::new(0, 0)) <= TERRITORY_RADIUS);
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
        for _ in 0..3 {
            play(&mut game);
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
        assert!(animal(&game).pos != Hex::new(2, 0));
    }

    #[test]
    fn an_animal_kills_a_worker_rather_than_capture_it() {
        let mut game = wolf_den();
        game.cities = vec![crate::game::city::City::new(0, Team::Blue, Hex::new(-5, 0))];
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
}
