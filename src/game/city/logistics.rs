//! Logistics: delivery routes from a city or a placed building, how much of a
//! worked tile's yield each route delivers, and the resulting income.
use std::collections::{HashMap, HashSet};

use super::Building;
use crate::game::GameState;
use crate::game::hex::Hex;
use crate::game::unit::Team;

pub(in crate::game) struct Routes {
    pub costs: HashMap<Hex, i32>,
    /// Each reached hex's next hex toward the origin on its cheapest route.
    toward_origin: HashMap<Hex, Hex>,
}

impl Routes {
    /// The hexes goods from `hex` pass through to reach the origin, `hex`
    /// first and the origin last; empty if they can't reach it.
    pub(in crate::game) fn path_from(&self, hex: Hex) -> Vec<Hex> {
        if !self.costs.contains_key(&hex) {
            return Vec::new();
        }
        let mut path = vec![hex];
        while let Some(&next) = self.toward_origin.get(path.last().unwrap()) {
            path.push(next);
        }
        path
    }
}

pub(in crate::game) fn delivered_share(cost: i32) -> i32 {
    match cost {
        0..=2 => 4,
        3..=4 => 3,
        5..=6 => 2,
        7..=8 => 1,
        _ => 0,
    }
}

impl GameState {
    /// Banks on river stretches connected to a friendly Canoe House.
    pub(in crate::game) fn navigable_river_banks(&self, team: Team) -> HashSet<Hex> {
        let mut banks: HashSet<Hex> = self
            .cities
            .iter()
            .filter(|c| c.team == team)
            .filter_map(|c| c.placed_site(Building::CanoeHouse))
            .collect();
        loop {
            let before = banks.len();
            for (a, b) in self.grid.rivers() {
                if banks.contains(&a) || banks.contains(&b) {
                    banks.insert(a);
                    banks.insert(b);
                }
            }
            if banks.len() == before {
                break;
            }
        }
        banks
    }

    pub(in crate::game) fn is_road_hex(&self, hex: Hex) -> bool {
        self.roads.contains(&hex) || self.cities.iter().any(|city| city.pos == hex)
    }
    /// What `hex` produces when worked: a city center's own yield, a site's,
    /// or its terrain's.
    pub(in crate::game) fn raw_yield(&self, hex: Hex) -> (i32, i32) {
        if self.cities.iter().any(|c| c.pos == hex) {
            (2, 1)
        } else {
            self.tile_yield(hex)
        }
    }

    pub(in crate::game) fn routes(&self, city: usize) -> Routes {
        let city = &self.cities[city];
        self.routes_from(city.team, city.pos)
    }

    /// Delivery network to a city center or a placed building. Each endpoint
    /// has its own falloff, so a worker can deliver differently to each.
    pub(in crate::game) fn routes_from(&self, team: Team, origin: Hex) -> Routes {
        let river_banks = self.navigable_river_banks(team);
        self.routes_from_by(
            origin,
            |hex| {
                self.enemy_of_team_at(hex, team).is_some()
                    || self.cities.iter().any(|c| c.pos == hex && c.team != team)
            },
            |from, to| self.can_cross(from, to, team),
            |from, to| {
                self.is_road_hex(to) || (river_banks.contains(&from) && river_banks.contains(&to))
            },
        )
    }

    /// Like `routes_from`, with `blocked` deciding which hexes goods can't
    /// cross, `crossable` which hex edges they can (walls and others' gates
    /// stop them) and `road` which carry them cheaply: the real board for the
    /// economy, or what the player knows of it for what's shown to them
    /// (`known_routes`).
    pub(in crate::game) fn routes_from_by(
        &self,
        origin: Hex,
        blocked: impl Fn(Hex) -> bool,
        crossable: impl Fn(Hex, Hex) -> bool,
        road: impl Fn(Hex, Hex) -> bool,
    ) -> Routes {
        let mut result = Routes {
            costs: HashMap::new(),
            toward_origin: HashMap::new(),
        };
        if blocked(origin) {
            return result;
        }
        result.costs.insert(origin, 0);
        let mut visited = HashSet::new();
        loop {
            let next = result
                .costs
                .iter()
                .filter(|(h, _)| !visited.contains(*h))
                .min_by_key(|(h, c)| (**c, h.q, h.r))
                .map(|(h, c)| (*h, *c));
            let Some((hex, cost)) = next else { break };
            visited.insert(hex);
            // Goods come in from water tiles but never travel across them.
            if self.grid.terrain(hex).is_water() {
                continue;
            }
            for n in hex.neighbors() {
                if !self.grid.contains(n)
                    || !self.grid.terrain(n).is_workable()
                    || blocked(n)
                    || !crossable(hex, n)
                {
                    continue;
                }
                let step = if road(hex, n) {
                    1
                } else {
                    self.grid.tile(n).route_cost()
                };
                let total = cost + step;
                if total <= 8 && result.costs.get(&n).is_none_or(|old| total < *old) {
                    result.costs.insert(n, total);
                    result.toward_origin.insert(n, hex);
                }
            }
        }
        result
    }

    pub(in crate::game) fn barracks_income(&self, city: usize) -> i32 {
        let c = &self.cities[city];
        let Some(barracks) = c.barracks.filter(|tile| c.worked.first() == Some(tile)) else {
            return 0;
        };
        let routes = self.routes_from(c.team, barracks);
        c.worked
            .iter()
            .map(|tile| {
                let (_, production) = self.tile_yield(*tile);
                production
                    * routes
                        .costs
                        .get(tile)
                        .map_or(0, |cost| delivered_share(*cost))
            })
            .sum()
    }

    /// What a worked tile produces: its site's yield or its terrain's, plus
    /// one food for fresh water (a river or lake beside it) and a special
    /// tile's bonus.
    pub(in crate::game) fn tile_yield(&self, hex: Hex) -> (i32, i32) {
        let (food, production) = self
            .sites
            .get(&hex)
            .map_or_else(|| self.grid.tile(hex).yields(), |s| (s.food, s.production));
        let (extra_food, extra_production) = self.grid.special(hex).map_or((0, 0), |s| s.bonus());
        (
            food + i32::from(self.grid.has_fresh_water(hex)) + extra_food,
            production + extra_production,
        )
    }

    pub(in crate::game) fn mill_food_share(&self, city: usize, hex: Hex, cost: i32) -> i32 {
        if self.cities.iter().any(|c| {
            c.team == self.cities[city].team && c.mill.is_some_and(|mill| mill.distance(hex) == 1)
        }) {
            4
        } else {
            delivered_share(cost)
        }
    }

    /// Remote processors collect from up to three owned, unworked sites.
    /// Their sites need a local route, but not an onward route to the city.
    pub(in crate::game) fn cannery_income(&self, city: usize) -> i32 {
        self.remote_site_income(city, Building::Cannery)
    }

    pub(in crate::game) fn smelter_income(&self, city: usize) -> i32 {
        self.remote_site_income(city, Building::Smelter)
    }

    fn remote_site_income(&self, city: usize, building: Building) -> i32 {
        let c = &self.cities[city];
        let Some(processor) = c.placed_site(building) else {
            return 0;
        };
        let routes = self.routes_from(c.team, processor);
        let mut output: Vec<_> = self
            .sites
            .iter()
            .filter_map(|(&hex, site)| {
                if site.team != c.team
                    || match building {
                        Building::Cannery => site.food <= 0,
                        Building::Smelter => site.label != "MINE" || site.production <= 0,
                        _ => unreachable!(),
                    }
                    || self.cities.iter().any(|other| other.worked.contains(&hex))
                {
                    return None;
                }
                let distance = processor.distance(hex);
                if !(1..=3).contains(&distance) || !routes.costs.contains_key(&hex) {
                    return None;
                }
                // One processor of each kind owns a site; ties go to the older city.
                if self.cities.iter().enumerate().any(|(other_id, other)| {
                    other_id != city
                        && other.team == c.team
                        && other.placed_site(building).is_some_and(|other_site| {
                            other_site.distance(hex) < distance
                                || (other_site.distance(hex) == distance && other_id < city)
                        })
                }) {
                    return None;
                }
                let share = match distance {
                    1 => 4,
                    2 => 3,
                    _ => 2,
                };
                let (food, production) = self.tile_yield(hex);
                Some((
                    if building == Building::Cannery {
                        food
                    } else {
                        production
                    } * share,
                    hex,
                ))
            })
            .collect();
        output.sort_by_key(|&(yield_value, hex)| (-yield_value, hex.q, hex.r));
        output.into_iter().take(3).map(|(value, _)| value).sum()
    }

    pub(in crate::game) fn income(&self, city: usize) -> (i32, i32) {
        let routes = self.routes(city);
        let granary_food = if self.cities[city].built.contains(&Building::Granary) {
            8
        } else {
            0
        };
        let worked =
            self.cities[city]
                .worked
                .iter()
                .fold((8 + granary_food, 4), |(food, prod), hex| {
                    let (food_share, production_share) =
                        routes.costs.get(hex).map_or((0, 0), |&cost| {
                            (
                                self.mill_food_share(city, *hex, cost),
                                delivered_share(cost),
                            )
                        });
                    let (f, p) = self.tile_yield(*hex);
                    (food + f * food_share, prod + p * production_share)
                });
        (
            worked.0 + self.cannery_income(city),
            worked.1 + self.smelter_income(city),
        )
    }
}
