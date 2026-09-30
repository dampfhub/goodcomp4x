//! The bottom-left command tray: a unit, a group, a city or a Barracks.

use super::builder::{ButtonSpec, CatalogEntry, PanelBuilder, Row};
use super::text::{
    ability_text, compare, cost_hint, resource_color, signed_quantity, stat_spans, turns_text,
};
use super::{
    BODY, BOOSTED_TEXT, DIM_TEXT, GAP, GOLD_TEXT, LABEL_TEXT, QueueItemSpec, QueueKind,
    REDUCED_TEXT, SMALL, TEXT, TITLE, Target, UnitAction,
};
use crate::game::GameState;
use crate::game::city::{
    Build, BuildUnit, Building, CITY_TRAINING_SLOWDOWN, CORE_HP, GATHER_SHORTCUT, GATHER_YIELD,
    GROW_SHORTCUT, Lane, MAX_CITY_POPULATION, SUPPLY_FULL_HINT, UNITS_PER_DEPOSIT,
    WORKERS_PER_MANAGER, manager_label, resource_icon, stock_icons, turns_icon,
};
use crate::game::orders::ClickMode;
use crate::game::terrain::Resource;
use crate::game::workers::JobKind;

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
                    if city.team == self.local_team {
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
            .filter(|f| f.team == self.local_team)
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
            // A fighter is a copy of a troop outside, named as the troop is.
            let name = self
                .units
                .iter()
                .find(|u| u.id == fighter.source_id)
                .map_or("TROOP", |u| self.unit_role(u));
            panel.gap(GAP);
            panel.text(
                BODY,
                vec![(format!("{name}  {:.0} HP", fighter.hp), BOOSTED_TEXT)],
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
                    if city.team == self.local_team {
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
        let leave = if city.team == self.local_team {
            "RETURN TO CITY"
        } else {
            "LEAVE INTERIOR"
        };
        panel.compact_buttons(vec![
            ButtonSpec::new(Target::InteriorClear, "CLEAR ORDERS", "BACKSPACE").unavailable(
                self.interior_selected
                    .is_none()
                    .then(|| "SELECT A TROOP ON THE MAP FIRST".into()),
            ),
            ButtonSpec::new(Target::OpenInterior, leave, "V / ESC"),
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
        // Instructions, and what a landing craft carries, are the player's
        // own units' alone.
        if self.is_player_controlled(idx) {
            if unit.unit_type == crate::game::unit::UnitType::LandingCraft {
                notes.push(format!(
                    "CARGO {}/4 - CLICK ADJACENT LAND TO UNLOAD",
                    unit.cargo.len()
                ));
            } else if !unit.is_naval() {
                notes.push("CLICK AN ADJACENT LANDING CRAFT TO BOARD".to_string());
            }
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
            if unit.alert {
                orders.push("ALERT");
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

    /// Why unit `idx` can't take `action` now, if it can't: its order
    /// button is then disabled (`unit_buttons`), and a group's when none of
    /// its members can (`group_action_unavailable`).
    pub(super) fn unit_action_unavailable(&self, action: UnitAction, idx: usize) -> Option<String> {
        let unit = &self.units[idx];
        let locked =
            (self.rival_of(idx).is_some()).then(|| "LOCKED IN A CONTESTED HEX".to_string());
        let cannot_move =
            (unit.stats().move_range == 0).then(|| "CANNOT MOVE THIS TURN".to_string());
        match action {
            UnitAction::Move => cannot_move,
            UnitAction::Attack => locked
                .or_else(|| (!unit.can_attack()).then(|| "BUSY WITH ITS ABILITY THIS TURN".into())),
            UnitAction::Swap => locked.or(cannot_move),
            UnitAction::Ability => (unit.ability_cooldown > 0)
                .then(|| format!("READY IN {}", turns_text(unit.ability_cooldown))),
            UnitAction::Alert if unit.alert || self.can_go_on_alert(idx) => None,
            UnitAction::Alert if unit.unit_type == crate::game::unit::UnitType::Siege => {
                Some("SET IT UP FIRST".into())
            }
            UnitAction::Alert => Some("ONLY TROOPS THAT FIGHT ON LAND".into()),
            UnitAction::ClearOrders => (!unit.has_orders()).then(|| "NO ORDERS TO CLEAR".into()),
            UnitAction::Hold | UnitAction::Guard | UnitAction::Settle | UnitAction::Disband => None,
        }
    }

    /// Why none of `group` can take `action`, if none can: the reason
    /// they share, or that none can. `None` when any member can.
    pub(super) fn group_action_unavailable(
        &self,
        action: UnitAction,
        group: &[usize],
    ) -> Option<String> {
        let mut reasons = group
            .iter()
            .map(|&i| self.unit_action_unavailable(action, i));
        let first = reasons.next()??;
        let mut same = true;
        for reason in reasons {
            same &= reason.as_ref() == Some(&first);
            reason.as_ref()?;
        }
        Some(if same {
            first
        } else {
            "NONE OF THEM CAN NOW".into()
        })
    }

    /// The selected unit's order buttons: what it can do depends on whether
    /// it's a fighter, a settler or a worker.
    pub(super) fn unit_buttons(&self, idx: usize) -> Vec<ButtonSpec> {
        let unit = &self.units[idx];
        let swapping = self.swap_partner(idx).is_some();
        let settler = self.settlers.contains(&unit.id);
        let armed = |mode| self.selected == Some(idx) && self.ui_click_mode == Some(mode);
        let order = |action, label: &str, hint: &str| {
            ButtonSpec::unit(action, label, hint)
                .unavailable(self.unit_action_unavailable(action, idx))
        };

        let mut buttons = vec![
            order(UnitAction::Move, "MOVE", "M")
                .queued(unit.planned_move.is_some() && !swapping)
                .armed(armed(ClickMode::Move)),
            order(UnitAction::Attack, "ATTACK", "X · RMB")
                .queued(unit.planned_attack.is_some())
                .armed(armed(ClickMode::Attack)),
            order(UnitAction::Swap, "SWAP", "CTRL")
                .queued(swapping)
                .armed(armed(ClickMode::Swap)),
        ];
        if settler {
            buttons.push(order(UnitAction::Settle, "FOUND CITY", "F"));
        } else {
            let (name, _) = ability_text(unit);
            let label = match unit.ability_cooldown {
                0 => name.to_string(),
                turns => format!("{name} ({turns})"),
            };
            buttons.push(order(UnitAction::Ability, &label, "Q").queued(unit.ability_queued));
        }
        buttons.push(order(UnitAction::Hold, "HOLD", "SPACE").queued(unit.holding));
        buttons.push(order(UnitAction::Guard, "GUARD", "G").queued(unit.guarding));
        // Only for troops that could ever go on alert; a siege not set up
        // shows it unavailable.
        if !settler && unit.unit_type.takes_alert() {
            buttons.push(order(UnitAction::Alert, "ALERT", "E").queued(unit.alert));
        }
        buttons.push(order(UnitAction::ClearOrders, "CLEAR ORDERS", "CTRL-RMB"));
        let confirming = self.disband_armed == Some(unit.id);
        let disband = if confirming { "CONFIRM?" } else { "DISBAND" };
        buttons.push(order(UnitAction::Disband, disband, "DEL").armed(confirming));
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
        // Alert acts on the members that can go on alert (or are on it).
        let alert_able: Vec<usize> = group
            .iter()
            .copied()
            .filter(|&i| self.units[i].alert || self.can_go_on_alert(i))
            .collect();
        let all_alert = !alert_able.is_empty() && alert_able.iter().all(|&i| self.units[i].alert);
        // Each order is off only when no member can take it.
        let order = |action, label: &str, hint: &str| {
            ButtonSpec::unit(action, label, hint)
                .unavailable(self.group_action_unavailable(action, group))
        };
        panel.action_toolbar(vec![
            order(UnitAction::Move, "MOVE", "M").armed(armed(ClickMode::Move)),
            order(UnitAction::Attack, "ATTACK", "X · RMB").armed(armed(ClickMode::Attack)),
            order(UnitAction::Hold, "HOLD", "SPACE"),
            order(UnitAction::Guard, "GUARD", "G").queued(all_guarding),
            order(UnitAction::Alert, "ALERT", "E").queued(all_alert),
            order(UnitAction::ClearOrders, "CLEAR ORDERS", "CTRL-RMB"),
        ]);
    }

    pub(super) fn city_tray(&self, i: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        let net = self.net_delivery(i);

        panel.text(
            TITLE,
            vec![
                (format!("CITY {}", city.id + 1), city.team.color()),
                (
                    format!("   POPULATION {}/{}", city.population, MAX_CITY_POPULATION),
                    DIM_TEXT,
                ),
            ],
        );
        panel.text(
            BODY,
            stat_spans(&[(
                "CITIZENS",
                format!(
                    "{} OF {} WORKING · {} OF {} MANAGERS",
                    city.working(),
                    city.capacity(),
                    city.clusters.len(),
                    city.managers_allowed()
                ),
                TEXT,
            )]),
        );
        // With several clusters, its citizens by cluster: each manager
        // (marked as on the map), how many workers it runs, and what they
        // deliver. One cluster is the line above.
        let several = city.clusters.len() > 1;
        for (k, cluster) in city.clusters.iter().enumerate().filter(|_| several) {
            let mut line = vec![
                (
                    format!("{:<3}", manager_label(k, city.clusters.len())),
                    GOLD_TEXT,
                ),
                (
                    format!("{}/{WORKERS_PER_MANAGER} WORKERS   ", cluster.workers.len()),
                    TEXT,
                ),
            ];
            if self.moving_manager == Some((i, k)) {
                line.push(("PICKED UP".to_string(), DIM_TEXT));
            } else {
                for (name, amount) in self.cluster_income(i, k).parts() {
                    line.push((
                        format!("{}{}  ", resource_icon(name), signed_quantity(amount)),
                        resource_color(name),
                    ));
                }
            }
            panel.text(SMALL, line);
        }
        // What this city adds to the side's stockpile a turn, net of the
        // food its citizens eat.
        let mut income_line = vec![("DELIVERS ".to_string(), LABEL_TEXT)];
        for (name, amount) in net.parts() {
            income_line.push((
                format!("{}{}   ", resource_icon(name), signed_quantity(amount)),
                resource_color(name),
            ));
        }
        panel.text(BODY, income_line);
        let status = self.queue_status(i, Lane::City);
        let worked = self.worked_item(i, Lane::City, &status);
        let building = match worked {
            Some((_, 0, _)) => ("READY - WAITING FOR AN OPEN HEX".to_string(), GOLD_TEXT),
            Some((name, turns, _)) => (
                format!("{name} · {} LEFT", turns_text(turns as u32)),
                GOLD_TEXT,
            ),
            None if city.queue.is_empty() => ("NOTHING - CHOOSE BELOW".to_string(), DIM_TEXT),
            None => ("NOTHING IT CAN PAY FOR".to_string(), REDUCED_TEXT),
        };
        panel.text(BODY, stat_spans(&[("BUILDING", building.0, building.1)]));
        if let Some((_, _, done)) = worked {
            panel.bar(done);
        }
        if let Some(waiting) = self.head_waiting_text(i, Lane::City, &status) {
            panel.text(SMALL, vec![(waiting, REDUCED_TEXT)]);
        }
        // The priority order: a chip per good, its rank in its corner. The
        // chip being dragged (classic) is gold, and the one it would land
        // on framed.
        panel.text(
            SMALL,
            vec![("PRIORITY - DRAG TO REORDER".into(), LABEL_TEXT)],
        );
        let drag = self
            .queue_drag
            .filter(|drag| drag.kind == QueueKind::Priority);
        panel.reorder_buttons(
            QueueKind::Priority,
            city.priorities
                .0
                .into_iter()
                .enumerate()
                .map(|(rank, good)| {
                    let label = format!("{} ({})", good.name(), rank + 1);
                    ButtonSpec::new(Target::Priority(good), label, "")
                        .queued(drag.is_some_and(|d| d.source == rank))
                        .armed(drag.is_some_and(|d| d.target == Some(rank) && d.source != rank))
                })
                .collect(),
        );

        panel.gap(GAP);
        // A build card's hint is its price and turns; its key is in its
        // tooltip, to keep the cards narrow. Anything can be queued: what
        // the side can't pay for yet waits in the queue, which its tooltip
        // says.
        let card = |target, label: String, build: Build, head: bool| {
            let price = self.queue_price(i, build);
            let hint = cost_hint(price, self.city_build_turns(i, build));
            ButtonSpec::new(target, label, hint).queued(head)
        };
        let queued_grows = city.queue.iter().filter(|q| q.build == Build::Grow).count();
        let first = city.queue.first().map(|q| q.build);
        let grow = if self.can_grow(i) {
            // One line, so there's room for its key.
            let grow = card(
                Target::Grow,
                format!("GROW TO {}", city.population + queued_grows + 1),
                Build::Grow,
                first == Some(Build::Grow),
            );
            ButtonSpec {
                hint: format!("{GROW_SHORTCUT} · {}", grow.hint),
                ..grow
            }
        } else {
            ButtonSpec::new(Target::Grow, "GROW", format!("{GROW_SHORTCUT} · FULL")).unavailable(
                Some(format!(
                    "FULL: {MAX_CITY_POPULATION} CITIZENS, COUNTING THE GROWTH QUEUED"
                )),
            )
        };
        // Gathering is free: a city can always do it.
        let gather = ButtonSpec::new(
            Target::Gather,
            "GATHER",
            format!(
                "{GATHER_SHORTCUT} · +{} {}",
                stock_icons(GATHER_YIELD),
                turns_icon(Build::Gather.turns())
            ),
        )
        .queued(first == Some(Build::Gather));
        panel.compact_buttons(vec![grow, gather]);

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
        // The side's supply, which every troop, ship and Scout card below
        // uses, on one line with the Barracks' pace.
        let mut line = self.supply_spans(city.team);
        line.push((
            format!(" · A BARRACKS TRAINS TROOPS {CITY_TRAINING_SLOWDOWN}× FASTER THAN THE CITY"),
            DIM_TEXT,
        ));
        panel.text(SMALL, line);
        // Dimmed, saying why, when the city can't queue one
        // (`city_build_issue`): a troop, ship or Scout needs supply, a
        // Settler needs citizens, and a Scout is one at a time.
        let unit_card = |target, build: Build| {
            let card = card(target, build.name().into(), build, first == Some(build));
            match self.city_build_issue(i, build) {
                Some(why) => ButtonSpec {
                    hint: why.clone(),
                    ..card
                }
                .unavailable(Some(why)),
                None => card,
            }
        };
        let unit_buttons: Vec<_> = builds
            .into_iter()
            .map(|build| unit_card(Target::Build(build), Build::Unit(build)))
            .chain(
                [
                    (Target::BuildScout, Build::Scout),
                    (Target::BuildSettler, Build::Settler),
                ]
                .map(|(target, build)| unit_card(target, build)),
            )
            .chain([card(
                Target::BuildWorker,
                "WORKER".into(),
                Build::Worker,
                first == Some(Build::Worker),
            )])
            .collect();
        panel.buttons(vec![ButtonSpec::new(
            Target::OpenInterior,
            "CITY INTERIOR",
            "V",
        )]);
        panel.gap(GAP);
        // While placing, map clicks and Escape are placing's: its own lines
        // (below) take this one's place.
        if self.placing_job.is_none() {
            panel.text(
                SMALL,
                vec![(
                    "CLICK MANAGER TO MOVE · CLICK TILES TO ASSIGN · ESC OR SPACE TO EXIT".into(),
                    LABEL_TEXT,
                )],
            );
        }
        let building_buttons: Vec<_> = Building::ALL
            .iter()
            .copied()
            .filter(|&building| !city.built.contains(&building))
            .filter(|&building| {
                !matches!(building, Building::Harbor | Building::CoastalBattery)
                    || self.city_is_coastal(i)
            })
            .map(|building| {
                // Placed for the workers to build: gold while placed (or
                // being placed), dimmed while it can't be
                // (`job_kind_unavailable`).
                let kind = JobKind::Build(building);
                let placed = self.building_job_queued(i, building);
                // The price alone, which the row lines up on the right; the
                // key is in the tooltip.
                let hint = cost_hint(building.price(), building.turns());
                ButtonSpec::new(Target::Building(building), building.name(), hint)
                    .queued(placed || self.placing_job == Some(kind))
                    .unavailable(
                        (!placed)
                            .then(|| self.job_kind_unavailable(i, kind))
                            .flatten(),
                    )
            })
            .collect();
        // Roads, improvements and structures: armed to place on the map.
        let work_buttons: Vec<_> = JobKind::ALL
            .into_iter()
            .map(|kind| {
                let placing = self.placing_job == Some(kind);
                let hint = cost_hint(kind.price(), kind.turns() as i32);
                ButtonSpec::new(Target::WorkerJob(kind), kind.name(), hint)
                    .queued(placing)
                    .unavailable(
                        (!placing)
                            .then(|| self.job_kind_unavailable(i, kind))
                            .flatten(),
                    )
            })
            .collect();
        if let Some(kind) = self.placing_job {
            let how = if kind.on_edge() {
                "CLICK OR DRAG ALONG HEX EDGES"
            } else if matches!(kind, JobKind::Build(_)) {
                "CLICK A LIT TILE"
            } else {
                "CLICK OR DRAG OVER LIT TILES"
            };
            panel.text(
                SMALL,
                vec![(
                    format!(
                        "PLACING {}: {how} · RIGHT-CLICK OR ESC TO CANCEL",
                        kind.name()
                    ),
                    GOLD_TEXT,
                )],
            );
            // Its armed card is gold too, and clicking it again also stops,
            // but that's easy to miss: say it plainly.
            panel.compact_buttons(vec![ButtonSpec::new(
                Target::CancelPlacing,
                format!("CANCEL PLACING {}", kind.name()),
                "",
            )]);
            panel.text(
                SMALL,
                vec![(
                    "LIT: WHERE WORKERS REACH, 3 TILES FROM A CITY OR WORK CAMP, OR NEXT TO A ROAD"
                        .into(),
                    DIM_TEXT,
                )],
            );
        }
        let mut production_entries = Vec::with_capacity(
            unit_buttons.len() + building_buttons.len() + work_buttons.len() + 3,
        );
        production_entries.push(CatalogEntry::Heading("UNITS"));
        production_entries.extend(unit_buttons.into_iter().map(CatalogEntry::Card));
        production_entries.push(CatalogEntry::Heading("BUILDINGS"));
        production_entries.extend(building_buttons.into_iter().map(CatalogEntry::Card));
        production_entries.push(CatalogEntry::Heading("WORKS"));
        production_entries.extend(work_buttons.into_iter().map(CatalogEntry::Card));
        panel.building_catalog(i, production_entries, city.building_scroll);
        panel.gap(GAP);
        self.city_workers(i, panel);
        if city.barracks.is_some() {
            let status = self.queue_status(i, Lane::Barracks);
            let training = match self.worked_item(i, Lane::Barracks, &status) {
                Some((name, ..)) => (format!("BARRACKS: TRAINING {name}"), GOLD_TEXT),
                None if city.barracks_queue.is_empty() => {
                    ("BARRACKS: IDLE - TRAIN TROOPS THERE".into(), GOLD_TEXT)
                }
                None => ("BARRACKS: NOTHING IT CAN PAY FOR".into(), REDUCED_TEXT),
            };
            panel.gap(GAP);
            panel.text(SMALL, vec![training]);
            panel.buttons(vec![ButtonSpec::new(
                Target::OpenBarracks,
                "SEE BARRACKS",
                "",
            )]);
        }
    }

    /// The city's workers: how many are home and out, what those out are
    /// doing, and the jobs waiting for them, which can be dragged into a new
    /// order or removed. All but the count are one list, which scrolls in
    /// classic when the tray has too little room for it (`fit_height`).
    fn city_workers(&self, i: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        let out: Vec<_> = self.field_workers.iter().filter(|w| w.home == i).collect();
        let held = match city.held_workers {
            0 => String::new(),
            n => format!(" ({n} HELD)"),
        };
        let line = vec![(
            format!("WORKERS: {} HOME{held}, {} OUT", city.workers, out.len()),
            LABEL_TEXT,
        )];
        panel.text(SMALL, line);
        let button = |target, label: String| ButtonSpec::new(target, label, "");
        let mut list = Vec::new();
        // Recalled workers stay home until released, one a click.
        if city.held_workers > 0 && city.team == self.local_team {
            let label = if city.held_workers == 1 {
                "HELD AT HOME - RELEASE"
            } else {
                "HELD AT HOME - RELEASE ONE"
            };
            list.push(Row::Buttons(
                vec![button(Target::ReleaseWorker, label.into())],
                true,
            ));
        }
        for worker in out {
            let doing = match (worker.job, worker.work_left) {
                (Some(job), Some(left)) => {
                    format!("{} · {}", self.job_title(job), turns_text(left))
                }
                (Some(job), None) => format!("TO {}", self.job_title(job)),
                (None, _) if worker.recalled => "RECALLED, WALKING HOME TO STAY".into(),
                (None, _) => "WALKING HOME".into(),
            };
            let mut buttons = vec![button(Target::ShowWorker(worker.id), doing)];
            if !worker.recalled {
                buttons.push(button(Target::RecallWorker(worker.id), "RECALL".into()));
            }
            list.push(Row::Buttons(buttons, true));
        }
        if !city.worker_jobs.is_empty() {
            list.push(Row::Text(
                SMALL,
                vec![(
                    "PLACED, WAITING FOR A WORKER - DRAG TO REORDER".into(),
                    LABEL_TEXT,
                )],
            ));
        }
        let drag = self
            .queue_drag
            .filter(|drag| drag.kind == QueueKind::Workers);
        for (index, job) in city.worker_jobs.iter().enumerate() {
            let total = self.job_turns(city.team, *job);
            // A job a worker left partway keeps its work: "2 OF 4 DONE".
            let turns = if job.done > 0 {
                let done = total.saturating_sub(self.job_turns_left(city.team, *job));
                format!("{done} OF {} DONE", turns_text(total))
            } else {
                turns_text(total)
            };
            list.push(Row::QueueItem(QueueItemSpec {
                kind: QueueKind::Workers,
                index,
                label: format!("{} · {turns}", self.job_title(*job)),
                active: false,
                waiting: false,
                dragging: drag.is_some_and(|drag| drag.source == index),
                drop_target: drag
                    .is_some_and(|drag| drag.target == Some(index) && drag.source != index),
            }));
        }
        panel.scroll_list(QueueKind::Workers, list, city.worker_scroll);
    }

    pub(super) fn barracks_tray(&self, i: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        let Some(tile) = city.barracks else {
            return;
        };
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
        panel.text(SMALL, self.supply_tray_line(city.team));
        let status = self.queue_status(i, Lane::Barracks);
        if let Some((name, turns, done)) = self.worked_item(i, Lane::Barracks, &status) {
            panel.text(
                SMALL,
                vec![(
                    format!("TRAINING {name} · {} LEFT", turns_text(turns as u32)),
                    GOLD_TEXT,
                )],
            );
            panel.bar(done);
        }
        if let Some(waiting) = self.head_waiting_text(i, Lane::Barracks, &status) {
            panel.text(SMALL, vec![(waiting, REDUCED_TEXT)]);
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
                    // Locked cards (no deposit, the deposits' cap or the
                    // supply used up) are dimmed; the tooltip says why, and
                    // a card the supply locks says so in place of its price.
                    // One the side can't pay for yet can be queued, and
                    // waits.
                    let lock = self.barracks_lock(i, build);
                    let supply_full = lock.is_some() && self.deposit_lock(i, build).is_none();
                    let hint = if supply_full {
                        SUPPLY_FULL_HINT.into()
                    } else {
                        cost_hint(build.price(), build.turns())
                    };
                    let head = city.barracks_queue.first().map(|q| q.build) == Some(build);
                    ButtonSpec::new(Target::BarracksBuild(build), build.name(), hint)
                        .queued(head)
                        .unavailable(lock)
                })
                .collect(),
        );
        panel.buttons(vec![ButtonSpec::new(Target::OpenCity, "OPEN CITY", "")]);
    }
}
