//! Barracks as the side's military building (`docs/rts-economy.md`): they
//! train troops faster than a city center (`CITY_TRAINING_SLOWDOWN`), and
//! only they train Cavalry and Armored, which draw on Horses and Iron
//! deposits: a Barracks on the deposit (or beside a Stable or Forge on or
//! next to it) can train `UNITS_PER_DEPOSIT` of them per deposit its side
//! holds. By default the cap counts those alive and queued, so a deposit
//! keeps its worth all game and a lost unit can be replaced; the Debug
//! panel's lifetime cap counts every one ever trained instead.
use std::collections::BTreeSet;

use super::{BuildUnit, Building};
use crate::game::GameState;
use crate::game::hex::Hex;
use crate::game::terrain::Resource;
use crate::game::unit::Team;

/// Cavalry or Armored a side may have per Horses or Iron deposit its
/// Barracks use.
pub(in crate::game) const UNITS_PER_DEPOSIT: usize = 3;
/// How many times longer a city center takes to train a land troop than a
/// Barracks.
pub(in crate::game) const CITY_TRAINING_SLOWDOWN: i32 = 2;

impl GameState {
    /// The deposits of `resource` city `city`'s Barracks draws on: the one
    /// under it, and those on or next to a Stable (Horses) or Forge (Iron)
    /// beside it. Held or not.
    pub(in crate::game) fn barracks_deposits(&self, city: usize, resource: Resource) -> Vec<Hex> {
        let c = &self.cities[city];
        let Some(barracks) = c.barracks else {
            return Vec::new();
        };
        let mut deposits = BTreeSet::new();
        if self.grid.resource(barracks) == Some(resource) {
            deposits.insert((barracks.q, barracks.r));
        }
        let support = match resource {
            Resource::Horses => Building::Stable,
            Resource::Iron => Building::Forge,
        };
        if let Some(site) = c
            .placed_site(support)
            .filter(|site| site.distance(barracks) == 1)
        {
            for hex in std::iter::once(site).chain(site.neighbors()) {
                if self.grid.contains(hex) && self.grid.resource(hex) == Some(resource) {
                    deposits.insert((hex.q, hex.r));
                }
            }
        }
        deposits.into_iter().map(|(q, r)| Hex::new(q, r)).collect()
    }

    /// The deposits of `resource` `team`'s Barracks draw on, leaving out any
    /// an enemy unit stands on: taking a deposit takes its units away.
    pub(in crate::game) fn side_deposits(&self, team: Team, resource: Resource) -> Vec<Hex> {
        let mut deposits = BTreeSet::new();
        for city in (0..self.cities.len()).filter(|&i| self.cities[i].team == team) {
            for hex in self.barracks_deposits(city, resource) {
                if self.enemy_of_team_at(hex, team).is_none() {
                    deposits.insert((hex.q, hex.r));
                }
            }
        }
        deposits.into_iter().map(|(q, r)| Hex::new(q, r)).collect()
    }

    /// How many troops needing `resource` `team` may have: `UNITS_PER_DEPOSIT`
    /// per deposit it holds.
    pub(in crate::game) fn special_cap(&self, team: Team, resource: Resource) -> usize {
        UNITS_PER_DEPOSIT * self.side_deposits(team, resource).len()
    }

    /// What counts against `team`'s cap for `resource`: its Barracks' queued
    /// troops needing it, and those trained from it that are alive (or,
    /// with the lifetime cap, ever trained).
    pub(in crate::game) fn special_used(&self, team: Team, resource: Resource) -> usize {
        let queued = self
            .cities
            .iter()
            .filter(|c| c.team == team)
            .flat_map(|c| &c.barracks_queue)
            .filter(|build| build.required_resource() == Some(resource))
            .count();
        let trained = if self.lifetime_special_cap {
            self.special_trained[team.index()][resource.index()] as usize
        } else {
            self.units
                .iter()
                .filter(|u| u.team == team && u.drawn_from == Some(resource))
                .count()
        };
        queued + trained
    }

    /// Why city `city`'s Barracks can't queue `build` now, if it can't: no
    /// Barracks, no deposit for it, or the side's cap is used up.
    pub(in crate::game) fn barracks_lock(&self, city: usize, build: BuildUnit) -> Option<String> {
        if self.cities[city].barracks.is_none() {
            return Some("BUILD A BARRACKS FIRST".into());
        }
        let resource = build.required_resource()?;
        if self.barracks_deposits(city, resource).is_empty() {
            let support = match resource {
                Resource::Horses => "STABLE",
                Resource::Iron => "FORGE",
            };
            return Some(format!(
                "NEEDS THIS BARRACKS ON {} (OR BESIDE A {support} ON {})",
                resource.name(),
                resource.name()
            ));
        }
        let team = self.cities[city].team;
        let cap = self.special_cap(team, resource);
        if cap == 0 {
            return Some(format!("AN ENEMY HOLDS YOUR {} DEPOSIT", resource.name()));
        }
        (self.special_used(team, resource) >= cap).then(|| {
            format!(
                "CAP REACHED: {cap} {} FOR {} {} DEPOSIT{}",
                build.name(),
                cap / UNITS_PER_DEPOSIT,
                resource.name(),
                if cap / UNITS_PER_DEPOSIT == 1 {
                    ""
                } else {
                    "S"
                }
            )
        })
    }

    /// Debug panel: switches the Cavalry and Armored cap between counting
    /// those alive and those ever trained.
    pub fn toggle_lifetime_special_cap(&mut self) {
        self.lifetime_special_cap = !self.lifetime_special_cap;
        self.notice = if self.lifetime_special_cap {
            "CAVALRY AND ARMORED CAP COUNTS EVERY ONE EVER TRAINED".into()
        } else {
            "CAVALRY AND ARMORED CAP COUNTS THOSE ALIVE".into()
        };
    }
}
