//! Button tooltips and the tile tooltip.

use super::action_icons;
use super::builder::PanelBuilder;
use super::paint::draw_shape;
use super::queue::{queue_place, wait_text};
use super::text::{ability_text, pending_text, price_hint, signed_quantity, turns_text, wrap};
use super::{
    BODY, BORDER, Button, DIM_TEXT, FOOD_TEXT, GOLD_TEXT, LABEL_TEXT, Layout, Line, MARGIN,
    METAL_TEXT, QueueKind, REDUCED_TEXT, SMALL, TEXT, TILE_TOOLTIP_OFFSET, TOOLTIP_GAP,
    TOOLTIP_WRAP, Target, UnitAction, WOOD_TEXT, contains,
};
use crate::game::GameState;
use crate::game::city::{
    Build, Building, GATHER_SHORTCUT, GATHER_YIELD, GROW_SHORTCUT, Lane, MAX_CITY_POPULATION,
    MIN_CITY_DISTANCE, SCOUT_SHORTCUT, SETTLER_MIN_POPULATION, SETTLER_SHORTCUT, Stock,
    UNITS_PER_DEPOSIT, WORKER_SHORTCUT, delivered_share, stock_icons, turns_icon,
};
use crate::game::hex::Hex;
use crate::game::keys::Command;
use crate::game::map_icons::{FOOD_ICON, METAL_ICON, WOOD_ICON};
use crate::game::strings::{text, tooltip};
use crate::game::unit::Unit;
use crate::game::workers::JobKind;
use crate::renderer::Vertex;
use glam::Vec2;

/// A tooltip's lines, each with its text size.
pub(super) type TooltipLines = Vec<(u32, Line)>;

/// What a button's tooltip describes it for: the unit (a group's first
/// member) and the city (or Barracks) of the panel the button is in. The
/// selection's for the classic tray and ImGui's Selection panel
/// (`selection_subject`); a captured ImGui panel's own unit or city
/// (`imgui.rs`), whatever is selected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Subject {
    pub unit: Option<usize>,
    pub city: Option<usize>,
}

impl GameState {
    /// The selection as a tooltip's `Subject`.
    pub(super) fn selection_subject(&self) -> Subject {
        Subject {
            unit: self.selected.or(self.group.first().copied()),
            city: self.selected_city.or(self.selected_barracks),
        }
    }

    /// What `build` queued in `city`'s queue (or its Barracks', with
    /// `barracks`) costs from the stockpile when work on it starts, and how
    /// long it takes there: the price's icons, then the clock and the turns.
    fn price_text(&self, build: Build, barracks: bool, city: Option<usize>) -> String {
        let price = city.map_or_else(|| build.price(), |city| self.queue_price(city, build));
        let turns = match city {
            Some(city) if !barracks => self.city_build_turns(city, build),
            _ => build.turns(),
        };
        format!("{} {}", stock_icons(price), turns_icon(turns))
    }

    /// For a troop that needs Horses or Iron, how many more its side may
    /// train (`city/barracks.rs`): " · 2 OF 3 LEFT".
    fn special_note(&self, build: crate::game::BuildUnit) -> String {
        let Some(resource) = build.required_resource() else {
            return String::new();
        };
        let cap = self.special_cap(self.local_team, resource);
        let left = cap.saturating_sub(self.special_used(self.local_team, resource));
        format!(" · {left} OF {cap} LEFT")
    }

    /// What `city`'s side can still spend this turn, once its queues have
    /// paid for what they start (`forecast`'s `spare`): what its build
    /// cards are priced against (`build_shortfall`).
    pub(super) fn card_spare(&self, city: usize) -> Stock {
        self.forecast(self.cities[city].team).spare
    }

    /// What the stockpile is short of to pay for `build` queued in `city`
    /// this turn, given what its side can still spend (`spare`, from
    /// `card_spare`): nothing when it can pay now. The one test of a build
    /// card's affordability: the card is drawn short (`ButtonSpec::short`)
    /// and its tooltip cautions (`shortfall_text`) exactly when this isn't
    /// nothing.
    pub(super) fn build_shortfall(&self, spare: Stock, city: usize, build: Build) -> Stock {
        spare.shortfall(self.queue_price(city, build))
    }

    /// What the stockpile is short of to pay for `build` in `city` this
    /// turn (`build_shortfall`), if anything: queued, it waits until the
    /// side can pay.
    fn shortfall_text(&self, build: Build, city: Option<usize>) -> Option<String> {
        let city = city?;
        let short = self.build_shortfall(self.card_spare(city), city, build);
        (short != Default::default()).then(|| {
            format!(
                "SHORT OF {} THIS TURN: QUEUED, IT WAITS UNTIL PAID",
                stock_icons(short)
            )
        })
    }

    /// What one of `city`'s queues works, by name, for a tile's tooltip:
    /// `empty` if it holds nothing.
    fn queue_word(&self, city: usize, lane: Lane, empty: &'static str) -> &'static str {
        let status = self.queue_status(city, lane);
        match self.worked_item(city, lane, &status) {
            Some((name, ..)) => name,
            None if self.lane_len(city, lane) == 0 => empty,
            None => "NOTHING IT CAN PAY FOR",
        }
    }

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
        let visible = |city: &&crate::game::city::City| seen_now || city.team == self.local_team;
        let city = self.cities.iter().filter(visible).find(|c| c.pos == hex);
        let barracks = self
            .cities
            .iter()
            .filter(visible)
            .find(|c| c.barracks == Some(hex));
        let placed = self.cities.iter().filter(visible).find_map(|c| {
            Building::PLACEABLE
                .into_iter()
                .find(|&b| b != Building::Barracks && c.placed_site(b) == Some(hex))
                .map(|b| (c, b))
        });
        let seen_city = memory.and_then(|m| m.city.filter(|c| c.team != self.local_team));
        let seen_barracks = memory.and_then(|m| m.barracks.filter(|b| b.team != self.local_team));
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
            (None, None, None, None) if placed.is_some() => {
                let (owner, building) = placed.unwrap();
                (building.name().into(), owner.team.color())
            }
            (None, None, None, None) => (tile.name(), TEXT),
        };
        let mut lines = vec![(BODY, vec![title])];
        if self.hovered_tile == Some(hex) {
            lines.extend(self.attack_preview_lines());
        }

        if !terrain.is_workable() {
            lines.push((SMALL, vec![("IMPASSABLE".into(), DIM_TEXT)]));
            return lines;
        }
        let (food, wood, metal) = self.known_yield(hex, &fog);
        lines.push((
            SMALL,
            vec![
                (format!("{FOOD_ICON}{food}"), FOOD_TEXT),
                (format!("   {WOOD_ICON}{wood}"), WOOD_TEXT),
                (format!("   {METAL_ICON}{metal}"), METAL_TEXT),
            ],
        ));
        // Another side's economy (what a city delivers, what its queues
        // work) depends on tiles, routes and plans the player can't see: of
        // its city or Barracks, only what's in sight (population, health),
        // or what the memory kept of it.
        let pop_line = |population: usize| {
            let text = format!("POP {population}/{MAX_CITY_POPULATION}");
            (SMALL, vec![(text, GOLD_TEXT)])
        };
        let hp_line = |hp: f32| {
            let text = format!("HP {hp:.0}/{:.0}", crate::game::city::BARRACKS_MAX_HP);
            (SMALL, vec![(text, GOLD_TEXT)])
        };
        if let Some(city) = city {
            if city.team == self.local_team {
                let city_index = self.cities.iter().position(|c| c.pos == hex).unwrap();
                let queue = self.queue_word(city_index, Lane::City, "NOTHING");
                lines.push((
                    SMALL,
                    vec![(
                        format!(
                            "POP {}/{MAX_CITY_POPULATION} · DELIVERS {}",
                            city.population,
                            price_hint(self.net_delivery(city_index))
                        ),
                        GOLD_TEXT,
                    )],
                ));
                lines.push((SMALL, vec![(format!("BUILDING {queue}"), DIM_TEXT)]));
            } else {
                lines.push(pop_line(city.population));
            }
        } else if let (None, Some(seen)) = (barracks, seen_city) {
            lines.push(pop_line(seen.population));
        }
        if let Some(city) = barracks {
            lines.push(hp_line(city.barracks_hp));
            if city.team == self.local_team {
                let index = self
                    .cities
                    .iter()
                    .position(|c| std::ptr::eq(c, city))
                    .unwrap();
                let queue = self.queue_word(index, Lane::Barracks, "EMPTY");
                lines.push((SMALL, vec![(format!("TRAINING: {queue}"), DIM_TEXT)]));
            }
        } else if let (None, None, Some(seen)) = (city, seen_city, seen_barracks) {
            lines.push(hp_line(seen.health * crate::game::city::BARRACKS_MAX_HP));
        }

        let mut notes = Vec::new();
        if self.grid.has_fresh_water(hex) {
            // Fresh water's food counts toward the cap, so it adds only
            // where the tile has room below it.
            let added = tile.yields(true).0 - tile.yields(false).0;
            notes.push(if added > 0 {
                format!("FRESH WATER +{added} FOOD")
            } else {
                "FRESH WATER".into()
            });
        }
        if let Some((owner, building)) = placed {
            notes.push(building.description().into());
            if building == Building::CoastalBattery {
                notes.push(format!("BATTERY {:.0}/150 HP", owner.coastal_battery_hp));
            }
            let city_index = self.cities.iter().position(|c| c.id == owner.id).unwrap();
            match building {
                Building::WorkCamp => notes.push(
                    if self.known_routes(city_index, &fog).costs.contains_key(&hex) {
                        "CONNECTED"
                    } else {
                        "CUT OFF FROM ITS CITY"
                    }
                    .into(),
                ),
                // Another side's Smelter makes what its city's worked tiles
                // and routes give it: not known.
                Building::Smelter if owner.team == self.local_team => notes.push(format!(
                    "{} METAL A TURN",
                    signed_quantity(self.smelter_income(city_index))
                )),
                Building::Railhead => notes.push(
                    if self.rail_connected(city_index, Some(&fog)) {
                        "RAIL LINK OPEN"
                    } else {
                        "RAIL LINK CUT: NEEDS A ROAD TO THE CITY"
                    }
                    .into(),
                ),
                _ => {}
            }
        }
        if tile.defense_multiplier() != 1.0 {
            let bonus = (tile.defense_multiplier() - 1.0) * 100.0;
            notes.push(format!("+{bonus:.0}% DEFENSE"));
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
            notes.push("ROAD".into());
        }
        if let Some(resource) = self.grid.resource(hex) {
            notes.push(resource.name().into());
        }
        if let Some(special) = self.grid.special(hex) {
            let (food, wood, metal) = special.bonus();
            let gains: Vec<String> = [(food, "FOOD"), (wood, "WOOD"), (metal, "METAL")]
                .into_iter()
                .filter(|&(amount, _)| amount > 0)
                .map(|(amount, what)| format!("+{amount} {what}"))
                .collect();
            notes.push(format!("{} {}", special.name(), gains.join(" ")));
        }
        notes.extend(self.ruin_notes(hex, memory.is_some()));
        notes.extend(self.den_notes(hex, memory.is_some()));
        if let Some(worker) = self.cities.iter().filter(visible).find(|c| c.works(hex)) {
            notes.push(format!("WORKED BY CITY {}", worker.id + 1));
        }
        if let Some(open) = self.selected_city
            && self.cities[open].pos != hex
        {
            let city = &self.cities[open];
            match self.known_routes(open, &fog).costs.get(&hex) {
                Some(&cost) => {
                    let (food, rest) =
                        (self.mill_food_share(open, hex, cost), delivered_share(cost));
                    notes.push(if food == rest {
                        format!("{}% REACHES CITY {}", rest * 25, city.id + 1)
                    } else {
                        format!(
                            "FOOD {}%, REST {}% REACHES CITY {}",
                            food * 25,
                            rest * 25,
                            city.id + 1
                        )
                    })
                }
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

    /// The attack preview in words (`attack_preview`), for the hovered
    /// tile's tooltip: the damage the target would take, the retaliation,
    /// and that it holds only if an enemy target stays and does nothing.
    pub(super) fn attack_preview_lines(&self) -> Vec<(u32, Line)> {
        let Some(preview) = self.attack_preview() else {
            return Vec::new();
        };
        let lethal = |lethal: bool| if lethal { ", LETHAL" } else { "" };
        let (dealt, kills) = preview.dealt();
        let mut lines = vec![(
            SMALL,
            vec![(format!("{dealt:.0} DAMAGE{}", lethal(kills)), REDUCED_TEXT)],
        )];
        let back = preview.retaliation();
        if back > 0.0 {
            let dies = preview.losses.iter().any(|l| !l.target_side && l.lethal());
            lines.push((
                SMALL,
                vec![(format!("RETALIATION {back:.0}{}", lethal(dies)), GOLD_TEXT)],
            ));
        }
        if preview.if_it_stays {
            lines.push((SMALL, vec![("IF IT STAYS".into(), DIM_TEXT)]));
        }
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

    /// The classic tooltip for what's under `point` in `layout`: a button's,
    /// or else a turn strip chip's, or else a queue row's (not while a row is
    /// dragged); with the rectangle it's for. `None` over nothing with one.
    pub(super) fn classic_tooltip_at(
        &self,
        layout: &Layout,
        point: Vec2,
    ) -> Option<(TooltipLines, (Vec2, Vec2))> {
        let (lines, rect) = if let Some(button) = layout.button_at(point) {
            (self.tooltip_lines(button), (button.min, button.max))
        } else if let Some(&(min, max, key)) = layout
            .roster_chips
            .iter()
            .find(|&&(min, max, _)| contains(min, max, point))
        {
            let lines = self.subject_tooltip_lines(
                Target::RosterSelect(key),
                "",
                None,
                self.selection_subject(),
            );
            (lines, (min, max))
        } else {
            let row = layout.queue_items.iter().find(|row| {
                row.kind != QueueKind::Priority
                    && point.x < row.body_max_x
                    && contains(row.min, row.max, point)
            })?;
            if self.queue_drag.is_some() {
                return None;
            }
            let target = Target::QueueItem(row.kind, row.index);
            let lines = self.subject_tooltip_lines(target, "", None, self.selection_subject());
            (lines, (row.min, Vec2::new(row.body_max_x, row.max.y)))
        };
        (!lines.is_empty()).then_some((lines, rect))
    }

    /// A panel of `lines` explaining what's at `rect` (a button, chip or
    /// row), above it (or below, for the top bar), kept inside the window.
    pub(super) fn draw_tooltip(
        &self,
        lines: Vec<(u32, Line)>,
        (min, max): (Vec2, Vec2),
        layout: &Layout,
        screen_size: Vec2,
        out: &mut Vec<Vertex>,
    ) {
        let mut panel = PanelBuilder::default();
        for (px, line) in lines {
            panel.text(px, line);
        }
        let size = panel.size();
        let left = ((min.x + max.x - size.x) / 2.0)
            .clamp(MARGIN, (screen_size.x - MARGIN - size.x).max(MARGIN));
        // Clear of the panel the button sits in, so the tooltip doesn't hide
        // what the panel says: below panels at the top of the window (the top
        // bar, the debug panel), above those at the bottom (the tray).
        let center = (min + max) / 2.0;
        let (panel_min, panel_max) = layout
            .panels
            .iter()
            .copied()
            .find(|&(min, max)| contains(min, max, center))
            .unwrap_or((min, max));
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

    /// A classic button's tooltip: `subject_tooltip_lines` for the
    /// selection.
    pub(super) fn tooltip_lines(&self, button: &Button) -> Vec<(u32, Line)> {
        self.subject_tooltip_lines(
            button.target,
            &button.label,
            button.unavailable.as_deref(),
            self.selection_subject(),
        )
    }

    /// A tooltip's title (with the shortcut) and description for the button
    /// for `target` (labelled `label`), for the unit or city `subject`
    /// names; then why it's `unavailable`, which the button carries
    /// (`ButtonSpec::unavailable`), or else a caution about pressing it
    /// (what the stockpile is short of for a build, which then waits).
    pub(super) fn subject_tooltip_lines(
        &self,
        target: Target,
        label: &str,
        unavailable: Option<&str>,
        subject: Subject,
    ) -> Vec<(u32, Line)> {
        let city = subject.city;
        let (title, shortcut, description, caution): (String, String, String, Option<String>) =
            match target {
                Target::Unit(action) => {
                    // A group's buttons are described for its first member.
                    let Some(idx) = subject.unit else {
                        return Vec::new();
                    };
                    let (title, shortcut, description) = self.unit_action_text(action, idx);
                    (title, shortcut.into(), description, None)
                }
                Target::Build(build) => (
                    build.name().into(),
                    build
                        .shortcut()
                        .map_or_else(|| "CLICK".into(), String::from),
                    format!(
                        "{}. {}{}",
                        build.description(),
                        self.price_text(Build::Unit(build), false, city),
                        self.supply_note(Build::Unit(build))
                    ),
                    self.shortfall_text(Build::Unit(build), city),
                ),
                Target::Building(building) => (
                    building.name().into(),
                    building
                        .shortcut()
                        .map_or_else(|| "CITY BUILD MENU".into(), String::from),
                    format!(
                        "{} {} {}",
                        building.description(),
                        stock_icons(building.price()),
                        turns_icon(building.turns())
                    ),
                    None,
                ),
                Target::BarracksBuild(build) => (
                    format!("TRAIN {}", build.name()),
                    "BARRACKS".into(),
                    format!(
                        "{}. {}{}{}",
                        build.description(),
                        self.price_text(Build::Unit(build), true, city),
                        self.special_note(build),
                        self.supply_note(Build::Unit(build))
                    ),
                    self.shortfall_text(Build::Unit(build), city),
                ),
                Target::OpenBarracks => (
                    "SEE BARRACKS".into(),
                    "CLICK".into(),
                    "ITS TRAINING AND QUEUE.".into(),
                    None,
                ),
                Target::OpenCity => (
                    "OPEN CITY".into(),
                    "CLICK".into(),
                    "BACK TO THE CITY.".into(),
                    None,
                ),
                Target::OpenInterior => (
                    "CITY INTERIOR".into(),
                    "V".into(),
                    "THE CITY'S TACTICAL MAP.".into(),
                    None,
                ),
                Target::InteriorClear => (
                    "CLEAR INTERIOR ORDERS".into(),
                    "BACKSPACE".into(),
                    "DROPS THIS COPY'S ORDERS.".into(),
                    None,
                ),
                Target::CityQueueRemove(_) | Target::BarracksQueueRemove(_) => (
                    "REMOVE".into(),
                    "CLICK".into(),
                    "REFUNDED IF IT WAS PAID FOR (WORK ON IT STARTED). ITS WORK IS LOST.".into(),
                    None,
                ),
                Target::ClearCityQueue | Target::ClearBarracksQueue => (
                    "CLEAR QUEUE".into(),
                    "CLICK".into(),
                    "TAKES EVERY ITEM OFF, EACH REFUNDED AS ITS X WOULD. THEIR WORK IS LOST."
                        .into(),
                    None,
                ),
                Target::WorkerJobRemove(_) => {
                    ("REMOVE".into(), "CLICK".into(), "REFUNDED.".into(), None)
                }
                Target::RosterSelect(key) | Target::RosterAdd(key) | Target::RosterRemove(key) => {
                    let (title, clicks, description) = self.roster_tooltip(key);
                    (title, clicks.into(), description, None)
                }
                Target::ShowWorker(_) => (
                    "WORKER".into(),
                    "CLICK".into(),
                    "SHOW IT ON THE MAP.".into(),
                    None,
                ),
                // A click shows a worker job on the map; other rows only drag.
                Target::QueueItem(QueueKind::Workers, _) => (
                    "PLACED JOB".into(),
                    "CLICK · DRAG".into(),
                    "CLICK: SHOW IT ON THE MAP. DRAG ONTO ANOTHER ROW: MOVE IT THERE. ITS X \
                     TAKES IT OFF."
                        .into(),
                    None,
                ),
                Target::QueueItem(..) => (
                    "QUEUED".into(),
                    "DRAG".into(),
                    "DRAG ONTO ANOTHER ROW: MOVE IT THERE. ITS X TAKES IT OFF.".into(),
                    None,
                ),
                Target::RecallWorker(_) => (
                    "RECALL".into(),
                    "CLICK".into(),
                    "SENDS IT HOME TO STAY UNTIL RELEASED. ITS JOB WAITS ON THE LIST.".into(),
                    None,
                ),
                Target::ReleaseWorker => (
                    "RELEASE".into(),
                    "CLICK".into(),
                    "A WORKER HELD AT HOME GOES BACK TO WORK: IT TAKES THE NEXT JOB ON THE LIST."
                        .into(),
                    None,
                ),
                Target::BuildWorker => (
                    "WORKER".into(),
                    WORKER_SHORTCUT.to_string(),
                    format!(
                        "BUILDS WHAT THE CITY PLACES. {}",
                        self.price_text(Build::Worker, false, city)
                    ),
                    self.shortfall_text(Build::Worker, city),
                ),
                Target::BuildScout => (
                    "SCOUT".into(),
                    SCOUT_SHORTCUT.to_string(),
                    format!(
                        "SEES FAR, MOVES FAST; NOT A TROOP, SO NO BARRACKS. ONE AT A TIME. {}{}",
                        self.price_text(Build::Scout, false, city),
                        self.supply_note(Build::Scout)
                    ),
                    self.shortfall_text(Build::Scout, city),
                ),
                Target::BuildSettler => (
                    "SETTLER".into(),
                    SETTLER_SHORTCUT.to_string(),
                    format!(
                        "FOUNDS A CITY (F) {MIN_CITY_DISTANCE} HEXES OR MORE FROM ANY OTHER. \
                         NEEDS POPULATION {SETTLER_MIN_POPULATION}, AND TAKES A CITIZEN WHEN DONE. {}",
                        self.price_text(Build::Settler, false, city)
                    ),
                    self.shortfall_text(Build::Settler, city),
                ),
                Target::Grow => (
                    "GROW".into(),
                    GROW_SHORTCUT.to_string(),
                    format!(
                        "ONE MORE CITIZEN. {}",
                        self.price_text(Build::Grow, false, city)
                    ),
                    self.shortfall_text(Build::Grow, city),
                ),
                Target::Gather => (
                    "GATHER".into(),
                    GATHER_SHORTCUT.to_string(),
                    format!(
                        "SPEND A TURN GATHERING: {}. FREE.",
                        stock_icons(GATHER_YIELD)
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
                        "{} {} {}",
                        kind.description(),
                        stock_icons(kind.price()),
                        turns_icon(kind.turns() as i32)
                    ),
                    None,
                ),
                Target::CancelPlacing => (
                    "CANCEL PLACING".into(),
                    "ESC · RIGHT-CLICK".into(),
                    "STOPS PLACING, LEAVING THE CITY OPEN. NOTHING IS PLACED OR PAID.".into(),
                    None,
                ),
                Target::Priority(good) => (
                    match action_icons::badge(label) {
                        Some(rank) => format!("{} · PRIORITY {rank}", good.name()),
                        None => good.name().into(),
                    },
                    "CLICK · DRAG".into(),
                    "CITIZENS WORK THE TILES WORTH THE MOST, EACH GOOD COUNTING BY ITS PLACE: \
                     1ST ×9, 2ND ×3, 3RD ×1. FOOD COMES FIRST UNTIL THE CITY'S TILES FEED ITS \
                     CITIZENS WITH 1 TO SPARE. CLICK: PUT IT FIRST. DRAG ONTO ANOTHER: MOVE IT \
                     THERE. EITHER REASSIGNS THE CITIZENS."
                        .into(),
                    None,
                ),
                Target::Scenario(scenario) => (
                    scenario.title().into(),
                    scenario.key(),
                    scenario.description().into(),
                    None,
                ),
                Target::SaveState => (
                    text!("debug_save").into(),
                    Command::Save.key(),
                    tooltip!("debug_save").into(),
                    None,
                ),
                Target::LoadState => (
                    text!("debug_load").into(),
                    Command::Load.key(),
                    self.saved_summary().unwrap_or_default(),
                    None,
                ),
                Target::CompleteProduction => (
                    text!("debug_finish_build").into(),
                    Command::FinishBuild.key(),
                    tooltip!("debug_finish_build").into(),
                    None,
                ),
                Target::TogglePlayback => (
                    text!("debug_playback_title").into(),
                    Command::Playback.key(),
                    tooltip!("debug_playback_title").into(),
                    None,
                ),
                Target::ToggleFog => (
                    text!("debug_fog_title").into(),
                    Command::Fog.key(),
                    String::new(),
                    None,
                ),
                Target::ToggleProductionSpeedup => (
                    text!("debug_speedup_title").into(),
                    text!("debug_no_key").into(),
                    tooltip!("debug_speedup_title").into(),
                    None,
                ),
                Target::ToggleLifetimeCap => (
                    text!("debug_unit_cap_title").into(),
                    text!("debug_no_key").into(),
                    tooltip!("debug_unit_cap_title", units = UNITS_PER_DEPOSIT),
                    None,
                ),
                // The current value's button says so.
                Target::SetSetting(setting, value) => {
                    let current = self.settings.get(setting);
                    let valid = setting.range().contains(&value);
                    (
                        setting.name().into(),
                        if valid {
                            setting.value_text(value)
                        } else {
                            String::new()
                        },
                        setting.description(),
                        (value == current)
                            .then(|| text!("setting_already", value = setting.value_text(current))),
                    )
                }
                Target::OpenSettings => (
                    text!("settings_open_title").into(),
                    String::new(),
                    tooltip!("settings_open_title").into(),
                    None,
                ),
                Target::CloseSettings => (
                    text!("settings_close_title").into(),
                    Command::Back.key(),
                    String::new(),
                    None,
                ),
                Target::Quit => (
                    text!("settings_quit_button").into(),
                    String::new(),
                    String::new(),
                    None,
                ),
                Target::OpenMultiplayer => (
                    text!("settings_multiplayer_button").into(),
                    String::new(),
                    tooltip!("settings_multiplayer_button").into(),
                    None,
                ),
                Target::CloseMultiplayer => (
                    text!("net_back_button").into(),
                    String::new(),
                    tooltip!("net_back_button").into(),
                    None,
                ),
                Target::NetPlayers(players) => (
                    text!("net_players").into(),
                    players.to_string(),
                    tooltip!("net_players").into(),
                    None,
                ),
                Target::EditNetField(field) => (
                    field.name().into(),
                    text!("net_field_typing").into(),
                    tooltip!(
                        "net_field_typing",
                        paste_key = Command::Paste.key(),
                        done_key = Command::StopTyping.key(),
                    ),
                    None,
                ),
                Target::HostGame => (
                    text!("net_host_button").into(),
                    String::new(),
                    tooltip!("net_host_button").into(),
                    None,
                ),
                Target::JoinGame => (
                    text!("net_join_button").into(),
                    String::new(),
                    tooltip!("net_join_button").into(),
                    None,
                ),
                Target::LeaveGame => (
                    text!("net_leave_button").into(),
                    String::new(),
                    if self.join_code().is_some() {
                        text!("net_leave_as_host").into()
                    } else {
                        text!("net_leave_as_guest").into()
                    },
                    None,
                ),
                Target::CopyJoinCode => (
                    text!("net_copy_code_title").into(),
                    String::new(),
                    tooltip!("net_copy_code_title").into(),
                    None,
                ),
                Target::CopyHostAddress => (
                    text!("net_copy_address_title").into(),
                    String::new(),
                    tooltip!("net_copy_address_title").into(),
                    None,
                ),
                Target::ToggleYields => (
                    text!("settings_yields_title").into(),
                    Command::Yields.key(),
                    tooltip!("settings_yields_title").into(),
                    None,
                ),
                Target::EndTurn if self.waiting_for_peers() => (
                    text!("end_turn_take_back_title").into(),
                    text!("click").into(),
                    tooltip!("end_turn_take_back_title").into(),
                    None,
                ),
                Target::EndTurn => {
                    // The queues that sit idle this turn, each with what
                    // its first item waits for (`idle_queues`).
                    let idle = self.idle_queues();
                    let mut description = if pending_text(self.pending()).is_some() {
                        text!("end_turn_selects").to_string()
                    } else {
                        text!("end_turn_resolves").to_string()
                    };
                    for wait in &idle {
                        let place = queue_place(self.cities[wait.city].id, wait.lane);
                        description.push(' ');
                        description.push_str(&text!(
                            "end_turn_idle_queue",
                            queue = place,
                            waits_for = wait_text(wait),
                        ));
                    }
                    let caution = match idle.len() {
                        0 => None,
                        1 => Some(text!("end_turn_one_queue_idle").to_string()),
                        n => Some(text!("end_turn_queues_idle_caution", queues = n)),
                    };
                    (
                        text!("end_turn_button").into(),
                        Command::HoldOrEndTurn.key(),
                        description,
                        caution,
                    )
                }
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
        if let Some(reason) = unavailable.map(String::from).or(caution) {
            lines.push((SMALL, vec![(reason, REDUCED_TEXT)]));
        }
        lines
    }

    /// A unit order's title, shortcut and description, for unit `idx`.
    fn unit_action_text(&self, action: UnitAction, idx: usize) -> (String, &'static str, String) {
        let unit = &self.units[idx];
        match action {
            UnitAction::Move => (
                "MOVE".into(),
                "M OR CLICK",
                "CLICK A GREEN HEX. SHIFT-CLICK QUEUES LATER TURNS.".into(),
            ),
            UnitAction::Attack => (
                "ATTACK".into(),
                "X OR RIGHT-CLICK",
                "CLICK A HEX IN RANGE OF WHERE IT ENDS ITS MOVE.".into(),
            ),
            UnitAction::Swap => (
                "SWAP".into(),
                "CTRL-CLICK",
                "CLICK AN ADJACENT ALLY.".into(),
            ),
            UnitAction::Ability => {
                let (name, description) = ability_text(unit);
                let description = match unit.ability().cooldown() {
                    0 => format!("{description}."),
                    turns => format!("{description}. COOLDOWN {}", turns_text(turns)),
                };
                (name.into(), "Q", description)
            }
            UnitAction::Hold => ("HOLD".into(), "SPACE", "SKIP IT THIS TURN.".into()),
            UnitAction::Guard => (
                "GUARD".into(),
                "G",
                "SKIP IT UNTIL IT'S GIVEN AN ORDER.".into(),
            ),
            UnitAction::Alert => (
                "ALERT".into(),
                "E",
                "STAYS PUT AND ATTACKS THE NEAREST ENEMY IN RANGE EACH TURN, UNTIL IT'S GIVEN \
                 AN ORDER."
                    .into(),
            ),
            UnitAction::Disband => ("DISBAND".into(), "DEL", "REMOVES IT FOR GOOD.".into()),
            UnitAction::Settle => (
                "FOUND CITY".into(),
                "F",
                format!(
                    "HERE: OPEN LAND, NOT RUINS, {MIN_CITY_DISTANCE} OR MORE HEXES FROM ANY CITY."
                ),
            ),
            UnitAction::ClearOrders => (
                "CLEAR ORDERS".into(),
                "CTRL-RIGHT-CLICK",
                "DROPS ALL ITS ORDERS AND QUEUED TURNS.".into(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::builder::{PanelBuilder, Row};
    use super::super::text::price_hint;
    use crate::game::GameState;
    use crate::game::city::{Build, BuildUnit, Building, Lane, Queued, Site};
    use crate::game::hex::Hex;
    use crate::game::unit::{Team, Unit, UnitType};

    /// The words of a tile's tooltip and, for a city or Barracks, its
    /// hover panel: what the player is shown of `hex`.
    fn shown(game: &GameState, hex: Hex) -> Vec<String> {
        let mut words: Vec<String> = game
            .tile_tooltip_lines(hex)
            .into_iter()
            .map(|(_, line)| line.into_iter().map(|(s, _)| s).collect())
            .collect();
        let structure = game.cities.iter().enumerate().find_map(|(i, c)| {
            (c.pos == hex)
                .then_some((i, false))
                .or_else(|| (c.barracks == Some(hex)).then_some((i, true)))
        });
        if let Some((city, barracks)) = structure {
            let mut panel = PanelBuilder::default();
            game.structure_hover_panel(city, barracks, &mut panel);
            words.extend(panel.rows.into_iter().filter_map(|row| match row {
                Row::Text(_, line) => Some(line.into_iter().map(|(s, _)| s).collect()),
                _ => None,
            }));
        }
        words
    }

    /// The Cities scenario with no units, Red's city given a Smelter and a
    /// Barracks beside it and a mine for the Smelter, and a Blue Scout that
    /// sees the city, Smelter and Barracks but neither the mine nor a tile
    /// the city works. Returns the game, Red's city, the Smelter, the
    /// Barracks, the mine and the worked tile out of sight.
    fn enemy_city_half_in_sight() -> (GameState, usize, Hex, Hex, Hex, Hex) {
        let mut base = GameState::city_scenario();
        base.units.clear();
        base.selected = None;
        base.selected_city = None;
        base.memory.clear();
        let red = base
            .cities
            .iter()
            .position(|c| c.team == Team::Red)
            .unwrap();
        let pos = base.cities[red].pos;
        let open = |game: &GameState, h: Hex| {
            game.grid.contains(h)
                && game.grid.terrain(h).is_workable()
                && !game.sites.contains_key(&h)
                && !game.cities.iter().any(|c| c.pos == h || c.works(h))
        };
        let hexes: Vec<Hex> = base.grid.all_hexes().collect();
        let near: Vec<Hex> = pos
            .neighbors()
            .into_iter()
            .filter(|&h| open(&base, h))
            .collect();
        for (smelter, barracks) in near.iter().flat_map(|&s| near.iter().map(move |&b| (s, b))) {
            if smelter == barracks {
                continue;
            }
            for &scout in &hexes {
                if !open(&base, scout) || scout == smelter || scout == barracks {
                    continue;
                }
                let mut game = base.clone();
                game.cities[red].set_placed_site(Building::Smelter, smelter);
                game.cities[red].set_placed_site(Building::Barracks, barracks);
                game.units
                    .push(Unit::new(50, scout, Team::Blue, UnitType::Scout));
                game.explore();
                let fog = game.fog();
                if ![pos, smelter, barracks].iter().all(|&h| fog.sees(h)) {
                    continue;
                }
                let Some(worked) = game.cities[red].worked().find(|&h| !fog.sees(h)) else {
                    continue;
                };
                let routes = game.routes_from(Team::Red, smelter);
                let Some(mine) = hexes.iter().copied().find(|&h| {
                    open(&game, h)
                        && !fog.sees(h)
                        && (1..=3).contains(&smelter.distance(h))
                        && routes.costs.contains_key(&h)
                }) else {
                    continue;
                };
                game.sites.insert(
                    mine,
                    Site {
                        team: Team::Red,
                        food: 0,
                        production: 4,
                        label: "MINE",
                    },
                );
                assert!(game.smelter_income(red) > 0);
                return (game, red, smelter, barracks, mine, worked);
            }
        }
        panic!("no place for a Scout that sees Red's city but not all its tiles");
    }

    #[test]
    fn an_enemy_citys_tooltips_do_not_change_with_what_the_player_cant_see() {
        let (game, red, smelter, barracks, mine, worked) = enemy_city_half_in_sight();
        let pos = game.cities[red].pos;
        let read = |game: &GameState| [pos, smelter, barracks].map(|h| shown(game, h));
        let before = read(&game);
        let real = |game: &GameState| {
            (
                game.net_delivery(red),
                game.smelter_income(red),
                game.queue_word(red, Lane::City, "NOTHING"),
                game.queue_word(red, Lane::Barracks, "EMPTY"),
            )
        };
        // What the player can see still shows.
        let pop = format!("POP {}/", game.cities[red].population);
        assert!(before[0].iter().any(|s| s.starts_with(&pop)), "{before:?}");
        assert!(before[2].iter().any(|s| s.starts_with("HP ")), "{before:?}");

        // An unseen Green unit on the worked tile, another on the mine: goods
        // from neither get through.
        let mut blocked = game.clone();
        for (id, hex) in [(60, worked), (61, mine)] {
            blocked
                .units
                .push(Unit::new(id, hex, Team::Green, UnitType::Melee));
        }
        // The worked tile out of sight yields more.
        let mut richer = game.clone();
        richer.sites.insert(
            worked,
            Site {
                team: Team::Red,
                food: 9,
                production: 9,
                label: "FARM",
            },
        );
        // Red queues something in each queue.
        let mut queued = game.clone();
        queued.cities[red]
            .queue
            .push(Queued::prepaid(Build::Unit(BuildUnit::Melee)));
        queued.cities[red]
            .barracks_queue
            .push(Queued::prepaid(BuildUnit::Ranged));

        let (delivers, smelts, building, training) = real(&game);
        let (blocked_delivers, blocked_smelts, ..) = real(&blocked);
        assert_ne!(blocked_delivers, delivers);
        assert_ne!(blocked_smelts, smelts);
        assert_ne!(real(&richer).0, delivers);
        let (.., queued_building, queued_training) = real(&queued);
        assert_ne!(queued_building, building);
        assert_ne!(queued_training, training);
        for (name, hidden) in [("blocked", blocked), ("richer", richer), ("queued", queued)] {
            assert_eq!(read(&hidden), before, "{name}");
        }
    }

    #[test]
    fn an_enemy_city_out_of_sight_shows_the_population_it_was_seen_with() {
        let (mut game, red, _, barracks, ..) = enemy_city_half_in_sight();
        let pos = game.cities[red].pos;
        let population = game.cities[red].population;
        // The Scout leaves; the city grows and its Barracks is hurt.
        game.units.clear();
        game.explore();
        assert!(!game.fog().sees(pos) && !game.fog().sees(barracks));
        game.cities[red].population += 3;
        game.cities[red].barracks_hp /= 2.0;
        let city = shown(&game, pos);
        let pop = format!("POP {population}/");
        assert!(city.iter().any(|s| s.starts_with(&pop)), "{city:?}");
        let max = crate::game::city::BARRACKS_MAX_HP;
        let hp = format!("HP {max:.0}/{max:.0}");
        let seen = shown(&game, barracks);
        assert!(seen.iter().any(|s| s == &hp), "{seen:?}");
    }

    #[test]
    fn the_players_own_city_tooltips_still_show_its_economy() {
        let (mut game, red, smelter, barracks, ..) = enemy_city_half_in_sight();
        // The same city, now the player's.
        game.local_team = Team::Red;
        game.cities[red]
            .queue
            .push(Queued::prepaid(Build::Unit(BuildUnit::Melee)));
        let city = shown(&game, game.cities[red].pos);
        let delivers = format!("DELIVERS {}", price_hint(game.net_delivery(red)));
        for expected in [delivers.as_str(), "BUILDING ", "QUEUE: "] {
            assert!(
                city.iter().any(|s| s.contains(expected)),
                "{expected} {city:?}"
            );
        }
        let smelts = shown(&game, smelter);
        assert!(
            smelts.iter().any(|s| s.ends_with("METAL A TURN")),
            "{smelts:?}"
        );
        let trains = shown(&game, barracks);
        for expected in ["TRAINING: ", "QUEUE: "] {
            assert!(
                trains.iter().any(|s| s.contains(expected)),
                "{expected} {trains:?}"
            );
        }
    }
}
