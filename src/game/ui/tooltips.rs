//! Button tooltips and the tile tooltip.

use super::action_icons;
use super::builder::PanelBuilder;
use super::paint::draw_shape;
use super::text::{ability_text, pending_text, price_hint, signed_quantity, turns_text, wrap};
use super::{
    BODY, BORDER, Button, DIM_TEXT, FOOD_TEXT, GOLD_TEXT, LABEL_TEXT, Layout, Line, MARGIN,
    METAL_TEXT, REDUCED_TEXT, SMALL, TEXT, TILE_TOOLTIP_OFFSET, TOOLTIP_GAP, TOOLTIP_WRAP, Target,
    UnitAction, WOOD_TEXT, contains,
};
use crate::game::GameState;
use crate::game::city::{
    Build, Building, GATHER_SHORTCUT, GATHER_YIELD, GROW_SHORTCUT, Lane, MAX_CITY_POPULATION,
    MIN_CITY_DISTANCE, SCOUT_SHORTCUT, SETTLER_MIN_POPULATION, SETTLER_SHORTCUT, UNITS_PER_DEPOSIT,
    WORKER_SHORTCUT, delivered_share, stock_icons, turns_icon,
};
use crate::game::hex::Hex;
use crate::game::map_icons::{FOOD_ICON, METAL_ICON, WOOD_ICON};
use crate::game::scenario::Scenario;
use crate::game::unit::Unit;
use crate::game::workers::JobKind;
use crate::renderer::Vertex;
use glam::Vec2;

/// Why a button that would change the plan is off while a network game
/// waits for the others' plans.
pub(super) const PLAN_SENT: &str = "YOUR ORDERS ARE SENT - THE WAITING BUTTON TAKES THEM BACK";

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

    /// What the stockpile is short of to pay for `build` in `city` this
    /// turn, on top of what its queues start (`forecast`'s `spare`), if
    /// anything: queued, it waits until the side can pay.
    fn shortfall_text(&self, build: Build, city: Option<usize>) -> Option<String> {
        let city = city?;
        let short = self
            .forecast(self.local_team)
            .spare
            .shortfall(self.queue_price(city, build));
        (short != Default::default()).then(|| {
            format!(
                "SHORT OF {} THIS TURN: QUEUED, IT WAITS UNTIL PAID",
                stock_icons(short)
            )
        })
    }

    /// Why `city` can't queue a Scout or Settler (`city_build_issue`), or
    /// else what the stockpile is short of for it (`shortfall_text`).
    fn civilian_unavailable(&self, build: Build, city: Option<usize>) -> Option<String> {
        city.and_then(|city| self.city_build_issue(city, build))
            .or_else(|| self.shortfall_text(build, city))
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
        if let Some(city) = city {
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
        }
        if let Some(city) = barracks {
            let index = self
                .cities
                .iter()
                .position(|c| std::ptr::eq(c, city))
                .unwrap();
            let queue = self.queue_word(index, Lane::Barracks, "EMPTY");
            lines.push((
                SMALL,
                vec![(
                    format!(
                        "HP {:.0}/{:.0}",
                        city.barracks_hp,
                        crate::game::city::BARRACKS_MAX_HP,
                    ),
                    GOLD_TEXT,
                )],
            ));
            lines.push((SMALL, vec![(format!("TRAINING: {queue}"), DIM_TEXT)]));
        }

        let mut notes = Vec::new();
        if self.grid.has_fresh_water(hex) {
            notes.push("FRESH WATER +1 FOOD".into());
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
                Building::Smelter => notes.push(format!(
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
            let (food, production) = special.bonus();
            let gains: Vec<String> = [(food, "FOOD"), (production, "PRODUCTION")]
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

    /// A classic button's tooltip: `subject_tooltip_lines` for the
    /// selection.
    pub(super) fn tooltip_lines(&self, button: &Button) -> Vec<(u32, Line)> {
        self.subject_tooltip_lines(button.target, &button.label, self.selection_subject())
    }

    /// A tooltip's title (with the shortcut), description, and why the
    /// button for `target` (labelled `label`) is unavailable, if it is, for
    /// the unit or city `subject` names.
    pub(super) fn subject_tooltip_lines(
        &self,
        target: Target,
        label: &str,
        subject: Subject,
    ) -> Vec<(u32, Line)> {
        let city = subject.city;
        let (title, shortcut, description, unavailable): (String, String, String, Option<String>) =
            match target {
                Target::Unit(action) => {
                    // A group's buttons are described for its first member.
                    let Some(idx) = subject.unit else {
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
                        "{}. {}",
                        build.description(),
                        self.price_text(Build::Unit(build), false, city)
                    ),
                    self.shortfall_text(Build::Unit(build), city),
                ),
                Target::Building(building) => (
                    building.name().into(),
                    if building.shortcut() == ' ' {
                        "CITY BUILD MENU".into()
                    } else {
                        building.shortcut().to_string()
                    },
                    format!(
                        "{} {} {}",
                        building.description(),
                        stock_icons(building.price()),
                        turns_icon(building.turns())
                    ),
                    city.and_then(|city| self.job_kind_unavailable(city, JobKind::Build(building))),
                ),
                Target::BarracksBuild(build) => (
                    format!("TRAIN {}", build.name()),
                    "BARRACKS".into(),
                    format!(
                        "{}. {}{}",
                        build.description(),
                        self.price_text(Build::Unit(build), true, city),
                        self.special_note(build)
                    ),
                    city.and_then(|city| self.barracks_lock(city, build))
                        .or_else(|| self.shortfall_text(Build::Unit(build), city)),
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
                    "TAKES EVERY ITEM OFF, EACH REFUNDED AS ITS X WOULD. THEIR WORK IS LOST.".into(),
                    self.is_resolving()
                        .then(|| "NOT WHILE THE TURN PLAYS OUT".into()),
                ),
                Target::WorkerJobRemove(_) => {
                    ("REMOVE".into(), "CLICK".into(), "REFUNDED.".into(), None)
                }
                // Unit strip tokens aren't buttons: their help is in the strip.
                Target::RosterSelect(_) | Target::RosterAdd(_) | Target::RosterRemove(_) => {
                    return Vec::new();
                }
                Target::ShowWorker(_) => (
                    "WORKER".into(),
                    "CLICK".into(),
                    "SHOW IT ON THE MAP.".into(),
                    None,
                ),
                Target::QueueItem(..) => (
                    "QUEUED".into(),
                    "CLICK · DRAG".into(),
                    "CLICK: SHOW IT. DRAG: REORDER.".into(),
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
                        "SEES FAR, MOVES FAST; NOT A TROOP, SO NO BARRACKS. ONE AT A TIME. {}",
                        self.price_text(Build::Scout, false, city)
                    ),
                    self.civilian_unavailable(Build::Scout, city),
                ),
                Target::BuildSettler => (
                    "SETTLER".into(),
                    SETTLER_SHORTCUT.to_string(),
                    format!(
                        "FOUNDS A CITY (F) {MIN_CITY_DISTANCE} HEXES OR MORE FROM ANY OTHER. \
                         NEEDS POPULATION {SETTLER_MIN_POPULATION}, AND TAKES A CITIZEN WHEN DONE. {}",
                        self.price_text(Build::Settler, false, city)
                    ),
                    self.civilian_unavailable(Build::Settler, city),
                ),
                Target::Grow => (
                    "GROW".into(),
                    GROW_SHORTCUT.to_string(),
                    format!("ONE MORE CITIZEN. {}", self.price_text(Build::Grow, false, city)),
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
                    city.and_then(|city| self.job_kind_unavailable(city, kind)),
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
                    scenario.name().into(),
                    scenario.key().into(),
                    match scenario {
                        Scenario::Combat => "FOUR UNITS A SIDE ACROSS A MOUNTAIN PASS.",
                        Scenario::Cities => "TWO ESTABLISHED CITIES WITH ARMIES.",
                        Scenario::Frontier => "A SETTLER AND A SCOUT EACH. BOTH SCOUTS ARE YOURS.",
                        Scenario::World => "A NEW RANDOM WORLD EVERY PRESS.",
                        Scenario::Siege => "A FIGHT ALREADY INSIDE A CITY.",
                        Scenario::Naval => "COASTAL CITIES, SHIPS AND BATTERIES.",
                    }
                    .into(),
                    None,
                ),
                Target::SaveState => (
                    "SAVE".into(),
                    "F6".into(),
                    "SNAPSHOTS THE GAME.".into(),
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
                    "FINISH BUILD".into(),
                    "F9".into(),
                    "FINISHES THE CURRENT BUILD NOW.".into(),
                    None,
                ),
                Target::TogglePlayback => (
                    "PLAYBACK".into(),
                    "F8".into(),
                    "STEP BY STEP OR ALL AT ONCE.".into(),
                    None,
                ),
                Target::ToggleFog => ("FOG OF WAR".into(), "F10".into(), String::new(), None),
                Target::ToggleProductionSpeedup => (
                    "PRODUCTION SPEEDS BUILDS".into(),
                    "DEBUG".into(),
                    "ON: A CITY'S WOOD AND METAL INCOME SPEEDS ITS QUEUE.".into(),
                    None,
                ),
                Target::ToggleLifetimeCap => (
                    "CAVALRY AND ARMORED CAP".into(),
                    "DEBUG".into(),
                    format!(
                        "{UNITS_PER_DEPOSIT} PER DEPOSIT. ALIVE: COUNTS LIVING ONES. EVER: COUNTS ALL TRAINED."
                    ),
                    None,
                ),
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
                        setting.description().into(),
                        (!valid || value == current)
                            .then(|| format!("ALREADY {}", setting.value_text(current))),
                    )
                }
                Target::OpenSettings => (
                    "SETTINGS".into(),
                    String::new(),
                    "GAME OPTIONS.".into(),
                    None,
                ),
                Target::CloseSettings => {
                    ("CLOSE SETTINGS".into(), "ESC".into(), String::new(), None)
                }
                Target::Quit => ("QUIT".into(), String::new(), String::new(), None),
                Target::OpenMultiplayer => (
                    "MULTIPLAYER".into(),
                    String::new(),
                    "HOST A GAME ON THE NETWORK, OR JOIN ONE.".into(),
                    None,
                ),
                Target::CloseMultiplayer => (
                    "BACK".into(),
                    String::new(),
                    "BACK TO THE SETTINGS.".into(),
                    None,
                ),
                Target::NetPlayers(players) => (
                    "PLAYERS".into(),
                    players.to_string(),
                    "HOW MANY PEOPLE PLAY, YOU INCLUDED. THE AI PLAYS THE OTHER SIDES.".into(),
                    None,
                ),
                Target::EditNetField(field) => (
                    field.name().into(),
                    "CLICK".into(),
                    "CLICK, THEN TYPE OR PASTE (CTRL+V). ENTER WHEN DONE.".into(),
                    None,
                ),
                Target::HostGame => (
                    "HOST GAME".into(),
                    String::new(),
                    "STARTS A NEW WORLD FOR THIS MANY PLAYERS AND SHOWS THE JOIN CODE TO GIVE THEM. THEY NEED YOUR ADDRESS AND THE PORT OPEN.".into(),
                    self.net_menu.busy.then(|| "JOINING A GAME".into()),
                ),
                Target::JoinGame => (
                    "JOIN GAME".into(),
                    String::new(),
                    "JOINS THE GAME HOSTED AT THIS ADDRESS (HOST OR HOST:PORT) WITH THE CODE IT SHOWS.".into(),
                    self.net_menu.busy.then(|| "ALREADY JOINING".into()),
                ),
                Target::LeaveGame => (
                    "LEAVE GAME".into(),
                    String::new(),
                    if self.join_code().is_some() {
                        "ENDS THE GAME FOR EVERYONE AND STARTS A NEW ONE OF YOUR OWN.".into()
                    } else {
                        "THE AI PLAYS YOUR SIDE; YOU START A NEW GAME OF YOUR OWN.".into()
                    },
                    None,
                ),
                Target::CopyJoinCode => (
                    "COPY JOIN CODE".into(),
                    String::new(),
                    "PUTS THE JOIN CODE ON THE CLIPBOARD, TO SEND TO THE OTHER PLAYERS.".into(),
                    None,
                ),
                Target::CopyHostAddress => (
                    "COPY ADDRESS".into(),
                    String::new(),
                    "PUTS YOUR ADDRESS ON THE LOCAL NETWORK ON THE CLIPBOARD. PLAYERS OVER THE INTERNET NEED YOUR PUBLIC IP INSTEAD.".into(),
                    None,
                ),
                Target::ToggleYields => (
                    "YIELDS".into(),
                    "Y".into(),
                    "TILE YIELDS AND DELIVERY SHARES.".into(),
                    None,
                ),
                Target::EndTurn if self.waiting_for_peers() => (
                    "TAKE BACK END TURN".into(),
                    "CLICK".into(),
                    "YOUR ORDERS ARE SENT. TAKE THEM BACK TO CHANGE THEM, THEN END THE TURN AGAIN: \
                     UNTIL EVERYONE HAS ENDED IT."
                        .into(),
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
        // With the plan sent, this is why a button that would change it is
        // off, whatever else might be.
        let unavailable = if self.plan_frozen() && target.changes_plan() {
            Some(PLAN_SENT.to_string())
        } else {
            unavailable
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
                "CLICK A GREEN HEX. SHIFT-CLICK QUEUES LATER TURNS.".into(),
                cannot_move,
            ),
            UnitAction::Attack => (
                "ATTACK".into(),
                "X OR RIGHT-CLICK",
                "CLICK A HEX IN RANGE OF WHERE IT ENDS ITS MOVE.".into(),
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
                "CLICK AN ADJACENT ALLY.".into(),
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
                    turns => format!("{description}. COOLDOWN {}", turns_text(turns)),
                };
                let unavailable = (unit.ability_cooldown > 0)
                    .then(|| format!("READY IN {}", turns_text(unit.ability_cooldown)));
                (name.into(), "Q", description, unavailable)
            }
            UnitAction::Hold => ("HOLD".into(), "SPACE", "SKIP IT THIS TURN.".into(), None),
            UnitAction::Guard => (
                "GUARD".into(),
                "G",
                "SKIP IT UNTIL IT'S GIVEN AN ORDER.".into(),
                None,
            ),
            UnitAction::Alert => (
                "ALERT".into(),
                "E",
                "STAYS PUT AND ATTACKS THE NEAREST ENEMY IN RANGE EACH TURN, UNTIL IT'S GIVEN \
                 AN ORDER."
                    .into(),
                if unit.alert || self.can_go_on_alert(idx) {
                    None
                } else if unit.unit_type == crate::game::unit::UnitType::Siege {
                    Some("SET IT UP FIRST".into())
                } else {
                    Some("ONLY TROOPS THAT FIGHT ON LAND".into())
                },
            ),
            UnitAction::Disband => ("DISBAND".into(), "DEL", "REMOVES IT FOR GOOD.".into(), None),
            UnitAction::Settle => (
                "FOUND CITY".into(),
                "F",
                format!(
                    "HERE: OPEN LAND, NOT RUINS, {MIN_CITY_DISTANCE} OR MORE HEXES FROM ANY CITY."
                ),
                None,
            ),
            UnitAction::ClearOrders => (
                "CLEAR ORDERS".into(),
                "CTRL-RIGHT-CLICK",
                "DROPS ALL ITS ORDERS AND QUEUED TURNS.".into(),
                None,
            ),
        }
    }
}
