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
//!
//! Hunting pays: the side that kills an animal gets food (`bounty`), and a
//! side that stands a unit (not a settler) on a den at a turn's end clears
//! it for food and metal (`DEN_SPOILS`). Until then, a den whose animal
//! died has another there `DEN_RETURN_TURNS` turns later (`resolve_dens`).
//! How many dens a world has is the Animals setting (`world_animals`).

use super::GameState;
use super::city::{Stock, stock_words};
use super::hex::Hex;
use super::unit::{Team, Unit, UnitType};

/// How far from its den an animal goes: it stays within this many hexes.
pub const TERRITORY_RADIUS: i32 = 3;
/// Turns after its animal dies that a den has another.
pub const DEN_RETURN_TURNS: u32 = 8;
/// What clearing a den gives the side that clears it.
pub const DEN_SPOILS: Stock = Stock::whole(6, 0, 2);

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
    /// The animal it keeps.
    pub kind: UnitType,
    /// With its animal dead: the turn ends until another is there (at 1,
    /// the next; at 0, as soon as nothing stands on the den).
    pub returns_in: Option<u32>,
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

impl GameState {
    /// The den on `hex`, if there is one.
    pub(super) fn den_at(&self, hex: Hex) -> Option<&Den> {
        self.dens.iter().find(|d| d.pos == hex)
    }

    /// The tile tooltip's lines about a den on `hex`: what it keeps, what
    /// clearing it gives, and when a dead animal's successor comes. Out of
    /// sight (`remembered`), only that it was there when last seen.
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
        let animal = animal_name(den.kind);
        let mut notes = vec![
            format!(
                "{}: ITS {animal} ATTACKS WHOEVER COMES WITHIN {TERRITORY_RADIUS} HEXES",
                den.name()
            ),
            format!(
                "END A TURN ON IT WITH A UNIT TO CLEAR IT: +{} FOR THE STOCKPILE",
                stock_words(DEN_SPOILS)
            ),
        ];
        if let Some(turns) = den.returns_in {
            notes.push(format!("ANOTHER {animal} IN {} TURNS", turns.max(1)));
        }
        notes
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
            self.dens.push(Den {
                pos,
                kind,
                returns_in: None,
            });
            self.spawn_animal(self.dens.len() - 1);
        }
    }

    /// At a turn's end, before the economy: a den with a side's unit (not a
    /// settler) on it is cleared, and gone, and the side gets
    /// `DEN_SPOILS`; its animal, if alive, keeps to its territory. A den
    /// whose animal has died counts down `DEN_RETURN_TURNS` and then has
    /// another, once nothing stands on it.
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
                let what = format!("+{} FOR THE STOCKPILE", stock_words(DEN_SPOILS));
                log::info!("{team:?} clears the {} at {pos:?}: {what}", den.name());
                if team == self.local_team {
                    self.notice = format!("{} CLEARED: {what}", den.name());
                }
                continue;
            }
            let alive = self.units.iter().any(|u| u.home == Some(pos));
            let returns_in = match self.dens[d].returns_in {
                None if alive => None,
                None => Some(DEN_RETURN_TURNS),
                Some(turns) if turns > 1 => Some(turns - 1),
                Some(_) => (!self.spawn_animal(d)).then_some(0),
            };
            self.dens[d].returns_in = returns_in;
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

    /// Den `den`'s animal appears on it, if nothing stands there.
    fn spawn_animal(&mut self, den: usize) -> bool {
        let Den { pos, kind, .. } = self.dens[den];
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
    use crate::game::turn::Phase;
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
        let mut game = wolf_den();
        // The wolf is out; a settler on the den doesn't clear it.
        game.units[0].pos = Hex::new(3, 0);
        let settler = add(&mut game, Hex::new(0, 0), Team::Blue, UnitType::Melee);
        game.settlers.insert(settler);
        let before = game.stock(Team::Blue);
        game.resolve_dens();
        assert_eq!(game.dens.len(), 1);
        assert_eq!(game.stock(Team::Blue), before);
        // A scout does, and the wolf keeps to its old territory.
        game.settlers.clear();
        game.units.last_mut().unwrap().unit_type = UnitType::Scout;
        game.resolve_dens();
        assert!(game.dens.is_empty(), "cleared");
        assert_eq!(game.stock(Team::Blue), before + DEN_SPOILS);
        assert_eq!(animal(&game).home, Some(Hex::new(0, 0)));
    }

    #[test]
    fn a_den_has_another_animal_some_turns_after_one_dies() {
        let mut game = wolf_den();
        game.units.clear();
        for turn in 1..=DEN_RETURN_TURNS {
            game.resolve_dens();
            assert!(game.units.is_empty(), "turn {turn}");
            assert_eq!(
                game.den_at(Hex::new(0, 0)).unwrap().returns_in,
                Some(DEN_RETURN_TURNS + 1 - turn)
            );
        }
        // Due, but a settler stands on the den: it waits for it to leave.
        let settler = add(&mut game, Hex::new(0, 0), Team::Blue, UnitType::Melee);
        game.settlers.insert(settler);
        game.resolve_dens();
        assert_eq!(game.den_at(Hex::new(0, 0)).unwrap().returns_in, Some(0));
        game.units.clear();
        game.resolve_dens();
        let wolf = animal(&game);
        assert_eq!((wolf.pos, wolf.unit_type), (Hex::new(0, 0), UnitType::Wolf));
        assert_eq!(game.den_at(Hex::new(0, 0)).unwrap().returns_in, None);
    }

    #[test]
    fn the_animals_setting_picks_how_many_dens_a_world_has() {
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
        let many = world(ANIMALS_MANY);
        assert_eq!(many.dens.len(), 8, "two a side");
        assert_eq!(many.dens[..4], few.dens[..], "the same first few");
        for game in [&few, &many] {
            assert_eq!(
                game.units.iter().filter(|u| u.is_animal()).count(),
                game.dens.len(),
                "an animal a den"
            );
        }
    }

    #[test]
    fn the_tooltip_tells_what_a_den_keeps_and_gives() {
        let mut game = wolf_den();
        let notes = game.den_notes(Hex::new(0, 0), false);
        assert!(
            notes[0].starts_with("WOLF DEN: ITS WOLF PACK ATTACKS"),
            "{notes:?}"
        );
        assert!(
            notes[1].contains("CLEAR IT") && notes[1].contains("FOOD"),
            "{notes:?}"
        );
        assert_eq!(notes.len(), 2);
        game.units.clear();
        game.resolve_dens();
        let notes = game.den_notes(Hex::new(0, 0), false);
        assert_eq!(
            notes[2],
            format!("ANOTHER WOLF PACK IN {DEN_RETURN_TURNS} TURNS")
        );
        // Out of sight, only what was seen: nothing, before anyone looked.
        assert!(game.den_notes(Hex::new(0, 0), true).is_empty());
        assert!(game.den_notes(Hex::new(1, 0), false).is_empty());
    }
}
