//! Experimental immediate-mode presentation of the game's existing panel content.
//! Game rules and button actions remain shared with the classic UI.

use ::imgui::{
    Condition, DragDropFlags, FontId, ItemHoveredFlags, MouseButton as ImMouseButton, ProgressBar,
    StyleColor, StyleVar, Ui, WindowFlags,
};

use super::*;

enum Action {
    Button(Target),
    Reorder(QueueKind, usize, usize),
}

const PANEL_MARGIN: f32 = 14.0;
const PANEL_GAP: f32 = 8.0;
const STATUS_HEIGHT: f32 = 52.0;
const SLOT_COUNT: usize = 4;
const SELECTION: usize = 0;
const QUEUE: usize = 1;
const DEBUG: usize = 2;
const INSPECT: usize = 3;

#[derive(Clone, Copy, Default)]
struct WindowGeometry {
    pos: Vec2,
    size: Vec2,
    manual: bool,
}

/// ImGui windows follow collision-free docks until the player drags a title
/// bar or resize grip. Manual windows become obstacles for the remaining ones.
#[derive(Default)]
pub struct ImGuiLayoutState {
    windows: [WindowGeometry; SLOT_COUNT],
}

impl ImGuiLayoutState {
    fn begin_frame(&mut self, ui: &Ui) {
        if !ui.is_mouse_clicked(ImMouseButton::Left) {
            return;
        }
        let mouse = Vec2::from_array(ui.io().mouse_pos);
        for window in &mut self.windows {
            if window.size == Vec2::ZERO {
                continue;
            }
            let relative = mouse - window.pos;
            let title = relative.x >= 0.0
                && relative.x < window.size.x
                && relative.y >= 0.0
                && relative.y < 30.0;
            let grip = relative.x >= window.size.x - 20.0
                && relative.x <= window.size.x
                && relative.y >= window.size.y - 20.0
                && relative.y <= window.size.y;
            if title || grip {
                window.manual = true;
                break;
            }
        }
    }

    fn size(&self, slot: usize, measured: Vec2, viewport: Vec2) -> Vec2 {
        let preferred = if self.windows[slot].manual {
            self.windows[slot].size
        } else {
            measured
        };
        preferred.min(Vec2::new(
            viewport.x - PANEL_MARGIN * 2.0,
            viewport.y - STATUS_HEIGHT - PANEL_MARGIN * 2.0,
        ))
    }

    fn plan(
        &self,
        viewport: Vec2,
        sizes: &mut [Option<Vec2>; SLOT_COUNT],
    ) -> [Option<Vec2>; SLOT_COUNT] {
        let mut positions = [None; SLOT_COUNT];
        let mut dock = Dock::new(
            viewport,
            PANEL_MARGIN,
            PANEL_GAP,
            STATUS_HEIGHT + PANEL_MARGIN,
        );
        for (slot, maybe_size) in sizes.iter().enumerate() {
            if let Some(size) = maybe_size
                && self.windows[slot].manual
            {
                let max =
                    (viewport - *size - Vec2::splat(PANEL_MARGIN)).max(Vec2::splat(PANEL_MARGIN));
                let pos = self.windows[slot]
                    .pos
                    .clamp(Vec2::new(PANEL_MARGIN, STATUS_HEIGHT + PANEL_MARGIN), max);
                positions[slot] = Some(pos);
                dock.reserve(Rect {
                    min: Vec2::new(pos.x, viewport.y - pos.y - size.y),
                    max: Vec2::new(pos.x + size.x, viewport.y - pos.y),
                });
            }
        }
        for slot in 0..SLOT_COUNT {
            if positions[slot].is_some() {
                continue;
            }
            let Some(size) = sizes[slot] else { continue };
            let zone = match slot {
                SELECTION | QUEUE | INSPECT => Zone::BottomLeft,
                _ => Zone::TopRight,
            };
            for height_scale in [1.0, 0.85, 0.65, 0.45, 0.25] {
                for width_scale in [1.0, 0.95, 0.85, 0.7, 0.55] {
                    let candidate = Vec2::new(
                        (size.x * width_scale).max(210.0),
                        (size.y * height_scale).max(80.0),
                    );
                    if let Some(rect) = dock.place(candidate, zone) {
                        sizes[slot] = Some(candidate);
                        positions[slot] = Some(Vec2::new(rect.min.x, viewport.y - rect.max.y));
                        break;
                    }
                }
                if positions[slot].is_some() {
                    break;
                }
            }
        }
        positions
    }

    fn condition(&self, slot: usize, viewport: Vec2) -> Condition {
        let window = self.windows[slot];
        if !window.manual {
            return Condition::Always;
        }
        let max = viewport - window.size - Vec2::splat(PANEL_MARGIN);
        if window.pos.x < PANEL_MARGIN
            || window.pos.y < STATUS_HEIGHT
            || window.pos.x > max.x
            || window.pos.y > max.y
        {
            Condition::Always
        } else {
            Condition::FirstUseEver
        }
    }

    fn record(&mut self, slot: usize, ui: &Ui) {
        self.windows[slot].pos = Vec2::from_array(ui.window_pos());
        self.windows[slot].size = Vec2::from_array(ui.window_size());
    }
}

fn measure_panel(ui: &Ui, panel: &PanelBuilder, fonts: &[FontId; 3], width: f32) -> f32 {
    let inner = (width - 45.0).max(100.0);
    let mut height = 56.0; // title, border and padding
    for row in &panel.rows {
        height += match row {
            Row::Text(px, line) => {
                let font = match *px {
                    TITLE => fonts[2],
                    SMALL => fonts[0],
                    _ => fonts[1],
                };
                let _font = ui.push_font(font);
                let text = line.iter().map(|(s, _)| s.as_str()).collect::<String>();
                let measured = ui.calc_text_size(text);
                let lines = if line.len() == 1 {
                    (measured[0] / inner).ceil().max(1.0)
                } else {
                    1.0
                };
                measured[1] * lines + 7.0
            }
            Row::Gap(gap) => gap + 6.0,
            Row::Bar(_) => 18.0,
            Row::Buttons(buttons, compact) => {
                let columns = ((inner + 7.0) / 135.0).floor().max(1.0) as usize;
                let rows = buttons.len().div_ceil(columns);
                rows as f32 * (if *compact { 34.0 } else { 54.0 })
            }
            Row::QueueItem(_) => 37.0,
        };
    }
    height + 12.0
}

fn text_line(ui: &Ui, line: &Line) {
    for (index, (text, color)) in line.iter().enumerate() {
        if index != 0 {
            ui.same_line_with_spacing(0.0, 0.0);
        }
        let readable = if *color == LABEL_TEXT {
            [0.66, 0.70, 0.72, 1.0]
        } else if *color == DIM_TEXT {
            [0.72, 0.75, 0.76, 1.0]
        } else {
            *color
        };
        ui.text_colored(readable, text);
    }
}

impl GameState {
    fn render_imgui_window(
        &self,
        ui: &Ui,
        layout: &mut ImGuiLayoutState,
        slot: usize,
        title: &str,
        position: Vec2,
        size: Vec2,
        viewport: Vec2,
        panel: &PanelBuilder,
        fonts: &[FontId; 3],
        actions: &mut Vec<Action>,
    ) {
        let condition = layout.condition(slot, viewport);
        let old_size = layout.windows[slot].size;
        let fits = old_size.x <= viewport.x - 2.0 * PANEL_MARGIN
            && old_size.y <= viewport.y - STATUS_HEIGHT - 2.0 * PANEL_MARGIN;
        let size_condition = if layout.windows[slot].manual && fits {
            Condition::FirstUseEver
        } else {
            Condition::Always
        };
        ui.window(title)
            .position(position.to_array(), condition)
            .size(size.to_array(), size_condition)
            .size_constraints(
                [size.x.min(240.0), size.y.min(100.0)],
                [
                    viewport.x - 2.0 * PANEL_MARGIN,
                    viewport.y - STATUS_HEIGHT - 2.0 * PANEL_MARGIN,
                ],
            )
            .build(|| {
                if ui.is_window_appearing() {
                    ui.set_scroll_y(0.0);
                }
                self.render_imgui_panel(ui, panel, fonts, actions);
                layout.record(slot, ui);
            });
    }

    fn render_imgui_panel(
        &self,
        ui: &Ui,
        panel: &PanelBuilder,
        fonts: &[FontId; 3],
        actions: &mut Vec<Action>,
    ) {
        for row in &panel.rows {
            match row {
                Row::Text(px, line) => {
                    let font = match *px {
                        TITLE => fonts[2],
                        SMALL => fonts[0],
                        _ => fonts[1],
                    };
                    let _font = ui.push_font(font);
                    let _wrap = (line.len() == 1).then(|| ui.push_text_wrap_pos());
                    text_line(ui, line);
                }
                Row::Gap(height) => ui.dummy([0.0, height.max(0.0)]),
                Row::Bar(fraction) => {
                    ProgressBar::new(fraction.clamp(0.0, 1.0))
                        .size([ui.content_region_avail()[0], 12.0])
                        .build(ui);
                }
                Row::Buttons(buttons, compact) => {
                    if buttons.is_empty() {
                        continue;
                    }
                    let available = ui.content_region_avail()[0];
                    let spacing = ui.clone_style().item_spacing[0];
                    let min_width = 128.0;
                    let columns = (((available + spacing) / (min_width + spacing)).floor()
                        as usize)
                        .clamp(1, buttons.len());
                    let width = ((available - spacing * (columns - 1) as f32) / columns as f32)
                        .max(min_width);
                    for (index, spec) in buttons.iter().enumerate() {
                        if index % columns != 0 {
                            ui.same_line();
                        }
                        let _accent = match spec.state {
                            ButtonState::Queued => Some(
                                ui.push_style_color(StyleColor::Button, [0.34, 0.30, 0.17, 1.0]),
                            ),
                            _ if spec.armed => Some(
                                ui.push_style_color(StyleColor::Button, [0.24, 0.32, 0.24, 1.0]),
                            ),
                            _ => None,
                        };
                        let _disabled = ui.begin_disabled(spec.state == ButtonState::Disabled);
                        let label = if *compact {
                            if spec.hint.is_empty()
                                || matches!(spec.hint.as_str(), "AUTO" | "CLICK")
                            {
                                spec.label.clone()
                            } else {
                                format!("{}  {}", spec.label, spec.hint)
                            }
                        } else if spec.hint.is_empty() {
                            spec.label.clone()
                        } else {
                            format!("{}\n{}", spec.label, spec.hint)
                        };
                        let label = format!("{label}##{:?}", spec.target);
                        if ui.button_with_size(label, [width, if *compact { 28.0 } else { 48.0 }]) {
                            actions.push(Action::Button(spec.target));
                        }
                        if ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED) {
                            let tooltip = Button {
                                target: spec.target,
                                label: spec.label.clone(),
                                hint: spec.hint.clone(),
                                state: spec.state,
                                armed: spec.armed,
                                faded: false,
                                min: Vec2::ZERO,
                                max: Vec2::ZERO,
                            };
                            ui.tooltip(|| {
                                for (_, line) in self.tooltip_lines(&tooltip) {
                                    text_line(ui, &line);
                                }
                            });
                        }
                    }
                }
                Row::QueueItem(item) => {
                    let width = (ui.content_region_avail()[0] - 39.0).max(50.0);
                    let _align = ui.push_style_var(StyleVar::ButtonTextAlign([0.03, 0.5]));
                    let _background = ui.push_style_color(
                        StyleColor::Button,
                        if item.active {
                            [0.26, 0.24, 0.15, 1.0]
                        } else {
                            [0.11, 0.14, 0.16, 1.0]
                        },
                    );
                    ui.button_with_size(
                        format!(
                            "::  {}##queue-{:?}-{}",
                            item.label.trim_start_matches("> ").trim(),
                            item.kind,
                            item.index
                        ),
                        [width, 30.0],
                    );
                    drop(_background);
                    drop(_align);
                    if !item.locked && !self.is_resolving() {
                        let name = match item.kind {
                            QueueKind::City => "city-queue",
                            QueueKind::Barracks => "barracks-queue",
                        };
                        if let Some(source) =
                            ui.drag_drop_source_config(name).begin_payload(item.index)
                        {
                            ui.text(&item.label);
                            source.end();
                        }
                        if let Some(target) = ui.drag_drop_target() {
                            if let Some(Ok(payload)) =
                                target.accept_payload::<usize, _>(name, DragDropFlags::empty())
                                && payload.delivery
                            {
                                actions.push(Action::Reorder(item.kind, payload.data, item.index));
                            }
                            target.pop();
                        }
                    }
                    ui.same_line();
                    let _remove_color =
                        ui.push_style_color(StyleColor::Button, [0.23, 0.13, 0.13, 1.0]);
                    if ui.small_button(format!("X##remove-{:?}-{}", item.kind, item.index)) {
                        actions.push(Action::Button(match item.kind {
                            QueueKind::City => Target::CityQueueRemove(item.index),
                            QueueKind::Barracks => Target::BarracksQueueRemove(item.index),
                        }));
                    }
                }
            }
        }
    }

    /// Draw native ImGui windows over the Vulkan map and return UI input to the
    /// same game actions used by keyboard shortcuts and the classic panels.
    pub fn draw_imgui(
        &mut self,
        ui: &Ui,
        size: Vec2,
        cursor: Option<Vec2>,
        fonts: &[FontId; 3],
        layout: &mut ImGuiLayoutState,
    ) {
        let viewport = Vec2::from_array(ui.io().display_size);
        layout.begin_frame(ui);
        let mut actions = Vec::new();
        let pending = self.pending();
        let turn = if self.is_resolving() {
            self.turn
        } else {
            self.turn + 1
        };
        ui.window("Status")
            .flags(
                WindowFlags::NO_TITLE_BAR
                    | WindowFlags::NO_RESIZE
                    | WindowFlags::NO_MOVE
                    | WindowFlags::NO_SAVED_SETTINGS,
            )
            .position([0.0, 0.0], Condition::Always)
            .size([viewport.x, STATUS_HEIGHT], Condition::Always)
            .build(|| {
                let end_width = 220.0;
                ui.text(format!("TURN {turn}"));
                ui.same_line();
                let max_notice = (viewport.x - end_width - 130.0).max(0.0);
                if ui.calc_text_size(&self.notice)[0] <= max_notice {
                    ui.text_colored(NOTICE_TEXT, &self.notice);
                } else {
                    let mut shortened = self.notice.clone();
                    while !shortened.is_empty() && ui.calc_text_size(&shortened)[0] > max_notice {
                        shortened.pop();
                    }
                    ui.text_colored(NOTICE_TEXT, shortened);
                }
                ui.set_cursor_pos([(viewport.x - end_width).max(8.0), 7.0]);
                let label = if self.is_resolving() {
                    "RESOLVING".into()
                } else {
                    end_turn_label(pending)
                };
                let _disabled = ui.begin_disabled(self.is_resolving());
                if ui.button_with_size(format!("{label}  [SPACE]"), [end_width - 15.0, 29.0]) {
                    actions.push(Action::Button(Target::EndTurn));
                }
            });

        let mut tray = PanelBuilder::default();
        if let Some(city) = self.selected_city {
            self.city_tray(city, &mut tray);
        } else if let Some(city) = self.selected_barracks {
            self.barracks_tray(city, &mut tray);
        } else if let Some(idx) = self.selected {
            self.unit_info(idx, &mut tray);
            tray.gap(GAP);
            tray.buttons(self.unit_buttons(idx));
        } else if !self.group.is_empty() {
            self.group_tray(&mut tray);
        }
        let mut queue = PanelBuilder::default();
        if let Some(city) = self.selected_city {
            self.city_queue_panel(city, usize::MAX, &mut queue);
        } else if let Some(city) = self.selected_barracks {
            self.barracks_queue_panel(city, usize::MAX, &mut queue);
        }
        let debug = self.debug_panel_content();
        let mut hover = PanelBuilder::default();
        if let Some(hex) = self.hovered_tile {
            if let Some(city) = self.cities.iter().position(|city| city.pos == hex) {
                self.structure_hover_panel(city, false, &mut hover);
            } else if let Some(city) = self
                .cities
                .iter()
                .position(|city| city.barracks == Some(hex))
            {
                self.structure_hover_panel(city, true, &mut hover);
            } else if let Some(cursor) = cursor
                && let Some(idx) = self.unit_at_screen(cursor, size)
                && (Some(idx) != self.selected || self.selected_city.is_some())
            {
                self.unit_info(idx, &mut hover);
            }
            if self.hover_seconds >= TILE_TOOLTIP_DELAY && !ui.io().want_capture_mouse {
                let lines = self.tile_tooltip_lines(hex);
                ui.tooltip(|| {
                    for (_, line) in &lines {
                        text_line(ui, line);
                    }
                });
            }
        }

        // Measure the actual ImGui row geometry in logical pixels. The dock
        // reserves every visible panel before any window is presented.
        let available = (viewport.y - STATUS_HEIGHT - 2.0 * PANEL_MARGIN).max(100.0);
        let max_width = (viewport.x - 2.0 * PANEL_MARGIN).max(240.0);
        let debug_width = 330.0_f32.min(max_width);
        let left_width = if (760.0..1100.0).contains(&viewport.x) {
            (viewport.x - debug_width - 3.0 * PANEL_MARGIN - PANEL_GAP)
                .max(260.0)
                .min(max_width)
        } else {
            560.0_f32.min(max_width)
        };
        let queue_height = measure_panel(ui, &queue, fonts, left_width).min(225.0);
        let mut tray_height = measure_panel(ui, &tray, fonts, left_width).min(available);
        // On a narrow screen the queue cannot wrap into a second column, so
        // reserve its space above the selection before docking either window.
        if !tray.rows.is_empty()
            && !queue.rows.is_empty()
            && viewport.x < 2.0 * left_width + 3.0 * PANEL_MARGIN + PANEL_GAP
        {
            tray_height = tray_height.min((available - queue_height - PANEL_GAP).max(120.0));
        }
        let measured = [
            (!tray.rows.is_empty()).then_some(Vec2::new(left_width, tray_height)),
            (!queue.rows.is_empty()).then_some(Vec2::new(left_width, queue_height)),
            Some(Vec2::new(
                debug_width,
                measure_panel(ui, &debug, fonts, debug_width).min(available),
            )),
            (!hover.rows.is_empty()).then_some(Vec2::new(
                345.0_f32.min(max_width),
                measure_panel(ui, &hover, fonts, 345.0_f32.min(max_width)).min(available),
            )),
        ];
        let mut sizes = std::array::from_fn(|slot| {
            measured[slot].map(|size| layout.size(slot, size, viewport))
        });
        let positions = layout.plan(viewport, &mut sizes);

        if let (Some(position), Some(size)) = (positions[SELECTION], sizes[SELECTION]) {
            let id = if let Some(city) = self.selected_city {
                format!("City {}###selection-city-{city}", self.cities[city].id + 1)
            } else if let Some(city) = self.selected_barracks {
                format!(
                    "Barracks {}###selection-barracks-{city}",
                    self.cities[city].id + 1
                )
            } else if let Some(unit) = self.selected {
                format!("Unit###selection-unit-{}", self.units[unit].id)
            } else {
                "Group###selection-group".into()
            };
            self.render_imgui_window(
                ui,
                layout,
                SELECTION,
                &id,
                position,
                size,
                viewport,
                &tray,
                fonts,
                &mut actions,
            );
        }
        if let (Some(position), Some(size)) = (positions[QUEUE], sizes[QUEUE]) {
            self.render_imgui_window(
                ui,
                layout,
                QUEUE,
                "Production Queue",
                position,
                size,
                viewport,
                &queue,
                fonts,
                &mut actions,
            );
        }
        if let (Some(position), Some(size)) = (positions[DEBUG], sizes[DEBUG]) {
            self.render_imgui_window(
                ui,
                layout,
                DEBUG,
                "Debug",
                position,
                size,
                viewport,
                &debug,
                fonts,
                &mut actions,
            );
        }
        if let (Some(position), Some(size)) = (positions[INSPECT], sizes[INSPECT]) {
            self.render_imgui_window(
                ui,
                layout,
                INSPECT,
                "Inspect",
                position,
                size,
                viewport,
                &hover,
                fonts,
                &mut actions,
            );
        }
        for action in actions {
            match action {
                Action::Button(target) => self.activate_target(target),
                Action::Reorder(kind, source, target) => self.reorder_queue(kind, source, target),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_fits_without_overlap(
        viewport: Vec2,
        sizes: [Option<Vec2>; SLOT_COUNT],
        positions: [Option<Vec2>; SLOT_COUNT],
    ) {
        let mut rects = Vec::new();
        for (size, position) in sizes.into_iter().zip(positions) {
            if let (Some(size), Some(position)) = (size, position) {
                assert!(position.x >= PANEL_MARGIN);
                assert!(position.y >= STATUS_HEIGHT + PANEL_MARGIN);
                assert!(position.x + size.x <= viewport.x - PANEL_MARGIN + 1.0);
                assert!(position.y + size.y <= viewport.y - PANEL_MARGIN + 1.0);
                let rect = Rect {
                    min: position,
                    max: position + size,
                };
                assert!(rects.iter().all(|other| !rect.overlaps(*other, 0.0)));
                rects.push(rect);
            }
        }
    }

    #[test]
    fn logical_viewport_docks_full_panels_without_overlap() {
        let viewport = Vec2::new(2048.0, 1078.0);
        let mut sizes = [
            Some(Vec2::new(560.0, 630.0)),
            Some(Vec2::new(560.0, 220.0)),
            Some(Vec2::new(330.0, 340.0)),
            Some(Vec2::new(345.0, 190.0)),
        ];
        let positions = ImGuiLayoutState::default().plan(viewport, &mut sizes);
        assert!(positions.iter().all(Option::is_some));
        assert_fits_without_overlap(viewport, sizes, positions);
    }

    #[test]
    fn manual_window_is_reserved_for_automatic_panels() {
        let viewport = Vec2::new(1280.0, 720.0);
        let mut sizes = [
            Some(Vec2::new(500.0, 370.0)),
            Some(Vec2::new(500.0, 180.0)),
            Some(Vec2::new(280.0, 280.0)),
            None,
        ];
        let mut layout = ImGuiLayoutState::default();
        layout.windows[DEBUG] = WindowGeometry {
            pos: Vec2::new(510.0, 80.0),
            size: sizes[DEBUG].unwrap(),
            manual: true,
        };
        let positions = layout.plan(viewport, &mut sizes);
        assert!(positions[..3].iter().all(Option::is_some));
        assert_eq!(sizes[SELECTION].unwrap().y, 370.0);
        assert_fits_without_overlap(viewport, sizes, positions);
    }

    #[test]
    fn compact_viewport_keeps_core_panels_onscreen() {
        let viewport = Vec2::new(800.0, 600.0);
        let mut sizes = [
            Some(Vec2::new(420.0, 287.0)),
            Some(Vec2::new(420.0, 225.0)),
            Some(Vec2::new(330.0, 330.0)),
            Some(Vec2::new(345.0, 190.0)),
        ];
        let positions = ImGuiLayoutState::default().plan(viewport, &mut sizes);
        assert!(positions[..3].iter().all(Option::is_some));
        assert_fits_without_overlap(viewport, sizes, positions);
    }
}
