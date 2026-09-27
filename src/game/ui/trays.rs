//! The bottom-left command tray: a unit, a group, a city or a Barracks.

use super::builder::{ButtonSpec, CatalogEntry, PanelBuilder};
use super::text::{ability_text, compare, quantity, signed_quantity, stat_spans, turns_text};
use super::{
    BODY, BOOSTED_TEXT, ButtonState, DIM_TEXT, GAP, GOLD_TEXT, LABEL_TEXT, QueueItemSpec,
    QueueKind, REDUCED_TEXT, SMALL, TEXT, TITLE, Target, UnitAction,
};
use crate::game::city::{
    Build, BuildUnit, Building, CORE_HP, LaborFocus, WORKER_COST, WORKER_SHORTCUT, delivered_share,
};
use crate::game::hex::Hex;
use crate::game::orders::ClickMode;
use crate::game::workers::{JobKind, WorkerJob};
use crate::game::{GameState, PLAYER_TEAM};

impl GameState {
    /// The tactical hex board inside a city. These fighters are independent
    /// copies of field troops on the six tiles neighboring the city.
    pub(super) fn interior_tray(&self, i: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        let interior = &city.interior;
        panel.text(
            TITLE,
            vec![(format!("CITY {} INTERIOR", city.id + 1), city.team.color())],
        );
        panel.text(
            BODY,
            vec![(
                if interior.core_hp <= 0.0 {
                    format!("POST BREACHED  0/{CORE_HP:.0} HP")
                } else {
                    format!("COMMAND POST  {:.0}/{:.0} HP", interior.core_hp, CORE_HP)
                },
                if interior.core_hp <= 0.0 {
                    REDUCED_TEXT
                } else {
                    GOLD_TEXT
                },
            )],
        );
        panel.bar(interior.core_hp / CORE_HP);
        if interior.core_hp <= 0.0 {
            panel.text(
                SMALL,
                vec![(
                    if city.team == PLAYER_TEAM {
                        "KEEP RED OFF THE CENTER TO PREVENT CAPTURE"
                    } else {
                        "MOVE A BLUE TROOP ONTO THE POST TO CAPTURE"
                    }
                    .into(),
                    BOOSTED_TEXT,
                )],
            );
        }
        let blue = interior
            .fighters
            .iter()
            .filter(|f| f.team == crate::game::PLAYER_TEAM)
            .count();
        let red = interior.fighters.len() - blue;
        panel.text(
            SMALL,
            vec![(format!("BLUE {blue} / RED {red} TROOPS"), DIM_TEXT)],
        );
        if let Some(fighter) = self
            .interior_selected
            .and_then(|source| interior.fighters.iter().find(|f| f.source_id == source))
        {
            panel.gap(GAP);
            panel.text(
                BODY,
                vec![(
                    format!("{:?}  {:.0} HP", fighter.unit_type, fighter.hp).to_uppercase(),
                    BOOSTED_TEXT,
                )],
            );
            panel.text(SMALL, vec![("GREEN: MOVE  RED: ATTACK".into(), LABEL_TEXT)]);
        } else {
            panel.text(
                SMALL,
                vec![("CLICK A BLUE TROOP ON THE MAP".into(), LABEL_TEXT)],
            );
        }
        panel.gap(GAP);
        panel.text(
            SMALL,
            vec![(
                if interior.core_hp <= 0.0 {
                    if city.team == PLAYER_TEAM {
                        "POST OPEN: DEFEND THE CENTER"
                    } else {
                        "POST OPEN: CLICK A BLUE TROOP, THEN THE CENTER"
                    }
                } else {
                    "BREACH POST, THEN OCCUPY CENTER"
                }
                .into(),
                GOLD_TEXT,
            )],
        );
        panel.compact_buttons(vec![
            ButtonSpec {
                target: Target::InteriorClear,
                label: "CLEAR ORDERS".into(),
                hint: "BACKSPACE".into(),
                state: ButtonState::new(false, self.interior_selected.is_none()),
                armed: false,
            },
            ButtonSpec {
                target: Target::OpenInterior,
                label: if city.team == crate::game::PLAYER_TEAM {
                    "RETURN TO CITY"
                } else {
                    "LEAVE INTERIOR"
                }
                .into(),
                hint: "V / ESC".into(),
                state: ButtonState::Ready,
                armed: false,
            },
        ]);
    }

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
        if unit.interior_hp < unit.max_hp() {
            panel.text(
                SMALL,
                vec![(
                    format!(
                        "INTERIOR HP {:.0}/{:.0}",
                        unit.interior_hp.ceil(),
                        unit.max_hp()
                    ),
                    REDUCED_TEXT,
                )],
            );
        }
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
        if !self.settlers.contains(&unit.id) {
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
        if unit.unit_type == crate::game::unit::UnitType::LandingCraft {
            notes.push(format!(
                "CARGO {}/4 - CLICK ADJACENT LAND TO UNLOAD",
                unit.cargo.len()
            ));
        } else if !unit.is_naval() {
            notes.push("CLICK AN ADJACENT LANDING CRAFT TO BOARD".to_string());
        }
        if unit.lookout {
            notes.push(format!(
                "LOOKOUT: +{} SIGHT",
                crate::game::fog::LOOKOUT_SIGHT
            ));
        }
        match unit.training_upgrade {
            Some(crate::game::terrain::Resource::Iron) => {
                notes.push("FORGED ARMOR: +20% HP, +15% DEFENSE".to_string());
            }
            Some(crate::game::terrain::Resource::Horses) => {
                notes.push("STABLE TRAINING: +1 MOVE".to_string());
            }
            None => {}
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
            if unit.has_queue() {
                notes.push(format!(
                    "QUEUED FOR {} - ANY OTHER ORDER CANCELS",
                    turns_text(unit.plan_len() as u32)
                ));
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
        let settler = self.settlers.contains(&unit.id);
        let armed = |mode| self.selected == Some(idx) && self.ui_click_mode == Some(mode);

        let mut buttons = vec![ButtonSpec {
            target: Target::Unit(UnitAction::Move),
            label: "MOVE".into(),
            hint: "M".into(),
            state: ButtonState::new(unit.planned_move.is_some() && !swapping, !can_move),
            armed: armed(ClickMode::Move),
        }];
        buttons.push(ButtonSpec {
            target: Target::Unit(UnitAction::Attack),
            label: "ATTACK".into(),
            hint: "X · RMB".into(),
            state: ButtonState::new(unit.planned_attack.is_some(), !unit.can_attack() || locked),
            armed: armed(ClickMode::Attack),
        });
        buttons.push(ButtonSpec {
            target: Target::Unit(UnitAction::Swap),
            label: "SWAP".into(),
            hint: "CTRL".into(),
            state: ButtonState::new(swapping, !can_move || locked),
            armed: armed(ClickMode::Swap),
        });
        if settler {
            buttons.push(ButtonSpec::plain(UnitAction::Settle, "FOUND CITY", "F"));
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
        buttons.push(ButtonSpec {
            target: Target::Unit(UnitAction::ClearOrders),
            label: "CLEAR ORDERS".into(),
            hint: "CTRL-RMB".into(),
            state: ButtonState::new(false, !unit.has_orders()),
            armed: false,
        });
        let confirming = self.disband_armed == Some(unit.id);
        buttons.push(ButtonSpec {
            target: Target::Unit(UnitAction::Disband),
            label: if confirming { "CONFIRM?" } else { "DISBAND" }.into(),
            hint: "DEL".into(),
            state: ButtonState::Ready,
            armed: confirming,
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
            "RIGHT-CLICK A HEX: EVERY UNIT IN RANGE ATTACKS IT",
            "SHIFT: QUEUE THE MOVE OR ATTACK FOR LATER TURNS FOR ALL",
            "CLICK ONE UNIT TO SELECT JUST IT · SHIFT-CLICK ADDS · CTRL-CLICK REMOVES",
        ] {
            panel.text(SMALL, vec![(help.into(), LABEL_TEXT)]);
        }
        let armed = |mode| self.group == group && self.ui_click_mode == Some(mode);
        let all_guarding = group.iter().all(|&i| self.units[i].guarding);
        let any_orders = group.iter().any(|&i| self.units[i].has_orders());
        panel.action_toolbar(vec![
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
                hint: "X · RMB".into(),
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
            ButtonSpec {
                target: Target::Unit(UnitAction::ClearOrders),
                label: "CLEAR ORDERS".into(),
                hint: "CTRL-RMB".into(),
                state: ButtonState::new(false, !any_orders),
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
        let builds: Vec<BuildUnit> =
            if self.city_is_coastal(i) && city.placed_site(Building::Harbor).is_some() {
                vec![
                    BuildUnit::Melee,
                    BuildUnit::Ranged,
                    BuildUnit::Siege,
                    BuildUnit::PatrolGalley,
                    BuildUnit::LandingCraft,
                    BuildUnit::BombardShip,
                ]
            } else {
                vec![BuildUnit::Melee, BuildUnit::Ranged, BuildUnit::Siege]
            };
        let unit_buttons: Vec<_> = builds
            .into_iter()
            .map(|build| ButtonSpec {
                target: Target::Build(build),
                label: build.name().into(),
                hint: format!("{} | {} PROD", build.shortcut(), quantity(build.cost())),
                state: ButtonState::new(city.queue.first() == Some(&Build::Unit(build)), false),
                armed: false,
            })
            .chain([ButtonSpec {
                target: Target::BuildWorker,
                label: "WORKER".into(),
                hint: format!("{WORKER_SHORTCUT} | {} PROD", quantity(WORKER_COST)),
                state: ButtonState::new(city.queue.first() == Some(&Build::Worker), false),
                armed: false,
            }])
            .collect();
        panel.buttons(vec![ButtonSpec {
            target: Target::OpenInterior,
            label: "CITY INTERIOR".into(),
            hint: "V".into(),
            state: ButtonState::Ready,
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
        let building_buttons: Vec<_> = Building::ALL
            .iter()
            .copied()
            .filter(|&building| !city.built.contains(&building))
            .filter(|&building| {
                !matches!(building, Building::Harbor | Building::CoastalBattery)
                    || self.city_is_coastal(i)
            })
            .map(|building| ButtonSpec {
                target: Target::Building(building),
                label: building.name().into(),
                hint: if building.shortcut() == ' ' {
                    format!("{} PROD", quantity(building.cost()))
                } else {
                    format!(
                        "{} | {} PROD",
                        building.shortcut(),
                        quantity(building.cost())
                    )
                },
                state: ButtonState::new(
                    city.queue.first() == Some(&Build::Building(building))
                        || self.needs_site(i, building)
                        || self.site_placement() == Some((i, building)),
                    (city.pending_building == Some(building)
                        || city.queue.contains(&Build::Building(building)))
                        && !self.needs_site(i, building),
                ),
                armed: false,
            })
            .collect();
        let mut production_entries =
            Vec::with_capacity(unit_buttons.len() + building_buttons.len() + 2);
        production_entries.push(CatalogEntry::Heading("UNITS"));
        production_entries.extend(unit_buttons.into_iter().map(CatalogEntry::Card));
        production_entries.push(CatalogEntry::Heading("BUILDINGS"));
        production_entries.extend(building_buttons.into_iter().map(CatalogEntry::Card));
        panel.building_catalog(i, production_entries, city.building_scroll);
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
        for building in Building::PLACEABLE {
            let Some(site) = city.planned_sites.get(&building) else {
                if self.needs_site(i, building) {
                    let how = if self.site_placement() == Some((i, building)) {
                        "CLICK AN OPEN TILE ON THE MAP"
                    } else {
                        "CLICK ITS CARD TO CHOOSE ONE"
                    };
                    panel.text(
                        SMALL,
                        vec![(format!("{} NEEDS A SITE", building.name()), GOLD_TEXT)],
                    );
                    panel.text(SMALL, vec![(how.into(), DIM_TEXT)]);
                }
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
        self.city_workers(i, panel);
    }

    /// The city's workers: how many are home and out, what those out are
    /// doing, and the jobs waiting for them, which can be dragged into a new
    /// order or removed.
    fn city_workers(&self, i: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        let out: Vec<_> = self.field_workers.iter().filter(|w| w.home == i).collect();
        let mut line = vec![(
            format!("WORKERS: {} HOME, {} OUT", city.workers, out.len()),
            LABEL_TEXT,
        )];
        if city.worker_jobs.is_empty() {
            line.push((" · CLOSE THE CITY, CLICK A TILE FOR JOBS".into(), DIM_TEXT));
        }
        panel.text(SMALL, line);
        for worker in out {
            let doing = match (worker.job, worker.work_left) {
                (Some(job), Some(left)) => format!(
                    "{} AT ({}, {}): {} LEFT",
                    job.kind.name(),
                    job.hex.q,
                    job.hex.r,
                    turns_text(left)
                ),
                (Some(job), None) => format!(
                    "WALKING TO A {} AT ({}, {})",
                    job.kind.name(),
                    job.hex.q,
                    job.hex.r
                ),
                (None, _) if worker.recalled => "RECALLED, WALKING HOME".into(),
                (None, _) => "WALKING HOME".into(),
            };
            if worker.recalled {
                panel.text(SMALL, vec![(format!("  {doing}"), DIM_TEXT)]);
                continue;
            }
            panel.compact_buttons(vec![ButtonSpec {
                target: Target::RecallWorker(worker.id),
                label: doing,
                hint: "RECALL".into(),
                state: ButtonState::Ready,
                armed: false,
            }]);
        }
        if city.worker_jobs.is_empty() {
            return;
        }
        panel.text(
            SMALL,
            vec![("WORKER JOBS - DRAG TO REORDER".into(), LABEL_TEXT)],
        );
        let drag = self
            .queue_drag
            .filter(|drag| drag.kind == QueueKind::Workers);
        for (index, job) in city.worker_jobs.iter().enumerate() {
            panel.queue_item(QueueItemSpec {
                kind: QueueKind::Workers,
                index,
                label: format!(
                    "{} AT ({}, {}) | {} OF WORK",
                    job.kind.name(),
                    job.hex.q,
                    job.hex.r,
                    turns_text(job.kind.turns())
                ),
                active: false,
                dragging: drag.is_some_and(|drag| drag.source == index),
                drop_target: drag
                    .is_some_and(|drag| drag.target == Some(index) && drag.source != index),
                locked: false,
            });
        }
    }

    /// A tile clicked with nothing selected: what it is, and the jobs the
    /// nearest city's workers can do there.
    pub(super) fn tile_tray(&self, hex: Hex, panel: &mut PanelBuilder) {
        let tile = self.grid.tile(hex);
        let mut name = tile.terrain.name().to_string();
        if tile.hills {
            name.push_str(" HILLS");
        }
        if let Some(feature) = tile.feature {
            name = format!("{} {name}", feature.name());
        }
        panel.text(
            TITLE,
            vec![
                ("TILE ".into(), LABEL_TEXT),
                (format!("({}, {})  ", hex.q, hex.r), TEXT),
                (name, DIM_TEXT),
            ],
        );
        let from = match self.job_city(hex) {
            Some(city) => {
                let c = &self.cities[city];
                format!(
                    "WORKERS FROM CITY {}: {} AT HOME, {} OUT, {} JOBS WAITING",
                    c.id + 1,
                    c.workers,
                    self.workers_out(city),
                    c.worker_jobs.len()
                )
            }
            None => "FOUND A CITY TO GET WORKERS".into(),
        };
        panel.text(SMALL, vec![(from, DIM_TEXT)]);
        // Your workers standing here can be sent home from the tile.
        let here: Vec<_> = self
            .field_workers
            .iter()
            .filter(|w| w.pos == hex && w.team == PLAYER_TEAM && !w.recalled)
            .collect();
        if !here.is_empty() {
            panel.buttons(
                here.iter()
                    .map(|worker| ButtonSpec {
                        target: Target::RecallWorker(worker.id),
                        label: "RECALL WORKER".into(),
                        hint: "SEND IT HOME".into(),
                        state: ButtonState::Ready,
                        armed: false,
                    })
                    .collect(),
            );
        }
        panel.text(
            SMALL,
            vec![("WORKER JOBS · ESC TO CLOSE".into(), LABEL_TEXT)],
        );
        for row in JobKind::ALL.chunks(3) {
            panel.buttons(
                row.iter()
                    .map(|&kind| {
                        let taken = self.job_taken(PLAYER_TEAM, WorkerJob::on_tile(hex, kind));
                        let key = match kind {
                            JobKind::Road => "R · ",
                            JobKind::Improve => "I · ",
                            JobKind::Wall | JobKind::Gate => "EDGES · ",
                            _ => "",
                        };
                        ButtonSpec {
                            target: Target::WorkerJob(kind),
                            label: kind.name().into(),
                            hint: format!("{key}{}", turns_text(kind.turns())),
                            state: ButtonState::new(
                                taken,
                                !taken && self.job_unavailable(hex, kind).is_some(),
                            ),
                            armed: self.placing_barrier == Some(kind),
                        }
                    })
                    .collect(),
            );
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
