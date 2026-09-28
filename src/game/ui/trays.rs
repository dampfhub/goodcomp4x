//! The bottom-left command tray: a unit, a group, a city or a Barracks.

use super::builder::{ButtonSpec, PanelBuilder};
use super::text::{
    ability_text, compare, cost_hint, quantity, resource_color, signed_quantity, stat_spans,
    turns_text,
};
use super::{
    BODY, BOOSTED_TEXT, ButtonState, DIM_TEXT, GAP, GOLD_TEXT, LABEL_TEXT, QueueItemSpec,
    QueueKind, REDUCED_TEXT, SMALL, TEXT, TITLE, Target, UnitAction,
};
use crate::game::city::{
    Build, BuildUnit, Building, CITY_TRAINING_SLOWDOWN, CORE_HP, FOOD_PER_CITIZEN, GROW_SHORTCUT,
    LaborFocus, MAX_CITY_POPULATION, UNITS_PER_DEPOSIT, resource_icon,
};
use crate::game::orders::ClickMode;
use crate::game::terrain::Resource;
use crate::game::workers::JobKind;
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
        panel.gap(GAP);

        let armed = |mode| self.group == group && self.ui_click_mode == Some(mode);
        let all_guarding = group.iter().all(|&i| self.units[i].guarding);
        let any_orders = group.iter().any(|&i| self.units[i].has_orders());
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
        let income = self.income(i);
        let upkeep = city.population as i32 * FOOD_PER_CITIZEN;
        let stock = self.stock(city.team);

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
        panel.text(
            BODY,
            stat_spans(&[(
                "CITIZENS",
                format!(
                    "{} OF {} WORKING",
                    city.worked.len(),
                    city.population.min(MAX_CITY_POPULATION)
                ),
                TEXT,
            )]),
        );
        // What this city adds to the side's stockpile, and what its
        // citizens eat from it.
        let mut income_line = vec![("DELIVERS ".to_string(), LABEL_TEXT)];
        for (name, amount) in income.parts() {
            income_line.push((
                format!("{}{}   ", resource_icon(name), signed_quantity(amount)),
                resource_color(name),
            ));
        }
        income_line.push((
            format!("EATS {}{}", resource_icon("FOOD"), quantity(upkeep)),
            DIM_TEXT,
        ));
        panel.text(BODY, income_line);
        let building = match (city.queue.first().copied(), self.turns_left(i)) {
            (Some(_), Some(0)) => ("READY - CONFIRM ITS SITE".to_string(), GOLD_TEXT),
            (Some(build), Some(turns)) => (
                format!("{} · {} LEFT", build.name(), turns_text(turns as u32)),
                GOLD_TEXT,
            ),
            _ => ("NOTHING - CHOOSE BELOW".to_string(), DIM_TEXT),
        };
        panel.text(BODY, stat_spans(&[("BUILDING", building.0, building.1)]));
        if let Some(build) = city.queue.first().copied() {
            let work = self.city_build_work(i, build);
            panel.bar((city.progress as f32 / work as f32).clamp(0.0, 1.0));
        }
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
        // A build card's hint is its price and turns; its key is in its
        // tooltip, to keep the cards narrow.
        let card = |target, label: String, build: Build, head: bool| {
            let price = self.queue_price(i, build);
            ButtonSpec {
                target,
                label,
                hint: cost_hint(price, self.city_build_turns(i, build)),
                state: ButtonState::new(head, !stock.covers(price)),
                armed: false,
            }
        };
        let queued_grows = city.queue.iter().filter(|&&b| b == Build::Grow).count();
        let grow = if self.can_grow(i) {
            // One line, so there's room for its key.
            let grow = card(
                Target::Grow,
                format!("GROW TO {}", city.population + queued_grows + 1),
                Build::Grow,
                city.queue.first() == Some(&Build::Grow),
            );
            ButtonSpec {
                hint: format!("{GROW_SHORTCUT} · {}", grow.hint),
                ..grow
            }
        } else {
            ButtonSpec {
                target: Target::Grow,
                label: "GROW".into(),
                hint: format!("{GROW_SHORTCUT} · FULL"),
                state: ButtonState::Disabled,
                armed: false,
            }
        };
        panel.compact_buttons(vec![grow]);

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
        panel.text(
            SMALL,
            vec![(
                format!("A BARRACKS TRAINS TROOPS {CITY_TRAINING_SLOWDOWN}× FASTER THAN THE CITY"),
                DIM_TEXT,
            )],
        );
        panel.buttons(
            builds
                .into_iter()
                .map(|build| {
                    card(
                        Target::Build(build),
                        build.name().into(),
                        Build::Unit(build),
                        city.queue.first() == Some(&Build::Unit(build)),
                    )
                })
                .chain([card(
                    Target::BuildWorker,
                    "WORKER".into(),
                    Build::Worker,
                    city.queue.first() == Some(&Build::Worker),
                )])
                .collect(),
        );
        panel.gap(GAP);
        panel.buttons(vec![
            ButtonSpec {
                target: Target::ToggleYields,
                label: "YIELDS".into(),
                hint: "Y".into(),
                state: ButtonState::new(self.show_yields, false),
                armed: false,
            },
            ButtonSpec {
                target: Target::OpenInterior,
                label: "CITY INTERIOR".into(),
                hint: "V".into(),
                state: ButtonState::Ready,
                armed: false,
            },
            worker_mode_button("WORKER JOBS"),
        ]);
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
            .map(|building| {
                let cost = cost_hint(building.price(), building.turns());
                let queued = city.pending_building == Some(building)
                    || city.queue.contains(&Build::Building(building));
                ButtonSpec {
                    target: Target::Building(building),
                    label: building.name().into(),
                    // The price alone, which the row lines up on the right;
                    // the key is in the tooltip.
                    hint: cost,
                    state: ButtonState::new(
                        city.queue.first() == Some(&Build::Building(building))
                            || self.needs_site(i, building)
                            || self.site_placement() == Some((i, building)),
                        if queued {
                            !self.needs_site(i, building)
                        } else {
                            !stock.covers(building.price())
                        },
                    ),
                    armed: false,
                }
            })
            .collect();
        panel.text(SMALL, vec![("BUILDINGS".into(), LABEL_TEXT)]);
        panel.building_catalog(i, building_buttons, city.building_scroll);
        if city.barracks.is_some() {
            let training = match city.barracks_queue.first() {
                Some(build) => format!("BARRACKS: TRAINING {}", build.name()),
                None => "BARRACKS: IDLE - TRAIN TROOPS THERE".into(),
            };
            panel.gap(GAP);
            panel.text(SMALL, vec![(training, GOLD_TEXT)]);
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
                && city.progress >= self.city_build_work(i, Build::Building(building));
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

    /// The city's workers: how many are home and out, what those out are
    /// doing, and the jobs waiting for them, which can be dragged into a new
    /// order or removed.
    fn city_workers(&self, i: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        let out: Vec<_> = self.field_workers.iter().filter(|w| w.home == i).collect();
        let line = vec![(
            format!("WORKERS: {} HOME, {} OUT", city.workers, out.len()),
            LABEL_TEXT,
        )];
        panel.text(SMALL, line);
        for worker in out {
            let doing = match (worker.job, worker.work_left) {
                (Some(job), Some(left)) => {
                    format!("{} · {}", self.job_title(job), turns_text(left))
                }
                (Some(job), None) => format!("TO {}", self.job_title(job)),
                (None, _) if worker.recalled => "RECALLED, WALKING HOME".into(),
                (None, _) => "WALKING HOME".into(),
            };
            if worker.recalled {
                panel.compact_buttons(vec![ButtonSpec {
                    target: Target::ShowWorker(worker.id),
                    label: doing,
                    hint: String::new(),
                    state: ButtonState::Ready,
                    armed: false,
                }]);
                continue;
            }
            panel.compact_buttons(vec![
                ButtonSpec {
                    target: Target::ShowWorker(worker.id),
                    label: doing,
                    hint: String::new(),
                    state: ButtonState::Ready,
                    armed: false,
                },
                ButtonSpec {
                    target: Target::RecallWorker(worker.id),
                    label: "RECALL".into(),
                    hint: String::new(),
                    state: ButtonState::Ready,
                    armed: false,
                },
            ]);
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
                    "{} · {}",
                    self.job_title(*job),
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

    /// The worker menu (W): the only way to give workers orders. Its job
    /// buttons arm a job to place on the map; it lists a city's workers
    /// (with Recall) and waiting jobs (drag to reorder, X to remove), with a
    /// button per city when there are several; Sleep rests that city's idle
    /// workers this turn, and Done closes it.
    pub(super) fn worker_menu(&self, panel: &mut PanelBuilder) {
        panel.text(TITLE, vec![("WORKERS".into(), TEXT)]);
        let how = match self.placing_job {
            Some(kind) if kind.on_edge() => format!(
                "PLACING {}S: CLICK OR DRAG ALONG HEX EDGES · ESC TO STOP",
                kind.name()
            ),
            Some(kind) => format!(
                "PLACING {}S: CLICK OR DRAG OVER LIT TILES · ESC TO STOP",
                kind.name()
            ),
            None => "PICK A JOB, THEN PLACE IT ON THE MAP.".into(),
        };
        panel.text(SMALL, vec![(how, LABEL_TEXT)]);
        panel.text(
            SMALL,
            vec![(
                "LIT: WHERE WORKERS REACH, 3 TILES FROM A CITY OR WORK CAMP, OR NEXT TO A ROAD"
                    .into(),
                DIM_TEXT,
            )],
        );
        for row in JobKind::ALL.chunks(3) {
            panel.compact_buttons(
                row.iter()
                    .map(|&kind| {
                        let key = match kind {
                            JobKind::Road => "R · ",
                            JobKind::Improve => "I · ",
                            _ => "",
                        };
                        ButtonSpec {
                            target: Target::WorkerJob(kind),
                            label: kind.name().into(),
                            hint: format!("{key}{}", turns_text(kind.turns())),
                            state: ButtonState::Ready,
                            armed: self.placing_job == Some(kind),
                        }
                    })
                    .collect(),
            );
        }
        let cities: Vec<usize> = (0..self.cities.len())
            .filter(|&i| self.cities[i].team == PLAYER_TEAM)
            .collect();
        if cities.len() > 1 {
            panel.gap(GAP);
            panel.compact_buttons(
                cities
                    .iter()
                    .map(|&i| ButtonSpec {
                        target: Target::WorkerCity(i),
                        label: format!("CITY {}", self.cities[i].id + 1),
                        hint: match self.idle_workers(i) {
                            0 => String::new(),
                            n => format!("{n} IDLE"),
                        },
                        state: ButtonState::new(self.worker_menu_city == Some(i), false),
                        armed: false,
                    })
                    .collect(),
            );
        }
        if let Some(city) = self.worker_menu_city {
            panel.gap(GAP);
            self.city_workers(city, panel);
        }
        panel.gap(GAP);
        let idle = self.worker_menu_city.map_or(0, |i| self.idle_workers(i));
        panel.compact_buttons(vec![
            ButtonSpec {
                target: Target::SleepWorkers,
                label: "SLEEP".into(),
                hint: "SPACE".into(),
                state: ButtonState::new(false, idle == 0),
                armed: false,
            },
            worker_mode_button("DONE"),
        ]);
    }

    pub(super) fn barracks_tray(&self, i: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        let Some(tile) = city.barracks else {
            return;
        };
        let stock = self.stock(city.team);
        panel.text(
            TITLE,
            vec![(format!("CITY {} BARRACKS", city.id + 1), city.team.color())],
        );
        let on = self
            .grid
            .resource(tile)
            .map_or_else(|| "OPEN GROUND".into(), |r| r.name().to_string());
        panel.text(
            SMALL,
            vec![(
                format!("ON {on} · TRAINS TROOPS TWICE AS FAST AS THE CITY"),
                DIM_TEXT,
            )],
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
        // Each deposit kind: how many troops it still allows, or why none.
        for (resource, unit) in [
            (Resource::Horses, BuildUnit::Cavalry),
            (Resource::Iron, BuildUnit::Armored),
        ] {
            let line = if self.barracks_deposits(i, resource).is_empty() {
                (
                    format!(
                        "{}: LOCKED - PUT A BARRACKS ON {}",
                        unit.name(),
                        resource.name()
                    ),
                    REDUCED_TEXT,
                )
            } else {
                let cap = self.special_cap(city.team, resource);
                let used = self.special_used(city.team, resource);
                let left = cap.saturating_sub(used);
                (
                    format!(
                        "{}: {left} OF {cap} LEFT ({} {} DEPOSIT{} × {UNITS_PER_DEPOSIT})",
                        unit.name(),
                        cap / UNITS_PER_DEPOSIT,
                        resource.name(),
                        if cap / UNITS_PER_DEPOSIT == 1 {
                            ""
                        } else {
                            "S"
                        }
                    ),
                    if left > 0 { BOOSTED_TEXT } else { GOLD_TEXT },
                )
            };
            panel.text(SMALL, vec![line]);
        }
        if let (Some(build), Some(turns)) = (
            city.barracks_queue.first().copied(),
            self.barracks_turns_left(i),
        ) {
            panel.text(
                SMALL,
                vec![(
                    format!(
                        "TRAINING {} · {} LEFT",
                        build.name(),
                        turns_text(turns as u32)
                    ),
                    GOLD_TEXT,
                )],
            );
            panel.bar((city.barracks_progress as f32 / build.work() as f32).clamp(0.0, 1.0));
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
                .map(|build| {
                    // Locked (no deposit, or the cap is used up) or
                    // unaffordable cards are dimmed; the tooltip says why.
                    ButtonSpec {
                        target: Target::BarracksBuild(build),
                        label: build.name().into(),
                        hint: cost_hint(build.price(), build.turns()),
                        state: ButtonState::new(
                            city.barracks_queue.first() == Some(&build),
                            self.barracks_lock(i, build).is_some() || !stock.covers(build.price()),
                        ),
                        armed: false,
                    }
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

/// The button that turns worker mode on (in the city panel) or off (Done).
fn worker_mode_button(label: &str) -> ButtonSpec {
    ButtonSpec {
        target: Target::WorkerMode,
        label: label.into(),
        hint: "W".into(),
        state: ButtonState::Ready,
        armed: false,
    }
}
