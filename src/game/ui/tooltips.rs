//! Button tooltips and the tile tooltip.

use super::builder::PanelBuilder;
use super::paint::draw_shape;
use super::text::{
    ability_text, pending_text, quantity, signed_quantity, stat_spans, turns_text, wrap,
};
use super::{
    BODY, BOOSTED_TEXT, BORDER, Button, DIM_TEXT, GOLD_TEXT, LABEL_TEXT, Layout, Line, MARGIN,
    REDUCED_TEXT, SMALL, TEXT, TILE_TOOLTIP_OFFSET, TOOLTIP_GAP, TOOLTIP_WRAP, Target, UnitAction,
    contains,
};
use crate::game::GameState;
use crate::game::city::{Building, WORKER_COST, WORKER_SHORTCUT, delivered_share};
use crate::game::hex::Hex;
use crate::game::scenario::Scenario;
use crate::game::unit::Unit;
use crate::game::workers::JobKind;
use crate::renderer::Vertex;
use glam::Vec2;

impl GameState {
    /// Everything about a map hex: terrain, what it yields, and what's on it.
    pub(super) fn tile_tooltip_lines(&self, hex: Hex) -> Vec<(u32, Line)> {
        if !self.is_explored(hex) {
            return vec![(BODY, vec![("UNEXPLORED".into(), DIM_TEXT)])];
        }
        let fog = self.fog();
        let seen_now = fog.sees(hex);
        let memory = if seen_now { None } else { self.remembered(hex) };
        let tile = self.grid.tile(hex);
        let terrain = tile.terrain;
        let visible =
            |city: &&crate::game::city::City| seen_now || city.team == crate::game::PLAYER_TEAM;
        let city = self.cities.iter().filter(visible).find(|c| c.pos == hex);
        let barracks = self
            .cities
            .iter()
            .filter(visible)
            .find(|c| c.barracks == Some(hex));
        let mill = self
            .cities
            .iter()
            .filter(visible)
            .find(|c| c.mill == Some(hex));
        let workshop = self
            .cities
            .iter()
            .filter(visible)
            .find(|c| c.workshop == Some(hex));
        let seen_city = memory.and_then(|m| m.city.filter(|c| c.team != crate::game::PLAYER_TEAM));
        let seen_barracks =
            memory.and_then(|m| m.barracks.filter(|b| b.team != crate::game::PLAYER_TEAM));
        let title = match (city, barracks, seen_city, seen_barracks) {
            (Some(city), ..) => (
                format!("{:?} CITY {}", city.team, city.id + 1).to_uppercase(),
                city.team.color(),
            ),
            (None, Some(city), ..) => (
                format!("{:?} BARRACKS", city.team).to_uppercase(),
                city.team.color(),
            ),
            (None, None, Some(seen), _) => (
                format!("{:?} CITY {}", seen.team, seen.id + 1).to_uppercase(),
                seen.team.color(),
            ),
            (None, None, None, Some(seen)) => (
                format!("{:?} BARRACKS", seen.team).to_uppercase(),
                seen.team.color(),
            ),
            (None, None, None, None) if mill.is_some() => {
                ("MILL".into(), mill.unwrap().team.color())
            }
            (None, None, None, None) if workshop.is_some() => {
                ("WORKSHOP".into(), workshop.unwrap().team.color())
            }
            (None, None, None, None) => (tile.name(), TEXT),
        };
        let mut lines = vec![(BODY, vec![title])];

        if !terrain.is_workable() {
            lines.push((SMALL, vec![("IMPASSABLE".into(), DIM_TEXT)]));
            return lines;
        }
        let (food, production) = self.known_yield(hex, &fog);
        lines.push((
            SMALL,
            stat_spans(&[
                ("FOOD", food.to_string(), BOOSTED_TEXT),
                ("PRODUCTION", production.to_string(), GOLD_TEXT),
            ]),
        ));
        if let Some(city) = city {
            let city_index = self.cities.iter().position(|c| c.pos == hex).unwrap();
            let (growth, _, _) = self.growth_status(city_index);
            let (_, production_per_turn) = self.income(city_index);
            let queue = city.queue.first().map_or("NOTHING", |build| build.name());
            lines.push((
                SMALL,
                vec![(
                    format!(
                        "GROWTH {growth}% · {} PRODUCTION",
                        signed_quantity(production_per_turn)
                    ),
                    GOLD_TEXT,
                )],
            ));
            lines.push((SMALL, vec![(format!("BUILDING {queue}"), DIM_TEXT)]));
        }
        if let Some(city) = barracks {
            let city_index = self
                .cities
                .iter()
                .position(|c| c.barracks == Some(hex))
                .unwrap();
            let active = city.worked.first() == Some(&hex);
            let production_per_turn = if active {
                self.barracks_income(city_index)
            } else {
                0
            };
            let queue = city
                .barracks_queue
                .first()
                .map_or("EMPTY", |build| build.name());
            lines.push((
                SMALL,
                vec![(
                    format!(
                        "HP {:.0}/{:.0} · {} PROD/T",
                        city.barracks_hp,
                        crate::game::city::BARRACKS_MAX_HP,
                        signed_quantity(production_per_turn)
                    ),
                    GOLD_TEXT,
                )],
            ));
            lines.push((SMALL, vec![(format!("TRAINING: {queue}"), DIM_TEXT)]));
        }

        let mut notes = Vec::new();
        if self.grid.has_fresh_water(hex) {
            notes.push("FRESH WATER: +1 FOOD".into());
        }
        if mill.is_some() {
            notes.push("ADJACENT REACHABLE TILES DELIVER 100% FOOD".into());
        }
        if workshop.is_some() {
            notes.push("ADJACENT BUILDINGS CONFIRM AT 50% PRODUCTION".into());
        }
        if let Some(open) = self.selected_city
            && let Some(building) = [Building::Barracks, Building::Mill, Building::Workshop]
                .into_iter()
                .find(|building| self.cities[open].planned_sites.get(building) == Some(&hex))
        {
            notes.push(format!(
                "{} PLANNED: CLICK CENTER BADGE TO MOVE",
                building.name()
            ));
        }
        if tile.defense_multiplier() != 1.0 {
            let bonus = (tile.defense_multiplier() - 1.0) * 100.0;
            notes.push(format!("+{bonus:.0}% DEFENSE FOR UNITS HERE"));
        }
        let (site, road) = match memory {
            Some(seen) => (seen.site, seen.road),
            None => (
                self.sites.get(&hex).map(|s| (s.label, s.team)),
                self.roads.contains(&hex),
            ),
        };
        if let Some((label, team)) = site {
            notes.push(format!("{team:?} {label}").to_uppercase());
        }
        if road {
            notes.push("ROAD: GOODS TRAVEL CHEAPER".into());
        }
        if let Some(resource) = self.grid.resource(hex) {
            notes.push(format!("{} RESOURCE", resource.name()));
        }
        if let Some(worker) = self
            .cities
            .iter()
            .filter(visible)
            .find(|c| c.worked.contains(&hex))
        {
            notes.push(format!("WORKED BY CITY {}", worker.id + 1));
        }
        if let Some(open) = self.selected_city
            && self.cities[open].pos != hex
        {
            let city = &self.cities[open];
            match self.known_routes(open, &fog).costs.get(&hex) {
                Some(&cost) => notes.push(format!(
                    "FOOD {}% / PRODUCTION {}% REACHES CITY {}",
                    self.mill_food_share(open, hex, cost) * 25,
                    delivered_share(cost) * 25,
                    city.id + 1
                )),
                None => notes.push(format!("OUT OF CITY {}'S REACH", city.id + 1)),
            }
        }
        let describe =
            |unit: &Unit| format!("{:?} {}", unit.team, self.unit_role(unit)).to_uppercase();
        let units: Vec<String> = self
            .units_at(hex)
            .filter(|&i| fog.shows(&self.units[i]))
            .map(|i| describe(&self.units[i]))
            .collect();
        if !units.is_empty() {
            notes.push(units.join(", "));
        }
        if self.is_contested(hex) && seen_now {
            notes.push("CONTESTED".into());
        }
        lines.extend(
            notes
                .into_iter()
                .map(|note| (SMALL, vec![(note, DIM_TEXT)])),
        );
        lines
    }

    /// The tile tooltip, just below and right of the cursor (`point`, in UI
    /// pixels), kept inside the window.
    pub(super) fn draw_tile_tooltip(
        &self,
        hex: Hex,
        point: Vec2,
        screen_size: Vec2,
        out: &mut Vec<Vertex>,
    ) {
        let mut panel = PanelBuilder::default();
        for (px, line) in self.tile_tooltip_lines(hex) {
            panel.text(px, line);
        }
        let size = panel.size();
        let top_left = point + TILE_TOOLTIP_OFFSET;
        let top_left = Vec2::new(
            top_left.x.min(screen_size.x - MARGIN - size.x),
            top_left.y.max(MARGIN + size.y),
        );
        let mut layout = Layout::default();
        panel.place_top_left(top_left, &mut layout);
        for shape in &layout.shapes {
            draw_shape(shape, out);
        }
    }

    /// A panel explaining what `button` does, above it (or below, for the
    /// top bar), kept inside the window.
    pub(super) fn draw_tooltip(
        &self,
        button: &Button,
        layout: &Layout,
        screen_size: Vec2,
        out: &mut Vec<Vertex>,
    ) {
        let lines = self.tooltip_lines(button);
        let mut panel = PanelBuilder::default();
        for (px, line) in lines {
            panel.text(px, line);
        }
        let size = panel.size();
        let left = ((button.min.x + button.max.x - size.x) / 2.0)
            .clamp(MARGIN, (screen_size.x - MARGIN - size.x).max(MARGIN));
        // Clear of the panel the button sits in, so the tooltip doesn't hide
        // what the panel says: below panels at the top of the window (the top
        // bar, the debug panel), above those at the bottom (the tray).
        let center = (button.min + button.max) / 2.0;
        let (panel_min, panel_max) = layout
            .panels
            .iter()
            .copied()
            .find(|&(min, max)| contains(min, max, center))
            .unwrap_or((button.min, button.max));
        let near_top = (panel_min.y + panel_max.y) / 2.0 > screen_size.y / 2.0;
        let bottom = if near_top {
            panel_min.y - TOOLTIP_GAP - 2.0 * BORDER - size.y
        } else {
            panel_max.y + TOOLTIP_GAP + 2.0 * BORDER
        };
        let mut layout = Layout::default();
        panel.place_bottom_left(Vec2::new(left, bottom), &mut layout);
        for shape in &layout.shapes {
            draw_shape(shape, out);
        }
    }

    /// A tooltip's title (with the shortcut), description, and why the
    /// button is unavailable, if it is.
    pub(super) fn tooltip_lines(&self, button: &Button) -> Vec<(u32, Line)> {
        let (title, shortcut, description, unavailable): (String, String, String, Option<String>) =
            match button.target {
                Target::Unit(action) => {
                    // A group's buttons are described for its first member.
                    let Some(idx) = self.selected.or(self.group.first().copied()) else {
                        return Vec::new();
                    };
                    let (title, shortcut, description, unavailable) =
                        self.unit_action_text(action, idx);
                    (title, shortcut.into(), description, unavailable)
                }
                Target::Build(build) => (
                    build.name().into(),
                    build.shortcut().to_string(),
                    format!(
                        "{}. {} PRODUCTION.",
                        build.description(),
                        quantity(build.cost())
                    ),
                    None,
                ),
                Target::Building(building) => (
                    building.name().into(),
                    building.shortcut().to_string(),
                    format!(
                        "{} COSTS {} PRODUCTION. ONE PER CITY.",
                        building.description(),
                        quantity(building.cost())
                    ),
                    None,
                ),
                Target::BarracksBuild(build) => (
                    format!("TRAIN {}", build.name()),
                    "BARRACKS".into(),
                    format!(
                        "{} COSTS {} PRODUCTION FROM THE ACTIVE MANAGER'S WORK GROUP.",
                        build.description(),
                        quantity(build.cost())
                    ),
                    None,
                ),
                Target::OpenBarracks => (
                    "SEE BARRACKS".into(),
                    "CLICK".into(),
                    "OPENS THE BARRACKS' OWN TRAINING AND QUEUE PANEL.".into(),
                    None,
                ),
                Target::OpenCity => (
                    "OPEN CITY".into(),
                    "CLICK".into(),
                    "RETURNS TO THIS CITY'S LABOR AND MAIN PRODUCTION PANEL.".into(),
                    None,
                ),
                Target::OpenInterior => (
                    "CITY INTERIOR".into(),
                    "V".into(),
                    "ENTER THE CITY'S TACTICAL MAP. YOU CAN ALSO CLICK ITS CENTER HEX FROM CITY VIEW.".into(),
                    None,
                ),
                Target::InteriorClear => (
                    "CLEAR INTERIOR ORDERS".into(),
                    "BACKSPACE".into(),
                    "REMOVES THE SELECTED COPY'S MOVE AND ATTACK FOR THIS TURN.".into(),
                    None,
                ),
                Target::CityQueueRemove(_) | Target::BarracksQueueRemove(_) => (
                    "REMOVE".into(),
                    "CLICK".into(),
                    "REMOVING THE ACTIVE ITEM LOSES ITS PRODUCTION.".into(),
                    None,
                ),
                Target::WorkerJobRemove(_) => (
                    "REMOVE".into(),
                    "CLICK".into(),
                    "TAKES THIS JOB OFF THE CITY'S WORKER LIST.".into(),
                    None,
                ),
                Target::RecallWorker(_) => (
                    "RECALL".into(),
                    "CLICK".into(),
                    "THE WORKER HEADS STRAIGHT HOME, 1 TILE A TURN, WHERE IT'S SAFE. ITS JOB GOES BACK ON TOP OF THE CITY'S LIST."
                        .into(),
                    None,
                ),
                Target::BuildWorker => (
                    "WORKER".into(),
                    WORKER_SHORTCUT.to_string(),
                    format!(
                        "JOINS THE CITY'S WORKERS, WHO GO OUT TO BUILD WHAT YOU ORDER FROM A TILE. {} PRODUCTION.",
                        quantity(WORKER_COST)
                    ),
                    None,
                ),
                Target::WorkerJob(kind) => (
                    kind.name().into(),
                    match kind {
                        JobKind::Road => "R".into(),
                        JobKind::Improve => "I".into(),
                        _ => "CLICK".into(),
                    },
                    format!(
                        "{} {} TURN{} OF WORK ONCE A WORKER GETS THERE.",
                        kind.description(),
                        kind.turns(),
                        if kind.turns() == 1 { "" } else { "S" }
                    ),
                    self.inspected_tile
                        .and_then(|hex| self.job_unavailable(hex, kind)),
                ),
                Target::Focus(focus) => (
                    format!("{} FOCUS", focus.name()),
                    "AUTO".into(),
                    "REASSIGNS CITY LABOR WITH THIS AS ITS DEFAULT PRIORITY.".into(),
                    None,
                ),
                Target::ConfirmBuilding(building) => (
                    format!("CONFIRM {}", building.name()),
                    "CLICK".into(),
                    "FINALIZES THE SELECTED BUILDING SITE.".into(),
                    None,
                ),
                Target::Scenario(scenario) => (
                    scenario.name().into(),
                    scenario.key().into(),
                    match scenario {
                        Scenario::Combat => "FOUR UNITS A SIDE ACROSS A MOUNTAIN PASS.",
                        Scenario::Cities => "TWO ESTABLISHED CITIES WITH ARMIES.",
                        Scenario::Frontier => "A SETTLER AND A SCOUT EACH. BOTH SCOUTS ARE YOURS.",
                        Scenario::World => "A NEW RANDOM CONTINENT EVERY PRESS, YOURS ALONE: NO AI OPPONENT.",
                        Scenario::Siege => "OPPOSING FIELD TROOPS ALREADY FIGHT INSIDE A CITY.",
                    }
                    .into(),
                    None,
                ),
                Target::SaveState => (
                    "SAVE".into(),
                    "F6".into(),
                    "SNAPSHOTS THE WHOLE GAME UNTIL IT CLOSES.".into(),
                    self.is_resolving()
                        .then(|| "NOT WHILE A TURN PLAYS OUT".to_string()),
                ),
                Target::LoadState => (
                    "LOAD".into(),
                    "F7".into(),
                    self.saved_summary().unwrap_or_default(),
                    self.savestate
                        .is_none()
                        .then(|| "NOTHING SAVED YET".to_string()),
                ),
                Target::CompleteProduction => (
                    "COMPLETE PRODUCTION".into(),
                    "F9".into(),
                    "INSTANTLY FINISHES THE CURRENT CITY BUILD OR BARRACKS UNIT FOR TESTING."
                        .into(),
                    None,
                ),
                Target::TogglePlayback => (
                    "PLAYBACK".into(),
                    "F8".into(),
                    "STEP BY STEP OR ALL AT ONCE. THE OUTCOME IS THE SAME.".into(),
                    None,
                ),
                Target::ToggleFog => ("FOG OF WAR".into(), "F10".into(), String::new(), None),
                Target::ToggleYields => (
                    "YIELDS".into(),
                    "Y".into(),
                    "TILE YIELDS AROUND THIS CITY, AND THE SHARE THAT REACHES IT.".into(),
                    None,
                ),
                Target::EndTurn => (
                    "END TURN".into(),
                    "SPACE".into(),
                    if pending_text(self.pending()).is_some() {
                        "SELECTS WHAT STILL NEEDS ORDERS.".into()
                    } else {
                        "RESOLVES EVERYONE'S ORDERS.".into()
                    },
                    None,
                ),
            };

        let mut lines = vec![(
            BODY,
            vec![(title, TEXT), (format!("   {shortcut}"), LABEL_TEXT)],
        )];
        lines.extend(
            wrap(&description, TOOLTIP_WRAP)
                .into_iter()
                .map(|line| (SMALL, vec![(line, DIM_TEXT)])),
        );
        if let Some(reason) = unavailable {
            lines.push((SMALL, vec![(reason, REDUCED_TEXT)]));
        }
        lines
    }

    fn unit_action_text(
        &self,
        action: UnitAction,
        idx: usize,
    ) -> (String, &'static str, String, Option<String>) {
        let unit = &self.units[idx];
        let can_move = unit.stats().move_range > 0;
        let locked = self.rival_of(idx).is_some();
        let cannot_move = (!can_move).then(|| "CANNOT MOVE THIS TURN".to_string());
        match action {
            UnitAction::Move => (
                "MOVE".into(),
                "M OR CLICK",
                "NEXT CLICK ON A GREEN HEX MOVES THERE. CLICK IT AGAIN TO CANCEL. \
                 SHIFT-CLICK QUEUES A MOVE FOR ONE MORE TURN."
                    .into(),
                cannot_move,
            ),
            UnitAction::Attack => (
                "ATTACK".into(),
                "X OR RIGHT-CLICK",
                "NEXT CLICK ATTACKS A HEX IN RANGE, HITTING WHOEVER IS THERE WHEN IT LANDS. \
                 RANGE COUNTS FROM WHERE THE UNIT ENDS ITS MOVE. SHIFT-RIGHT-CLICK QUEUES \
                 AN ATTACK FOR A LATER TURN."
                    .into(),
                if locked {
                    Some("LOCKED IN A CONTESTED HEX".into())
                } else if !unit.can_attack() {
                    Some("BUSY WITH ITS ABILITY THIS TURN".into())
                } else {
                    None
                },
            ),
            UnitAction::Swap => (
                "SWAP".into(),
                "CTRL-CLICK",
                "NEXT CLICK ON AN ADJACENT ALLY SWAPS PLACES WITH IT.".into(),
                if locked {
                    Some("LOCKED IN A CONTESTED HEX".into())
                } else {
                    cannot_move
                },
            ),
            UnitAction::Ability => {
                let (name, description) = ability_text(unit);
                let description = match unit.ability().cooldown() {
                    0 => format!("{description}."),
                    turns => format!("{description}. COOLDOWN {}.", turns_text(turns)),
                };
                let unavailable = (unit.ability_cooldown > 0)
                    .then(|| format!("READY IN {}", turns_text(unit.ability_cooldown)));
                (name.into(), "Q", description, unavailable)
            }
            UnitAction::Hold => (
                "HOLD".into(),
                "SPACE",
                "SKIPS THIS UNIT FOR THE TURN, KEEPING ANY QUEUED ORDERS.".into(),
                None,
            ),
            UnitAction::Guard => (
                "GUARD".into(),
                "G",
                "SKIPS THIS UNIT EVERY TURN UNTIL IT'S GIVEN AN ORDER.".into(),
                None,
            ),
            UnitAction::Disband => (
                "DISBAND".into(),
                "DEL",
                "REMOVES THIS UNIT FOR GOOD. PRESS TWICE: THE FIRST PRESS ASKS TO CONFIRM.".into(),
                None,
            ),
            UnitAction::Settle => (
                "FOUND CITY".into(),
                "F",
                "AT LEAST 3 HEXES FROM ANY OTHER CITY.".into(),
                None,
            ),
        }
    }
}
