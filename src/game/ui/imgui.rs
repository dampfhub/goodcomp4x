//! Experimental immediate-mode presentation of the game's existing panel content.
//! Game rules and button actions remain shared with the classic UI.

use ::imgui::{Condition, DragDropFlags, ProgressBar, Ui, WindowFlags};

use super::*;

enum Action {
    Button(Target),
    Reorder(QueueKind, usize, usize),
}

fn text_line(ui: &Ui, line: &Line) {
    for (index, (text, color)) in line.iter().enumerate() {
        if index != 0 {
            ui.same_line_with_spacing(0.0, 0.0);
        }
        ui.text_colored(*color, text);
    }
}

impl GameState {
    fn render_imgui_panel(&self, ui: &Ui, panel: &PanelBuilder, actions: &mut Vec<Action>) {
        for row in &panel.rows {
            match row {
                Row::Text(_, line) => text_line(ui, line),
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
                    let width = ((available - spacing * (buttons.len() - 1) as f32)
                        / buttons.len() as f32)
                        .max(35.0);
                    for (index, spec) in buttons.iter().enumerate() {
                        if index != 0 {
                            ui.same_line();
                        }
                        let _disabled = ui.begin_disabled(spec.state == ButtonState::Disabled);
                        let label = if *compact || spec.hint.is_empty() {
                            spec.label.clone()
                        } else {
                            format!("{}\n{}", spec.label, spec.hint)
                        };
                        let label = format!("{label}##{:?}", spec.target);
                        if ui.button_with_size(label, [width, if *compact { 28.0 } else { 48.0 }]) {
                            actions.push(Action::Button(spec.target));
                        }
                        if ui.is_item_hovered() {
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
                    let width = (ui.content_region_avail()[0] - 42.0).max(50.0);
                    ui.button_with_size(
                        format!("{}##queue-{:?}-{}", item.label, item.kind, item.index),
                        [width, 33.0],
                    );
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
    pub fn draw_imgui(&mut self, ui: &Ui, size: Vec2, cursor: Option<Vec2>) {
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
            .size([size.x, 52.0], Condition::Always)
            .build(|| {
                ui.text(format!("TURN {turn}"));
                ui.same_line();
                ui.text_colored(NOTICE_TEXT, &self.notice);
                ui.same_line_with_pos((size.x - 230.0).max(270.0));
                let label = if self.is_resolving() {
                    "RESOLVING".into()
                } else {
                    end_turn_label(pending)
                };
                let _disabled = ui.begin_disabled(self.is_resolving());
                if ui.button_with_size(format!("{label}  [SPACE]"), [205.0, 28.0]) {
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
        if !tray.rows.is_empty() {
            let width = tray.size().x.max(340.0).min(size.x * 0.45);
            let height = tray.size().y.max(150.0).min(size.y * 0.48);
            ui.window("Selection")
                .position(
                    [18.0, (size.y - height - 18.0).max(60.0)],
                    Condition::FirstUseEver,
                )
                .size([width, height], Condition::FirstUseEver)
                .build(|| self.render_imgui_panel(ui, &tray, &mut actions));
        }

        let mut queue = PanelBuilder::default();
        if let Some(city) = self.selected_city {
            self.city_queue_panel(city, usize::MAX, &mut queue);
        } else if let Some(city) = self.selected_barracks {
            self.barracks_queue_panel(city, usize::MAX, &mut queue);
        }
        if !queue.rows.is_empty() {
            ui.window("Production Queue")
                .position(
                    [
                        18.0,
                        (size.y - tray.size().y.min(size.y * 0.48) - 260.0).max(60.0),
                    ],
                    Condition::FirstUseEver,
                )
                .size(
                    [500.0_f32.min(size.x * 0.45), 230.0_f32.min(size.y * 0.35)],
                    Condition::FirstUseEver,
                )
                .build(|| self.render_imgui_panel(ui, &queue, &mut actions));
        }

        let debug = self.debug_panel_content();
        ui.window("Debug")
            .position([(size.x - 350.0).max(18.0), 70.0], Condition::FirstUseEver)
            .size([330.0, 235.0], Condition::FirstUseEver)
            .build(|| self.render_imgui_panel(ui, &debug, &mut actions));

        if let Some(hex) = self.hovered_tile {
            let mut hover = PanelBuilder::default();
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
            if !hover.rows.is_empty() {
                ui.window("Inspect")
                    .position([size.x - 365.0, size.y - 265.0], Condition::FirstUseEver)
                    .size([345.0, 245.0], Condition::FirstUseEver)
                    .build(|| self.render_imgui_panel(ui, &hover, &mut actions));
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
        for action in actions {
            match action {
                Action::Button(target) => self.activate_target(target),
                Action::Reorder(kind, source, target) => self.reorder_queue(kind, source, target),
            }
        }
    }
}
