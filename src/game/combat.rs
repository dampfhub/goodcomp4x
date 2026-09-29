//! Civ-style combat math: damage grows exponentially with the gap between an
//! attacker's attack and the target's defense (boosted by the terrain it
//! stands on). There is no random spread: the same fight always deals the
//! same damage, so the attack preview (`attack_preview`, drawn on the health
//! bars by `draw.rs`) calls these same functions.

use super::GameState;
use super::ability::{Ability, VOLLEY_DAMAGE};
use super::city::{BARRACKS_DEFENSE, COASTAL_BATTERY_DEFENSE};
use super::fog::Fog;
use super::hex::{Hex, HexGrid};
use super::turn::{Phase, moves_before_attacking, step_rank};
use super::unit::{Unit, UnitType};

/// Damage `attacker` deals to `target` in one blow, with `defense_multiplier`
/// the terrain and fort bonus on the target (`GameState::defense_multiplier`).
pub fn damage(attacker: &Unit, target: &Unit, defense_multiplier: f32) -> f32 {
    let defense = target.stats().defense * defense_multiplier;
    damage_against(attacker.stats().attack, defense)
}

/// Damage an attack of `attack` deals in one blow against `defense`: the one
/// formula every fight uses, units and structures alike. A structure's listed
/// defense already includes its fortification, so terrain does not modify it
/// again.
pub fn damage_against(attack: f32, defense: f32) -> f32 {
    (30.0 * ((attack - defense) * 0.04).exp()).clamp(1.0, 100.0)
}

/// The share of `attacker`'s damage that reaches `defender` across the
/// waterline: land melee can't reach ships at all, and a galley's guns do
/// little to troops ashore. 1 for a fight on one side of it.
pub fn shore_scale(attacker: &Unit, defender: &Unit) -> f32 {
    if !attacker.is_naval() && defender.is_naval() {
        match attacker.unit_type {
            UnitType::Ranged => 0.4,
            UnitType::Siege => 0.6,
            _ => 0.0,
        }
    } else if attacker.unit_type == UnitType::PatrolGalley && !defender.is_naval() {
        0.35
    } else {
        1.0
    }
}

/// Melee attacks draw retaliation from a surviving defender; ranged ones don't.
pub fn draws_retaliation(attacker: &Unit) -> bool {
    attacker.unit_type.stats().attack_range == 1
}

/// Whether `defender`, hit by `attacker` for `hit`, strikes back: a melee
/// attack it survives, with both on the same side of the waterline.
pub fn retaliates(attacker: &Unit, defender: &Unit, hit: f32) -> bool {
    draws_retaliation(attacker) && attacker.is_naval() == defender.is_naval() && defender.hp > hit
}

/// Combat-relevant conditions on `unit`, like " (on hills, shield wall)", for
/// combat log lines. Empty when there are none.
pub fn unit_note(unit: &Unit, grid: &HexGrid, in_fort: bool) -> String {
    let mut notes = Vec::new();
    if in_fort {
        notes.push("in a fort");
    }
    if grid.tile(unit.pos).feature.is_some() {
        notes.push("under cover");
    }
    if grid.tile(unit.pos).hills {
        notes.push("on hills");
    }
    if unit.deployed {
        notes.push("deployed");
    }
    if unit.ability_queued {
        match unit.ability() {
            Ability::ShieldWall => notes.push("shield wall"),
            Ability::Charge => notes.push("charging"),
            Ability::Volley | Ability::Deploy | Ability::Lookout => {}
        }
    }
    if notes.is_empty() {
        String::new()
    } else {
        format!(" ({})", notes.join(", "))
    }
}

pub fn hp_status(unit: &Unit) -> String {
    if unit.is_alive() {
        format!("{:.0}/{:.0} hp", unit.hp, unit.max_hp())
    } else {
        "destroyed".to_string()
    }
}

/// Something the attack preview shows losing HP.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Hurt {
    /// A unit on the map, by index into `GameState::units`.
    Unit(usize),
    /// A city's Barracks, by city index.
    Barracks(usize),
    /// A city's coastal battery, by city index.
    Battery(usize),
    /// A fighter inside the open city, by its source unit's id.
    Fighter(u32),
    /// The open city's command post.
    Post,
}

/// One thing's HP loss in the preview.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Loss {
    pub hurt: Hurt,
    /// The HP it would lose.
    pub amount: f32,
    /// Its HP now.
    pub hp: f32,
    /// On the target's side, rather than one of the attackers'.
    pub target_side: bool,
}

impl Loss {
    /// Whether the loss would kill it (or destroy it).
    pub fn lethal(&self) -> bool {
        self.amount >= self.hp
    }
}

/// What the attacks on the hovered hex would do this turn, as the player
/// knows the board (`GameState::attack_preview`): every attack on it the
/// selection would make, and those the player's other units already plan.
#[derive(Clone, PartialEq, Debug, Default)]
pub(super) struct AttackPreview {
    pub losses: Vec<Loss>,
    /// The target is an enemy unit, whose own plan (a move, an ability) the
    /// player can't know: the preview assumes it stays and does nothing.
    pub if_it_stays: bool,
}

impl AttackPreview {
    pub fn loss(&self, hurt: Hurt) -> Option<&Loss> {
        self.losses.iter().find(|loss| loss.hurt == hurt)
    }

    /// Adds `amount` to `hurt`'s loss, starting it at `hp`.
    pub(super) fn add(&mut self, hurt: Hurt, amount: f32, hp: f32, target_side: bool) {
        match self.losses.iter_mut().find(|loss| loss.hurt == hurt) {
            Some(loss) => loss.amount += amount,
            None => self.losses.push(Loss {
                hurt,
                amount,
                hp,
                target_side,
            }),
        }
    }

    /// `hurt`'s HP left, from `hp`, once the losses so far land.
    fn hp_left(&self, hurt: Hurt, hp: f32) -> f32 {
        hp - self.loss(hurt).map_or(0.0, |loss| loss.amount)
    }

    /// The target side's total loss, and whether any of it is lethal.
    pub fn dealt(&self) -> (f32, bool) {
        let dealt = self.losses.iter().filter(|loss| loss.target_side);
        let lethal = dealt.clone().any(Loss::lethal);
        (dealt.map(|loss| loss.amount).sum(), lethal)
    }

    /// The attackers' total loss to retaliation.
    pub fn retaliation(&self) -> f32 {
        self.losses
            .iter()
            .filter(|loss| !loss.target_side)
            .map(|loss| loss.amount)
            .sum()
    }
}

impl GameState {
    /// The attack preview for the hovered hex (the hovered tile in a city's
    /// interior), or `None` if no attack of the player's this turn would hit
    /// anything there the player knows of. Only the hovered hex is ever
    /// computed.
    pub(super) fn attack_preview(&self) -> Option<AttackPreview> {
        if self.is_playing_out() {
            return None;
        }
        if let Some(city) = self.interior_view {
            return self.interior_attack_preview(city);
        }
        let target = self.hovered_tile?;
        // The cheap test first: most hexes hold nothing to attack.
        if !self.has_enemy_target_at(target, self.local_team) {
            return None;
        }
        // Only what's in sight: units aren't remembered, so out of sight
        // nobody can say what an attack there would hit.
        let fog = self.fog();
        if !fog.sees(target) {
            return None;
        }
        let selection = self.selection();
        // Each attack of the player's that would reach the target: the
        // attacker, the hex it attacks and where it attacks from.
        let mut attacks: Vec<(usize, Hex, Hex)> = Vec::new();
        for (i, unit) in self.units.iter().enumerate() {
            if !self.is_player_controlled(i) || !unit.can_attack() || self.rival_of(i).is_some() {
                continue;
            }
            let range = unit.stats().attack_range;
            // What a right-click would order, as `try_queue_attack` and
            // `group_attack` decide it; else the attack it already plans.
            let ordered = selection.contains(&i)
                && self.known_attack_target_legal(i, target, false, &fog)
                && unit.planned_pos().distance(target) <= range;
            let aim = if ordered {
                target
            } else if let Some(aim) = unit.planned_attack {
                aim
            } else {
                continue;
            };
            let volley = unit.ability_queued && unit.ability() == Ability::Volley;
            if aim != target && !(volley && aim.distance(target) == 1) {
                continue;
            }
            let from = if moves_before_attacking(unit.unit_type) {
                unit.planned_pos()
            } else {
                unit.pos
            };
            if from.distance(aim) <= range {
                attacks.push((i, aim, from));
            }
        }
        if attacks.is_empty() {
            return None;
        }
        // Step by step in resolution order: each step sees the board as it
        // stood when the step began.
        let rank = |i: usize| step_rank(self.units[i].unit_type, Phase::Attack);
        attacks.sort_by_key(|&(i, ..)| (rank(i), i));
        let mut preview = AttackPreview::default();
        for step in attacks.chunk_by(|a, b| rank(a.0) == rank(b.0)) {
            let before = preview.clone();
            for &(a, aim, from) in step {
                self.preview_attack(a, aim, from, &before, &mut preview, &fog);
            }
        }
        preview.if_it_stays = self.units_at(target).any(|i| {
            !self.is_player_controlled(i)
                && preview
                    .loss(Hurt::Unit(i))
                    .is_some_and(|loss| loss.target_side)
        });
        Some(preview)
    }

    /// Adds what `a`'s attack on `aim` from `from` would do to `preview`,
    /// with everything's HP as `before` leaves it (the step's start).
    fn preview_attack(
        &self,
        a: usize,
        aim: Hex,
        from: Hex,
        before: &AttackPreview,
        preview: &mut AttackPreview,
        fog: &Fog,
    ) {
        let mut attacker = self.units[a].clone();
        attacker.pos = from;
        let volley = attacker.ability_queued && attacker.ability() == Ability::Volley;
        let (hexes, scale) = if volley {
            let mut hexes = vec![aim];
            hexes.extend(aim.neighbors());
            (hexes, VOLLEY_DAMAGE)
        } else {
            (vec![aim], 1.0)
        };
        let defenders: Vec<usize> = hexes
            .iter()
            .filter(|&&hex| fog.sees(hex))
            .filter_map(|&hex| self.enemy_of_team_at(hex, attacker.team))
            .filter(|&d| before.hp_left(Hurt::Unit(d), self.units[d].hp) > 0.0)
            .collect();
        // A structure is hit only when no enemy unit is.
        if defenders.is_empty() {
            let attack = attacker.stats().attack;
            let standing = |hurt, hp| before.hp_left(hurt, hp) > 0.0;
            if let Some(city) = self.enemy_barracks_at(aim, attacker.team)
                && standing(Hurt::Barracks(city), self.cities[city].barracks_hp)
            {
                let hit = scale * damage_against(attack, BARRACKS_DEFENSE);
                let hp = self.cities[city].barracks_hp;
                preview.add(Hurt::Barracks(city), hit, hp, true);
            } else if let Some(city) = self.enemy_coastal_battery_at(aim, attacker.team)
                && standing(Hurt::Battery(city), self.cities[city].coastal_battery_hp)
            {
                let hit = scale * damage_against(attack, COASTAL_BATTERY_DEFENSE);
                let hp = self.cities[city].coastal_battery_hp;
                preview.add(Hurt::Battery(city), hit, hp, true);
            }
            return;
        }
        let attacker_cover = self.defense_multiplier(&attacker);
        for d in defenders {
            let mut defender = self.units[d].clone();
            // Its ability is part of its plan, which the player can't know.
            if !self.is_player_controlled(d) {
                defender.ability_queued = false;
            }
            defender.hp = before.hp_left(Hurt::Unit(d), defender.hp);
            let hit = shore_scale(&attacker, &defender)
                * scale
                * damage(&attacker, &defender, self.defense_multiplier(&defender));
            preview.add(Hurt::Unit(d), hit, self.units[d].hp, true);
            if retaliates(&attacker, &defender, hit) {
                let back = shore_scale(&defender, &attacker)
                    * damage(&defender, &attacker, attacker_cover);
                preview.add(Hurt::Unit(a), back, self.units[a].hp, false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::orders::ClickMode;
    use crate::game::terrain::{Terrain, Tile};
    use crate::game::turn::RESOLUTION_ORDER;
    use crate::game::unit::Team;

    /// A game with only `units` on a small plain board, with `tiles` set.
    fn board(units: Vec<Unit>, tiles: &[(Hex, Tile)]) -> GameState {
        let mut game = GameState::new();
        game.grid = HexGrid::new(3, tiles.iter().copied());
        game.cities.clear();
        game.units = units;
        game.selected = None;
        game.group.clear();
        game
    }

    /// Resolves every unit step of the turn, in order, planning nothing.
    fn resolve_every_step(game: &mut GameState) {
        for (unit_type, phase) in RESOLUTION_ORDER {
            game.resolve_step(unit_type, phase);
        }
    }

    /// The HP each unit lost, by id, from `before`.
    fn hp_lost(game: &GameState, before: &[Unit]) -> Vec<(u32, f32)> {
        before
            .iter()
            .map(|old| {
                let now = game
                    .units
                    .iter()
                    .find(|u| u.id == old.id)
                    .map_or(0.0, |u| u.hp);
                (old.id, old.hp - now)
            })
            .collect()
    }

    /// What `preview` shows each unit losing, by id, in `before`'s order.
    fn previewed(game: &GameState, preview: &AttackPreview, before: &[Unit]) -> Vec<(u32, f32)> {
        before
            .iter()
            .map(|old| {
                let i = game.units.iter().position(|u| u.id == old.id).unwrap();
                let loss = preview.loss(Hurt::Unit(i));
                (old.id, loss.map_or(0.0, |loss| loss.amount.min(old.hp)))
            })
            .collect()
    }

    fn assert_close(a: &[(u32, f32)], b: &[(u32, f32)]) {
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b) {
            assert_eq!(x.0, y.0);
            assert!((x.1 - y.1).abs() < 1e-3, "{a:?} != {b:?}");
        }
    }

    #[test]
    fn the_preview_is_the_damage_the_turn_deals() {
        // A group of a ranged unit and a cavalry, and a melee already
        // attacking, on an armored unit on hills: three steps, two blows
        // back.
        let target = Hex::new(1, 0);
        let mut melee = Unit::new(3, Hex::new(1, -1), Team::Blue, UnitType::Melee);
        melee.planned_attack = Some(target);
        let units = vec![
            Unit::new(1, Hex::new(-1, 0), Team::Blue, UnitType::Ranged),
            Unit::new(2, Hex::new(0, 0), Team::Blue, UnitType::Cavalry),
            melee,
            Unit::new(4, target, Team::Red, UnitType::Armored),
        ];
        let mut game = board(units, &[(target, Tile::HILLS)]);
        game.set_selection(vec![0, 1]);
        game.hovered_tile = Some(target);
        let preview = game.attack_preview().expect("the group can attack it");
        assert!(preview.if_it_stays);
        assert!(preview.retaliation() > 0.0);
        let before = game.units.clone();
        let shown = previewed(&game, &preview, &before);

        game.group_order(target, ClickMode::Attack);
        resolve_every_step(&mut game);
        assert_close(&hp_lost(&game, &before), &shown);
    }

    #[test]
    fn a_lethal_hit_draws_no_retaliation() {
        let target = Hex::new(1, 0);
        let mut weak = Unit::new(2, target, Team::Red, UnitType::Melee);
        weak.hp = 5.0;
        let mut game = board(
            vec![
                Unit::new(1, Hex::new(0, 0), Team::Blue, UnitType::Melee),
                weak,
            ],
            &[],
        );
        game.selected = Some(0);
        game.hovered_tile = Some(target);
        let preview = game.attack_preview().unwrap();
        assert!(preview.loss(Hurt::Unit(1)).unwrap().lethal());
        assert!(preview.dealt().1);
        assert_eq!(preview.retaliation(), 0.0);
        assert!(preview.loss(Hurt::Unit(0)).is_none());

        let before = game.units.clone();
        game.try_queue_attack(0, target);
        resolve_every_step(&mut game);
        assert_eq!(game.units.len(), 1);
        assert_eq!(game.units[0].hp, before[0].hp);
    }

    #[test]
    fn only_melee_on_its_own_side_of_the_waterline_draws_retaliation() {
        let (shore, sea) = (Hex::new(1, 0), Hex::new(2, 0));
        let preview_of = |attacker: Unit, target: Hex| {
            let defender = Unit::new(9, shore, Team::Red, UnitType::Melee);
            let mut game = board(vec![attacker, defender], &[(sea, Terrain::Coast.into())]);
            game.selected = Some(0);
            game.hovered_tile = Some(target);
            game.attack_preview().unwrap()
        };
        let melee = preview_of(
            Unit::new(1, Hex::new(0, 0), Team::Blue, UnitType::Melee),
            shore,
        );
        assert!(
            melee
                .loss(Hurt::Unit(0))
                .is_some_and(|loss| !loss.target_side)
        );
        let ranged = preview_of(
            Unit::new(1, Hex::new(-1, 0), Team::Blue, UnitType::Ranged),
            shore,
        );
        assert_eq!(ranged.retaliation(), 0.0);
        // A galley's melee blow lands ashore at 35%, and nothing comes back
        // across the water.
        let galley = Unit::new(1, sea, Team::Blue, UnitType::PatrolGalley);
        let defender = Unit::new(9, shore, Team::Red, UnitType::Melee);
        let full = damage(&galley, &defender, 1.0);
        let preview = preview_of(galley, shore);
        assert_eq!(preview.retaliation(), 0.0);
        assert!((preview.dealt().0 - 0.35 * full).abs() < 1e-4);
    }

    #[test]
    fn the_preview_shows_only_what_the_player_knows() {
        let far = Hex::new(3, 0);
        let near = Hex::new(-2, 1);
        let mut shielded = Unit::new(3, near, Team::Red, UnitType::Melee);
        shielded.ability_queued = true;
        let mut blue = Unit::new(1, Hex::new(-3, 1), Team::Blue, UnitType::Melee);
        // An attack already planned on a hex out of sight shows nothing.
        blue.planned_attack = Some(far);
        let mut game = board(
            vec![
                blue,
                Unit::new(2, far, Team::Red, UnitType::Melee),
                shielded,
            ],
            &[],
        );
        game.fog_of_war = true;
        assert!(!game.fog().sees(far));
        game.hovered_tile = Some(far);
        assert_eq!(game.attack_preview(), None);

        // An enemy in sight: its planned Shield Wall isn't known, so the
        // preview assumes it does nothing, and says so.
        game.units[0].planned_attack = None;
        game.selected = Some(0);
        game.hovered_tile = Some(near);
        let preview = game.attack_preview().unwrap();
        assert!(preview.if_it_stays);
        let mut plain = game.units[2].clone();
        plain.ability_queued = false;
        let expected = damage(&game.units[0], &plain, 1.0);
        assert!(expected > damage(&game.units[0], &game.units[2], 1.0));
        assert_eq!(preview.loss(Hurt::Unit(2)).unwrap().amount, expected);
    }

    #[test]
    fn the_preview_of_a_barracks_is_its_damage() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        let city = game
            .cities
            .iter()
            .position(|c| c.team == Team::Red)
            .unwrap();
        let barracks = game.cities[city].pos.neighbors()[0];
        game.cities[city].barracks = Some(barracks);
        let from = barracks.neighbors()[0];
        game.units
            .push(Unit::new(901, from, Team::Blue, UnitType::Ranged));
        game.selected = Some(0);
        game.hovered_tile = Some(barracks);
        let preview = game.attack_preview().unwrap();
        assert!(!preview.if_it_stays);
        let loss = *preview.loss(Hurt::Barracks(city)).unwrap();
        let hp = game.cities[city].barracks_hp;

        game.try_queue_attack(0, barracks);
        resolve_every_step(&mut game);
        assert!((hp - game.cities[city].barracks_hp - loss.amount).abs() < 1e-3);
    }

    #[test]
    fn no_preview_without_an_attack_or_a_target() {
        let target = Hex::new(1, 0);
        let mut game = board(
            vec![
                Unit::new(1, Hex::new(-2, 0), Team::Blue, UnitType::Melee),
                Unit::new(2, target, Team::Red, UnitType::Melee),
            ],
            &[],
        );
        game.hovered_tile = Some(target);
        // Nothing selected, nothing planned.
        assert_eq!(game.attack_preview(), None);
        // Selected, but out of reach this turn.
        game.selected = Some(0);
        assert_eq!(game.attack_preview(), None);
        // Hovering an empty hex.
        game.units[0].pos = Hex::new(0, 0);
        game.hovered_tile = Some(Hex::new(0, 1));
        assert_eq!(game.attack_preview(), None);
        game.hovered_tile = Some(target);
        assert!(game.attack_preview().is_some());
    }
}
