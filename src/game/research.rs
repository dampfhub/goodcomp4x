//! Research (#392, `docs/game-rules.md`, Research): the tech tree, each
//! side's researched techs and its research queue, and what a tech locks
//! until it's researched.
//!
//! **The tree is `TECHS`**, the one table: rewrite it to change the tree. A
//! tech has an `id` that other techs' `requires` name it by, its name (an
//! entry in `text/research.ini`), its price in goods, the turns it takes
//! and what it unlocks: units and buildings. Anything no tech unlocks is
//! there from the start (`FREE` lists what that is today, for the docs and
//! a test). The tests below check the table: every id is unique, every
//! `requires` names an earlier tech, and nothing is unlocked twice.
//!
//! **Researching** is side-wide, apart from the cities' queues: one queue
//! per side (`Research::queue`), whose first tech alone is worked. It is
//! paid like a build (`Queued`): queued unpaid, paid when the economy starts
//! it, refunded if taken off once paid, and it waits in place while the
//! stockpile is short. The economy pays for it first, before any city's
//! queue (`work_queues`), then adds `WORK_PER_TURN` a turn until it's done
//! (`complete_research`, with the builds). A tech queues only once its
//! prerequisites are researched or queued ahead of it; queuing one the
//! player can't yet queues its missing prerequisites first.
//!
//! **Later** (#393) some techs may also cost a resource of their own, made
//! by science buildings. Its place is next to `price` in `TechDef` (and in
//! `Tech::price`'s users: paying, refunding, the plan's books in
//! `multiplayer.rs`), which this file keeps to one function each.
use std::collections::BTreeSet;

use super::GameState;
use super::city::{Build, BuildUnit, Building, Queued, Stock, WORK_PER_TURN};
use super::strings::text;
use super::unit::Team;

/// What a tech unlocks: a troop or ship, or a building.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::game) enum Unlock {
    Unit(BuildUnit),
    Building(Building),
}

impl Unlock {
    pub(in crate::game) fn name(self) -> &'static str {
        match self {
            Self::Unit(unit) => unit.name(),
            Self::Building(building) => building.name(),
        }
    }

    /// What `build` needs unlocked, if anything: a troop or ship. The
    /// Worker, Settler, Scout, Grow and Gather are always there.
    pub(in crate::game) fn of_build(build: Build) -> Option<Self> {
        match build {
            Build::Unit(unit) => Some(Self::Unit(unit)),
            Build::Worker | Build::Grow | Build::Gather | Build::Settler | Build::Scout => None,
        }
    }
}

/// One tech of the tree (`TECHS`).
pub(in crate::game) struct TechDef {
    /// What `requires` names it by. Not shown: its name is `name`.
    pub id: &'static str,
    /// Its name, an entry in `text/research.ini`.
    pub name: fn() -> &'static str,
    /// What it takes from the stockpile when its research starts.
    pub price: Stock,
    /// Turns of research it takes once paid.
    pub turns: i32,
    /// The techs researched before it can be, by `id`.
    pub requires: &'static [&'static str],
    pub unlocks: &'static [Unlock],
}

use Unlock::{Building as B, Unit as U};

/// The tech tree. Tier I needs nothing; each later tier needs a tech of the
/// one before. The names are placeholders, and so are the prices: food and
/// wood, small, and metal only late, as metal is what an army runs short of
/// and food is what a city grows on (`docs/rts-economy.md`).
pub(in crate::game) static TECHS: &[TechDef] = &[
    TechDef {
        id: "I-A",
        name: || text!("tech_i_a"),
        price: Stock::whole(3, 3, 0),
        turns: 3,
        requires: &[],
        unlocks: &[U(BuildUnit::Ranged), B(Building::Watchpost)],
    },
    TechDef {
        id: "I-B",
        name: || text!("tech_i_b"),
        price: Stock::whole(4, 2, 0),
        turns: 3,
        requires: &[],
        unlocks: &[U(BuildUnit::Pikeman), B(Building::Mill)],
    },
    TechDef {
        id: "I-C",
        name: || text!("tech_i_c"),
        price: Stock::whole(3, 3, 0),
        turns: 3,
        requires: &[],
        unlocks: &[B(Building::WorkCamp), B(Building::CanoeHouse)],
    },
    TechDef {
        id: "II-A",
        name: || text!("tech_ii_a"),
        price: Stock::whole(6, 4, 0),
        turns: 4,
        requires: &["I-A"],
        unlocks: &[U(BuildUnit::Cavalry), B(Building::Stable)],
    },
    TechDef {
        id: "II-B",
        name: || text!("tech_ii_b"),
        price: Stock::whole(4, 6, 0),
        turns: 4,
        requires: &["I-A"],
        unlocks: &[
            U(BuildUnit::Siege),
            B(Building::Workshop),
            B(Building::Smelter),
        ],
    },
    TechDef {
        id: "II-C",
        name: || text!("tech_ii_c"),
        price: Stock::whole(6, 3, 0),
        turns: 4,
        requires: &["I-B"],
        unlocks: &[B(Building::Cannery), B(Building::FieldHospital)],
    },
    TechDef {
        id: "II-D",
        name: || text!("tech_ii_d"),
        price: Stock::whole(4, 6, 0),
        turns: 4,
        requires: &["I-C"],
        unlocks: &[U(BuildUnit::PatrolGalley), B(Building::Harbor)],
    },
    TechDef {
        id: "III-A",
        name: || text!("tech_iii_a"),
        price: Stock::whole(8, 6, 2),
        turns: 5,
        requires: &["II-A"],
        unlocks: &[U(BuildUnit::Armored), B(Building::Forge)],
    },
    TechDef {
        id: "III-B",
        name: || text!("tech_iii_b"),
        price: Stock::whole(6, 8, 2),
        turns: 5,
        requires: &["II-D"],
        unlocks: &[
            U(BuildUnit::LandingCraft),
            U(BuildUnit::BombardShip),
            B(Building::CoastalBattery),
        ],
    },
    TechDef {
        id: "III-C",
        name: || text!("tech_iii_c"),
        price: Stock::whole(8, 8, 4),
        turns: 5,
        requires: &["II-B", "II-C"],
        unlocks: &[B(Building::Railhead)],
    },
];

/// What every side has from the start, which no tech unlocks: the city,
/// and its Worker, Settler, Scout, Grow and Gather (never locked, `of_build`),
/// the Melee and the Barracks. For the docs; a test checks it against
/// `TECHS`.
#[cfg(test)]
const FREE: [Unlock; 2] = [U(BuildUnit::Melee), B(Building::Barracks)];

/// A tech: its place in `TECHS`. One from the network may be out of range
/// until `check_plan` has checked it (`is_valid`).
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, serde::Serialize, serde::Deserialize,
)]
pub struct Tech(pub(in crate::game) u8);

impl Tech {
    /// Every tech, in `TECHS` order.
    pub(in crate::game) fn all() -> impl Iterator<Item = Tech> {
        (0..TECHS.len()).map(|i| Tech(i as u8))
    }

    /// Whether it's one of `TECHS`.
    pub(in crate::game) fn is_valid(self) -> bool {
        usize::from(self.0) < TECHS.len()
    }

    fn def(self) -> &'static TechDef {
        &TECHS[usize::from(self.0)]
    }

    /// The tech `TECHS` gives `id`.
    fn by_id(id: &str) -> Option<Tech> {
        Tech::all().find(|t| t.def().id == id)
    }

    pub(in crate::game) fn name(self) -> &'static str {
        (self.def().name)()
    }

    pub(in crate::game) fn price(self) -> Stock {
        self.def().price
    }

    pub(in crate::game) fn turns(self) -> i32 {
        self.def().turns
    }

    /// Work it needs, in quarter turns.
    pub(in crate::game) fn work(self) -> i32 {
        self.turns() * WORK_PER_TURN
    }

    /// The techs it needs researched first.
    pub(in crate::game) fn requires(self) -> impl Iterator<Item = Tech> {
        self.def()
            .requires
            .iter()
            .map(|id| Tech::by_id(id).expect("TECHS names only its own techs"))
    }

    pub(in crate::game) fn unlocks(self) -> &'static [Unlock] {
        self.def().unlocks
    }

    /// "RANGED, WATCHPOST": what it unlocks.
    pub(in crate::game) fn unlocks_text(self) -> String {
        names(self.unlocks().iter().map(|u| u.name()))
    }

    /// The tech that unlocks `unlock`, if one does.
    pub(in crate::game) fn unlocking(unlock: Unlock) -> Option<Tech> {
        Tech::all().find(|t| t.unlocks().contains(&unlock))
    }
}

/// Names joined for a line of text: "I-A, II-B".
pub(in crate::game) fn names<'a>(names: impl IntoIterator<Item = &'a str>) -> String {
    names.into_iter().collect::<Vec<_>>().join(", ")
}

/// A side's research: the techs it has, and its queue, whose first tech
/// alone is worked.
#[derive(Clone, Default, Debug, PartialEq)]
pub(in crate::game) struct Research {
    pub done: BTreeSet<Tech>,
    pub queue: Vec<Queued<Tech>>,
}

/// What a side's research does this turn, as things stand
/// (`GameState::research_forecast`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::game) enum ResearchForecast {
    /// Nothing queued.
    Idle,
    /// Its first tech is worked: paid already, or paid this turn at `price`.
    Works { tech: Tech, price: Option<Stock> },
    /// Its first tech waits, the stockpile short of this for it.
    Waits { tech: Tech, short: Stock },
}

/// Why a queue of techs can't be a side's, given what it has researched:
/// a tech twice, one it has, one that isn't a tech, or one queued ahead of
/// a prerequisite it hasn't researched.
pub(in crate::game) fn queue_issue(
    done: &BTreeSet<Tech>,
    queue: &[Queued<Tech>],
) -> Option<String> {
    let mut ahead = BTreeSet::new();
    for item in queue {
        let tech = item.build;
        if !tech.is_valid() {
            return Some("NOT A TECH".into());
        }
        if done.contains(&tech) || !ahead.insert(tech) {
            return Some(format!("{} TWICE", tech.name()));
        }
        if let Some(missing) = tech
            .requires()
            .find(|r| !done.contains(r) && !ahead.contains(r))
        {
            return Some(text!(
                "research_needs_first",
                tech = tech.name(),
                needs = missing.name()
            ));
        }
    }
    None
}

impl GameState {
    pub(in crate::game) fn research(&self, team: Team) -> &Research {
        &self.research[team.index()]
    }

    pub(in crate::game) fn researched(&self, team: Team, tech: Tech) -> bool {
        self.research(team).done.contains(&tech)
    }

    /// Whether `team` has `unlock`: no tech unlocks it, or it has that tech.
    pub(in crate::game) fn unlocked(&self, team: Team, unlock: Unlock) -> bool {
        Tech::unlocking(unlock).is_none_or(|tech| self.researched(team, tech))
    }

    /// Why `team` can't have `unlock` yet: the tech it needs ("NEEDS TECH
    /// I-A"), for a card's hint and tooltip.
    pub(in crate::game) fn tech_lock(&self, team: Team, unlock: Unlock) -> Option<String> {
        let tech = Tech::unlocking(unlock).filter(|&t| !self.researched(team, t))?;
        Some(text!("research_lock", tech = tech.name()))
    }

    /// `tech_lock` for a city build: a troop or ship.
    pub(in crate::game) fn build_tech_lock(&self, team: Team, build: Build) -> Option<String> {
        Unlock::of_build(build).and_then(|unlock| self.tech_lock(team, unlock))
    }

    /// Whether `team` has `build` (`build_tech_lock`).
    pub(in crate::game) fn build_unlocked(&self, team: Team, build: Build) -> bool {
        Unlock::of_build(build).is_none_or(|unlock| self.unlocked(team, unlock))
    }

    /// Tests and scenario setup: `team` has every tech.
    pub(in crate::game) fn research_all(&mut self, team: Team) {
        let research = &mut self.research[team.index()];
        research.done = Tech::all().collect();
        research.queue.clear();
    }

    /// Where `tech` stands in `team`'s queue, if it's there.
    pub(in crate::game) fn research_place(&self, team: Team, tech: Tech) -> Option<usize> {
        self.research(team)
            .queue
            .iter()
            .position(|q| q.build == tech)
    }

    /// What queuing `tech` for `team` adds to its queue, in order: the
    /// prerequisites it hasn't researched or queued, then the tech. Empty if
    /// it's researched or queued already.
    pub(in crate::game) fn research_path(&self, team: Team, tech: Tech) -> Vec<Tech> {
        let mut path = Vec::new();
        self.add_to_path(team, tech, &mut path);
        path
    }

    fn add_to_path(&self, team: Team, tech: Tech, path: &mut Vec<Tech>) {
        if self.researched(team, tech)
            || self.research_place(team, tech).is_some()
            || path.contains(&tech)
        {
            return;
        }
        for required in tech.requires() {
            self.add_to_path(team, required, path);
        }
        path.push(tech);
    }

    /// Adds `tech` to `team`'s research queue, unpaid, with what it needs
    /// first (`research_path`), and says what it queued.
    pub(in crate::game) fn queue_research(&mut self, team: Team, tech: Tech) -> Vec<Tech> {
        let path = self.research_path(team, tech);
        self.research[team.index()]
            .queue
            .extend(path.iter().map(|&t| Queued::new(t)));
        path
    }

    /// The research view's tech card, or the AI: queues `tech` for the
    /// player's side, and its missing prerequisites first.
    pub fn queue_selected_research(&mut self, tech: Tech) {
        if self.is_playing_out() || !tech.is_valid() {
            return;
        }
        let team = self.local_team;
        if self.researched(team, tech) {
            self.notice = text!("research_had_already", tech = tech.name());
            return;
        }
        if self.research_place(team, tech).is_some() {
            self.notice = text!("research_queued_already", tech = tech.name());
            return;
        }
        let path = self.queue_research(team, tech);
        let cost = [
            super::city::stock_icons(tech.price()),
            super::city::turns_icon(tech.turns()),
        ]
        .join(" ");
        self.notice = text!(
            "research_queued",
            techs = names(path.iter().map(|t| t.name())),
            cost = cost
        );
    }

    /// Takes item `index` off `team`'s research queue, refunded if it was
    /// paid, and with it every tech after it that needs it. Returns what it
    /// took off and whether any of it was paid.
    fn take_research(&mut self, team: Team, index: usize) -> (Vec<Tech>, bool) {
        let mut taken = Vec::new();
        let mut paid = false;
        let research = &mut self.research[team.index()];
        if index >= research.queue.len() {
            return (taken, paid);
        }
        let first = research.queue.remove(index);
        taken.push(first.build);
        let mut refund = Stock::default();
        if first.paid {
            refund += first.build.price();
            paid = true;
        }
        // Whatever needs a tech taken off goes too, as nothing else can
        // research it.
        let mut i = index;
        while i < research.queue.len() {
            let item = research.queue[i];
            if item.build.requires().any(|r| taken.contains(&r)) {
                research.queue.remove(i);
                taken.push(item.build);
                if item.paid {
                    refund += item.build.price();
                    paid = true;
                }
            } else {
                i += 1;
            }
        }
        *self.stock_mut(team) += refund;
        (taken, paid)
    }

    /// A research queue row's X: takes the tech off, and what needs it.
    pub fn remove_selected_research(&mut self, index: usize) {
        if self.is_playing_out() {
            return;
        }
        let (taken, paid) = self.take_research(self.local_team, index);
        if taken.is_empty() {
            return;
        }
        self.notice = text!(
            "research_removed",
            techs = names(taken.iter().map(|t| t.name())),
            refund = super::city::refund_word(paid)
        );
    }

    /// The research queue's Clear button: every tech off, refunded if paid.
    pub fn clear_selected_research(&mut self) {
        if self.is_playing_out() {
            return;
        }
        let team = self.local_team;
        let count = self.research(team).queue.len();
        if count == 0 {
            return;
        }
        let paid = self.research(team).queue.iter().filter(|q| q.paid).count();
        while let Some(last) = self.research(team).queue.len().checked_sub(1) {
            self.take_research(team, last);
        }
        self.research_queue_scroll = 0;
        self.notice = text!(
            "research_cleared",
            refund = super::city::cleared_words(count, paid)
        );
    }

    /// Dragging a research queue row: moves it, unless that would put a
    /// tech ahead of one it needs.
    pub(in crate::game) fn reorder_research(&mut self, source: usize, target: usize) {
        if self.is_playing_out() {
            return;
        }
        let team = self.local_team;
        let mut queue = self.research(team).queue.clone();
        if source >= queue.len() || target >= queue.len() {
            return;
        }
        let item = queue.remove(source);
        queue.insert(target, item);
        if let Some(why) = queue_issue(&self.research(team).done, &queue) {
            self.notice = why;
            return;
        }
        self.research[team.index()].queue = queue;
        self.notice = text!("research_reordered").into();
    }

    /// What `team`'s research does this turn with `stock` in the stockpile:
    /// the first tech in its queue is worked if it's paid or `stock` pays
    /// for it, and else waits.
    pub(in crate::game) fn research_with(&self, team: Team, stock: Stock) -> ResearchForecast {
        let Some(head) = self.research(team).queue.first() else {
            return ResearchForecast::Idle;
        };
        let tech = head.build;
        if head.paid {
            return ResearchForecast::Works { tech, price: None };
        }
        let price = tech.price();
        if stock.covers(price) {
            ResearchForecast::Works {
                tech,
                price: Some(price),
            }
        } else {
            ResearchForecast::Waits {
                tech,
                short: stock.shortfall(price),
            }
        }
    }

    /// `research_with` on what the stockpile will hold when this turn's
    /// economy reaches the research (`expected_stock`): for the panels and
    /// the AI.
    pub(in crate::game) fn research_forecast(&self, team: Team) -> ResearchForecast {
        self.research_with(team, self.expected_stock(team))
    }

    /// The turn's research, first in the economy, before any city's queue:
    /// each side's first tech is paid for if it's unpaid and the stockpile
    /// can, and gets `WORK_PER_TURN`, up to what it needs.
    pub(in crate::game) fn work_research(&mut self) {
        for team in Team::ALL {
            let stock = self.stock(team);
            let ResearchForecast::Works { tech, price } = self.research_with(team, stock) else {
                continue;
            };
            if let Some(price) = price {
                *self.stock_mut(team) -= price;
            }
            let head = &mut self.research[team.index()].queue[0];
            head.paid = true;
            head.progress = (head.progress + WORK_PER_TURN).min(tech.work());
        }
    }

    /// Each side's first tech, once its research is done, is researched:
    /// off the queue, and what it unlocks can be built.
    pub(in crate::game) fn complete_research(&mut self) {
        for team in Team::ALL {
            let research = &mut self.research[team.index()];
            let Some(&head) = research.queue.first() else {
                continue;
            };
            if !head.paid || head.progress < head.build.work() {
                continue;
            }
            research.queue.remove(0);
            research.done.insert(head.build);
            log::info!("{team:?} researched {}", head.build.name());
            if team == self.local_team {
                self.notice = text!(
                    "research_done",
                    tech = head.build.name(),
                    unlocks = head.build.unlocks_text()
                );
            }
        }
    }

    /// F9 in the research view: researches the player's first tech now,
    /// paid for if it wasn't and the stockpile can.
    pub(in crate::game) fn debug_complete_research(&mut self) {
        let team = self.local_team;
        let ResearchForecast::Works { tech, price } = self.research_with(team, self.stock(team))
        else {
            self.notice = text!("research_debug_nothing").into();
            return;
        };
        if let Some(price) = price {
            *self.stock_mut(team) -= price;
        }
        let head = &mut self.research[team.index()].queue[0];
        head.paid = true;
        head.progress = tech.work();
        self.complete_research();
    }

    /// Turns until `team`'s first tech is researched, from the work it
    /// has, once it's worked.
    pub(in crate::game) fn research_turns_left(&self, team: Team) -> Option<i32> {
        let head = self.research(team).queue.first()?;
        let left = (head.build.work() - head.progress).max(0);
        Some((left + WORK_PER_TURN - 1) / WORK_PER_TURN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tech(id: &str) -> Tech {
        Tech::by_id(id).unwrap()
    }

    #[test]
    fn the_tree_names_each_tech_once_and_only_earlier_ones_as_prerequisites() {
        for (i, def) in TECHS.iter().enumerate() {
            assert!(
                TECHS[..i].iter().all(|other| other.id != def.id),
                "{} twice",
                def.id
            );
            for required in def.requires {
                let at = TECHS.iter().position(|t| t.id == *required);
                assert!(
                    at.is_some_and(|at| at < i),
                    "{} needs {required}, which isn't a tech before it",
                    def.id
                );
            }
            assert!(def.turns > 0 && def.price != Stock::default(), "{}", def.id);
        }
        assert!(TECHS.len() <= usize::from(u8::MAX));
    }

    #[test]
    fn every_unit_and_building_is_free_or_unlocked_by_one_tech() {
        let all = BuildUnit::ALL
            .map(Unlock::Unit)
            .into_iter()
            .chain(Building::ALL.map(Unlock::Building));
        for unlock in all {
            let techs = Tech::all()
                .filter(|t| t.unlocks().contains(&unlock))
                .count();
            let free = FREE.contains(&unlock);
            assert_eq!(
                techs,
                usize::from(!free),
                "{unlock:?}: free {free}, unlocked by {techs} techs"
            );
        }
    }

    #[test]
    fn queuing_a_tech_queues_what_it_needs_first() {
        let mut g = GameState::city_scenario();
        g.queue_selected_research(tech("III-C"));
        let queued: Vec<&str> = g
            .research(Team::Blue)
            .queue
            .iter()
            .map(|q| q.build.def().id)
            .collect();
        assert_eq!(queued, ["I-A", "II-B", "I-B", "II-C", "III-C"]);
        assert!(queue_issue(&BTreeSet::new(), &g.research(Team::Blue).queue).is_none());
        // Again: it's queued already.
        g.queue_selected_research(tech("III-C"));
        assert_eq!(g.research(Team::Blue).queue.len(), 5);
    }

    #[test]
    fn taking_a_tech_off_takes_what_needs_it_and_refunds_what_was_paid() {
        let mut g = GameState::city_scenario();
        g.queue_selected_research(tech("II-A"));
        g.queue_selected_research(tech("I-C"));
        g.research[0].queue[0] = Queued::worked(tech("I-A"), 4);
        let before = g.stock(Team::Blue);
        g.remove_selected_research(0);
        let left: Vec<Tech> = g
            .research(Team::Blue)
            .queue
            .iter()
            .map(|q| q.build)
            .collect();
        assert_eq!(left, [tech("I-C")]);
        assert_eq!(g.stock(Team::Blue), before + tech("I-A").price());
    }

    #[test]
    fn a_tech_never_moves_ahead_of_one_it_needs() {
        let mut g = GameState::city_scenario();
        g.queue_selected_research(tech("II-A"));
        g.queue_selected_research(tech("I-C"));
        g.reorder_research(1, 0);
        let order: Vec<Tech> = g
            .research(Team::Blue)
            .queue
            .iter()
            .map(|q| q.build)
            .collect();
        assert_eq!(order, [tech("I-A"), tech("II-A"), tech("I-C")]);
        g.reorder_research(2, 0);
        let order: Vec<Tech> = g
            .research(Team::Blue)
            .queue
            .iter()
            .map(|q| q.build)
            .collect();
        assert_eq!(order, [tech("I-C"), tech("I-A"), tech("II-A")]);
    }

    #[test]
    fn research_is_paid_first_and_takes_its_turns() {
        let mut g = GameState::city_scenario();
        let ranged = Unlock::Unit(BuildUnit::Ranged);
        assert!(!g.unlocked(Team::Blue, ranged));
        assert_eq!(
            g.tech_lock(Team::Blue, ranged),
            Some(text!("research_lock", tech = tech("I-A").name()))
        );
        g.queue_selected_research(tech("I-A"));
        *g.stock_mut(Team::Blue) = tech("I-A").price();
        // A city item that the same stockpile would pay for waits: research
        // pays first.
        g.cities[0].queue.clear();
        g.queue_build(0, Build::Worker);
        g.cities[0].barracks_queue.clear();
        let upkeep = g.upkeep(Team::Blue);
        let income = g.side_income(Team::Blue);
        *g.stock_mut(Team::Blue) -= income;
        g.stock_mut(Team::Blue).food += upkeep;
        g.resolve_economy();
        assert!(g.research(Team::Blue).queue[0].paid);
        assert!(
            !g.cities[0].queue[0].paid,
            "the Worker waits for the stockpile"
        );
        for _ in 1..tech("I-A").turns() {
            assert!(!g.researched(Team::Blue, tech("I-A")));
            g.resolve_economy();
        }
        assert!(g.researched(Team::Blue, tech("I-A")));
        assert!(g.unlocked(Team::Blue, ranged));
        assert!(g.research(Team::Blue).queue.is_empty());
    }

    #[test]
    fn a_tech_waits_while_the_stockpile_is_short() {
        let mut g = GameState::city_scenario();
        g.queue_selected_research(tech("III-A"));
        *g.stock_mut(Team::Blue) = Stock::default();
        assert!(matches!(
            g.research_with(Team::Blue, Stock::default()),
            ResearchForecast::Waits { .. }
        ));
        g.work_research();
        assert!(!g.research(Team::Blue).queue[0].paid);
        assert_eq!(g.research(Team::Blue).queue[0].progress, 0);
    }

    #[test]
    fn locked_units_and_buildings_cannot_be_queued_or_placed() {
        let mut g = GameState::city_scenario();
        g.fund(Team::Blue);
        g.research[0] = Research::default();
        g.selected_city = Some(0);
        g.queue_selected_city_unit(BuildUnit::Ranged);
        assert!(
            !g.cities[0]
                .queue
                .iter()
                .any(|q| q.build == Build::Unit(BuildUnit::Ranged))
        );
        assert!(
            g.city_build_issue(0, Build::Unit(BuildUnit::Ranged))
                .is_some()
        );
        assert!(
            g.city_build_issue(0, Build::Unit(BuildUnit::Melee))
                .is_none()
        );
        let mill = crate::game::JobKind::Build(Building::Mill);
        assert!(g.job_kind_unavailable(0, mill).is_some());
        g.research_all(Team::Blue);
        assert!(
            g.city_build_issue(0, Build::Unit(BuildUnit::Ranged))
                .is_none()
        );
    }
}
