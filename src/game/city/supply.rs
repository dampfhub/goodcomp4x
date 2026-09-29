//! The military supply limit (#237, `docs/game-rules.md`, Supply): a hard
//! cap on a side's army. Its cities give supply (`supply_from_cities`), and
//! every troop, ship and scout uses some (`unit_type_supply`); settlers and
//! workers use none.
//!
//! - **Queueing** a unit that uses supply needs room for it: the side's
//!   units alive (passengers aboard included) and every such item queued,
//!   paid or not (`supply_used`), plus it, within the cap (`supply_lock`).
//! - **Starting** one (paying for it, `work_queues`) needs room too, counting
//!   only the units alive and the items already started
//!   (`supply_started`): an item queued before the side lost a city or
//!   citizens waits in the queue, like one waiting for the stockpile, until
//!   there's room again. One already started finishes.
//! - **Over the cap**, nothing else happens: units alive stay, and only new
//!   training waits.
//!
//! The formula is meant to change: it's `supply_from_cities` and the table
//! in `unit_type_supply`, nothing else.
use super::{Build, BuildUnit, City, Lane, Queued};
use crate::game::GameState;
use crate::game::unit::{Team, Unit, UnitType};

/// Supply each city gives its side, whatever its size: room for the AI's
/// two troops a city (`AI_ARMY_PER_CITY`) and a scout.
pub(in crate::game) const SUPPLY_PER_CITY: u32 = 3;
/// Supply each citizen of a city adds.
pub(in crate::game) const SUPPLY_PER_CITIZEN: u32 = 1;
/// What a card the supply locks says (`supply_lock` adds the numbers).
pub(in crate::game) const SUPPLY_FULL_HINT: &str = "SUPPLY FULL";

/// The supply cities of these populations give their side: the one place
/// the formula lives.
pub(in crate::game) fn supply_from_cities(populations: impl IntoIterator<Item = usize>) -> u32 {
    populations
        .into_iter()
        .map(|population| SUPPLY_PER_CITY + SUPPLY_PER_CITIZEN * population as u32)
        .sum()
}

/// The supply a unit of `kind` uses: 1 for every troop, ship and scout (a
/// settler, which has the Melee body, is left out by `unit_supply`), none
/// for animals, which belong to no side.
pub(in crate::game) fn unit_type_supply(kind: UnitType) -> u32 {
    match kind {
        UnitType::Melee
        | UnitType::Ranged
        | UnitType::Cavalry
        | UnitType::Siege
        | UnitType::Armored
        | UnitType::Scout
        | UnitType::PatrolGalley
        | UnitType::LandingCraft
        | UnitType::BombardShip => 1,
        UnitType::Wolf | UnitType::Bear => 0,
    }
}

/// The supply what `build` turns out uses: a troop's or ship's, a scout's;
/// none for a settler, a worker, growth or gathering.
pub(in crate::game) fn build_supply(build: Build) -> u32 {
    match build {
        Build::Unit(unit) => unit_type_supply(unit.unit_type()),
        Build::Scout => unit_type_supply(UnitType::Scout),
        Build::Settler | Build::Worker | Build::Grow | Build::Gather => 0,
    }
}

/// The supply a Barracks troop uses.
pub(in crate::game) fn troop_supply(build: BuildUnit) -> u32 {
    build_supply(Build::Unit(build))
}

/// The supply `city`'s queued items use: all of them, or only those paid
/// for (started).
fn city_queued_supply(city: &City, paid_only: bool) -> u32 {
    let counts = |paid: bool| !paid_only || paid;
    let own: u32 = city
        .queue
        .iter()
        .filter(|q| counts(q.paid))
        .map(|q| build_supply(q.build))
        .sum();
    let barracks: u32 = city
        .barracks_queue
        .iter()
        .filter(|q| counts(q.paid))
        .map(|q| troop_supply(q.build))
        .sum();
    own + barracks
}

/// The supply the items of a city's two queues use, as a plan carries them.
pub(in crate::game) fn queues_supply(
    queue: &[Queued<Build>],
    barracks: &[Queued<BuildUnit>],
) -> u32 {
    queue.iter().map(|q| build_supply(q.build)).sum::<u32>()
        + barracks.iter().map(|q| troop_supply(q.build)).sum::<u32>()
}

impl GameState {
    /// The supply `team` has: what its cities give (`supply_from_cities`).
    pub(in crate::game) fn supply_cap(&self, team: Team) -> u32 {
        let cities = supply_from_cities(
            self.cities
                .iter()
                .filter(|c| c.team == team)
                .map(|c| c.population),
        );
        #[cfg(test)]
        let cities = cities + self.extra_supply[team.index()];
        cities
    }

    /// The supply `unit` uses, with its passengers: none for a settler.
    pub(in crate::game) fn unit_supply(&self, unit: &Unit) -> u32 {
        let own = if self.settlers.contains(&unit.id) || unit.is_animal() {
            0
        } else {
            unit_type_supply(unit.unit_type)
        };
        own + unit
            .cargo
            .iter()
            .map(|passenger| self.unit_supply(passenger))
            .sum::<u32>()
    }

    /// The supply `team`'s units alive use.
    pub(in crate::game) fn units_supply(&self, team: Team) -> u32 {
        self.units
            .iter()
            .filter(|u| u.team == team)
            .map(|u| self.unit_supply(u))
            .sum()
    }

    /// What counts against `team`'s supply: its units alive and everything
    /// its queues hold that uses supply, paid for or not. The top bar shows
    /// it against `supply_cap`, and queueing keeps it within.
    pub(in crate::game) fn supply_used(&self, team: Team) -> u32 {
        self.units_supply(team)
            + self
                .cities
                .iter()
                .filter(|c| c.team == team)
                .map(|c| city_queued_supply(c, false))
                .sum::<u32>()
    }

    /// What starting another item counts against: `team`'s units alive and
    /// the items its queues have started (paid for).
    pub(in crate::game) fn supply_started(&self, team: Team) -> u32 {
        self.units_supply(team)
            + self
                .cities
                .iter()
                .filter(|c| c.team == team)
                .map(|c| city_queued_supply(c, true))
                .sum::<u32>()
    }

    /// The supply `team` may still start items with this turn: its cap less
    /// what it has started, below zero if it is over the cap.
    pub(in crate::game) fn supply_room(&self, team: Team) -> i64 {
        i64::from(self.supply_cap(team)) - i64::from(self.supply_started(team))
    }

    /// Why `team` can't queue something using `supply` more now, if it
    /// can't: its units and queued items with it would pass its cap.
    pub(in crate::game) fn supply_lock(&self, team: Team, supply: u32) -> Option<String> {
        let (used, cap) = (self.supply_used(team), self.supply_cap(team));
        (supply > 0 && used + supply > cap).then(|| format!("{SUPPLY_FULL_HINT} ({used}/{cap})"))
    }

    /// The supply item `index` of one of `city`'s queues uses.
    pub(in crate::game) fn item_supply(&self, city: usize, lane: Lane, index: usize) -> u32 {
        let c = &self.cities[city];
        match lane {
            Lane::City => build_supply(c.queue[index].build),
            Lane::Barracks => troop_supply(c.barracks_queue[index].build),
        }
    }

    /// For a card's tooltip: what the unit it trains uses, and the side's
    /// supply now. Empty for a build that uses none.
    pub(in crate::game) fn supply_note(&self, build: Build) -> String {
        let supply = build_supply(build);
        if supply == 0 {
            return String::new();
        }
        let team = self.local_team;
        format!(
            " · USES {supply} SUPPLY: {} OF {} USED",
            self.supply_used(team),
            self.supply_cap(team)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::city::{Building, Stock};
    use crate::game::hex::Hex;

    /// The Cities scenario with Blue's city open, a stockpile for anything
    /// and only the supply its city gives.
    fn blue_city() -> GameState {
        let mut g = GameState::city_scenario();
        g.fund(Team::Blue);
        g.extra_supply = [0; Team::ALL.len()];
        g.selected_city = Some(0);
        assert_eq!(g.cities[0].team, Team::Blue);
        g
    }

    /// Queues Melee in Blue's city until its supply is all used.
    fn fill_supply(g: &mut GameState) {
        for _ in 0..g.supply_cap(Team::Blue) {
            if g.supply_used(Team::Blue) < g.supply_cap(Team::Blue) {
                g.queue_selected_city_unit(BuildUnit::Melee);
            }
        }
        assert_eq!(g.supply_used(Team::Blue), g.supply_cap(Team::Blue));
    }

    #[test]
    fn supply_is_a_base_per_city_and_an_amount_per_citizen() {
        assert_eq!(supply_from_cities([]), 0);
        assert_eq!(
            supply_from_cities([1]),
            SUPPLY_PER_CITY + SUPPLY_PER_CITIZEN
        );
        assert_eq!(
            supply_from_cities([2, 5]),
            2 * SUPPLY_PER_CITY + 7 * SUPPLY_PER_CITIZEN
        );
        let mut g = blue_city();
        let cap = g.supply_cap(Team::Blue);
        assert_eq!(cap, supply_from_cities([g.cities[0].population]));
        g.cities[0].population += 1;
        assert_eq!(g.supply_cap(Team::Blue), cap + SUPPLY_PER_CITIZEN);
    }

    #[test]
    fn troops_ships_and_scouts_use_supply_but_settlers_and_workers_do_not() {
        for kind in [
            UnitType::Melee,
            UnitType::Ranged,
            UnitType::Cavalry,
            UnitType::Siege,
            UnitType::Armored,
            UnitType::Scout,
            UnitType::PatrolGalley,
            UnitType::LandingCraft,
            UnitType::BombardShip,
        ] {
            assert_eq!(unit_type_supply(kind), 1, "{kind:?}");
        }
        assert_eq!(build_supply(Build::Scout), 1);
        for build in [Build::Settler, Build::Worker, Build::Grow, Build::Gather] {
            assert_eq!(build_supply(build), 0, "{build:?}");
        }
        let mut g = blue_city();
        g.units.retain(|u| u.team != Team::Blue);
        assert_eq!(g.supply_used(Team::Blue), 0);
        let at = |q| Hex::new(q, 5);
        let settler = Unit::new(100, at(-3), Team::Blue, UnitType::Melee);
        g.settlers.insert(settler.id);
        let mut craft = Unit::new(101, at(-2), Team::Blue, UnitType::LandingCraft);
        craft
            .cargo
            .push(Unit::new(102, at(-2), Team::Blue, UnitType::Melee));
        g.units.extend([
            settler,
            craft,
            Unit::new(103, at(-1), Team::Blue, UnitType::Scout),
        ]);
        // The craft, its passenger and the scout; not the settler.
        assert_eq!(g.units_supply(Team::Blue), 3);
        // Queued items count, paid or not; only those paid have started.
        g.cities[0].queue = vec![
            Queued::new(Build::Unit(BuildUnit::Melee)),
            Queued::prepaid(Build::Scout),
            Queued::new(Build::Settler),
            Queued::new(Build::Worker),
        ];
        g.cities[0].barracks_queue = vec![Queued::new(BuildUnit::Siege)];
        assert_eq!(g.supply_used(Team::Blue), 6);
        assert_eq!(g.supply_started(Team::Blue), 4);
    }

    #[test]
    fn nothing_that_uses_supply_is_queued_past_the_cap() {
        let mut g = blue_city();
        g.cities[0].population = 3;
        g.cities[0].barracks = Some(Hex::new(-2, 0));
        g.cities[0].built.push(Building::Barracks);
        fill_supply(&mut g);
        let (used, cap) = (g.supply_used(Team::Blue), g.supply_cap(Team::Blue));
        let queued = g.cities[0].queue.len();
        g.queue_selected_city_unit(BuildUnit::Melee);
        assert_eq!(g.notice, format!("MELEE: SUPPLY FULL ({used}/{cap})"));
        g.queue_selected_city_scout();
        assert_eq!(g.notice, format!("SCOUT: SUPPLY FULL ({used}/{cap})"));
        assert_eq!(g.cities[0].queue.len(), queued);
        // The Barracks too; its deposit cap still says why first.
        g.queue_selected_barracks_unit(BuildUnit::Ranged);
        assert_eq!(g.notice, format!("RANGED: SUPPLY FULL ({used}/{cap})"));
        assert!(g.cities[0].barracks_queue.is_empty());
        assert!(g.deposit_lock(0, BuildUnit::Armored).is_some());
        assert_eq!(
            g.barracks_lock(0, BuildUnit::Armored),
            g.deposit_lock(0, BuildUnit::Armored)
        );
        // A Settler, a Worker and growth use none.
        g.queue_selected_city_settler();
        g.queue_selected_city_worker();
        g.queue_selected_city_growth();
        assert_eq!(g.cities[0].queue.len(), queued + 3);
        // Losing a troop makes room.
        let blue = g.units.iter().position(|u| u.team == Team::Blue).unwrap();
        g.units.remove(blue);
        g.queue_selected_barracks_unit(BuildUnit::Ranged);
        assert_eq!(g.cities[0].barracks_queue.len(), 1);
    }

    #[test]
    fn an_item_queued_before_the_cap_fell_waits_for_supply_in_place() {
        let mut g = blue_city();
        g.cities[0].population = 3;
        fill_supply(&mut g);
        g.cities[0].queue.truncate(1);
        g.queue_selected_city_worker();
        let melee = Build::Unit(BuildUnit::Melee);
        let units = g.units.len();
        // Citizens lost: Blue is over its cap, and keeps its units.
        assert!(g.remove_citizen(0) && g.remove_citizen(0));
        assert!(g.supply_used(Team::Blue) > g.supply_cap(Team::Blue));
        assert_eq!(g.supply_room(Team::Blue), 0);
        assert_eq!(g.units.len(), units);
        let forecast = g.forecast(Team::Blue);
        let lane = forecast.lane(0, Lane::City).unwrap();
        assert_eq!(lane.worked, Some(1), "the Worker behind it goes ahead");
        assert_eq!(g.supply_waiting_items(lane), [0]);
        assert!(g.waiting_items(lane).is_empty(), "not for the stockpile");
        g.resolve_economy();
        assert!(!g.cities[0].queue[0].paid, "it waits, unpaid");
        assert_eq!(g.cities[0].queue[0].build, melee);
        assert!(g.cities[0].queue[1].paid);
        // A troop lost makes room, and it starts.
        let blue = g.units.iter().position(|u| u.team == Team::Blue).unwrap();
        g.units.remove(blue);
        let lane = g.forecast(Team::Blue).lane(0, Lane::City).unwrap();
        assert_eq!(lane.worked, Some(0));
        g.resolve_economy();
        assert!(g.cities[0].queue[0].paid);
        assert!(g.supply_overruns.is_empty(), "{:?}", g.supply_overruns);
    }

    #[test]
    fn a_troop_started_before_the_cap_fell_finishes() {
        let mut g = blue_city();
        g.cities[0].population = 3;
        fill_supply(&mut g);
        g.cities[0].queue.truncate(1);
        g.resolve_economy();
        assert!(g.cities[0].queue[0].paid);
        assert!(g.remove_citizen(0) && g.remove_citizen(0));
        assert!(g.supply_started(Team::Blue) > g.supply_cap(Team::Blue));
        let troops = |g: &GameState| g.units.iter().filter(|u| u.team == Team::Blue).count();
        let before = troops(&g);
        for _ in 0..12 {
            if g.cities[0].queue.is_empty() {
                break;
            }
            g.resolve_economy();
        }
        assert!(g.cities[0].queue.is_empty(), "the Melee came out");
        assert_eq!(troops(&g), before + 1);
        assert!(g.supply_used(Team::Blue) > g.supply_cap(Team::Blue));
        assert!(g.supply_overruns.is_empty(), "{:?}", g.supply_overruns);
    }

    #[test]
    fn the_ai_queues_no_troop_or_scout_past_its_supply() {
        for (population, room) in [(1, false), (2, true)] {
            let mut g = GameState::city_scenario();
            g.fog_of_war = false;
            let red = 1;
            assert_eq!(g.cities[red].team, Team::Red);
            g.cities[red].population = population;
            g.auto_assign_city(red);
            g.cities[red].barracks = Some(Hex::new(2, 0));
            g.cities[red].built.push(Building::Barracks);
            g.stockpiles[Team::Red.index()] = Stock::whole(999, 999, 999);
            let full = g.supply_used(Team::Red) >= g.supply_cap(Team::Red);
            assert_eq!(full, !room, "population {population}");
            g.plan_ai_turn(Team::Red);
            let c = &g.cities[red];
            let queued = queues_supply(&c.queue, &c.barracks_queue);
            if room {
                assert!(queued > 0, "with room, it trains");
            } else {
                assert_eq!(queued, 0, "queued {:?}", c.queue);
                assert!(!c.queue.is_empty(), "it grows or gathers instead");
            }
            assert!(g.supply_used(Team::Red) <= g.supply_cap(Team::Red));
        }
    }
}
