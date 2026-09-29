//! Citizens: the priority order, tile assignment (the manager and its
//! workers), growth, and the end-of-turn economy tick.

use super::{
    Building, FOOD_PER_CITIZEN, Good, MAX_CITY_POPULATION, Priorities, Routes, delivered_share,
};
use crate::game::GameState;
use crate::game::fast_hash::HashSet;
use crate::game::hex::Hex;

/// Food a city's tiles bring in past its citizens' upkeep before it's fed
/// and follows its priority order (the food floor): one whole food.
const FOOD_FLOOR_MARGIN: i32 = 4;
/// The city center's own food, which counts toward the food floor.
const CENTER_FOOD: i32 = 8;

impl GameState {
    /// The open city's priority order changes (a chip dragged, or clicked
    /// to put first): its citizens are reassigned by it.
    pub fn set_selected_city_priorities(&mut self, priorities: Priorities) {
        if self.is_resolving() || !priorities.is_order() {
            return;
        }
        if let Some(city) = self.selected_city {
            self.cities[city].priorities = priorities;
            self.auto_assign_city(city);
            self.notice = format!("CITY PRIORITIES: {}", priorities.text());
        }
    }

    /// A priority chip clicked: its good goes first in the open city's
    /// order.
    pub fn prioritize_selected_city(&mut self, good: Good) {
        if let Some(city) = self.selected_city {
            let priorities = self.cities[city].priorities.with_first(good);
            self.set_selected_city_priorities(priorities);
        }
    }

    /// A tile's food, wood and metal as delivered to `city`, along a route
    /// of `cost`.
    pub(super) fn delivered_goods(&self, city: usize, hex: Hex, cost: i32) -> (i32, i32, i32) {
        let (food, wood, metal) = self.tile_goods(hex);
        let share = delivered_share(cost);
        (
            food * self.mill_food_share(city, hex, cost),
            wood * share,
            metal * share,
        )
    }

    /// Whether `food` (the center's and its worked tiles', delivered)
    /// feeds `city`: its citizens' upkeep and one more (the food floor).
    pub(super) fn is_fed(&self, city: usize, food: i32) -> bool {
        food >= self.cities[city].population as i32 * FOOD_PER_CITIZEN + FOOD_FLOOR_MARGIN
    }

    /// The food `city`'s center and worked tiles deliver, as auto-assign
    /// counts it for the food floor.
    fn worked_food(&self, city: usize, routes: &Routes) -> i32 {
        CENTER_FOOD
            + self.cities[city]
                .worked
                .iter()
                .filter_map(|h| routes.costs.get(h).map(|&cost| (h, cost)))
                .map(|(&h, cost)| self.delivered_goods(city, h, cost).0)
                .sum::<i32>()
    }

    /// Whether a city center or a placed building (any side's) stands on
    /// `hex`. No citizen works such a tile: a city center yields on its own,
    /// and a building covers the ground.
    pub(in crate::game) fn closed_to_citizens(&self, hex: Hex) -> bool {
        self.cities.iter().any(|c| {
            c.pos == hex
                || Building::PLACEABLE
                    .iter()
                    .any(|&building| c.placed_site(building) == Some(hex))
        })
    }

    pub(super) fn may_assign(&self, city: usize, hex: Hex) -> bool {
        let manager_can_reach = self.cities[city]
            .worked
            .first()
            .is_none_or(|manager| manager.distance(hex) == 1);
        manager_can_reach
            && !self.closed_to_citizens(hex)
            && !self.cities.iter().any(|c| c.worked.contains(&hex))
            && self
                .sites
                .get(&hex)
                .is_none_or(|s| s.team == self.cities[city].team)
    }

    pub(super) fn may_be_manager(&self, city: usize, hex: Hex) -> bool {
        !self.closed_to_citizens(hex)
            && !self
                .cities
                .iter()
                .enumerate()
                .any(|(i, c)| i != city && c.worked.contains(&hex))
            && self
                .sites
                .get(&hex)
                .is_none_or(|site| site.team == self.cities[city].team)
            && !self.grid.terrain(hex).is_water()
            && self.routes(city).costs.contains_key(&hex)
    }

    /// Whether `hex` can take the city's next citizen as far as the manager
    /// goes: the first citizen, the manager, has to be on land.
    pub(super) fn may_manage_or_work(&self, city: usize, hex: Hex) -> bool {
        !self.cities[city].worked.is_empty() || !self.grid.terrain(hex).is_water()
    }

    /// Relocates the manager and carries each worker's axial offset with it.
    /// Workers whose matching tile is unavailable are filled by normal auto-assignment.
    pub(super) fn move_manager(&mut self, city: usize, new_manager: Hex) {
        let old = self.cities[city].worked.clone();
        let old_manager = old[0];
        let routes = self.routes(city);
        let other_claims: HashSet<Hex> = self
            .cities
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != city)
            .flat_map(|(_, c)| c.worked.iter().copied())
            .collect();
        let mut relocated = vec![new_manager];
        for worker in old.into_iter().skip(1) {
            let target = Hex::new(
                new_manager.q + worker.q - old_manager.q,
                new_manager.r + worker.r - old_manager.r,
            );
            let legal = routes.costs.contains_key(&target)
                && !other_claims.contains(&target)
                && !self.closed_to_citizens(target)
                && self
                    .sites
                    .get(&target)
                    .is_none_or(|site| site.team == self.cities[city].team);
            if legal && !relocated.contains(&target) {
                relocated.push(target);
            }
        }
        self.cities[city].worked = relocated;
        self.cities[city].remembered_worked = self.cities[city].worked.clone();
        self.reconcile_citizens(city);
        self.notice = "MANAGER MOVED - WORKERS FOLLOWED WHERE POSSIBLE".into();
    }

    /// Assigns the city's citizens afresh, one at a time, each to the free
    /// tile worth the most by its priority order (`Priorities::score`,
    /// food first until the city is fed); ties go by hex coordinates.
    pub(in crate::game) fn auto_assign_city(&mut self, city: usize) {
        self.cities[city].worked.clear();
        let routes = self.routes(city);
        // Each tile's food, wood and metal as delivered to the city.
        let mut tiles: Vec<_> = routes
            .costs
            .iter()
            .filter(|(h, _)| self.may_assign(city, **h))
            .map(|(&h, &cost)| (h, self.delivered_goods(city, h, cost)))
            .collect();
        let mut food = CENTER_FOOD;
        while self.cities[city].worked.len() < self.cities[city].population.min(MAX_CITY_POPULATION)
            && !tiles.is_empty()
        {
            if let Some(manager) = self.cities[city].worked.first().copied() {
                tiles.retain(|(hex, _)| manager.distance(*hex) == 1);
            }
            let Some(pick) = self.best_tile(city, &tiles, food) else {
                break;
            };
            let (hex, (f, _, _)) = tiles.remove(pick);
            self.cities[city].worked.push(hex);
            food += f;
        }
        self.cities[city].remembered_worked = self.cities[city].worked.clone();
    }

    /// Of `tiles` (each with its delivered goods), the one `city`'s next
    /// citizen takes: the most by its priority order, food first while
    /// `food` doesn't feed it, ties by hex coordinates; the first citizen,
    /// the manager, only on land.
    fn best_tile(&self, city: usize, tiles: &[(Hex, (i32, i32, i32))], food: i32) -> Option<usize> {
        let priorities = self.cities[city].priorities;
        let fed = self.is_fed(city, food);
        tiles
            .iter()
            .enumerate()
            .filter(|(_, (h, _))| self.may_manage_or_work(city, *h))
            .min_by_key(|(_, (h, goods))| (-priorities.score(*goods, fed), h.q, h.r))
            .map(|(i, _)| i)
    }

    /// Keeps manual choices where possible, substitutes around temporarily
    /// blocked tiles, then restores those choices once they are available.
    pub(super) fn reconcile_citizens(&mut self, city: usize) {
        let routes = self.routes(city);
        // Tiles no citizen may take: another city's worked tiles, and city
        // centers and placed buildings (`closed_to_citizens`).
        let other_claims: HashSet<Hex> = self
            .cities
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != city)
            .flat_map(|(_, c)| c.worked.iter().copied())
            .chain(
                routes
                    .costs
                    .keys()
                    .copied()
                    .filter(|&h| self.closed_to_citizens(h)),
            )
            .collect();
        let centers: HashSet<Hex> = self.cities.iter().map(|c| c.pos).collect();
        self.cities[city].worked.retain(|h| {
            routes.costs.contains_key(h) && !other_claims.contains(h) && !centers.contains(h)
        });
        // The first remembered tile is the manager. Restore it to the first
        // slot before restoring workers, so an automatically promoted worker
        // never becomes a permanent manager after the original tile clears.
        let remembered = self.cities[city].remembered_worked.clone();
        let capacity = self.cities[city].population.min(MAX_CITY_POPULATION);
        if let Some(manager) = remembered.first().copied()
            && routes.costs.contains_key(&manager)
            && !other_claims.contains(&manager)
            && !centers.contains(&manager)
        {
            let worked = &mut self.cities[city].worked;
            if !worked.contains(&manager) && worked.len() >= capacity {
                if let Some(fallback) = worked.iter().rposition(|h| !remembered.contains(h)) {
                    worked.remove(fallback);
                } else {
                    worked.pop();
                }
            }
            worked.retain(|h| *h != manager);
            worked.insert(0, manager);
        }

        // Desired assignments take priority over temporary replacements. A
        // manual change updates `remembered_worked`, so this only restores
        // tiles that the player has not since replaced.
        for h in remembered.iter().copied().skip(1) {
            if routes.costs.contains_key(&h)
                && !other_claims.contains(&h)
                && !centers.contains(&h)
                && !self.cities[city].worked.contains(&h)
                && self.may_assign(city, h)
            {
                if self.cities[city].worked.len() >= capacity {
                    if let Some(fallback) = self.cities[city]
                        .worked
                        .iter()
                        .rposition(|tile| !remembered.contains(tile))
                    {
                        self.cities[city].worked.remove(fallback);
                    } else {
                        continue;
                    }
                }
                self.cities[city].worked.push(h);
            }
        }
        if let Some(manager) = self.cities[city].worked.first().copied() {
            self.cities[city]
                .worked
                .retain(|h| *h == manager || manager.distance(*h) == 1);
        }
        let mut candidates: Vec<_> = routes
            .costs
            .iter()
            .filter(|(h, _)| {
                !self.cities[city].worked.contains(h)
                    && !other_claims.contains(h)
                    && !centers.contains(h)
            })
            .filter(|(h, _)| {
                self.sites
                    .get(h)
                    .is_none_or(|s| s.team == self.cities[city].team)
            })
            .map(|(&h, &cost)| (h, self.delivered_goods(city, h, cost)))
            .collect();
        // An open slot goes as auto-assign would fill it: by the priority
        // order, food first until the tiles kept feed the city.
        let mut food = self.worked_food(city, &routes);
        while self.cities[city].worked.len() < self.cities[city].population.min(MAX_CITY_POPULATION)
            && !candidates.is_empty()
        {
            if let Some(manager) = self.cities[city].worked.first().copied() {
                candidates.retain(|(hex, _)| manager.distance(*hex) == 1);
            }
            let Some(pick) = self.best_tile(city, &candidates, food) else {
                break;
            };
            let (hex, (f, _, _)) = candidates.remove(pick);
            self.cities[city].worked.push(hex);
            food += f;
        }
    }

    /// `city` loses a citizen (never its last): the last tile it works is
    /// given up, and the rest reconciled (`reconcile_citizens`). A finished
    /// Settler takes one this way (`complete_builds`).
    pub(in crate::game) fn remove_citizen(&mut self, city: usize) {
        let c = &mut self.cities[city];
        c.population = c.population.saturating_sub(1).max(1);
        let keep = c.population.min(MAX_CITY_POPULATION);
        c.worked.truncate(keep);
        self.reconcile_citizens(city);
    }

    pub fn auto_assign_selected_city(&mut self) {
        if self.is_resolving() {
            return;
        }
        if let Some(city) = self.selected_city {
            self.auto_assign_city(city);
            self.notice = "CITIZENS AUTO ASSIGNED".into();
        }
    }

    /// The turn's economy (`economy.rs`): every city's goods go to its
    /// side's stockpile, the citizens eat from it, each queue pays for the
    /// item it starts and does a turn's work (`work_queues`), and finished
    /// builds complete.
    pub(in crate::game) fn resolve_economy(&mut self) {
        self.notice = "PLANNING - C CITY - SPACE HOLD OR END TURN".into();
        let income: Vec<_> = (0..self.cities.len()).map(|i| self.income(i)).collect();
        let rates: Vec<_> = (0..self.cities.len())
            .map(|i| {
                (
                    self.work_rate(income[i].production()),
                    self.work_rate(self.barracks_income(i)),
                )
            })
            .collect();
        for (i, &goods) in income.iter().enumerate() {
            let team = self.cities[i].team;
            *self.stock_mut(team) += goods;
        }
        self.feed_citizens();
        for city in &mut self.cities {
            city.worked
                .truncate(city.population.min(MAX_CITY_POPULATION));
        }
        // Each queue pays for what it starts and works it. A Barracks trains
        // whatever the manager does; with production speeding builds, the
        // manager on it adds its group's work.
        self.work_queues(&rates);
        self.complete_builds();
        self.heal_at_hospitals();
        for i in 0..self.cities.len() {
            self.reconcile_citizens(i);
        }
    }

    /// Each hospital treats two nearby survivors once per turn. Both health
    /// bars belong to the same unit, so a projected fighter benefits too.
    fn heal_at_hospitals(&mut self) {
        let mut treated = crate::game::fast_hash::HashSet::default();
        let hospitals: Vec<_> = self
            .cities
            .iter()
            .filter_map(|city| {
                city.placed_site(super::Building::FieldHospital)
                    .map(|site| (city.team, site))
            })
            .collect();
        for (team, site) in hospitals {
            let mut candidates: Vec<_> = self
                .units
                .iter()
                .enumerate()
                .filter(|(_, unit)| {
                    unit.team == team
                        && unit.pos.distance(site) <= 2
                        && !treated.contains(&unit.id)
                        && (unit.hp < unit.max_hp() || unit.interior_hp < unit.max_hp())
                })
                .map(|(index, unit)| {
                    let missing = 2.0 * unit.max_hp() - unit.hp - unit.interior_hp;
                    (index, missing, unit.id)
                })
                .collect();
            candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.2.cmp(&b.2)));
            for (index, _, id) in candidates.into_iter().take(2) {
                let unit = &mut self.units[index];
                unit.hp = (unit.hp + 20.0).min(unit.max_hp());
                unit.interior_hp = (unit.interior_hp + 20.0).min(unit.max_hp());
                treated.insert(id);
            }
        }
        for city in &mut self.cities {
            for fighter in &mut city.interior.fighters {
                if treated.contains(&fighter.source_id)
                    && let Some(source) =
                        self.units.iter().find(|unit| unit.id == fighter.source_id)
                {
                    fighter.hp = source.interior_hp;
                }
            }
        }
    }
}
