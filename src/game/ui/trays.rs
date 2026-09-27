//! The bottom-left command tray: a unit, a group, a city or a Barracks.

use super::builder::{ButtonSpec, PanelBuilder};
use super::text::{ability_text, compare, quantity, signed_quantity, stat_spans, turns_text};
use super::{
    BODY, BOOSTED_TEXT, ButtonState, DIM_TEXT, GAP, GOLD_TEXT, LABEL_TEXT, REDUCED_TEXT, SMALL,
    TEXT, TITLE, Target, UnitAction,
};
use crate::game::GameState;
use crate::game::city::{Build, BuildUnit, Building, LaborFocus, delivered_share};
use crate::game::orders::ClickMode;

impl GameState {
    /// A unit's name, stats as they stand this turn, and anything notable
    /// about it. Stats boosted above their base value are green, reduced red.
    pub(super) fn unit_info(&self, idx: usize, panel: &mut PanelBuilder) {
        let unit = &self.units[idx];
        let base = unit.unit_type.stats();
        let stats = unit.stats();
        let terrain = self.grid.tile(unit.pos);
        let defense = stats.defense * terrain.defense_multiplier();
        let role = self.unit_role(unit);

        panel.text(
            TITLE,
            vec![(
                format!("{:?} {role}", unit.team).to_uppercase(),
                unit.team.color(),
            )],
        );
        let hp_color = if unit.hp < unit.max_hp() * 0.5 {
            REDUCED_TEXT
        } else {
            TEXT
        };
        panel.text(
            BODY,
            stat_spans(&[
                (
                    "HP",
                    format!("{:.0}/{:.0}", unit.hp.ceil(), unit.max_hp()),
                    hp_color,
                ),
                (
                    "ATTACK",
                    format!("{:.0}", stats.attack),
                    compare(stats.attack, base.attack),
                ),
                (
                    "DEFENSE",
                    format!("{defense:.0}"),
                    compare(defense, base.defense),
                ),
            ]),
        );
        let (ability_name, _) = ability_text(unit);
        let (ability_status, ability_color) = if unit.ability_queued {
            ("QUEUED".to_string(), GOLD_TEXT)
        } else if unit.ability_cooldown > 0 {
            (
                format!("IN {}", turns_text(unit.ability_cooldown)),
                DIM_TEXT,
            )
        } else {
            ("READY".to_string(), TEXT)
        };
        let mut second = stat_spans(&[
            (
                "RANGE",
                stats.attack_range.to_string(),
                compare(stats.attack_range as f32, base.attack_range as f32),
            ),
            (
                "MOVE",
                stats.move_range.to_string(),
                compare(stats.move_range as f32, base.move_range as f32),
            ),
        ]);
        if !self.settlers.contains(&unit.id) && !self.workers.contains(&unit.id) {
            second.push((format!("   {ability_name} "), LABEL_TEXT));
            second.push((ability_status, ability_color));
        }
        panel.text(BODY, second);

        let mut notes = Vec::new();
        if terrain.defense_multiplier() != 1.0 {
            let bonus = (terrain.defense_multiplier() - 1.0) * 100.0;
            notes.push(format!("+{bonus:.0}% DEFENSE FROM TERRAIN"));
        }
        if unit.deployed {
            notes.push("DEPLOYED".to_string());
        }
        if unit.lookout {
            notes.push(format!(
                "LOOKOUT: +{} SIGHT",
                crate::game::fog::LOOKOUT_SIGHT
            ));
        }
        if self.rival_of(idx).is_some() {
            notes.push("CONTESTED".to_string());
        }
        if self.is_player_controlled(idx) {
            let mut orders = Vec::new();
            if unit.planned_move.is_some() {
                orders.push("MOVE");
            }
            if unit.planned_attack.is_some() {
                orders.push("ATTACK");
            }
            if unit.holding {
                orders.push("HOLD");
            }
            if unit.guarding {
                orders.push("GUARD");
            }
            if !orders.is_empty() {
                notes.push(format!("ORDERS: {}", orders.join(", ")));
            }
        }
        for note in notes {
            panel.text(SMALL, vec![(note, DIM_TEXT)]);
        }
    }

    /// The selected unit's order buttons: what it can do depends on whether
    /// it's a fighter, a settler or a worker.
    pub(super) fn unit_buttons(&self, idx: usize) -> Vec<ButtonSpec> {
        let unit = &self.units[idx];
        let can_move = unit.stats().move_range > 0;
        let locked = self.rival_of(idx).is_some();
        let swapping = self.swap_partner(idx).is_some();
        let worker = self.workers.contains(&unit.id);
        let settler = self.settlers.contains(&unit.id);
        let armed = |mode| self.selected == Some(idx) && self.ui_click_mode == Some(mode);

        let mut buttons = vec![ButtonSpec {
            target: Target::Unit(UnitAction::Move),
            label: "MOVE".into(),
            hint: "M".into(),
            state: ButtonState::new(unit.planned_move.is_some() && !swapping, !can_move),
            armed: armed(ClickMode::Move),
        }];
        if !worker {
            buttons.push(ButtonSpec {
                target: Target::Unit(UnitAction::Attack),
                label: "ATTACK".into(),
                hint: "X · SHIFT".into(),
                state: ButtonState::new(
                    unit.planned_attack.is_some(),
                    !unit.can_attack() || locked,
                ),
                armed: armed(ClickMode::Attack),
            });
        }
        buttons.push(ButtonSpec {
            target: Target::Unit(UnitAction::Swap),
            label: "SWAP".into(),
            hint: "CTRL".into(),
            state: ButtonState::new(swapping, !can_move || locked),
            armed: armed(ClickMode::Swap),
        });
        if settler {
            buttons.push(ButtonSpec::plain(UnitAction::Settle, "FOUND CITY", "F"));
        } else if worker {
            buttons.push(ButtonSpec::plain(UnitAction::Road, "BUILD ROAD", "R"));
            buttons.push(ButtonSpec::plain(UnitAction::Improve, "IMPROVE", "I"));
        } else {
            let (name, _) = ability_text(unit);
            let label = match unit.ability_cooldown {
                0 => name.to_string(),
                turns => format!("{name} ({turns})"),
            };
            buttons.push(ButtonSpec {
                target: Target::Unit(UnitAction::Ability),
                label,
                hint: "Q".into(),
                state: ButtonState::new(unit.ability_queued, unit.ability_cooldown > 0),
                armed: false,
            });
        }
        buttons.push(ButtonSpec {
            target: Target::Unit(UnitAction::Hold),
            label: "HOLD".into(),
            hint: "SPACE".into(),
            state: ButtonState::new(unit.holding, false),
            armed: false,
        });
        buttons.push(ButtonSpec {
            target: Target::Unit(UnitAction::Guard),
            label: "GUARD".into(),
            hint: "G".into(),
            state: ButtonState::new(unit.guarding, false),
            armed: false,
        });
        buttons
    }

    /// A group of selected units: how many of each, how to order them, and
    /// buttons for what every member can do at once.
    pub(super) fn group_tray(&self, panel: &mut PanelBuilder) {
        self.group_tray_for(&self.group, panel);
    }

    pub(super) fn group_tray_for(&self, group: &[usize], panel: &mut PanelBuilder) {
        let members: Vec<&str> = group
            .iter()
            .map(|&i| self.unit_role(&self.units[i]))
            .collect();
        let mut kinds: Vec<(&str, usize)> = Vec::new();
        for role in members {
            match kinds.iter_mut().find(|(kind, _)| *kind == role) {
                Some((_, count)) => *count += 1,
                None => kinds.push((role, 1)),
            }
        }
        let summary: Vec<String> = kinds
            .into_iter()
            .map(|(kind, count)| format!("{count} {kind}"))
            .collect();

        panel.text(
            TITLE,
            vec![(format!("{} UNITS SELECTED", group.len()), TEXT)],
        );
        panel.text(BODY, vec![(summary.join(", "), DIM_TEXT)]);
        for help in [
            "CLICK A HEX: EACH MOVES AS CLOSE TO IT AS IT CAN",
            "CLICK AN ENEMY: EVERY UNIT IN RANGE ATTACKS IT",
            "CLICK ONE UNIT TO SELECT JUST IT · ALT-CLICK ADDS OR REMOVES",
        ] {
            panel.text(SMALL, vec![(help.into(), LABEL_TEXT)]);
        }
        panel.gap(GAP);

        let armed = |mode| self.group == group && self.ui_click_mode == Some(mode);
        let all_guarding = group.iter().all(|&i| self.units[i].guarding);
        panel.buttons(vec![
            ButtonSpec {
                target: Target::Unit(UnitAction::Move),
                label: "MOVE".into(),
                hint: "M".into(),
                state: ButtonState::Ready,
                armed: armed(ClickMode::Move),
            },
            ButtonSpec {
                target: Target::Unit(UnitAction::Attack),
                label: "ATTACK".into(),
                hint: "X · SHIFT".into(),
                state: ButtonState::Ready,
                armed: armed(ClickMode::Attack),
            },
            ButtonSpec::plain(UnitAction::Hold, "HOLD", "SPACE"),
            ButtonSpec {
                target: Target::Unit(UnitAction::Guard),
                label: "GUARD".into(),
                hint: "G".into(),
                state: ButtonState::new(all_guarding, false),
                armed: false,
            },
        ]);
    }

    pub(super) fn city_tray(&self, i: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        let (food, production) = self.income(i);
        let net_food = food - city.population as i32 * 8;
        let (growth_percent, _, growth_label) = self.growth_status(i);

        panel.text(
            TITLE,
            vec![
                (format!("CITY {}", city.id + 1), city.team.color()),
                (
                    format!(
                        "   POPULATION {}/{}",
                        city.population,
                        crate::game::city::MAX_CITY_POPULATION
                    ),
                    DIM_TEXT,
                ),
            ],
        );
        let net_color = match net_food.signum() {
            1 => BOOSTED_TEXT,
            -1 => REDUCED_TEXT,
            _ => TEXT,
        };
        panel.text(
            BODY,
            stat_spans(&[(
                "CITIZENS",
                format!(
                    "{} OF {} WORKING",
                    city.worked.len(),
                    city.population.min(crate::game::city::MAX_CITY_POPULATION)
                ),
                TEXT,
            )]),
        );
        let mut food_line = stat_spans(&[("FOOD", quantity(city.food), TEXT)]);
        food_line.push((
            format!("   {} PER TURN", signed_quantity(net_food)),
            net_color,
        ));
        panel.text(BODY, food_line);
        let mut production_line = stat_spans(&[("PRODUCTION", quantity(city.production), TEXT)]);
        production_line.push((
            format!("   {} PER TURN", signed_quantity(production)),
            DIM_TEXT,
        ));
        panel.text(BODY, production_line);
        if let Some(build) = city.queue.first().copied() {
            let cost = self.city_build_cost(i, build);
            panel.text(
                SMALL,
                vec![(
                    format!(
                        "{}: {} / {} PRODUCTION",
                        build.name(),
                        quantity(city.production.min(cost)),
                        quantity(cost)
                    ),
                    GOLD_TEXT,
                )],
            );
            panel.bar((city.production as f32 / cost as f32).clamp(0.0, 1.0));
        }
        let building = match city.queue.first().copied() {
            Some(build) => (
                format!(
                    "{} ({} OF {})",
                    build.name(),
                    quantity(city.production.min(self.city_build_cost(i, build))),
                    quantity(self.city_build_cost(i, build))
                ),
                GOLD_TEXT,
            ),
            None => ("NOTHING - CHOOSE BELOW".to_string(), DIM_TEXT),
        };
        panel.text(BODY, stat_spans(&[("BUILDING", building.0, building.1)]));
        panel.text(SMALL, vec![("LABOR FOCUS".into(), LABEL_TEXT)]);
        panel.compact_buttons(
            [
                LaborFocus::Food,
                LaborFocus::Production,
                LaborFocus::Balanced,
            ]
            .into_iter()
            .map(|focus| ButtonSpec {
                target: Target::Focus(focus),
                label: focus.name().into(),
                hint: "AUTO".into(),
                state: ButtonState::new(city.focus == focus, false),
                armed: false,
            })
            .collect(),
        );

        panel.gap(GAP);
        panel.text(SMALL, vec![(growth_label, DIM_TEXT)]);
        panel.bar(growth_percent as f32 / 100.0);

        if let Some(hex) = self
            .inspected_tile
            .filter(|_| self.selected_city == Some(i))
        {
            let fog = self.fog();
            let (tile_food, tile_production) = self.known_yield(hex, &fog);
            let shares = self
                .known_routes(i, &fog)
                .costs
                .get(&hex)
                .map_or((0, 0), |cost| {
                    (self.mill_food_share(i, hex, *cost), delivered_share(*cost))
                });
            panel.gap(GAP);
            panel.text(
                SMALL,
                vec![(
                    format!(
                        "SELECTED TILE: {tile_food} FOOD, {tile_production} PRODUCTION · FOOD {}% / PROD {}%",
                        shares.0 * 25, shares.1 * 25
                    ),
                    DIM_TEXT,
                )],
            );
        }

        panel.gap(GAP);
        let builds = [BuildUnit::Melee, BuildUnit::Ranged, BuildUnit::Siege];
        panel.buttons(
            builds
                .into_iter()
                .map(|build| ButtonSpec {
                    target: Target::Build(build),
                    label: build.name().into(),
                    hint: format!("{} · {} PROD", build.shortcut(), quantity(build.cost())),
                    state: ButtonState::new(city.queue.first() == Some(&Build::Unit(build)), false),
                    armed: false,
                })
                .collect(),
        );
        panel.gap(GAP);
        panel.buttons(vec![ButtonSpec {
            target: Target::ToggleYields,
            label: "YIELDS".into(),
            hint: "Y".into(),
            state: ButtonState::new(self.show_yields, false),
            armed: false,
        }]);
        panel.gap(GAP);
        panel.text(
            SMALL,
            vec![(
                "CLICK MANAGER TO MOVE · CLICK TILES TO ASSIGN · ESC OR SPACE TO EXIT".into(),
                LABEL_TEXT,
            )],
        );
        let buildings = [
            Building::Granary,
            Building::Barracks,
            Building::Mill,
            Building::Workshop,
        ];
        panel.buttons(
            buildings
                .into_iter()
                .filter(|&building| !city.built.contains(&building))
                .map(|building| ButtonSpec {
                    target: Target::Building(building),
                    label: building.name().into(),
                    hint: format!(
                        "{} · {} PROD",
                        building.shortcut(),
                        quantity(building.cost())
                    ),
                    state: ButtonState::new(
                        city.queue.first() == Some(&Build::Building(building))
                            || self.placing_building == Some((i, building)),
                        city.pending_building == Some(building)
                            || city.queue.contains(&Build::Building(building)),
                    ),
                    armed: false,
                })
                .collect(),
        );
        if let Some(tile) = city.barracks {
            let active = city.worked.first() == Some(&tile);
            let status = if active {
                "MANAGER ACTIVE"
            } else {
                "MOVE MANAGER ONTO BARRACKS"
            };
            panel.gap(GAP);
            panel.text(
                SMALL,
                vec![(
                    format!("BARRACKS: {status}"),
                    if active { BOOSTED_TEXT } else { REDUCED_TEXT },
                )],
            );
            panel.buttons(vec![ButtonSpec {
                target: Target::OpenBarracks,
                label: "SEE BARRACKS".into(),
                hint: "CLICK".into(),
                state: ButtonState::new(false, false),
                armed: false,
            }]);
        }
        for building in [Building::Barracks, Building::Mill, Building::Workshop] {
            let Some(site) = city.planned_sites.get(&building) else {
                continue;
            };
            let ready = city.queue.first() == Some(&Build::Building(building))
                && city.production >= self.city_build_cost(i, Build::Building(building));
            let status = if ready { "READY" } else { "PLANNED" };
            panel.text(
                SMALL,
                vec![(
                    format!(
                        "{} {status}: SITE ({}, {})",
                        building.name(),
                        site.q,
                        site.r
                    ),
                    GOLD_TEXT,
                )],
            );
            panel.text(
                SMALL,
                vec![("CLICK ITS MAP BADGE TO MOVE THE SITE".into(), DIM_TEXT)],
            );
            if ready {
                panel.buttons(vec![ButtonSpec {
                    target: Target::ConfirmBuilding(building),
                    label: format!("CONFIRM {}", building.name()),
                    hint: "CLICK".into(),
                    state: ButtonState::Ready,
                    armed: false,
                }]);
            }
        }
    }

    pub(super) fn barracks_tray(&self, i: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        let Some(tile) = city.barracks else {
            return;
        };
        let active = city.worked.first() == Some(&tile);
        let barracks_production = if active { self.barracks_income(i) } else { 0 };
        panel.text(
            TITLE,
            vec![(format!("CITY {} BARRACKS", city.id + 1), city.team.color())],
        );
        panel.text(
            BODY,
            stat_spans(&[(
                "HP",
                format!(
                    "{:.0}/{:.0}",
                    city.barracks_hp,
                    crate::game::city::BARRACKS_MAX_HP
                ),
                GOLD_TEXT,
            )]),
        );
        panel.text(
            SMALL,
            vec![(
                format!(
                    "{} · {} PROD/T",
                    if active {
                        "MANAGER ACTIVE"
                    } else {
                        "NEEDS MANAGER"
                    },
                    signed_quantity(barracks_production)
                ),
                if active { BOOSTED_TEXT } else { REDUCED_TEXT },
            )],
        );
        if let Some(build) = city.barracks_queue.first().copied() {
            panel.text(
                SMALL,
                vec![(
                    format!(
                        "TRAINING {}: {} / {} PROD",
                        build.name(),
                        quantity(city.barracks_production.min(build.cost())),
                        quantity(build.cost())
                    ),
                    GOLD_TEXT,
                )],
            );
            panel.bar((city.barracks_production as f32 / build.cost() as f32).clamp(0.0, 1.0));
        }
        let builds = [
            BuildUnit::Melee,
            BuildUnit::Ranged,
            BuildUnit::Cavalry,
            BuildUnit::Siege,
            BuildUnit::Armored,
        ];
        panel.gap(GAP);
        panel.buttons(
            builds
                .into_iter()
                .map(|build| ButtonSpec {
                    target: Target::BarracksBuild(build),
                    label: format!("TRAIN {}", build.name()),
                    hint: build.required_resource().map_or_else(
                        || format!("{} PROD", quantity(build.cost())),
                        |resource| format!("{} · {}", resource.name(), quantity(build.cost())),
                    ),
                    state: ButtonState::new(
                        city.barracks_queue.first() == Some(&build),
                        !self.barracks_can_train(i, build),
                    ),
                    armed: false,
                })
                .collect(),
        );
        panel.buttons(vec![ButtonSpec {
            target: Target::OpenCity,
            label: "OPEN CITY".into(),
            hint: "CLICK".into(),
            state: ButtonState::new(false, false),
            armed: false,
        }]);
    }
}
