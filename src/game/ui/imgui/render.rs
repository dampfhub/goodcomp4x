//! Drawing a panel's rows: the shared `PanelBuilder` content as ImGui
//! widgets.

use super::*;

/// A setting's name and what it does, for the tooltip on its label and
/// control.
fn setting_tooltip(ui: &Ui, setting: Setting) {
    ui.tooltip(|| {
        ui.text_colored(TEXT, setting.name());
        for line in crate::game::ui::text::wrap(&setting.description(), TOOLTIP_WRAP) {
            ui.text_colored([0.72, 0.75, 0.76, 1.0], line);
        }
    });
}

/// A setting's row in the settings menu: its name in the label column, then
/// the control `Setting::control` names across the rest of the width, or
/// under the name when it has no room there (`setting_fit`). A change is a
/// `Target::SetSetting` action.
fn render_setting(
    ui: &Ui,
    setting: Setting,
    value: i32,
    label_width: f32,
    scope: Option<PinnedPanel>,
    actions: &mut Vec<Action>,
) {
    let range = setting.range();
    let mut set = |to: i32| {
        if to != value {
            actions.push(Action::Button(scope, Target::SetSetting(setting, to)));
        }
    };
    let tooltip = || {
        if ui.is_item_hovered() {
            setting_tooltip(ui, setting);
        }
    };
    let left = ui.cursor_pos()[0];
    let fit = setting_fit(ui, setting, label_width, ui.content_region_avail()[0]);
    ui.align_text_to_frame_padding();
    ui.text_colored([0.82, 0.84, 0.86, 1.0], setting.name());
    note_text_item(ui, setting.name());
    tooltip();
    if !fit.stacked {
        ui.same_line_with_pos(left + label_width);
    }
    let width = ui.content_region_avail()[0];
    let id = format!("##setting-{setting:?}");
    match setting.control() {
        Control::Toggle => {
            let mut on = value == 1;
            if ui.checkbox(format!("{}{id}", setting.value_text(value)), &mut on) {
                set(on as i32);
            }
            note_text_item(ui, "checkbox");
            tooltip();
        }
        Control::Slider => {
            let mut to = value;
            ui.set_next_item_width(width);
            // ImGui formats the value itself; the text has no %d, so it
            // shows `value_text` as it is.
            let shown = setting.value_text(value).replace('%', "%%");
            if ui
                .slider_config(&id, *range.start(), *range.end())
                .display_format(shown)
                .flags(SliderFlags::ALWAYS_CLAMP | SliderFlags::NO_INPUT)
                .build(&mut to)
            {
                set(to);
            }
            note_button_label(ui, &setting.value_text(value));
            tooltip();
        }
        Control::Choice if fit.combo => {
            ui.set_next_item_width(width);
            match ui.begin_combo(&id, setting.value_text(value)) {
                Some(_combo) => {
                    for to in range {
                        let label = format!("{}{id}-{to}", setting.value_text(to));
                        if ui.selectable_config(label).selected(to == value).build() {
                            set(to);
                        }
                    }
                }
                None => tooltip(),
            }
        }
        Control::Choice => {
            let count = range.clone().count() as f32;
            let spacing = ui.clone_style().item_spacing[0];
            let each = ((width - spacing * (count - 1.0)) / count).floor();
            for (index, to) in range.enumerate() {
                if index != 0 {
                    ui.same_line();
                }
                // The current choice is gold, like a queued build.
                let _accent = (to == value).then(|| {
                    (
                        ui.push_style_color(StyleColor::Button, [0.34, 0.30, 0.17, 1.0]),
                        ui.push_style_color(StyleColor::ButtonHovered, [0.42, 0.37, 0.20, 1.0]),
                    )
                });
                let label = format!("{}{id}-{to}", setting.value_text(to));
                if ui.button_with_size(&label, [each, 0.0]) {
                    set(to);
                }
                note_button_label(ui, &label);
                tooltip();
            }
        }
    }
}

/// A typed field's row (the Multiplayer section): its name in the label
/// column, then a text box across the rest (under the name when there is
/// no room beside it); an edit is an `Action::Text`.
fn render_field(ui: &Ui, field: NetField, text: &str, label_width: f32, actions: &mut Vec<Action>) {
    let left = ui.cursor_pos()[0];
    let stacked = field_stacked(label_width, ui.content_region_avail()[0]);
    ui.align_text_to_frame_padding();
    ui.text_colored([0.82, 0.84, 0.86, 1.0], field.name());
    note_text_item(ui, field.name());
    if !stacked {
        ui.same_line_with_pos(left + label_width);
    }
    ui.set_next_item_width(ui.content_region_avail()[0]);
    let mut edited = text.to_string();
    let flags = match field {
        NetField::Port => InputTextFlags::CHARS_DECIMAL,
        NetField::Code => InputTextFlags::CHARS_UPPERCASE | InputTextFlags::CHARS_NO_BLANK,
        NetField::Address => InputTextFlags::CHARS_NO_BLANK,
    };
    // `flags` replaces imgui-rs's own, so it keeps the one that lets the
    // text outgrow the string it started as (without it, an empty box took
    // seven characters). Ctrl+C, X, V and A work through the clipboard
    // `App` gives the context; the box keeps what's typed or pasted as the
    // field does.
    let changed = ui
        .input_text(format!("##field-{field:?}"), &mut edited)
        .flags(flags | InputTextFlags::CALLBACK_RESIZE)
        .callback(InputTextCallback::EDIT, CleanField(field))
        .build();
    // The box is where classic's button to type into the field would be.
    note_drawn_button(ui, Target::EditNetField(field));
    if changed && edited != text {
        actions.push(Action::Text(field, edited));
    }
}

/// Keeps a field's text box to what the field takes (`NetField::clean`)
/// while it's being edited, so a long paste shows cut to the field's length
/// rather than until the box lets go.
struct CleanField(NetField);

impl InputTextCallbackHandler for CleanField {
    fn on_edit(&mut self, mut data: TextCallbackData) {
        let clean = self.0.clean(data.str());
        if clean != data.str() {
            data.clear();
            data.push_str(&clean);
        }
    }
}

impl GameState {
    /// A row of buttons (`Row::Buttons`), wrapping to the window's width;
    /// with `reorder`, each can be dragged onto another to reorder that
    /// queue, index by index (`Row::Reorder`), and a click is still its
    /// own target.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_buttons(
        &self,
        ui: &Ui,
        panel: &PanelBuilder,
        buttons: &[ButtonSpec],
        compact: bool,
        reorder: Option<QueueKind>,
        fonts: &[FontId; 3],
        scope: Option<PinnedPanel>,
        subject: Subject,
        actions: &mut Vec<Action>,
    ) {
        if buttons.is_empty() {
            return;
        }
        let available = ui.content_region_avail()[0];
        let icons = icon_row(buttons);
        let grid = button_grid(ui, buttons, compact, panel.faded, available);
        let columns = grid.columns;
        let size = [grid.width, grid.height];
        for (index, spec) in buttons.iter().enumerate() {
            if index % columns != 0 {
                ui.same_line();
            }
            let _accent = match spec.state() {
                ButtonState::Queued => {
                    Some(ui.push_style_color(StyleColor::Button, [0.34, 0.30, 0.17, 1.0]))
                }
                _ if spec.armed => {
                    Some(ui.push_style_color(StyleColor::Button, [0.24, 0.32, 0.24, 1.0]))
                }
                _ => None,
            };
            let _short = short_style(ui, spec);
            let disabled = spec.unavailable.is_some();
            let _disabled = ui.begin_disabled(disabled);
            let id = format!("{:?}", spec.target);
            // Hovered, its hover text, laid out in the button as it is.
            let hovered = hovered_spec(spec, &id).filter(|_| !icons).map(|shown| {
                let one = button_grid(ui, &[shown], compact, panel.faded, grid.width);
                (
                    one.lines.into_iter().next().unwrap_or_default(),
                    one.hint_from[0],
                )
            });
            let (lines, hint_from) = match &hovered {
                Some((lines, hint_from)) => (lines, *hint_from),
                None => (&grid.lines[index], grid.hint_from[index]),
            };
            let hint = hint_from.zip(short_price(spec));
            if rich_button_hinted(ui, &id, lines, size, false, hint) {
                actions.push(Action::Button(scope, spec.target));
            }
            note_drawn_button(ui, spec.target);
            if icons {
                let icon = action_icons::for_button(spec.target, &spec.label).expect("icon row");
                let color = match spec.state() {
                    ButtonState::Disabled => DIM_TEXT,
                    ButtonState::Queued => GOLD_TEXT,
                    ButtonState::Ready if spec.armed => BOOSTED_TEXT,
                    ButtonState::Ready => TEXT,
                };
                draw_action_icon(
                    ui,
                    ui.item_rect_min(),
                    ui.item_rect_max(),
                    icon,
                    color,
                    action_icons::badge(&spec.label),
                    fonts[0],
                );
            }
            if let Some(kind) = reorder
                && !disabled
                && !self.is_playing_out()
            {
                // Scoped to the panel, so one city's chips can't reorder
                // another's.
                let name = format!("{kind:?}-reorder-{scope:?}");
                if let Some(source) = ui.drag_drop_source_config(&name).begin_payload(index) {
                    rich_text(ui, &spec.label, TEXT);
                    source.end();
                }
                if let Some(target) = ui.drag_drop_target() {
                    if let Some(Ok(payload)) =
                        target.accept_payload::<usize, _>(&name, DragDropFlags::empty())
                        && payload.delivery
                    {
                        actions.push(Action::Reorder(scope, kind, payload.data, index));
                    }
                    target.pop();
                }
            }
            self.button_tooltip(ui, spec, subject);
        }
    }

    /// The tooltip of the panel button just drawn, for the unit or city of
    /// its panel (`subject`), while it's hovered (dimmed or not).
    fn button_tooltip(&self, ui: &Ui, spec: &ButtonSpec, subject: Subject) {
        if !ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED) {
            return;
        }
        show_tooltip(
            ui,
            &self.subject_tooltip_lines(
                spec.target,
                &spec.label,
                spec.unavailable.as_deref(),
                subject,
            ),
        );
    }

    /// Draws `panel`'s rows. `scope` is the captured panel they're in, if
    /// any, and `subject` the unit or city its buttons act on.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_imgui_panel(
        &self,
        ui: &Ui,
        panel: &PanelBuilder,
        fonts: &[FontId; 3],
        scope: Option<PinnedPanel>,
        subject: Subject,
        actions: &mut Vec<Action>,
    ) {
        let label_width = setting_label_width(ui, panel, fonts[1]);
        for row in flat_rows(&panel.rows) {
            match row {
                Row::Heading(text) => {
                    let _font = ui.push_font(fonts[0]);
                    ui.text_colored(GOLD_TEXT, text);
                    note_text_item(ui, text);
                    ui.separator();
                }
                Row::Setting(setting, value) => {
                    let _font = ui.push_font(fonts[1]);
                    render_setting(ui, *setting, *value, label_width, scope, actions);
                }
                Row::Field(field, text, _) => {
                    let _font = ui.push_font(fonts[1]);
                    render_field(ui, *field, text, label_width, actions);
                }
                Row::Text(px, line) => {
                    let font = match *px {
                        TITLE => fonts[2],
                        SMALL => fonts[0],
                        _ => fonts[1],
                    };
                    let _font = ui.push_font(font);
                    panel_text_line(ui, line);
                }
                Row::Gap(height) => ui.dummy([0.0, height.max(0.0)]),
                Row::ScrollList(_) => unreachable!("flattened by flat_rows"),
                Row::LabeledButtons(..) => unreachable!("classic only"),
                Row::Roster(chips) => {
                    let io = ui.io();
                    let mode = if io.key_shift {
                        ClickMode::QueueMove
                    } else if io.key_ctrl {
                        ClickMode::Swap
                    } else {
                        ClickMode::Normal
                    };
                    for (index, chip) in chips.iter().enumerate() {
                        if index != 0 {
                            ui.same_line_with_spacing(0.0, ROSTER_CHIP_GAP);
                        }
                        let clicked = ui.invisible_button(
                            format!("##roster-{:?}", chip.key),
                            [ROSTER_CHIP, ROSTER_CHIP],
                        );
                        let hovered = ui.is_item_hovered();
                        draw_roster_chip(ui, ui.item_rect_min(), ui.item_rect_max(), chip, hovered);
                        note_drawn_button(ui, Target::RosterSelect(chip.key));
                        if clicked {
                            actions.push(Action::Button(scope, roster_target(chip.key, mode)));
                        }
                        if hovered {
                            let target = Target::RosterSelect(chip.key);
                            show_tooltip(
                                ui,
                                &self.subject_tooltip_lines(target, "", None, subject),
                            );
                        }
                    }
                }
                Row::Bar(fraction) => {
                    // No "0%" overlay: it doesn't fit a 12 px bar, and the row
                    // above already says what the bar counts toward.
                    ProgressBar::new(fraction.clamp(0.0, 1.0))
                        .size([ui.content_region_avail()[0], BAR_HEIGHT])
                        .overlay_text("")
                        .build(ui);
                }
                Row::Buttons(buttons, compact) => self.render_buttons(
                    ui, panel, buttons, *compact, None, fonts, scope, subject, actions,
                ),
                Row::Reorder(kind, buttons) => self.render_buttons(
                    ui,
                    panel,
                    buttons,
                    true,
                    Some(*kind),
                    fonts,
                    scope,
                    subject,
                    actions,
                ),
                Row::TitleWithButton(line, spec) => {
                    let label = title_button_label(spec, panel.faded);
                    let width = rich_width(ui, &label) + 2.0 * ui.clone_style().frame_padding[0];
                    let beside =
                        title_button_fits(ui, line, &label, fonts[0], ui.content_region_avail()[0]);
                    {
                        let _font = ui.push_font(fonts[0]);
                        if beside {
                            ui.align_text_to_frame_padding();
                            text_line(ui, line);
                        } else {
                            panel_text_line(ui, line);
                        }
                    }
                    // At the right end of the title's line, or of the next
                    // when the title leaves it no room.
                    if beside {
                        ui.same_line();
                    }
                    let room = ui.content_region_avail()[0] - width;
                    if room > 0.0 {
                        let [x, y] = ui.cursor_pos();
                        ui.set_cursor_pos([x + room, y]);
                    }
                    let _disabled = ui.begin_disabled(spec.unavailable.is_some());
                    let id = format!("{:?}", spec.target);
                    // Hovered, its hover text, in the button as it is.
                    let label = hovered_spec(spec, &id)
                        .map_or(label, |shown| title_button_label(&shown, panel.faded));
                    if rich_button(ui, &id, &[label], [width, 0.0], false) {
                        actions.push(Action::Button(scope, spec.target));
                    }
                    note_drawn_button(ui, spec.target);
                    self.button_tooltip(ui, spec, subject);
                }
                Row::BuildingCatalog(city, buttons, ..) => {
                    let height = buttons.len().clamp(1, 5) as f32 * 34.0 + 18.0;
                    ui.child_window(format!("##building-catalog-{city}-{scope:?}"))
                        .size([0.0, height])
                        .border(true)
                        .build(|| {
                            let _align = ui.push_style_var(StyleVar::ButtonTextAlign([0.03, 0.5]));
                            for entry in buttons {
                                let CatalogEntry::Card(spec) = entry else {
                                    if let CatalogEntry::Heading(label) = entry {
                                        ui.text_colored(LABEL_TEXT, *label);
                                        note_text_item(ui, label);
                                        ui.dummy([1.0, 4.0]);
                                    }
                                    continue;
                                };
                                let _accent = match spec.state() {
                                    ButtonState::Queued => Some(ui.push_style_color(
                                        StyleColor::Button,
                                        [0.34, 0.30, 0.17, 1.0],
                                    )),
                                    _ => None,
                                };
                                let _short = short_style(ui, spec);
                                let _disabled = ui.begin_disabled(spec.unavailable.is_some());
                                let width = ui.content_region_avail()[0].max(80.0);
                                let hint = visible_button_hint(&spec.hint, false);
                                let has_icon =
                                    action_icons::production_unit_icon(spec.target).is_some();
                                let indent = if has_icon { CARD_ICON_ROOM } else { 0.0 };
                                let id = format!("{:?}", spec.target);
                                let price = short_price(spec);
                                let size = [width, 28.0];
                                if split_button(ui, &id, &spec.label, indent, (hint, price), size) {
                                    actions.push(Action::Button(scope, spec.target));
                                }
                                note_drawn_button(ui, spec.target);
                                if let Some(icon) = action_icons::production_unit_icon(spec.target)
                                {
                                    draw_production_icon(
                                        ui,
                                        ui.item_rect_min(),
                                        ui.item_rect_max(),
                                        icon,
                                    );
                                }
                                self.button_tooltip(ui, spec, subject);
                            }
                        });
                }
                Row::QueueItem(item) => {
                    let width = (ui.content_region_avail()[0] - QUEUE_X_ROOM).max(QUEUE_ROW_MIN);
                    let lines = queue_row_lines(ui, item, width);
                    let _align = ui.push_style_var(StyleVar::ButtonTextAlign([0.03, 0.5]));
                    let _background = ui.push_style_color(
                        StyleColor::Button,
                        if item.active {
                            [0.26, 0.24, 0.15, 1.0]
                        } else if item.waiting {
                            // Waits for the stockpile: a dull red.
                            [0.24, 0.10, 0.08, 1.0]
                        } else {
                            [0.11, 0.14, 0.16, 1.0]
                        },
                    );
                    if rich_button(
                        ui,
                        &format!("queue-{:?}-{}", item.kind, item.index),
                        &lines,
                        [width, queue_row_height(ui, lines.len())],
                        true,
                    ) {
                        actions.push(Action::Button(
                            scope,
                            Target::QueueItem(item.kind, item.index),
                        ));
                    }
                    let row = Target::QueueItem(item.kind, item.index);
                    note_drawn_button(ui, row);
                    // Not while it is pressed or dragged: its payload shows then.
                    if ui.is_item_hovered() && !ui.is_item_active() {
                        show_tooltip(ui, &self.subject_tooltip_lines(row, "", None, subject));
                    }
                    drop(_background);
                    drop(_align);
                    if !self.is_playing_out() {
                        let name = match item.kind {
                            QueueKind::City => "city-queue",
                            QueueKind::Barracks => "barracks-queue",
                            QueueKind::Workers => "worker-jobs",
                            QueueKind::Priority => "priority",
                            QueueKind::Research => "research-queue",
                        };
                        let name = format!("{name}-{:?}", scope);
                        if let Some(source) =
                            ui.drag_drop_source_config(&name).begin_payload(item.index)
                        {
                            rich_text(ui, &item.label, TEXT);
                            source.end();
                        }
                        if let Some(target) = ui.drag_drop_target() {
                            if let Some(Ok(payload)) =
                                target.accept_payload::<usize, _>(&name, DragDropFlags::empty())
                                && payload.delivery
                            {
                                actions.push(Action::Reorder(
                                    scope,
                                    item.kind,
                                    payload.data,
                                    item.index,
                                ));
                            }
                            target.pop();
                        }
                    }
                    ui.same_line();
                    let _remove_color =
                        ui.push_style_color(StyleColor::Button, [0.23, 0.13, 0.13, 1.0]);
                    let remove = item.kind.remove_target(item.index);
                    if ui.small_button(format!("X##remove-{:?}-{}", item.kind, item.index)) {
                        actions.push(Action::Button(scope, remove));
                    }
                    note_button_label(ui, "X");
                    note_drawn_button(ui, remove);
                    if ui.is_item_hovered() {
                        show_tooltip(ui, &self.subject_tooltip_lines(remove, "X", None, subject));
                    }
                }
            }
        }
    }
}
