//! Citizens: labor focus, tile assignment (the manager and its workers),
//! growth, and the end-of-turn economy tick.
use std::collections::HashSet;

use super::{LaborFocus, MAX_CITY_POPULATION, delivered_share};
use crate::game::GameState;
use crate::game::hex::Hex;

impl GameState {
    pub fn set_selected_city_focus(&mut self, focus: LaborFocus) {
        if let Some(city) = self.selected_city {
            self.cities[city].focus = focus;
            self.auto_assign_city(city);
            self.notice = format!("CITY FOCUS: {}", focus.name());
        }
    }

    pub(super) fn may_assign(&self, city: usize, hex: Hex) -> bool {
        let manager_can_reach = self.cities[city]
            .worked
            .first()
            .is_none_or(|manager| manager.distance(hex) == 1);
        manager_can_reach
            && !self
                .cities
                .iter()
                .any(|c| c.pos == hex || c.worked.contains(&hex))
            && self
                .sites
                .get(&hex)
                .is_none_or(|s| s.team == self.cities[city].team)
    }

    pub(super) fn may_be_manager(&self, city: usize, hex: Hex) -> bool {
        !self
            .cities
            .iter()
            .enumerate()
            .any(|(i, c)| i != city && (c.pos == hex || c.worked.contains(&hex)))
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
                && !self.cities.iter().any(|c| c.pos == target)
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

    pub(in crate::game) fn auto_assign_city(&mut self, city: usize) {
        self.cities[city].worked.clear();
        let routes = self.routes(city);
        let mut tiles: Vec<_> = routes
            .costs
            .iter()
            .filter(|(h, _)| self.may_assign(city, **h))
            .map(|(h, cost)| {
                let (f, p) = self.tile_yield(*h);
                (
                    *h,
                    f * self.mill_food_share(city, *h, *cost),
                    p * delivered_share(*cost),
                )
            })
            .collect();
        let mut food = 8;
        while self.cities[city].worked.len() < self.cities[city].population.min(MAX_CITY_POPULATION)
            && !tiles.is_empty()
        {
            if let Some(manager) = self.cities[city].worked.first().copied() {
                tiles.retain(|(hex, _, _)| manager.distance(*hex) == 1);
            }
            let needs_food = food < self.cities[city].population as i32 * 8 + 4;
            let focus = self.cities[city].focus;
            tiles.sort_by_key(|(h, f, p)| {
                let score = match focus {
                    LaborFocus::Food => f * 5 + p,
                    LaborFocus::Production => p * 5 + f,
                    LaborFocus::Balanced => {
                        if needs_food {
                            f * 4 + p
                        } else {
                            p * 4 + f
                        }
                    }
                };
                (-score, h.q, h.r)
            });
            // The first citizen is the manager, who must stand on land.
            let Some(pick) = tiles
                .iter()
                .position(|(h, _, _)| self.may_manage_or_work(city, *h))
            else {
                break;
            };
            let (hex, f, _) = tiles.remove(pick);
            self.cities[city].worked.push(hex);
            food += f;
        }
        self.cities[city].remembered_worked = self.cities[city].worked.clone();
    }

    /// Keeps manual choices where possible, substitutes around temporarily
    /// blocked tiles, then restores those choices once they are available.
    pub(super) fn reconcile_citizens(&mut self, city: usize) {
        let routes = self.routes(city);
        let other_claims: HashSet<Hex> = self
            .cities
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != city)
            .flat_map(|(_, c)| c.worked.iter().copied())
            .collect();
        self.cities[city]
            .worked
            .retain(|h| routes.costs.contains_key(h) && !other_claims.contains(h));
        // The first remembered tile is the manager. Restore it to the first
        // slot before restoring workers, so an automatically promoted worker
        // never becomes a permanent manager after the original tile clears.
        let remembered = self.cities[city].remembered_worked.clone();
        let capacity = self.cities[city].population.min(MAX_CITY_POPULATION);
        if let Some(manager) = remembered.first().copied()
            && routes.costs.contains_key(&manager)
            && !other_claims.contains(&manager)
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
            .filter(|(h, _)| !self.cities[city].worked.contains(h) && !other_claims.contains(h))
            .filter(|(h, _)| {
                self.sites
                    .get(h)
                    .is_none_or(|s| s.team == self.cities[city].team)
            })
            .map(|(h, cost)| {
                let (f, p) = self.tile_yield(*h);
                (
                    *h,
                    f * self.mill_food_share(city, *h, *cost),
                    p * delivered_share(*cost),
                )
            })
            .collect();
        while self.cities[city].worked.len() < self.cities[city].population.min(MAX_CITY_POPULATION)
            && !candidates.is_empty()
        {
            if let Some(manager) = self.cities[city].worked.first().copied() {
                candidates.retain(|(hex, _, _)| manager.distance(*hex) == 1);
            }
            candidates.sort_by_key(|(h, f, p)| (-(f * 3 + p), h.q, h.r));
            let Some(pick) = candidates
                .iter()
                .position(|(h, _, _)| self.may_manage_or_work(city, *h))
            else {
                break;
            };
            self.cities[city].worked.push(candidates.remove(pick).0);
        }
    }

    pub(in crate::game) fn growth_status(&self, city: usize) -> (i32, i32, String) {
        let c = &self.cities[city];
        let threshold = (10 + 5 * c.population as i32) * 4;
        let net = self.income(city).0 - c.population as i32 * 8;
        let turns = if net > 0 {
            ((threshold - c.food).max(0) + net - 1) / net
        } else {
            0
        };
        let label = if net > 0 {
            format!("GROWTH IN {turns} TURNS")
        } else {
            "NO GROWTH: NEED FOOD".into()
        };
        ((c.food * 100 / threshold).clamp(0, 100), threshold, label)
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

    pub(in crate::game) fn resolve_economy(&mut self) {
        let income: Vec<_> = (0..self.cities.len()).map(|i| self.income(i)).collect();
        let barracks_income: Vec<_> = (0..self.cities.len())
            .map(|i| self.barracks_income(i))
            .collect();
        for ((city, (food, production)), barracks_production) in
            self.cities.iter_mut().zip(income).zip(barracks_income)
        {
            city.food += food - city.population as i32 * 8;
            if !city.queue.is_empty() {
                // A manager at a Barracks directs the work group there, but
                // does not stop the city itself benefiting from its labor.
                city.production += production;
            } else {
                city.production = 0;
            }
            if !city.barracks_queue.is_empty() && barracks_production > 0 {
                city.barracks_production += barracks_production;
            } else if city.barracks_queue.is_empty() {
                city.barracks_production = 0;
            }
            let threshold = (10 + 5 * city.population as i32) * 4;
            if city.food >= threshold && city.population < MAX_CITY_POPULATION {
                city.food -= threshold;
                city.population += 1;
            }
            if city.food < 0 {
                city.population = city.population.saturating_sub(1).max(1);
                city.food = 0;
            }
            city.worked
                .truncate(city.population.min(MAX_CITY_POPULATION));
        }
        self.complete_builds();
        for i in 0..self.cities.len() {
            self.reconcile_citizens(i);
        }
        self.notice = "PLANNING - C CITY - SPACE HOLD OR END TURN".into();
    }
}
