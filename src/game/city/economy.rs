//! The stockpile economy (`docs/rts-economy.md`): each side's one store of
//! food, wood and metal, which every city's delivered goods fill and every
//! build is paid from when it is queued (and refunded to when it is taken
//! out); how a tile's production splits into wood and metal; feeding the
//! citizens; and how much work a queue does in a turn. Amounts are in
//! quarters, like the rest of the city code.
use std::ops::{Add, AddAssign, Sub, SubAssign};

use super::{Build, BuildUnit};
use crate::game::GameState;
use crate::game::hex::Hex;
use crate::game::map_icons::{FOOD_ICON, METAL_ICON, TIME_ICON, WOOD_ICON};
use crate::game::terrain::Special;
use crate::game::unit::Team;

/// Work a queue does in a turn: a build of `turns` turns needs
/// `turns * WORK_PER_TURN`. With production speeding builds (the Debug
/// panel's variant), a city's production adds to it (`work_rate`).
pub const WORK_PER_TURN: i32 = 4;
/// What every side starts with.
pub(in crate::game) const STARTING_STOCK: Stock = Stock::whole(10, 10, 4);
/// Food each citizen eats a turn.
pub(in crate::game) const FOOD_PER_CITIZEN: i32 = 8;
/// Growth costs `GROW_BASE + GROW_PER_CITIZEN * population` whole food.
const GROW_BASE: i32 = 5;
const GROW_PER_CITIZEN: i32 = 5;

/// Food, wood and metal, in quarters: a side's stockpile, a price or an
/// income.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Stock {
    pub food: i32,
    pub wood: i32,
    pub metal: i32,
}

impl Stock {
    /// From whole units.
    pub const fn whole(food: i32, wood: i32, metal: i32) -> Self {
        Self {
            food: food * 4,
            wood: wood * 4,
            metal: metal * 4,
        }
    }

    pub fn covers(self, price: Stock) -> bool {
        self.food >= price.food && self.wood >= price.wood && self.metal >= price.metal
    }

    /// How much more of each it takes to pay `price`.
    pub fn shortfall(self, price: Stock) -> Stock {
        Stock {
            food: (price.food - self.food).max(0),
            wood: (price.wood - self.wood).max(0),
            metal: (price.metal - self.metal).max(0),
        }
    }

    /// Wood and metal together: a city's production, for the variant where
    /// production speeds builds.
    pub fn production(self) -> i32 {
        self.wood + self.metal
    }

    /// Each resource with its name, in display order.
    pub fn parts(self) -> [(&'static str, i32); 3] {
        [
            ("FOOD", self.food),
            ("WOOD", self.wood),
            ("METAL", self.metal),
        ]
    }
}

impl Add for Stock {
    type Output = Stock;
    fn add(self, other: Stock) -> Stock {
        Stock {
            food: self.food + other.food,
            wood: self.wood + other.wood,
            metal: self.metal + other.metal,
        }
    }
}

impl Sub for Stock {
    type Output = Stock;
    fn sub(self, other: Stock) -> Stock {
        Stock {
            food: self.food - other.food,
            wood: self.wood - other.wood,
            metal: self.metal - other.metal,
        }
    }
}

impl AddAssign for Stock {
    fn add_assign(&mut self, other: Stock) {
        *self = *self + other;
    }
}

impl SubAssign for Stock {
    fn sub_assign(&mut self, other: Stock) {
        *self = *self - other;
    }
}

/// Growing a city of `population` by one citizen.
pub(in crate::game) fn grow_price(population: usize) -> Stock {
    Stock::whole(GROW_BASE + GROW_PER_CITIZEN * population as i32, 0, 0)
}

/// "3 WOOD AND 1 METAL": the nonzero parts of `stock`, in whole units.
pub(in crate::game) fn stock_words(stock: Stock) -> String {
    let parts: Vec<String> = stock
        .parts()
        .into_iter()
        .filter(|&(_, amount)| amount > 0)
        .map(|(name, amount)| format!("{} {name}", super::amount(amount)))
        .collect();
    match parts.len() {
        0 => "NOTHING".into(),
        1 => parts[0].clone(),
        n => format!("{} AND {}", parts[..n - 1].join(", "), parts[n - 1]),
    }
}

/// The icon character (`map_icons`) that stands in UI text for each of
/// `Stock::parts`' names.
pub(in crate::game) fn resource_icon(name: &str) -> char {
    match name {
        "FOOD" => FOOD_ICON,
        "WOOD" => WOOD_ICON,
        _ => METAL_ICON,
    }
}

/// The nonzero parts of `stock` for UI text, each its icon and whole amount:
/// "🌾2 🪵6" with the icon characters. Empty for nothing.
pub(in crate::game) fn stock_icons(stock: Stock) -> String {
    stock
        .parts()
        .into_iter()
        .filter(|&(_, amount)| amount > 0)
        .map(|(name, amount)| format!("{}{}", resource_icon(name), super::amount(amount)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A number of turns for UI text: the clock icon and the number.
pub(in crate::game) fn turns_icon(turns: i32) -> String {
    format!("{TIME_ICON}{turns}")
}

impl GameState {
    /// `team`'s stockpile.
    pub(in crate::game) fn stock(&self, team: Team) -> Stock {
        self.stockpiles[team.index()]
    }

    pub(in crate::game) fn stock_mut(&mut self, team: Team) -> &mut Stock {
        &mut self.stockpiles[team.index()]
    }

    /// How much of a worked tile's `production` is metal: what hills (1), a
    /// mine (2) and a Quarry (3) dig out of the ground. The rest is wood.
    pub(in crate::game) fn metal_yield(&self, hex: Hex, production: i32) -> i32 {
        let hills = i32::from(self.grid.tile(hex).hills);
        let mine = if self.sites.get(&hex).is_some_and(|s| s.label == "MINE") {
            2
        } else {
            0
        };
        let quarry = if self.grid.special(hex) == Some(Special::Quarry) {
            3
        } else {
            0
        };
        (hills + mine + quarry).min(production)
    }

    /// What queuing `build` at the end of `city`'s queue costs now: a Grow
    /// costs more for each Grow already queued ahead of it.
    pub(in crate::game) fn queue_price(&self, city: usize, build: Build) -> Stock {
        let c = &self.cities[city];
        match build {
            Build::Grow => {
                let queued = c.queue.iter().filter(|&&b| b == Build::Grow).count();
                grow_price(c.population + queued)
            }
            _ => build.price(),
        }
    }

    /// What taking the item at `index` out of `city`'s queue refunds: its
    /// price, or for a Grow the dearest Grow's, since the Grows behind it
    /// each become one citizen cheaper.
    fn refund(&self, city: usize, index: usize) -> Stock {
        let c = &self.cities[city];
        match c.queue[index] {
            Build::Grow => {
                let queued = c.queue.iter().filter(|&&b| b == Build::Grow).count();
                grow_price(c.population + queued - 1)
            }
            build => build.price(),
        }
    }

    /// Tests: a stockpile big enough for anything a test queues.
    #[cfg(test)]
    pub(in crate::game) fn fund(&mut self, team: Team) {
        *self.stock_mut(team) = Stock::whole(999, 999, 999);
    }

    /// Pays for `build` from the city's side and adds it to the end of the
    /// city's queue, or returns what the side is short.
    pub(in crate::game) fn try_queue_build(
        &mut self,
        city: usize,
        build: Build,
    ) -> Result<(), Stock> {
        let price = self.queue_price(city, build);
        let team = self.cities[city].team;
        if !self.stock(team).covers(price) {
            return Err(self.stock(team).shortfall(price));
        }
        *self.stock_mut(team) -= price;
        let c = &mut self.cities[city];
        // A city only keeps progress while it has something to work on.
        if c.queue.is_empty() {
            c.progress = 0;
        }
        c.queue.push(build);
        Ok(())
    }

    /// Takes the item at `index` out of `city`'s queue, refunding its price.
    /// Taking the head resets the queue's progress.
    pub(in crate::game) fn take_queue_item(&mut self, city: usize, index: usize) -> Build {
        let price = self.refund(city, index);
        let team = self.cities[city].team;
        *self.stock_mut(team) += price;
        let c = &mut self.cities[city];
        let removed = c.queue.remove(index);
        if index == 0 {
            c.progress = 0;
        }
        removed
    }

    /// Like `try_queue_build`, for the Barracks' queue.
    pub(in crate::game) fn try_queue_barracks(
        &mut self,
        city: usize,
        build: BuildUnit,
    ) -> Result<(), Stock> {
        let price = build.price();
        let team = self.cities[city].team;
        if !self.stock(team).covers(price) {
            return Err(self.stock(team).shortfall(price));
        }
        *self.stock_mut(team) -= price;
        let c = &mut self.cities[city];
        if c.barracks_queue.is_empty() {
            c.barracks_progress = 0;
        }
        c.barracks_queue.push(build);
        Ok(())
    }

    /// Like `take_queue_item`, for the Barracks' queue.
    pub(in crate::game) fn take_barracks_item(&mut self, city: usize, index: usize) -> BuildUnit {
        let team = self.cities[city].team;
        let c = &mut self.cities[city];
        let removed = c.barracks_queue.remove(index);
        if index == 0 {
            c.barracks_progress = 0;
        }
        *self.stock_mut(team) += removed.price();
        removed
    }

    /// Debug panel: switches between fixed build times and production
    /// speeding builds (`work_rate`).
    pub fn toggle_production_speedup(&mut self) {
        if self.refuses_debug() {
            return;
        }
        self.production_speedup = !self.production_speedup;
        self.notice = if self.production_speedup {
            "PRODUCTION NOW SPEEDS BUILDS".into()
        } else {
            "BUILDS NOW TAKE THEIR FIXED TIME".into()
        };
    }

    /// Work a queue does this turn: `WORK_PER_TURN`, plus, with production
    /// speeding builds, a quarter turn per whole point of `production` (in
    /// quarters).
    pub(in crate::game) fn work_rate(&self, production: i32) -> i32 {
        if self.production_speedup {
            WORK_PER_TURN + production.max(0) / 4
        } else {
            WORK_PER_TURN
        }
    }

    /// Turns until the head of `city`'s queue is done at this turn's rate,
    /// `None` with nothing queued.
    pub(in crate::game) fn turns_left(&self, city: usize) -> Option<i32> {
        let c = &self.cities[city];
        let build = *c.queue.first()?;
        let remaining = (self.city_build_work(city, build) - c.progress).max(0);
        let rate = self.work_rate(self.income(city).production());
        Some((remaining + rate - 1) / rate)
    }

    /// Like `turns_left`, for the Barracks' queue while the manager stands on
    /// it.
    pub(in crate::game) fn barracks_turns_left(&self, city: usize) -> Option<i32> {
        let c = &self.cities[city];
        let build = *c.barracks_queue.first()?;
        let remaining = (build.work() - c.barracks_progress).max(0);
        let rate = self.work_rate(self.barracks_income(city));
        Some((remaining + rate - 1) / rate)
    }

    /// Every side's citizens eat from its stockpile. A side that can't feed
    /// them all has its stockpile's food emptied, and its largest city
    /// (the first, on ties) loses a citizen, never going below one.
    pub(in crate::game) fn feed_citizens(&mut self) {
        for team in Team::ALL {
            let upkeep = self.upkeep(team);
            let stock = self.stock_mut(team);
            if stock.food >= upkeep {
                stock.food -= upkeep;
                continue;
            }
            stock.food = 0;
            let largest = self
                .cities
                .iter()
                .enumerate()
                .filter(|(_, c)| c.team == team && c.population > 1)
                .max_by_key(|&(i, c)| (c.population, std::cmp::Reverse(i)))
                .map(|(i, _)| i);
            if let Some(city) = largest {
                self.cities[city].population -= 1;
                log::info!("{team:?} city {} starves", self.cities[city].id + 1);
                if team == self.local_team {
                    self.notice = format!(
                        "CITY {} STARVES - NOT ENOUGH FOOD FOR EVERY CITIZEN",
                        self.cities[city].id + 1
                    );
                }
            }
        }
    }

    /// The food all of `team`'s citizens eat a turn.
    pub(in crate::game) fn upkeep(&self, team: Team) -> i32 {
        self.cities
            .iter()
            .filter(|c| c.team == team)
            .map(|c| c.population as i32 * FOOD_PER_CITIZEN)
            .sum()
    }

    /// `team`'s income a turn from all its cities, before upkeep.
    pub(in crate::game) fn side_income(&self, team: Team) -> Stock {
        (0..self.cities.len())
            .filter(|&i| self.cities[i].team == team)
            .map(|i| self.income(i))
            .fold(Stock::default(), |sum, income| sum + income)
    }
}
