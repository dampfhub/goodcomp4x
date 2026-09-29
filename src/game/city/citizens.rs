//! Citizens: the priority order, tile assignment (up to `MAX_MANAGERS`
//! clusters of a manager and its workers), losing a citizen, and the
//! end-of-turn economy tick.

use super::{
    Building, Cluster, FOOD_PER_CITIZEN, Good, Priorities, Routes, WORKERS_PER_MANAGER,
    cluster_tiles, delivered_share,
};
use crate::game::GameState;
use crate::game::fast_hash::HashSet;
use crate::game::hex::Hex;

/// Food a city's tiles bring in past its citizens' upkeep before it's fed
/// and follows its priority order (the food floor): one whole food.
const FOOD_FLOOR_MARGIN: i32 = 4;
/// The city center's own food, which counts toward the food floor (as
/// does a Cannery's, `unworked_food`).
const CENTER_FOOD: i32 = 8;

/// A tile a citizen might take, with its delivered food, wood and metal.
type Candidate = (Hex, (i32, i32, i32));

/// Trims `clusters` to `capacity` citizens and `allowed` managers: the
/// last cluster's last worker goes first, and a manager only once its
/// cluster has no workers left.
pub(super) fn trim_clusters(clusters: &mut Vec<Cluster>, capacity: usize, allowed: usize) {
    while clusters.iter().map(Cluster::citizens).sum::<usize>() > capacity
        || clusters.len() > allowed
    {
        let Some(last) = clusters.last_mut() else {
            break;
        };
        if last.workers.pop().is_none() {
            clusters.pop();
        }
    }
}

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

    /// Whether `food` (the center's, its Cannery's and its worked tiles',
    /// delivered) feeds `city`: its citizens' upkeep and one more (the food
    /// floor).
    pub(super) fn is_fed(&self, city: usize, food: i32) -> bool {
        food >= self.cities[city].population as i32 * FOOD_PER_CITIZEN + FOOD_FLOOR_MARGIN
    }

    /// The food `city` gets that none of its citizens work for, which counts
    /// toward the food floor as its income does: its center's, and what its
    /// Cannery collects from sites no citizen works (as they stand now).
    fn unworked_food(&self, city: usize) -> i32 {
        CENTER_FOOD + self.cannery_income(city)
    }

    /// The food `city`'s worked tiles deliver.
    fn worked_food(&self, city: usize, routes: &Routes) -> i32 {
        self.cities[city]
            .worked()
            .filter_map(|h| routes.costs.get(&h).map(|&cost| (h, cost)))
            .map(|(h, cost)| self.delivered_goods(city, h, cost).0)
            .sum()
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

    /// Whether `city`'s citizens may take `hex` at all: no city center or
    /// building on it, no citizen of any city working it, and no other
    /// side's site.
    pub(super) fn is_open(&self, city: usize, hex: Hex) -> bool {
        !self.closed_to_citizens(hex)
            && !self.cities.iter().any(|c| c.works(hex))
            && self
                .sites
                .get(&hex)
                .is_none_or(|s| s.team == self.cities[city].team)
    }

    /// The first of `city`'s clusters with room for a worker on `hex`: its
    /// manager beside it, and fewer than `WORKERS_PER_MANAGER` workers.
    pub(super) fn cluster_with_room(&self, city: usize, hex: Hex) -> Option<usize> {
        self.cities[city]
            .clusters
            .iter()
            .position(|c| c.workers.len() < WORKERS_PER_MANAGER && c.manager.distance(hex) == 1)
    }

    /// Whether `hex` could take a new manager of `city`, as far as its
    /// other managers go: on land, and beside none of them (`except`, the
    /// cluster whose manager is moving, aside).
    pub(super) fn clear_of_managers(&self, city: usize, hex: Hex, except: Option<usize>) -> bool {
        !self.grid.terrain(hex).is_water()
            && self.cities[city]
                .clusters
                .iter()
                .enumerate()
                .all(|(k, c)| Some(k) == except || c.manager.distance(hex) > 1)
    }

    /// Whether a citizen can be assigned to `hex`: it's open, and either a
    /// manager with room is beside it, or the city may start a cluster
    /// there (it has fewer managers than its population allows, and `hex`
    /// is land clear of its other managers). A click assigns by the same
    /// rules (`city_click`), each with its own notice.
    #[cfg(test)]
    pub(super) fn may_assign(&self, city: usize, hex: Hex) -> bool {
        let c = &self.cities[city];
        self.is_open(city, hex)
            && (self.cluster_with_room(city, hex).is_some()
                || (c.clusters.len() < c.managers_allowed()
                    && self.clear_of_managers(city, hex, None)))
    }

    /// Whether cluster `cluster`'s manager can move to `hex`: land in the
    /// city's reach that no building, other city or other of its clusters
    /// takes, beside none of its other managers.
    pub(super) fn may_be_manager(&self, city: usize, cluster: usize, hex: Hex) -> bool {
        let c = &self.cities[city];
        !self.closed_to_citizens(hex)
            && !self
                .cities
                .iter()
                .enumerate()
                .any(|(i, other)| i != city && other.works(hex))
            && c.cluster_of(hex).is_none_or(|k| k == cluster)
            && self.sites.get(&hex).is_none_or(|site| site.team == c.team)
            && self.clear_of_managers(city, hex, Some(cluster))
            && self.routes(city).costs.contains_key(&hex)
    }

    /// Relocates cluster `cluster`'s manager and carries each of its
    /// workers' axial offsets with it; the other clusters stay. Workers
    /// whose matching tile is unavailable are filled by normal
    /// auto-assignment.
    pub(super) fn move_manager(&mut self, city: usize, cluster: usize, new_manager: Hex) {
        let old = self.cities[city].clusters[cluster].clone();
        let routes = self.routes(city);
        let claims: HashSet<Hex> = self
            .cities
            .iter()
            .enumerate()
            .flat_map(|(i, c)| {
                c.clusters
                    .iter()
                    .enumerate()
                    .filter(move |&(k, _)| i != city || k != cluster)
                    .flat_map(|(_, c)| c.tiles())
            })
            .collect();
        let mut relocated = Cluster::new(new_manager);
        for worker in old.workers {
            let target = Hex::new(
                new_manager.q + worker.q - old.manager.q,
                new_manager.r + worker.r - old.manager.r,
            );
            let legal = routes.costs.contains_key(&target)
                && !claims.contains(&target)
                && !self.closed_to_citizens(target)
                && self
                    .sites
                    .get(&target)
                    .is_none_or(|site| site.team == self.cities[city].team);
            if legal && target != new_manager && !relocated.workers.contains(&target) {
                relocated.workers.push(target);
            }
        }
        self.cities[city].clusters[cluster] = relocated;
        self.cities[city].remembered = self.cities[city].clusters.clone();
        self.reconcile_citizens(city);
        self.notice = "MANAGER MOVED - WORKERS FOLLOWED WHERE POSSIBLE".into();
    }

    /// Assigns the city's citizens afresh (`fill_citizens`).
    pub(in crate::game) fn auto_assign_city(&mut self, city: usize) {
        self.cities[city].clusters.clear();
        let routes = self.routes(city);
        self.fill_citizens(city, &routes);
        self.cities[city].remembered = self.cities[city].clusters.clone();
    }

    /// Fills `city`'s open slots one citizen at a time, each on the open
    /// tile worth the most by its priority order (`Priorities::score`,
    /// food first until the city is fed; ties go by hex coordinates): a
    /// worker, beside a manager with room (the first such cluster takes
    /// it), or, once no manager has room or an open tile beside it, a new
    /// manager, on land clear of its other managers, while its population
    /// allows another.
    fn fill_citizens(&mut self, city: usize, routes: &Routes) {
        let mut tiles: Vec<Candidate> = routes
            .costs
            .iter()
            .filter(|(h, _)| self.is_open(city, **h))
            .map(|(&h, &cost)| (h, self.delivered_goods(city, h, cost)))
            .collect();
        let mut tile_food = self.worked_food(city, routes);
        while self.cities[city].working() < self.cities[city].capacity() {
            // Anew each time: a tile taken may be one its Cannery collected.
            let food = self.unworked_food(city) + tile_food;
            let c = &self.cities[city];
            let worker = self.best_tile(city, &tiles, food, |h| {
                self.cluster_with_room(city, h).is_some()
            });
            let pick = match worker {
                Some(pick) => Some((pick, false)),
                None if c.clusters.len() < c.managers_allowed() => self
                    .best_tile(city, &tiles, food, |h| {
                        self.clear_of_managers(city, h, None)
                    })
                    .map(|pick| (pick, true)),
                None => None,
            };
            let Some((pick, as_manager)) = pick else {
                break;
            };
            let (hex, (f, _, _)) = tiles.remove(pick);
            if as_manager {
                self.cities[city].clusters.push(Cluster::new(hex));
            } else {
                let k = self.cluster_with_room(city, hex).expect("room beside it");
                self.cities[city].clusters[k].workers.push(hex);
            }
            tile_food += f;
        }
    }

    /// Of `tiles` that `allowed` lets it take, the one `city`'s next
    /// citizen takes: the most by its priority order, food first while
    /// `food` doesn't feed it, ties by hex coordinates.
    fn best_tile(
        &self,
        city: usize,
        tiles: &[Candidate],
        food: i32,
        allowed: impl Fn(Hex) -> bool,
    ) -> Option<usize> {
        let priorities = self.cities[city].priorities;
        let fed = self.is_fed(city, food);
        tiles
            .iter()
            .enumerate()
            .filter(|(_, (h, _))| allowed(*h))
            .min_by_key(|(_, (h, goods))| (-priorities.score(*goods, fed), h.q, h.r))
            .map(|(i, _)| i)
    }

    /// Works the city's clusters out again from what was last assigned
    /// (`City::remembered`), then fills what's left open (`fill_citizens`).
    /// A remembered tile the city can't keep (out of reach, or another
    /// city's, a city center or a building now) is skipped until it can; a
    /// cluster whose manager can't be kept is run meanwhile by the first of
    /// its workers that could manage, until the manager's tile is back.
    /// Clusters past what the population allows go, last first
    /// (`trim_clusters`).
    pub(super) fn reconcile_citizens(&mut self, city: usize) {
        let routes = self.routes(city);
        let team = self.cities[city].team;
        let others: HashSet<Hex> = self
            .cities
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != city)
            .flat_map(|(_, c)| c.worked())
            .collect();
        let keeps = |h: Hex| {
            routes.costs.contains_key(&h)
                && !others.contains(&h)
                && !self.closed_to_citizens(h)
                && self.sites.get(&h).is_none_or(|s| s.team == team)
        };
        let mut clusters: Vec<Cluster> = Vec::new();
        for plan in &self.cities[city].remembered {
            let taken: HashSet<Hex> = cluster_tiles(&clusters).collect();
            let Some(manager) = plan.tiles().find(|&h| {
                keeps(h)
                    && !taken.contains(&h)
                    && !self.grid.terrain(h).is_water()
                    && clusters.iter().all(|c| c.manager.distance(h) > 1)
            }) else {
                continue;
            };
            let mut cluster = Cluster::new(manager);
            for &worker in &plan.workers {
                if worker != manager
                    && keeps(worker)
                    && !taken.contains(&worker)
                    && manager.distance(worker) == 1
                    && cluster.workers.len() < WORKERS_PER_MANAGER
                {
                    cluster.workers.push(worker);
                }
            }
            clusters.push(cluster);
        }
        let c = &self.cities[city];
        trim_clusters(&mut clusters, c.capacity(), c.managers_allowed());
        self.cities[city].clusters = clusters;
        self.fill_citizens(city, &routes);
    }

    /// `city` loses a citizen (it starves, `feed_citizens`, or a finished
    /// Settler takes one, `complete_builds`). An
    /// idle citizen goes first, if it has one; then the last cluster's last
    /// worker, and a manager only once its cluster has no workers left
    /// (`trim_clusters`, on what it works and what it remembers alike). A
    /// city never goes below one citizen: returns whether it lost one.
    pub(in crate::game) fn remove_citizen(&mut self, city: usize) -> bool {
        let c = &mut self.cities[city];
        if c.population <= 1 {
            return false;
        }
        c.population -= 1;
        let (capacity, allowed) = (c.capacity(), c.managers_allowed());
        trim_clusters(&mut c.clusters, capacity, allowed);
        trim_clusters(&mut c.remembered, capacity, allowed);
        true
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
        // Each queue pays for what it starts and works it. A Barracks trains
        // whatever its managers do; with production speeding builds, each
        // manager beside it adds its cluster's work.
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
