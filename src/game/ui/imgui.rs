//! Experimental immediate-mode presentation of the game's existing panel content.
//! Game rules and button actions remain shared with the classic UI.

use ::imgui::{
    Condition, DragDropFlags, FontId, ItemHoveredFlags, ProgressBar, StyleColor, StyleVar, Ui,
    WindowFlags,
};

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
    pub fn draw_imgui(&mut self, ui: &Ui, size: Vec2, cursor: Option<Vec2>, fonts: &[FontId; 3]) {
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
                let end_width = 220.0;
                ui.text(format!("TURN {turn}"));
                ui.same_line();
                let max_notice = (size.x - end_width - 130.0).max(0.0);
                if ui.calc_text_size(&self.notice)[0] <= max_notice {
                    ui.text_colored(NOTICE_TEXT, &self.notice);
                } else {
                    let mut shortened = self.notice.clone();
                    while !shortened.is_empty() && ui.calc_text_size(&shortened)[0] > max_notice {
                        shortened.pop();
                    }
                    ui.text_colored(NOTICE_TEXT, shortened);
                }
                ui.set_cursor_pos([(size.x - end_width).max(8.0), 7.0]);
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
        let tray_height = tray.size().y.max(170.0).min(size.y * 0.48);
        let tray_width = 620.0_f32.min(size.x * 0.40).max(300.0);
        let tray_y = (size.y - tray_height - 18.0).max(60.0);
        if !tray.rows.is_empty() {
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
            ui.window(id)
                .position([18.0, tray_y], Condition::FirstUseEver)
                .size([tray_width, tray_height], Condition::FirstUseEver)
                .build(|| {
                    if ui.is_window_appearing() {
                        ui.set_scroll_y(0.0);
                    }
                    self.render_imgui_panel(ui, &tray, fonts, &mut actions)
                });
        }

        let mut queue = PanelBuilder::default();
        if let Some(city) = self.selected_city {
            self.city_queue_panel(city, usize::MAX, &mut queue);
        } else if let Some(city) = self.selected_barracks {
            self.barracks_queue_panel(city, usize::MAX, &mut queue);
        }
        if !queue.rows.is_empty() {
            let queue_height = 245.0_f32.min(size.y * 0.35);
            let queue_y = tray_y - queue_height - 8.0;
            let queue_position = if queue_y >= 58.0 {
                [18.0, queue_y]
            } else {
                [tray_width + 26.0, (size.y - queue_height - 18.0).max(58.0)]
            };
            ui.window("Production Queue")
                .position(queue_position, Condition::FirstUseEver)
                .size([tray_width, queue_height], Condition::FirstUseEver)
                .build(|| self.render_imgui_panel(ui, &queue, fonts, &mut actions));
        }

        let debug = self.debug_panel_content();
        ui.window("Debug")
            .position([(size.x - 350.0).max(18.0), 70.0], Condition::FirstUseEver)
            .size([330.0, 330.0], Condition::FirstUseEver)
            .build(|| self.render_imgui_panel(ui, &debug, fonts, &mut actions));

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
                let occupied_above_tray = if queue.rows.is_empty() {
                    0.0
                } else {
                    245.0_f32.min(size.y * 0.35) + 8.0
                };
                let inspect_y = tray_y - occupied_above_tray - 223.0;
                let inspect_position = if inspect_y >= 60.0 {
                    [18.0, inspect_y]
                } else {
                    [tray_width + 26.0, (size.y - 240.0).max(60.0)]
                };
                ui.window("Inspect")
                    .position(inspect_position, Condition::FirstUseEver)
                    .size([345.0, 215.0], Condition::FirstUseEver)
                    .build(|| self.render_imgui_panel(ui, &hover, fonts, &mut actions));
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
