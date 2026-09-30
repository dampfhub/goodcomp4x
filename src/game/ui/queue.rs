//! Production queue panels (city and Barracks): scrolling, drag to reorder, X to remove, Clear
//! to empty.

use super::builder::{ButtonSpec, PanelBuilder};
use super::{
    BODY, LABEL_TEXT, Line, PADDING, QUEUE_ITEM_HEIGHT, QueueDrag, QueueItemSpec, QueueKind,
    REDUCED_TEXT, SMALL, TITLE_ROW_HEIGHT, Target, WAITING_TEXT, contains, to_ui,
};
use crate::game::GameState;
use crate::game::city::{
    HeadWait, Lane, SETTLER_MIN_POPULATION, Stock, WaitsFor, stock_icons, turns_icon,
};
use glam::Vec2;

/// What a queue that works nothing this turn because its first item waits
/// is doing, for the panels and the turn strip: a city gathers by itself
/// (`gathers_this_turn`); a Barracks idles. The one place it's written.
pub(super) fn idle_word(lane: Lane) -> &'static str {
    match lane {
        Lane::City => "GATHERING WHILE IT WAITS",
        Lane::Barracks => "IDLE",
    }
}

/// "CITY 2" or "CITY 2 BARRACKS": the queue's building, by its city's id.
pub(super) fn queue_place(city_id: u32, lane: Lane) -> String {
    match lane {
        Lane::City => format!("CITY {}", city_id + 1),
        Lane::Barracks => format!("CITY {} BARRACKS", city_id + 1),
    }
}

/// "MELEE WAITS, SHORT OF [wood]3": the item that waits and what for, with
/// how much the stockpile is short of when that's what it waits for.
pub(super) fn wait_text(wait: &HeadWait) -> String {
    let name = wait.build.name();
    match wait.waits_for {
        WaitsFor::Stock(short) => format!("{name} WAITS, SHORT OF {}", stock_icons(short)),
        WaitsFor::Supply => format!("{name} WAITS FOR SUPPLY: NO ROOM TO START IT"),
        WaitsFor::Citizens => format!("{name} WAITS FOR POPULATION {SETTLER_MIN_POPULATION}"),
    }
}

/// A panel's line for a queue whose first item waits (`wait_text`): when
/// the queue works nothing else, a warning in the waiting red, "IDLE —
/// RANGED WAITS, SHORT OF [wood]3" (`idle_word`); otherwise, small, the
/// wait alone.
pub(super) fn waiting_line(wait: &HeadWait) -> (u32, Line) {
    if wait.idle {
        let text = format!("{} — {}", idle_word(wait.lane), wait_text(wait));
        (BODY, vec![(text, WAITING_TEXT)])
    } else {
        (SMALL, vec![(wait_text(wait), REDUCED_TEXT)])
    }
}

pub(super) fn queue_items_that_fit(available_height: f32) -> usize {
    // The title, with the Clear button at its end (`queue_title`).
    let fixed = 2.0 * PADDING + TITLE_ROW_HEIGHT;
    let item = QUEUE_ITEM_HEIGHT;
    let mut visible = 1;
    while (fixed + (visible + 1) as f32 * item).round() <= available_height.round() {
        visible += 1;
    }
    visible
}

impl GameState {
    /// Wheel input over the nested city building catalogue stays in that list.
    pub fn scroll_buildings_at(&mut self, cursor: Vec2, screen_size: Vec2, steps: f32) -> bool {
        let point = to_ui(cursor, screen_size);
        let layout = self.layout(screen_size);
        let Some(region) = layout
            .building_scrollbars
            .iter()
            .find(|region| contains(region.min, region.max, point))
        else {
            return false;
        };
        let current = self.cities[region.city].building_scroll;
        let delta = steps.abs().ceil() as usize;
        self.cities[region.city].building_scroll = if steps > 0.0 {
            current.saturating_sub(delta)
        } else {
            current.saturating_add(delta).min(region.max_offset)
        };
        true
    }

    pub fn drag_building_scrollbar_at(
        &mut self,
        cursor: Vec2,
        screen_size: Vec2,
        captured: bool,
    ) -> bool {
        let point = to_ui(cursor, screen_size);
        let layout = self.layout(screen_size);
        let Some(region) = layout.building_scrollbars.iter().find(|region| {
            region.max_offset > 0
                && (captured || contains(region.track_min, region.track_max, point))
        }) else {
            return false;
        };
        let travel = region.track_max.y - region.track_min.y - region.thumb_height;
        let fraction = if travel > 0.0 {
            ((region.track_max.y - region.thumb_height / 2.0 - point.y) / travel).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.cities[region.city].building_scroll =
            (fraction * region.max_offset as f32).round() as usize;
        true
    }

    fn set_queue_scroll(&mut self, kind: QueueKind, offset: usize) {
        match kind {
            QueueKind::City => self.city_queue_scroll = offset,
            QueueKind::Barracks => self.barracks_queue_scroll = offset,
            QueueKind::Workers => {
                if let Some(city) = self.worker_list_city() {
                    self.cities[city].worker_scroll = offset;
                }
            }
            QueueKind::Priority => {}
        }
    }

    /// Moves a queue scrollbar under the pointer; also used while dragging it.
    pub fn drag_queue_scrollbar_at(
        &mut self,
        cursor: Vec2,
        screen_size: Vec2,
        captured: bool,
    ) -> bool {
        let point = to_ui(cursor, screen_size);
        let layout = self.layout(screen_size);
        let Some(scroll) = layout
            .queue_scrollbars
            .iter()
            .find(|s| captured || contains(s.track_min, s.track_max, point))
        else {
            return false;
        };
        let travel = scroll.track_max.y - scroll.track_min.y - scroll.thumb_height;
        let fraction = if travel > 0.0 {
            ((scroll.track_max.y - scroll.thumb_height / 2.0 - point.y) / travel).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.set_queue_scroll(
            scroll.kind,
            (fraction * scroll.max_offset as f32).round() as usize,
        );
        true
    }

    /// Returns true when the wheel belongs to an overflowing queue panel.
    pub fn scroll_queue_at(&mut self, cursor: Vec2, screen_size: Vec2, steps: f32) -> bool {
        let point = to_ui(cursor, screen_size);
        let layout = self.layout(screen_size);
        let Some(scroll) = layout
            .queue_scrollbars
            .iter()
            .find(|s| contains(s.panel_min, s.panel_max, point))
        else {
            return false;
        };
        let current = match scroll.kind {
            QueueKind::City => self.city_queue_scroll,
            QueueKind::Barracks => self.barracks_queue_scroll,
            QueueKind::Workers => self
                .worker_list_city()
                .map_or(0, |city| self.cities[city].worker_scroll),
            QueueKind::Priority => 0,
        }
        // Kept past the end (by removing rows), it steps back from the end.
        .min(scroll.max_offset);
        let delta = steps.abs().ceil() as usize;
        let next = if steps > 0.0 {
            current.saturating_sub(delta)
        } else {
            current.saturating_add(delta).min(scroll.max_offset)
        };
        self.set_queue_scroll(scroll.kind, next);
        true
    }

    /// Capture a queue row body. Its separate X button remains an ordinary click.
    pub fn start_queue_drag_at(&mut self, cursor: Vec2, screen_size: Vec2) -> bool {
        if self.is_playing_out() {
            return false;
        }
        let point = to_ui(cursor, screen_size);
        let layout = self.layout(screen_size);
        let Some(item) = layout.queue_items.iter().find(|item| {
            !item.locked && point.x < item.body_max_x && contains(item.min, item.max, point)
        }) else {
            return false;
        };
        self.queue_drag = Some(QueueDrag {
            kind: item.kind,
            source: item.index,
            target: Some(item.index),
        });
        true
    }

    pub fn update_queue_drag_at(&mut self, cursor: Vec2, screen_size: Vec2) {
        let Some(drag) = self.queue_drag else { return };
        let point = to_ui(cursor, screen_size);
        let layout = self.layout(screen_size);
        let target = layout
            .queue_items
            .iter()
            .find(|item| item.kind == drag.kind && contains(item.min, item.max, point))
            .map(|item| item.index);
        if let Some(current) = &mut self.queue_drag {
            current.target = target;
        }
    }

    pub fn finish_queue_drag_at(&mut self, cursor: Vec2, screen_size: Vec2) {
        self.update_queue_drag_at(cursor, screen_size);
        let Some(QueueDrag {
            kind,
            source,
            target: Some(target),
        }) = self.queue_drag.take()
        else {
            return;
        };
        if source == target {
            self.queue_item_clicked(kind, source);
        } else {
            self.reorder_queue(kind, source, target);
        }
    }

    /// A click on queue row `index`, rather than a drag: the camera goes to a
    /// worker job's tile (or edge), and a priority chip's good goes first.
    /// City and barracks rows do nothing.
    pub(super) fn queue_item_clicked(&mut self, kind: QueueKind, index: usize) {
        if kind == QueueKind::Priority
            && let Some(city) = self.selected_city
            && let Some(&good) = self.cities[city].priorities.0.get(index)
        {
            self.prioritize_selected_city(good);
            return;
        }
        if kind != QueueKind::Workers {
            return;
        }
        let Some(job) = self
            .worker_list_city()
            .and_then(|city| self.cities[city].worker_jobs.get(index).copied())
        else {
            return;
        };
        let spot = job.across.map_or(job.hex.to_world(), |across| {
            (job.hex.to_world() + across.to_world()) / 2.0
        });
        self.camera.focus_on(spot);
        self.notice = format!("{} - WAITING FOR A WORKER", self.job_title(job));
    }

    pub(super) fn reorder_queue(&mut self, kind: QueueKind, source: usize, target: usize) {
        if source == target || self.is_playing_out() {
            return;
        }
        let city = match kind {
            QueueKind::Workers => self.worker_list_city(),
            _ => self.selected_city.or(self.selected_barracks),
        };
        let Some(city) = city else {
            return;
        };
        match kind {
            QueueKind::City => {
                let queue = &mut self.cities[city].queue;
                if source < queue.len() && target < queue.len() {
                    let item = queue.remove(source);
                    queue.insert(target, item);
                    self.notice = "CITY QUEUE REORDERED".into();
                }
            }
            QueueKind::Barracks => {
                let queue = &mut self.cities[city].barracks_queue;
                if source < queue.len() && target < queue.len() {
                    let item = queue.remove(source);
                    queue.insert(target, item);
                    self.notice = "BARRACKS QUEUE REORDERED".into();
                }
            }
            QueueKind::Workers => {
                let jobs = &mut self.cities[city].worker_jobs;
                if source < jobs.len() && target < jobs.len() {
                    let job = jobs.remove(source);
                    jobs.insert(target, job);
                    self.notice = "WORKER JOBS REORDERED".into();
                }
            }
            QueueKind::Priority => {
                if self.selected_city == Some(city) {
                    let priorities = self.cities[city].priorities.moved(source, target);
                    self.set_selected_city_priorities(priorities);
                }
            }
        }
    }

    pub fn cancel_queue_drag(&mut self) {
        self.queue_drag = None;
    }

    /// A queue panel's title, with its Clear button (`clear`) at the end of
    /// the line: every item off, refunded. Dimmed while a turn plays out,
    /// when the queue can't change.
    fn queue_title(&self, title: &str, clear: Target, panel: &mut PanelBuilder) {
        panel.title_with_button(
            vec![(title.into(), LABEL_TEXT)],
            ButtonSpec::new(clear, "CLEAR", "").unavailable(
                self.is_playing_out()
                    .then(|| "NOT WHILE THE TURN PLAYS OUT".into()),
            ),
        );
    }

    /// What one of `city`'s queues does this turn, for the panels. For the
    /// player's own cities it's `forecast`'s view: the item worked, which
    /// items wait for the stockpile or for supply, and why the first
    /// waits (`head_wait`). Other sides' stockpiles aren't the player's to
    /// see, so their queues show only their first item, and never that the
    /// city gathers.
    pub(super) fn queue_status(&self, city: usize, lane: Lane) -> QueueStatus {
        let len = self.lane_len(city, lane);
        let side =
            (self.cities[city].team == self.local_team).then(|| self.forecast(self.local_team));
        let gathers = lane == Lane::City
            && side
                .as_ref()
                .is_some_and(|side| self.gathers_this_turn(side, city));
        match side.as_ref().and_then(|side| side.lane(city, lane)) {
            Some(forecast) => QueueStatus {
                worked: forecast.worked,
                waiting: self.waiting_items(forecast),
                supply: self.supply_waiting_items(forecast),
                gathers,
                head: self.head_wait(forecast),
            },
            None => QueueStatus {
                worked: (len > 0).then_some(0),
                waiting: Vec::new(),
                supply: Vec::new(),
                gathers,
                head: None,
            },
        }
    }

    /// The player's queues that work nothing this turn because their first
    /// item waits (`HeadWait::idle`), in city order: the turn strip's
    /// waiting chips and End Turn's count.
    pub(super) fn idle_queues(&self) -> Vec<HeadWait> {
        let forecast = self.forecast(self.local_team);
        self.head_waits(&forecast)
            .into_iter()
            .filter(|wait| wait.idle)
            .collect()
    }

    /// Whether item `index` of one of `city`'s queues is a Settler waiting
    /// for citizens (`waits_for_citizens`).
    fn waits_for_citizens_at(&self, city: usize, lane: Lane, index: usize) -> bool {
        lane == Lane::City
            && index < self.lane_len(city, lane)
            && self.waits_for_citizens(city, index)
    }

    /// The name of item `index` of one of `city`'s queues.
    fn item_name(&self, city: usize, lane: Lane, index: usize) -> &'static str {
        match lane {
            Lane::City => self.cities[city].queue[index].build.name(),
            Lane::Barracks => self.cities[city].barracks_queue[index].build.name(),
        }
    }

    /// The item one of `city`'s queues works (`status`): its name, the
    /// turns it has left and how far along it is (0 to 1).
    pub(super) fn worked_item(
        &self,
        city: usize,
        lane: Lane,
        status: &QueueStatus,
    ) -> Option<(&'static str, i32, f32)> {
        let index = status.worked?;
        let (_, progress, work) = self.lane_item(city, lane, index);
        Some((
            self.item_name(city, lane, index),
            self.item_turns_left(city, lane, index),
            (progress as f32 / work as f32).clamp(0.0, 1.0),
        ))
    }

    /// Row `index` of one of `city`'s queues: the item's `name` and the
    /// turns it has left, or what it waits for (and it's tinted). The item
    /// the city works is marked.
    fn queue_row(
        &self,
        (city, lane, index): (usize, Lane, usize),
        name: &str,
        status: &QueueStatus,
        drag: Option<QueueDrag>,
        kind: QueueKind,
    ) -> QueueItemSpec {
        let active = status.worked == Some(index);
        let prefix = if active { "> " } else { "  " };
        let waits = status.waits(index);
        let citizens = self.waits_for_citizens_at(city, lane, index);
        let supply = status.supply.contains(&index);
        let mut state = match waits {
            _ if citizens => format!("WAITS FOR POP {SETTLER_MIN_POPULATION}"),
            _ if supply => "WAITS FOR SUPPLY".into(),
            Some(short) => format!("WAITS, SHORT OF {}", stock_icons(short)),
            None => match self.item_turns_left(city, lane, index) {
                0 => "READY".into(),
                turns => format!("{} LEFT", turns_icon(turns)),
            },
        };
        let (_, progress, _) = self.lane_item(city, lane, index);
        if !active && progress > 0 {
            state.push_str(" · SAVED");
        }
        QueueItemSpec {
            kind,
            index,
            label: format!("{prefix}{name} | {state}"),
            active,
            waiting: waits.is_some() || citizens || supply,
            dragging: drag.is_some_and(|drag| drag.source == index),
            drop_target: drag
                .is_some_and(|drag| drag.target == Some(index) && drag.source != index),
        }
    }

    pub(super) fn barracks_queue_panel(&self, i: usize, visible: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        if city.barracks_queue.is_empty() {
            return;
        }
        let status = self.queue_status(i, Lane::Barracks);
        let offset = self
            .barracks_queue_scroll
            .min(city.barracks_queue.len().saturating_sub(visible));
        if city.barracks_queue.len() > visible {
            panel.scrollbar = Some((
                QueueKind::Barracks,
                offset,
                city.barracks_queue.len(),
                visible,
            ));
        }
        self.queue_title(
            "BARRACKS QUEUE - DRAG TO REORDER",
            Target::ClearBarracksQueue,
            panel,
        );
        let drag = self
            .queue_drag
            .filter(|drag| drag.kind == QueueKind::Barracks);
        for (index, item) in city
            .barracks_queue
            .iter()
            .enumerate()
            .skip(offset)
            .take(visible)
        {
            panel.queue_item(self.queue_row(
                (i, Lane::Barracks, index),
                item.build.name(),
                &status,
                drag,
                QueueKind::Barracks,
            ));
        }
    }

    /// The production queues sit above the city tray so they remain usable
    /// when a city has many queued items.
    pub(super) fn city_queue_panel(&self, i: usize, visible: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        if city.queue.is_empty() {
            return;
        }
        let status = self.queue_status(i, Lane::City);
        let offset = self
            .city_queue_scroll
            .min(city.queue.len().saturating_sub(visible));
        if city.queue.len() > visible {
            panel.scrollbar = Some((QueueKind::City, offset, city.queue.len(), visible));
        }
        // Everything in it waits: the city gathers by itself
        // (`gathers_this_turn`).
        let title = if status.gathers {
            "CITY QUEUE - GATHERING THIS TURN"
        } else {
            "CITY QUEUE - DRAG TO REORDER"
        };
        self.queue_title(title, Target::ClearCityQueue, panel);
        let drag = self.queue_drag.filter(|drag| drag.kind == QueueKind::City);
        for (index, item) in city.queue.iter().enumerate().skip(offset).take(visible) {
            panel.queue_item(self.queue_row(
                (i, Lane::City, index),
                item.build.name(),
                &status,
                drag,
                QueueKind::City,
            ));
        }
    }
}

/// What a queue does this turn (`GameState::queue_status`).
pub(super) struct QueueStatus {
    /// The item its city works, if it can pay for any.
    pub(super) worked: Option<usize>,
    /// The items that wait for the stockpile, with what it's short of for
    /// each (`GameState::waiting_items`).
    pub(super) waiting: Vec<(usize, Stock)>,
    /// The items that wait for supply (`GameState::supply_waiting_items`).
    pub(super) supply: Vec<usize>,
    /// The city queue works nothing, so its city gathers by itself
    /// (`GameState::gathers_this_turn`): the player's own cities only.
    pub(super) gathers: bool,
    /// Why the first item waits, if it does (`GameState::head_wait`): the
    /// panels' waiting line (`waiting_line`).
    pub(super) head: Option<HeadWait>,
}

impl QueueStatus {
    /// What item `index` waits for, if it waits.
    pub(super) fn waits(&self, index: usize) -> Option<Stock> {
        self.waiting
            .iter()
            .find(|&&(waiting, _)| waiting == index)
            .map(|&(_, short)| short)
    }
}
