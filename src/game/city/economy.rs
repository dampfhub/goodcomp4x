//! The stockpile economy (`docs/rts-economy.md`): each side's one store of
//! food, wood and metal, which every city's delivered goods fill and every
//! build is paid from when its city starts work on it (and refunded to, if
//! it was paid, when it is taken out); which item each queue works, paying
//! for it (`work_queues`), and what it will work as things stand
//! (`forecast`, `waiting_items`); how a tile's production splits into wood
//! and metal; feeding the citizens; and how much work a queue does in a
//! turn. Amounts are in quarters, like the rest of the city code.
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
/// Growth costs `GROW_BASE + GROW_PER_CITIZEN * population` whole food:
/// 1 to 7 citizens costs 270 food in all. Tempo tuning (#239): option B
/// doubled both from 5 (`docs/rts-economy.md`, Round 7).
const GROW_BASE: i32 = 10;
/// The part of a Grow's price per citizen the city has (`GROW_BASE`).
const GROW_PER_CITIZEN: i32 = 10;

/// An item in a city's or a Barracks' queue: the build, whether its side
/// has paid for it, and the work done on it, in quarter turns. An item is
/// queued unpaid, whatever the stockpile holds, and paid the first economy
/// its city works on it (`GameState::work_queues`); until then it has no
/// work. Its work stays with it when the queue is reordered.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Queued<B> {
    pub build: B,
    pub paid: bool,
    pub progress: i32,
}

impl<B> Queued<B> {
    /// Newly queued: unpaid, with no work done.
    pub fn new(build: B) -> Self {
        Self {
            build,
            paid: false,
            progress: 0,
        }
    }

    /// Tests: paid for, with `progress` work done.
    #[cfg(test)]
    pub fn worked(build: B, progress: i32) -> Self {
        Self {
            build,
            paid: true,
            progress,
        }
    }

    /// Marks it paid and adds `work`, up to the `needed` it takes.
    fn work_on(&mut self, work: i32, needed: i32) {
        self.paid = true;
        self.progress = (self.progress + work).min(needed);
    }

    /// Already paid for, with no work done: for scenario setup and tests,
    /// which skip the price.
    pub fn prepaid(build: B) -> Self {
        Self {
            paid: true,
            ..Self::new(build)
        }
    }
}

/// One of a city's two queues.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::game) enum Lane {
    City,
    Barracks,
}

/// What one queue will work this turn, as things stand
/// (`GameState::forecast`).
#[derive(Clone, Copy, Debug)]
pub(in crate::game) struct LaneForecast {
    pub city: usize,
    pub lane: Lane,
    /// The item it works: the first paid for, or the first the stockpile
    /// can pay for when this queue's turn comes. None if it can pay for
    /// none.
    pub worked: Option<usize>,
    /// What the stockpile holds when this queue's turn to pay comes.
    pub stock: Stock,
}

/// Every queue of a side, as `GameState::forecast` sees this turn going.
pub(in crate::game) struct QueueForecast {
    pub lanes: Vec<LaneForecast>,
    /// What the stockpile keeps once every queue has paid for what it
    /// starts: what the side can still spend this turn.
    pub spare: Stock,
}

impl QueueForecast {
    pub(in crate::game) fn lane(&self, city: usize, lane: Lane) -> Option<LaneForecast> {
        self.lanes
            .iter()
            .find(|l| l.city == city && l.lane == lane)
            .copied()
    }
}

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

    /// How many of `city`'s queued Grows are paid for.
    pub(in crate::game) fn paid_grows(&self, city: usize) -> usize {
        self.cities[city]
            .queue
            .iter()
            .filter(|q| q.paid && q.build == Build::Grow)
            .count()
    }

    /// What `build` queued at the end of `city`'s queue will cost when its
    /// turn comes: a Grow is priced when it's paid, at the population then,
    /// so each Grow queued ahead of it (done by then) adds a citizen.
    pub(in crate::game) fn queue_price(&self, city: usize, build: Build) -> Stock {
        let c = &self.cities[city];
        match build {
            Build::Grow => {
                let queued = c.queue.iter().filter(|q| q.build == Build::Grow).count();
                grow_price(c.population + queued)
            }
            _ => build.price(),
        }
    }

    /// What paying for unpaid item `index` of one of `city`'s queues costs
    /// now: its price, and for a Grow the population's price plus a citizen
    /// for each Grow already paid for (`paid_grows`), whose citizens are
    /// coming.
    pub(in crate::game) fn item_price(&self, city: usize, lane: Lane, index: usize) -> Stock {
        let c = &self.cities[city];
        match lane {
            Lane::City => match c.queue[index].build {
                Build::Grow => grow_price(c.population + self.paid_grows(city)),
                build => build.price(),
            },
            Lane::Barracks => c.barracks_queue[index].build.price(),
        }
    }

    /// How many items one of `city`'s queues holds.
    pub(in crate::game) fn lane_len(&self, city: usize, lane: Lane) -> usize {
        let c = &self.cities[city];
        match lane {
            Lane::City => c.queue.len(),
            Lane::Barracks => c.barracks_queue.len(),
        }
    }

    /// Item `index` of one of `city`'s queues: whether it's paid, the work
    /// done on it and the work it needs there.
    pub(in crate::game) fn lane_item(
        &self,
        city: usize,
        lane: Lane,
        index: usize,
    ) -> (bool, i32, i32) {
        let c = &self.cities[city];
        match lane {
            Lane::City => {
                let q = c.queue[index];
                (q.paid, q.progress, self.city_build_work(city, q.build))
            }
            Lane::Barracks => {
                let q = c.barracks_queue[index];
                (q.paid, q.progress, q.build.work())
            }
        }
    }

    /// What taking item `index` out of `city`'s queue refunds: nothing if
    /// it's unpaid; else its price, or for a Grow the dearest paid Grow's,
    /// since each paid Grow was priced a citizen above the one paid before.
    fn refund(&self, city: usize, index: usize) -> Stock {
        let c = &self.cities[city];
        let item = c.queue[index];
        match item.build {
            _ if !item.paid => Stock::default(),
            Build::Grow => grow_price(c.population + self.paid_grows(city) - 1),
            build => build.price(),
        }
    }

    /// Tests: a stockpile big enough for anything a test queues.
    #[cfg(test)]
    pub(in crate::game) fn fund(&mut self, team: Team) {
        *self.stock_mut(team) = Stock::whole(999, 999, 999);
    }

    /// Adds `build` to the end of `city`'s queue, unpaid, whatever the
    /// stockpile holds: it's paid when the city starts work on it
    /// (`work_queues`).
    pub(in crate::game) fn queue_build(&mut self, city: usize, build: Build) {
        self.cities[city].queue.push(Queued::new(build));
    }

    /// Takes the item at `index` out of `city`'s queue, refunding its price
    /// if it was paid (`refund`); its work goes with it.
    pub(in crate::game) fn take_queue_item(&mut self, city: usize, index: usize) -> Build {
        let price = self.refund(city, index);
        let team = self.cities[city].team;
        *self.stock_mut(team) += price;
        self.cities[city].queue.remove(index).build
    }

    /// Like `queue_build`, for the Barracks' queue.
    pub(in crate::game) fn queue_barracks(&mut self, city: usize, build: BuildUnit) {
        self.cities[city].barracks_queue.push(Queued::new(build));
    }

    /// Like `take_queue_item`, for the Barracks' queue.
    pub(in crate::game) fn take_barracks_item(&mut self, city: usize, index: usize) -> BuildUnit {
        let team = self.cities[city].team;
        let removed = self.cities[city].barracks_queue.remove(index);
        if removed.paid {
            *self.stock_mut(team) += removed.build.price();
        }
        removed.build
    }

    /// Which item of one of `city`'s queues it works with `stock` in the
    /// stockpile: the first that's paid for or that `stock` can pay for,
    /// with its price if it's still to be paid. Items before it wait.
    pub(in crate::game) fn pick_item(
        &self,
        city: usize,
        lane: Lane,
        stock: Stock,
    ) -> Option<(usize, Option<Stock>)> {
        (0..self.lane_len(city, lane)).find_map(|index| {
            if self.lane_item(city, lane, index).0 {
                return Some((index, None));
            }
            let price = self.item_price(city, lane, index);
            stock.covers(price).then_some((index, Some(price)))
        })
    }

    /// The turn's work on every queue, after income and upkeep. City by city
    /// in order, each city's queue before its Barracks', a queue works the
    /// item `pick_item` picks: pays for it if it's unpaid, and adds its
    /// city's work for the turn (`rates`, the city's and the Barracks'), up
    /// to what it needs. Items it can't pay for wait in place, unpaid.
    pub(in crate::game) fn work_queues(&mut self, rates: &[(i32, i32)]) {
        for (city, &(rate, barracks_rate)) in rates.iter().enumerate() {
            for (lane, rate) in [(Lane::City, rate), (Lane::Barracks, barracks_rate)] {
                let team = self.cities[city].team;
                if let Some((index, price)) = self.pick_item(city, lane, self.stock(team)) {
                    self.work_item(city, lane, index, price, rate);
                }
            }
        }
    }

    /// Pays `price` for item `index` of one of `city`'s queues, if it's due,
    /// and adds `work` to the item, up to what it needs.
    pub(in crate::game) fn work_item(
        &mut self,
        city: usize,
        lane: Lane,
        index: usize,
        price: Option<Stock>,
        work: i32,
    ) {
        let team = self.cities[city].team;
        if let Some(price) = price {
            *self.stock_mut(team) -= price;
        }
        let needed = self.lane_item(city, lane, index).2;
        let c = &mut self.cities[city];
        match lane {
            Lane::City => c.queue[index].work_on(work, needed),
            Lane::Barracks => c.barracks_queue[index].work_on(work, needed),
        }
    }

    /// What `team`'s stockpile will hold when this turn's economy reaches
    /// its queues, as things stand: this turn's income in and its citizens
    /// fed (a side that can't feed them all has no food left).
    pub(in crate::game) fn expected_stock(&self, team: Team) -> Stock {
        let mut stock = self.stock(team) + self.side_income(team);
        stock.food = (stock.food - self.upkeep(team)).max(0);
        stock
    }

    /// Which item each of `team`'s queues will work this turn, as things
    /// stand: `work_queues`' choices played out on `expected_stock`,
    /// changing nothing.
    pub(in crate::game) fn forecast(&self, team: Team) -> QueueForecast {
        let mut stock = self.expected_stock(team);
        let mut lanes = Vec::new();
        for city in (0..self.cities.len()).filter(|&i| self.cities[i].team == team) {
            for lane in [Lane::City, Lane::Barracks] {
                let pick = self.pick_item(city, lane, stock);
                lanes.push(LaneForecast {
                    city,
                    lane,
                    worked: pick.map(|(index, _)| index),
                    stock,
                });
                if let Some((_, Some(price))) = pick {
                    stock -= price;
                }
            }
        }
        QueueForecast {
            lanes,
            spare: stock,
        }
    }

    /// The items of a queue that wait for the stockpile, each with what the
    /// stockpile is short of for it: the unpaid ones ahead of the item it
    /// works, or all of them when it can pay for none.
    pub(in crate::game) fn waiting_items(&self, lane: LaneForecast) -> Vec<(usize, Stock)> {
        let end = lane
            .worked
            .unwrap_or_else(|| self.lane_len(lane.city, lane.lane));
        (0..end)
            .filter(|&index| !self.lane_item(lane.city, lane.lane, index).0)
            .map(|index| {
                let price = self.item_price(lane.city, lane.lane, index);
                (index, lane.stock.shortfall(price))
            })
            .collect()
    }

    /// What `city` waits for, if the first item of either of its queues
    /// waits for the stockpile (`waiting_items`): for each resource, the
    /// most either is short of it.
    pub(in crate::game) fn city_waits_for(
        &self,
        forecast: &QueueForecast,
        city: usize,
    ) -> Option<Stock> {
        [Lane::City, Lane::Barracks]
            .into_iter()
            .filter_map(|lane| forecast.lane(city, lane))
            .filter_map(|lane| {
                self.waiting_items(lane)
                    .first()
                    .filter(|&&(index, _)| index == 0)
                    .map(|&(_, short)| short)
            })
            .reduce(|a, b| Stock {
                food: a.food.max(b.food),
                wood: a.wood.max(b.wood),
                metal: a.metal.max(b.metal),
            })
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

    /// Turns until item `index` of one of `city`'s queues is done at this
    /// turn's rate, from the work it has.
    pub(in crate::game) fn item_turns_left(&self, city: usize, lane: Lane, index: usize) -> i32 {
        let (_, progress, work) = self.lane_item(city, lane, index);
        let rate = match lane {
            Lane::City => self.work_rate(self.income(city).production()),
            Lane::Barracks => self.work_rate(self.barracks_income(city)),
        };
        ((work - progress).max(0) + rate - 1) / rate
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
