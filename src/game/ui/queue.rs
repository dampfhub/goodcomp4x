//! Production queue panels (city and Barracks): scrolling, drag to reorder, X to remove.

use super::builder::PanelBuilder;
use super::text::{quantity, turns_at_rate};
use super::{
    LABEL_TEXT, LINE_GAP, PADDING, QUEUE_ITEM_HEIGHT, QueueDrag, QueueItemSpec, QueueKind, SMALL,
    contains, to_ui,
};
use crate::game::GameState;
use crate::game::font;
use glam::Vec2;

pub(super) fn queue_items_that_fit(available_height: f32) -> usize {
    let text_row = font::ui(SMALL).line_height + LINE_GAP;
    let fixed = 2.0 * PADDING + text_row;
    let item = QUEUE_ITEM_HEIGHT;
    let mut visible = 1;
    while (fixed + (visible + 1) as f32 * item).round() <= available_height.round() {
        visible += 1;
    }
    visible
}

impl GameState {
    fn set_queue_scroll(&mut self, kind: QueueKind, offset: usize) {
        match kind {
            QueueKind::City => self.city_queue_scroll = offset,
            QueueKind::Barracks => self.barracks_queue_scroll = offset,
            // Worker jobs are listed in full; they don't scroll.
            QueueKind::Workers => {}
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
            QueueKind::Workers => 0,
        };
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
        self.notice = format!(
            "{} AT ({}, {}) - WAITING FOR A WORKER",
            job.kind.name(),
            job.hex.q,
            job.hex.r
        );
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
                if self.cities[city].pending_building.is_some() && (source == 0 || target == 0) {
                    self.notice = "CONFIRM OR REMOVE THE READY BUILDING FIRST".into();
                    return;
                }
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

    pub(super) fn barracks_queue_panel(&self, i: usize, visible: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        if city.barracks_queue.is_empty() {
            return;
        }
        let production = self.barracks_income(i);
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
        panel.text(
            SMALL,
            vec![("BARRACKS QUEUE - DRAG TO REORDER".into(), LABEL_TEXT)],
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
            let remaining = build.cost()
                - if index == 0 {
                    city.barracks_production
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
                    "{prefix}{} | {} PROD | {} LEFT",
                    build.name(),
                    quantity(build.cost()),
                    turns_at_rate(remaining, production)
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
        let (_, production) = self.income(i);
        let offset = self
            .city_queue_scroll
            .min(city.queue.len().saturating_sub(visible));
        if city.queue.len() > visible {
            panel.scrollbar = Some((QueueKind::City, offset, city.queue.len(), visible));
        }
        if !city.queue.is_empty() {
            panel.text(
                SMALL,
                vec![("CITY QUEUE - DRAG TO REORDER".into(), LABEL_TEXT)],
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
                let cost = self.city_build_cost(i, build);
                let remaining = cost - if index == 0 { city.production } else { 0 };
                let drag = self.queue_drag.filter(|drag| drag.kind == QueueKind::City);
                panel.queue_item(QueueItemSpec {
                    kind: QueueKind::City,
                    index,
                    label: format!(
                        "{prefix}{} | {} PROD | {} LEFT",
                        build.name(),
                        quantity(cost),
                        turns_at_rate(remaining, production)
                    ),
                    active: index == 0,
                    dragging: drag.is_some_and(|drag| drag.source == index),
                    drop_target: drag
                        .is_some_and(|drag| drag.target == Some(index) && drag.source != index),
                    locked: index == 0 && city.pending_building.is_some(),
                });
            }
        }
    }
}
