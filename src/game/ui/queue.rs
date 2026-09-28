//! Production queue panels (city and Barracks): scrolling, drag to reorder, X to remove, Clear
//! to empty.

use super::builder::{ButtonSpec, PanelBuilder};
use super::text::turns_at_rate;
use super::{
    ButtonState, LABEL_TEXT, PADDING, QUEUE_ITEM_HEIGHT, QueueDrag, QueueItemSpec, QueueKind,
    TITLE_ROW_HEIGHT, Target, contains, to_ui,
};
use crate::game::GameState;
use glam::Vec2;

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
        if self.is_resolving() {
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
    /// worker job's tile (or edge). City and barracks rows do nothing.
    pub(super) fn queue_item_clicked(&mut self, kind: QueueKind, index: usize) {
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
        if source == target {
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
            ButtonSpec {
                target: clear,
                label: "CLEAR".into(),
                hint: String::new(),
                state: ButtonState::new(false, self.is_resolving()),
                armed: false,
            },
        );
    }

    pub(super) fn barracks_queue_panel(&self, i: usize, visible: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        if city.barracks_queue.is_empty() {
            return;
        }
        let rate = self.work_rate(self.barracks_income(i));
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
        for (index, build) in city
            .barracks_queue
            .iter()
            .copied()
            .enumerate()
            .skip(offset)
            .take(visible)
        {
            let prefix = if index == 0 { "> " } else { "  " };
            let remaining = build.work()
                - if index == 0 {
                    city.barracks_progress
                } else {
                    0
                };
            let drag = self
                .queue_drag
                .filter(|drag| drag.kind == QueueKind::Barracks);
            panel.queue_item(QueueItemSpec {
                kind: QueueKind::Barracks,
                index,
                label: format!(
                    "{prefix}{} | {} LEFT",
                    build.name(),
                    turns_at_rate(remaining, rate)
                ),
                active: index == 0,
                dragging: drag.is_some_and(|drag| drag.source == index),
                drop_target: drag
                    .is_some_and(|drag| drag.target == Some(index) && drag.source != index),
                locked: false,
            });
        }
    }

    /// The production queues sit above the city tray so they remain usable
    /// when a city has many queued items.
    pub(super) fn city_queue_panel(&self, i: usize, visible: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        if city.queue.is_empty() {
            return;
        }
        let rate = self.work_rate(self.income(i).production());
        let offset = self
            .city_queue_scroll
            .min(city.queue.len().saturating_sub(visible));
        if city.queue.len() > visible {
            panel.scrollbar = Some((QueueKind::City, offset, city.queue.len(), visible));
        }
        if !city.queue.is_empty() {
            self.queue_title(
                "CITY QUEUE - DRAG TO REORDER",
                Target::ClearCityQueue,
                panel,
            );
            for (index, build) in city
                .queue
                .iter()
                .copied()
                .enumerate()
                .skip(offset)
                .take(visible)
            {
                let prefix = if index == 0 { "> " } else { "  " };
                let work = self.city_build_work(i, build);
                let remaining = work - if index == 0 { city.progress } else { 0 };
                let drag = self.queue_drag.filter(|drag| drag.kind == QueueKind::City);
                let left = if remaining <= 0 {
                    "READY".into()
                } else {
                    format!("{} LEFT", turns_at_rate(remaining, rate))
                };
                panel.queue_item(QueueItemSpec {
                    kind: QueueKind::City,
                    index,
                    label: format!("{prefix}{} | {left}", build.name()),
                    active: index == 0,
                    dragging: drag.is_some_and(|drag| drag.source == index),
                    drop_target: drag
                        .is_some_and(|drag| drag.target == Some(index) && drag.source != index),
                    locked: false,
                });
            }
        }
    }
}
