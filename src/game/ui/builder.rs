//! `PanelBuilder`, the panel content primitive, and its text and button measuring.

use super::action_icons::{self, CLASSIC_ICON_COLUMNS, ICON_BUTTON_SIZE};
use super::network_menu::NetField;
use super::settings_menu::classic_setting_rows;
use super::{
    BODY, BUILDING_LIST_VISIBLE, BUTTON_HEIGHT, BUTTON_MIN_WIDTH, BUTTON_PADDING,
    BuildingScrollRegion, Button, ButtonState, END_TURN_HEIGHT, GAP, GOLD_TEXT, GROWTH_BAR_HEIGHT,
    LABEL_TEXT, LINE_GAP, Layout, Line, PADDING, QUEUE_ITEM_GAP, QUEUE_ITEM_HEIGHT,
    QUEUE_REMOVE_WIDTH, QueueItemRegion, QueueItemSpec, QueueKind, QueueScrollRegion, ROSTER_CHIP,
    ROSTER_CHIP_GAP, RosterChip, SCROLLBAR_WIDTH, SMALL, Shape, TITLE_BUTTON_HEIGHT,
    TITLE_ROW_HEIGHT, Target, UnitAction,
};
use crate::game::font::{self, Face};
use crate::game::settings::Setting;
use glam::Vec2;

/// A button before it's placed.
#[derive(Clone)]
pub(super) struct ButtonSpec {
    pub(super) target: Target,
    pub(super) label: String,
    pub(super) hint: String,
    pub(super) state: ButtonState,
    pub(super) armed: bool,
}

impl ButtonSpec {
    pub(super) fn plain(action: UnitAction, label: &str, hint: &str) -> Self {
        Self {
            target: Target::Unit(action),
            label: label.into(),
            hint: hint.into(),
            state: ButtonState::Ready,
            armed: false,
        }
    }
}

/// `PanelBuilder::freeze_plan` on `rows`, and on the rows of any scrolling
/// list among them.
fn freeze_rows(rows: &mut [Row]) {
    let freeze = |spec: &mut ButtonSpec| {
        if spec.target.changes_plan() {
            spec.state = ButtonState::Disabled;
            spec.armed = false;
        }
    };
    for row in rows {
        match row {
            Row::Buttons(buttons, _)
            | Row::Reorder(_, buttons)
            | Row::LabeledButtons(_, buttons) => buttons.iter_mut().for_each(freeze),
            Row::TitleWithButton(_, button) => freeze(button),
            Row::BuildingCatalog(_, entries, ..) => {
                for entry in entries {
                    if let CatalogEntry::Card(button) = entry {
                        freeze(button);
                    }
                }
            }
            Row::ScrollList(list) => freeze_rows(&mut list.entries),
            Row::QueueItem(item) => item.locked = true,
            Row::Text(..)
            | Row::Gap(_)
            | Row::Bar(_)
            | Row::Roster(_)
            | Row::Heading(_)
            | Row::Setting(..)
            | Row::Field(..) => {}
        }
    }
}

/// Keep costs and work times on buttons, but show keyboard shortcuts only
/// where `reveal_shortcut` asks (the Debug panel): elsewhere a button's key
/// is in its tooltip, so hovering never changes a button's text.
pub(super) fn visible_button_hint(hint: &str, reveal_shortcut: bool) -> &str {
    if reveal_shortcut {
        return hint;
    }
    for separator in [" · ", " | "] {
        if let Some((prefix, rest)) = hint.split_once(separator)
            && is_shortcut(prefix)
        {
            return if is_shortcut(rest) { "" } else { rest };
        }
    }
    if is_shortcut(hint) || matches!(hint, "AUTO" | "CLICK") {
        ""
    } else {
        hint
    }
}

pub(super) fn icon_row(buttons: &[ButtonSpec]) -> bool {
    !buttons.is_empty()
        && buttons
            .iter()
            .all(|button| action_icons::for_button(button.target, &button.label).is_some())
}

fn is_shortcut(text: &str) -> bool {
    let text = text.trim();
    if let Some((first, second)) = text.split_once(" / ") {
        return is_shortcut(first) && is_shortcut(second);
    }
    if let Some(rest) = text
        .strip_prefix("CTRL-")
        .or_else(|| text.strip_prefix("CTRL+"))
    {
        return is_shortcut(rest);
    }
    if let Some(rest) = text.strip_prefix("SHIFT+") {
        return is_shortcut(rest);
    }
    matches!(
        text,
        "CTRL" | "SHIFT" | "ALT" | "SPACE" | "ESC" | "DEL" | "BACKSPACE" | "RMB"
    ) || (text.len() == 1
        && text
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
        || text
            .strip_prefix('F')
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit()))
}

#[derive(Clone)]
pub(super) enum Row {
    Text(u32, Line),
    Gap(f32),
    Bar(f32),
    QueueItem(QueueItemSpec),
    /// Buttons of equal width; compact ones are one line, label then hint.
    Buttons(Vec<ButtonSpec>, bool),
    /// A line of small text with a one-line button at the row's right end: a
    /// queue panel's title and its Clear button.
    TitleWithButton(Line, ButtonSpec),
    /// A bounded, independently scrollable list within the city tray: the
    /// city, its cards, the first card shown and how many cards show at once
    /// in classic (`PanelBuilder::fit_height` can make that fewer).
    BuildingCatalog(usize, Vec<CatalogEntry>, usize, usize),
    /// Rows that scroll within the panel when it has to be shorter than
    /// they are (classic): the city tray's workers and jobs. ImGui, whose
    /// windows scroll, shows every one (`flat_rows`).
    ScrollList(ScrollList),
    /// A row of unit tokens in the unit strip.
    Roster(Vec<RosterChip>),
    /// A heading over a group of rows, with a rule under it in ImGui.
    Heading(String),
    /// A player setting at its current value, changed with the control its
    /// `Setting::control` names: ImGui draws that widget, classic places
    /// the buttons `settings_menu::classic_setting_rows` lays out.
    Setting(Setting, i32),
    /// A field the player types into (the Multiplayer section), with its
    /// text and whether keys go into it: ImGui draws a text box, classic a
    /// button that starts typing into it (`classic_rows`).
    Field(NetField, String, bool),
    /// A row of one-line buttons (icons, if every one has one) that can be
    /// dragged onto one another to reorder them, as `kind`'s queue rows
    /// are, index by index; a click (not a drag) is the button's own
    /// target. The open city's priority chips.
    Reorder(QueueKind, Vec<ButtonSpec>),
    /// Classic only (`classic_rows` makes it of a setting): a line of text
    /// at the row's left and compact buttons of equal width at its right
    /// end.
    LabeledButtons(Line, Vec<ButtonSpec>),
}

impl Row {
    /// Whether this is a row of buttons, whose borders, drawn just outside
    /// it, need a gap from another such row.
    pub(super) fn is_buttons(&self) -> bool {
        matches!(
            self,
            Row::Buttons(..) | Row::Reorder(..) | Row::LabeledButtons(..)
        )
    }
}

#[derive(Clone)]
pub(super) enum CatalogEntry {
    Heading(&'static str),
    Card(ButtonSpec),
}

/// A run of a panel's rows that scrolls, a whole row at a time, in a window
/// of its own inside the panel, with a scrollbar beside it when the rows
/// don't all fit. The window is as tall as the rows until
/// `PanelBuilder::fit_height` gives it less room.
#[derive(Clone)]
pub(super) struct ScrollList {
    /// Whose scroll position `offset` is (`GameState::set_queue_scroll`).
    pub(super) kind: QueueKind,
    /// A line of text, a row of buttons or a queue row each. A row of
    /// buttons after another gets the gap `PanelBuilder::buttons` would give
    /// it.
    pub(super) entries: Vec<Row>,
    /// The first entry shown (clamped to the last offset that fills the
    /// window).
    pub(super) offset: usize,
    /// The window's height, when it's shorter than the entries.
    pub(super) height: Option<f32>,
}

impl ScrollList {
    /// Each entry's height, with the gap above it that it needs after the
    /// entry before it.
    fn pitches(&self) -> Vec<f32> {
        let mut after_buttons = false;
        self.entries
            .iter()
            .map(|entry| {
                let buttons = entry.is_buttons();
                let gap = if buttons && after_buttons { GAP } else { 0.0 };
                after_buttons = buttons;
                gap + PanelBuilder::row_height(entry)
            })
            .collect()
    }

    /// The height of entries `from..to` shown together: the first one's gap
    /// above it is left out.
    fn span(&self, pitches: &[f32], from: usize, to: usize) -> f32 {
        let lead = pitches[from] - PanelBuilder::row_height(&self.entries[from]);
        pitches[from..to].iter().sum::<f32>() - lead
    }

    /// The window's height: the entries', or less if it has less room.
    fn window(&self) -> f32 {
        let pitches = self.pitches();
        let full = if pitches.is_empty() {
            0.0
        } else {
            self.span(&pitches, 0, pitches.len())
        };
        self.height.map_or(full, |height| height.min(full))
    }

    /// The last offset that still fills the window, and the entries shown
    /// from the current offset.
    fn shown(&self) -> (usize, std::ops::Range<usize>) {
        let pitches = self.pitches();
        let count = pitches.len();
        if count == 0 {
            return (0, 0..0);
        }
        let window = self.window() + 0.5;
        let max_offset = (0..count)
            .find(|&from| self.span(&pitches, from, count) <= window)
            .unwrap_or(count - 1);
        let from = self.offset.min(max_offset);
        let mut to = from + 1;
        while to < count && self.span(&pitches, from, to + 1) <= window {
            to += 1;
        }
        (max_offset, from..to)
    }
}

/// Room a building catalogue card takes, with the space under it.
const CATALOG_PITCH: f32 = 34.0;

/// A row of buttons after another row of buttons in a scroll list: the gap
/// between them, as ImGui gets it (`flat_rows`).
static BUTTON_ROW_GAP: Row = Row::Gap(GAP);

/// `rows` with each scroll list's entries in its place, as ImGui shows them:
/// its windows scroll by themselves.
pub(super) fn flat_rows(rows: &[Row]) -> Vec<&Row> {
    fn push<'a>(out: &mut Vec<&'a Row>, row: &'a Row) {
        if row.is_buttons() && out.last().is_some_and(|last| last.is_buttons()) {
            out.push(&BUTTON_ROW_GAP);
        }
        out.push(row);
    }
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        match row {
            Row::ScrollList(list) => list.entries.iter().for_each(|entry| push(&mut out, entry)),
            row => push(&mut out, row),
        }
    }
    out
}

/// Reusable panel content primitive. Stacks rows top to bottom and measures
/// its own size; place persistent panels through `Layout::dock_panel`.
#[derive(Clone, Default)]
pub(super) struct PanelBuilder {
    pub(super) rows: Vec<Row>,
    /// See-through, for the debug panel.
    pub(super) faded: bool,
    pub(super) scrollbar: Option<(QueueKind, usize, usize, usize)>,
}

impl PanelBuilder {
    pub(super) fn text(&mut self, px: u32, line: Line) {
        self.rows.push(Row::Text(px, line));
    }

    pub(super) fn gap(&mut self, height: f32) {
        self.rows.push(Row::Gap(height));
    }

    /// A progress bar across the panel, `fraction` full.
    pub(super) fn bar(&mut self, fraction: f32) {
        self.rows.push(Row::Bar(fraction));
    }

    pub(super) fn queue_item(&mut self, item: QueueItemSpec) {
        self.rows.push(Row::QueueItem(item));
    }

    /// A compact, scrollable list of building cards in the city tray.
    pub(super) fn building_catalog(
        &mut self,
        city: usize,
        entries: Vec<CatalogEntry>,
        offset: usize,
    ) {
        self.rows.push(Row::BuildingCatalog(
            city,
            entries,
            offset,
            BUILDING_LIST_VISIBLE,
        ));
    }

    /// `entries` (each a line of text, a row of buttons or a queue row), in
    /// a list that scrolls within the panel, from entry `offset`, when the
    /// panel has less room than they need (`fit_height`).
    pub(super) fn scroll_list(&mut self, kind: QueueKind, entries: Vec<Row>, offset: usize) {
        if entries.is_empty() {
            return;
        }
        self.rows.push(Row::ScrollList(ScrollList {
            kind,
            entries,
            offset,
            height: None,
        }));
    }

    /// Makes the panel no taller than `height`, if it can, by showing less of
    /// what scrolls within it: first its scroll lists, down to two rows'
    /// room, then its building catalogue, down to two cards, then the
    /// lists down to one row. Classic uses it before docking a panel that
    /// may have grown past the screen (ImGui's windows scroll instead).
    pub(super) fn fit_height(&mut self, height: f32) {
        for (list_rows, catalog_cards) in [(2.0, BUILDING_LIST_VISIBLE), (2.0, 2), (1.0, 2)] {
            for i in 0..self.rows.len() {
                if !matches!(self.rows[i], Row::ScrollList(_) | Row::BuildingCatalog(..)) {
                    continue;
                }
                let excess = self.size().y - height.round();
                if excess <= 0.0 {
                    return;
                }
                match &mut self.rows[i] {
                    Row::ScrollList(list) => {
                        let window = list.window();
                        let floor = window.min(list_rows * QUEUE_ITEM_HEIGHT);
                        list.height = Some((window - excess).max(floor));
                    }
                    Row::BuildingCatalog(_, entries, _, visible) => {
                        let shown = entries.len().clamp(1, *visible);
                        let shed = (excess / CATALOG_PITCH).ceil() as usize;
                        *visible = shown.saturating_sub(shed).max(shown.min(catalog_cards));
                    }
                    _ => {}
                }
            }
        }
    }

    /// A row of unit tokens, each clickable.
    pub(super) fn roster(&mut self, chips: Vec<RosterChip>) {
        self.rows.push(Row::Roster(chips));
    }

    /// A heading over the rows that follow.
    pub(super) fn heading(&mut self, text: &str) {
        self.rows.push(Row::Heading(text.into()));
    }

    /// A player setting's label and control, showing `value`.
    pub(super) fn setting(&mut self, setting: Setting, value: i32) {
        self.rows.push(Row::Setting(setting, value));
    }

    /// A line of small text with `button` at the right end of the same row.
    pub(super) fn title_with_button(&mut self, line: Line, button: ButtonSpec) {
        self.rows.push(Row::TitleWithButton(line, button));
    }

    /// A row of equally wide buttons.
    pub(super) fn buttons(&mut self, buttons: Vec<ButtonSpec>) {
        self.space_button_rows();
        self.rows.push(Row::Buttons(buttons, false));
    }

    /// A labeled order toolbar; pictograms and click targets stay shared
    /// between the classic and ImGui presentations.
    pub(super) fn action_toolbar(&mut self, buttons: Vec<ButtonSpec>) {
        self.gap(GAP);
        self.text(SMALL, vec![("ORDERS".into(), LABEL_TEXT)]);
        self.buttons(buttons);
    }

    /// A row of equally wide one-line buttons.
    pub(super) fn compact_buttons(&mut self, buttons: Vec<ButtonSpec>) {
        self.space_button_rows();
        self.rows.push(Row::Buttons(buttons, true));
    }

    /// A row of one-line buttons that drag onto one another to reorder
    /// `kind` (`Row::Reorder`).
    pub(super) fn reorder_buttons(&mut self, kind: QueueKind, buttons: Vec<ButtonSpec>) {
        self.space_button_rows();
        self.rows.push(Row::Reorder(kind, buttons));
    }

    /// For a plan that can't change (a network game waiting for the others'):
    /// every button that would change it shows disabled, and queue rows
    /// lock (no dragging, no X). Both presentations call it on the panels
    /// they show, so looking stays open and ordering doesn't.
    pub(super) fn freeze_plan(&mut self) {
        freeze_rows(&mut self.rows);
    }

    /// Button borders are drawn just outside their rows, so a row of buttons
    /// right under another needs a gap to keep them from overlapping.
    fn space_button_rows(&mut self) {
        if self.rows.last().is_some_and(Row::is_buttons) {
            self.rows.push(Row::Gap(GAP));
        }
    }

    fn row_height(row: &Row) -> f32 {
        match row {
            Row::Text(px, _) => font::ui(*px).line_height + LINE_GAP,
            Row::Gap(height) => *height,
            Row::Bar(_) => GROWTH_BAR_HEIGHT,
            Row::QueueItem(_) => QUEUE_ITEM_HEIGHT,
            Row::Buttons(buttons, compact) => buttons_height(buttons, *compact),
            Row::Reorder(_, buttons) => buttons_height(buttons, true),
            Row::TitleWithButton(..) => TITLE_ROW_HEIGHT,
            Row::LabeledButtons(..) => END_TURN_HEIGHT,
            Row::Roster(_) => ROSTER_CHIP,
            Row::BuildingCatalog(_, buttons, _, visible) => {
                let visible = buttons.len().clamp(1, *visible);
                visible as f32 * 30.0 + (visible + 1) as f32 * 4.0
            }
            Row::ScrollList(list) => list.window(),
            Row::Heading(_) | Row::Setting(..) | Row::Field(..) => {
                unreachable!("expanded by classic_rows")
            }
        }
    }

    fn row_width(row: &Row) -> f32 {
        match row {
            Row::Text(px, line) => line_width(font::ui(*px), line),
            Row::Gap(_) | Row::Bar(_) => 0.0,
            Row::TitleWithButton(line, button) => {
                line_width(font::ui(SMALL), line)
                    + GAP
                    + single_line_button_width(&button.label, &button.hint)
            }
            Row::QueueItem(item) => {
                font::ui(SMALL).width(&item.label) + 2.0 * BUTTON_PADDING + QUEUE_REMOVE_WIDTH
            }
            Row::Buttons(buttons, compact) => buttons_width(buttons, *compact),
            Row::LabeledButtons(line, buttons) => {
                line_width(font::ui(BODY), line) + GAP + buttons_width(buttons, true)
            }
            Row::Reorder(_, buttons) => buttons_width(buttons, true),
            Row::ScrollList(list) => {
                let widest = list.entries.iter().map(Self::row_width).fold(0.0, f32::max);
                let (max_offset, _) = list.shown();
                // Room for the scrollbar once there's something to scroll.
                widest
                    + if max_offset > 0 {
                        GAP + SCROLLBAR_WIDTH
                    } else {
                        0.0
                    }
            }
            Row::BuildingCatalog(_, entries, ..) => entries
                .iter()
                .filter_map(|entry| match entry {
                    CatalogEntry::Card(button) => {
                        Some(single_line_button_width(&button.label, &button.hint))
                    }
                    CatalogEntry::Heading(_) => None,
                })
                .fold(320.0, f32::max),
            Row::Roster(chips) => {
                chips.len() as f32 * ROSTER_CHIP
                    + chips.len().saturating_sub(1) as f32 * ROSTER_CHIP_GAP
            }
            Row::Heading(_) | Row::Setting(..) | Row::Field(..) => {
                unreachable!("expanded by classic_rows")
            }
        }
    }

    /// The panel's size, padding included.
    pub(super) fn size(&self) -> Vec2 {
        let rows = classic_rows(self.rows.clone());
        let width = rows.iter().map(Self::row_width).fold(0.0, f32::max);
        let height: f32 = rows.iter().map(Self::row_height).sum();
        let scroll_extra = if self.scrollbar.is_some() {
            GAP + SCROLLBAR_WIDTH
        } else {
            0.0
        };
        (Vec2::new(width + scroll_extra, height) + 2.0 * PADDING).round()
    }

    pub(super) fn place_bottom_left(self, min: Vec2, layout: &mut Layout) {
        let top = min.y + self.size().y;
        self.place_top_left(Vec2::new(min.x, top), layout);
    }

    pub(super) fn place_top_left(self, top_left: Vec2, layout: &mut Layout) {
        let size = self.size();
        let min = Vec2::new(top_left.x, top_left.y - size.y).round();
        let max = min + size;
        layout.panel(min, max, self.faded);

        let left = min.x + PADDING;
        let scroll_extra = if self.scrollbar.is_some() {
            GAP + SCROLLBAR_WIDTH
        } else {
            0.0
        };
        let inner_width = size.x - 2.0 * PADDING - scroll_extra;
        if let Some((kind, offset, total, visible)) = self.scrollbar {
            let track_min = Vec2::new(max.x - PADDING - SCROLLBAR_WIDTH, min.y + PADDING);
            let track_max = Vec2::new(max.x - PADDING, max.y - PADDING);
            let height = track_max.y - track_min.y;
            let thumb_height = (height * visible as f32 / total as f32)
                .max(24.0)
                .min(height);
            let max_offset = total - visible;
            let travel = height - thumb_height;
            let thumb_top =
                track_max.y - travel * offset.min(max_offset) as f32 / max_offset as f32;
            let thumb_max = Vec2::new(track_max.x, thumb_top);
            let thumb_min = Vec2::new(track_min.x, thumb_top - thumb_height);
            layout.shapes.push(Shape::Scrollbar {
                track_min,
                track_max,
                thumb_min,
                thumb_max,
            });
            layout.queue_scrollbars.push(QueueScrollRegion {
                kind,
                panel_min: min,
                panel_max: max,
                track_min,
                track_max,
                thumb_height,
                max_offset,
            });
        }
        let mut top = max.y - PADDING;
        for row in classic_rows(self.rows) {
            let height = Self::row_height(&row);
            place_row(layout, row, Vec2::new(left, top), inner_width, self.faded);
            top -= height;
        }
    }
}

/// Places one of a classic panel's rows (after `classic_rows`) with its top
/// left corner at `top_left`, `inner_width` wide.
fn place_row(layout: &mut Layout, row: Row, top_left: Vec2, inner_width: f32, faded: bool) {
    let (left, top) = (top_left.x, top_left.y);
    let height = PanelBuilder::row_height(&row);
    match row {
        Row::Text(px, line) => {
            // Center capital letters in the line, ignoring the gap.
            let middle = top - (height - LINE_GAP) / 2.0;
            push_text_row(layout, Vec2::new(left, middle), px, line);
        }
        Row::Gap(_) => {}
        Row::TitleWithButton(line, spec) => {
            let middle = top - TITLE_BUTTON_HEIGHT / 2.0;
            push_text_row(layout, Vec2::new(left, middle), SMALL, line);
            let width = single_line_button_width(&spec.label, &spec.hint);
            let right = left + inner_width;
            layout.buttons.push(Button {
                target: spec.target,
                label: spec.label,
                hint: spec.hint,
                state: spec.state,
                armed: spec.armed,
                faded,
                min: Vec2::new(right - width, top - TITLE_BUTTON_HEIGHT).round(),
                max: Vec2::new(right, top).round(),
            });
        }
        Row::Bar(fraction) => layout.shapes.push(Shape::Bar {
            min: Vec2::new(left, top - height),
            max: Vec2::new(left + inner_width, top),
            fraction,
        }),
        Row::QueueItem(item) => {
            let min = Vec2::new(left, top - height + QUEUE_ITEM_GAP).round();
            let max = Vec2::new(left + inner_width, top).round();
            let body_max_x = max.x - QUEUE_REMOVE_WIDTH;
            layout.shapes.push(Shape::QueueItem {
                min,
                max: Vec2::new(body_max_x, max.y),
                label: item.label,
                active: item.active,
                waiting: item.waiting,
                dragging: item.dragging,
                drop_target: item.drop_target,
                locked: item.locked,
            });
            layout.buttons.push(Button {
                target: item.kind.remove_target(item.index),
                label: "X".into(),
                hint: String::new(),
                state: ButtonState::new(false, item.locked),
                armed: false,
                faded,
                min: Vec2::new(body_max_x, min.y),
                max,
            });
            layout.queue_items.push(QueueItemRegion {
                kind: item.kind,
                index: item.index,
                min,
                max,
                body_max_x,
                locked: item.locked,
            });
        }
        Row::Reorder(kind, buttons) => {
            // The buttons, each also a drag region (index by index), which
            // takes the press before the button would (`start_queue_drag_at`):
            // let go where it started and it's the button's click.
            let first = layout.buttons.len();
            place_row(
                layout,
                Row::Buttons(buttons, true),
                top_left,
                inner_width,
                faded,
            );
            let placed: Vec<_> = layout.buttons[first..]
                .iter()
                .map(|b| (b.min, b.max, b.state == ButtonState::Disabled))
                .collect();
            for (index, (min, max, locked)) in placed.into_iter().enumerate() {
                layout.queue_items.push(QueueItemRegion {
                    kind,
                    index,
                    min,
                    max,
                    body_max_x: max.x,
                    locked,
                });
            }
        }
        Row::LabeledButtons(line, buttons) => {
            push_text_row(layout, Vec2::new(left, top - height / 2.0), BODY, line);
            // The buttons, as a compact row would place them, at the end.
            let width = buttons_width(&buttons, true);
            place_row(
                layout,
                Row::Buttons(buttons, true),
                Vec2::new(left + inner_width - width, top),
                width,
                faded,
            );
        }
        Row::Buttons(buttons, compact) => {
            let icons = icon_row(&buttons);
            let width = if icons {
                ICON_BUTTON_SIZE
            } else {
                button_width(&buttons, compact)
            };
            for (i, spec) in buttons.into_iter().enumerate() {
                let (min, size) = if icons {
                    let column = i % CLASSIC_ICON_COLUMNS;
                    let row = i / CLASSIC_ICON_COLUMNS;
                    (
                        Vec2::new(
                            left + column as f32 * (ICON_BUTTON_SIZE + GAP),
                            top - ICON_BUTTON_SIZE - row as f32 * (ICON_BUTTON_SIZE + GAP),
                        ),
                        Vec2::splat(ICON_BUTTON_SIZE),
                    )
                } else {
                    (
                        Vec2::new(left + i as f32 * (width + GAP), top - height),
                        Vec2::new(width, height),
                    )
                };
                layout.buttons.push(Button {
                    target: spec.target,
                    label: spec.label,
                    hint: spec.hint,
                    state: spec.state,
                    armed: spec.armed,
                    faded,
                    min: min.round(),
                    max: (min + size).round(),
                });
            }
        }
        Row::ScrollList(list) => place_scroll_list(layout, list, top_left, inner_width, faded),
        Row::BuildingCatalog(city, buttons, offset, visible) => {
            let total = buttons.len();
            let visible = total.min(visible);
            let overflow = buttons.len() > visible;
            let region_min = Vec2::new(left, top - height).round();
            let region_max = Vec2::new(left + inner_width, top).round();
            layout.shapes.push(Shape::Panel {
                min: region_min,
                max: region_max,
                faded,
            });
            let list_width = inner_width - 8.0 - if overflow { SCROLLBAR_WIDTH + 4.0 } else { 0.0 };
            let offset = offset.min(buttons.len().saturating_sub(visible));
            for (index, entry) in buttons.into_iter().enumerate().skip(offset).take(visible) {
                let y = region_max.y - 4.0 - (index - offset) as f32 * 34.0;
                let max = Vec2::new(left + 4.0 + list_width, y).round();
                let min = Vec2::new(left + 4.0, y - 30.0).round();
                match entry {
                    CatalogEntry::Card(spec) => layout.buttons.push(Button {
                        target: spec.target,
                        label: spec.label,
                        hint: spec.hint,
                        state: spec.state,
                        armed: spec.armed,
                        faded,
                        min,
                        max,
                    }),
                    CatalogEntry::Heading(label) => push_text_row(
                        layout,
                        Vec2::new(min.x + 4.0, (min.y + max.y) / 2.0),
                        SMALL,
                        vec![(label.into(), LABEL_TEXT)],
                    ),
                }
            }
            if overflow {
                let track_min = Vec2::new(region_max.x - 4.0 - SCROLLBAR_WIDTH, region_min.y + 4.0);
                let track_max = Vec2::new(region_max.x - 4.0, region_max.y - 4.0);
                let track_height = track_max.y - track_min.y;
                let thumb_height = (track_height * visible as f32 / total as f32)
                    .max(24.0)
                    .min(track_height);
                let max_offset = total - visible;
                let travel = track_height - thumb_height;
                let thumb_top = track_max.y - travel * offset as f32 / max_offset as f32;
                layout.shapes.push(Shape::Scrollbar {
                    track_min,
                    track_max,
                    thumb_min: Vec2::new(track_min.x, thumb_top - thumb_height),
                    thumb_max: Vec2::new(track_max.x, thumb_top),
                });
                layout.building_scrollbars.push(BuildingScrollRegion {
                    city,
                    min: region_min,
                    max: region_max,
                    track_min,
                    track_max,
                    thumb_height,
                    max_offset,
                });
            } else {
                layout.building_scrollbars.push(BuildingScrollRegion {
                    city,
                    min: region_min,
                    max: region_max,
                    track_min: region_min,
                    track_max: region_min,
                    thumb_height: 0.0,
                    max_offset: 0,
                });
            }
        }
        Row::Roster(chips) => {
            for (i, chip) in chips.into_iter().enumerate() {
                let min = Vec2::new(
                    left + i as f32 * (ROSTER_CHIP + ROSTER_CHIP_GAP),
                    top - height,
                )
                .round();
                let max = min + Vec2::splat(ROSTER_CHIP);
                layout.shapes.push(Shape::UnitChip { min, max, chip });
                layout.roster_chips.push((min, max, chip.key));
            }
        }
        Row::Heading(_) | Row::Setting(..) | Row::Field(..) => {
            unreachable!("expanded by classic_rows")
        }
    }
}

/// Places the entries of `list` that its window shows, from its offset, and
/// beside them, if they don't all show, a scrollbar: its wheel and drag
/// regions scroll the list (`GameState::scroll_queue_at`).
fn place_scroll_list(
    layout: &mut Layout,
    list: ScrollList,
    top_left: Vec2,
    inner_width: f32,
    faded: bool,
) {
    let window = list.window();
    let (max_offset, shown) = list.shown();
    let overflow = max_offset > 0;
    let width = inner_width - if overflow { GAP + SCROLLBAR_WIDTH } else { 0.0 };
    let pitches = list.pitches();
    let mut top = top_left.y;
    for (index, entry) in list.entries.into_iter().enumerate() {
        if !shown.contains(&index) {
            continue;
        }
        let height = PanelBuilder::row_height(&entry);
        // The first row shown needs no gap above it.
        if index > shown.start {
            top -= pitches[index] - height;
        }
        place_row(layout, entry, Vec2::new(top_left.x, top), width, faded);
        top -= height;
    }
    if !overflow {
        return;
    }
    let min = Vec2::new(top_left.x, top_left.y - window).round();
    let max = Vec2::new(top_left.x + inner_width, top_left.y).round();
    let track_min = Vec2::new(max.x - SCROLLBAR_WIDTH, min.y);
    let track_max = max;
    let height = track_max.y - track_min.y;
    let thumb_height = (height * shown.len() as f32 / pitches.len() as f32)
        .max(24.0)
        .min(height);
    let thumb_top = track_max.y - (height - thumb_height) * shown.start as f32 / max_offset as f32;
    layout.shapes.push(Shape::Scrollbar {
        track_min,
        track_max,
        thumb_min: Vec2::new(track_min.x, thumb_top - thumb_height),
        thumb_max: Vec2::new(track_max.x, thumb_top),
    });
    layout.queue_scrollbars.push(QueueScrollRegion {
        kind: list.kind,
        panel_min: min,
        panel_max: max,
        track_min,
        track_max,
        thumb_height,
        max_offset,
    });
}
/// Whether `row` is a row of buttons, or a scroll list whose first (`first`)
/// or last entry is: a row of buttons next to another needs a gap between
/// them, their borders being drawn just outside them.
fn buttons_at(row: &Row, first: bool) -> bool {
    match row {
        Row::Buttons(..) | Row::Reorder(..) | Row::LabeledButtons(..) => true,
        Row::ScrollList(list) => {
            let edge = if first {
                list.entries.first()
            } else {
                list.entries.last()
            };
            edge.is_some_and(Row::is_buttons)
        }
        _ => false,
    }
}

/// `rows` as the classic presentation measures and places them: a heading
/// becomes a line of gold text and a setting its label and buttons
/// (`classic_setting_rows`). Rows of buttons that end up adjacent get a gap
/// between them, as `PanelBuilder::buttons` would give them.
pub(super) fn classic_rows(rows: Vec<Row>) -> Vec<Row> {
    let mut out = Vec::with_capacity(rows.len());
    let push = |out: &mut Vec<Row>, row: Row| {
        if buttons_at(&row, true) && out.last().is_some_and(|last| buttons_at(last, false)) {
            out.push(Row::Gap(GAP));
        }
        out.push(row);
    };
    for row in rows {
        match row {
            Row::Heading(text) => push(&mut out, Row::Text(SMALL, vec![(text, GOLD_TEXT)])),
            Row::Setting(setting, value) => {
                for row in classic_setting_rows(setting, value) {
                    push(&mut out, row);
                }
            }
            Row::Field(field, text, editing) => {
                // The text, with a cursor while typing; a button to click
                // to type.
                let shown = match (text.is_empty(), editing) {
                    (_, true) => format!("{text}_"),
                    (true, false) => "-".into(),
                    (false, false) => text,
                };
                push(&mut out, Row::Gap(GAP / 2.0));
                push(
                    &mut out,
                    Row::Text(BODY, vec![(field.name().into(), LABEL_TEXT)]),
                );
                push(
                    &mut out,
                    Row::Buttons(
                        vec![ButtonSpec {
                            target: Target::EditNetField(field),
                            label: shown,
                            hint: String::new(),
                            state: ButtonState::new(editing, false),
                            armed: false,
                        }],
                        true,
                    ),
                );
            }
            row => push(&mut out, row),
        }
    }
    out
}

/// A row of buttons' height in classic: icon buttons wrap at
/// `CLASSIC_ICON_COLUMNS`.
fn buttons_height(buttons: &[ButtonSpec], compact: bool) -> f32 {
    if icon_row(buttons) {
        let rows = buttons.len().div_ceil(CLASSIC_ICON_COLUMNS);
        rows as f32 * ICON_BUTTON_SIZE + rows.saturating_sub(1) as f32 * GAP
    } else if compact {
        END_TURN_HEIGHT
    } else {
        BUTTON_HEIGHT
    }
}

/// A row of buttons' width in classic.
fn buttons_width(buttons: &[ButtonSpec], compact: bool) -> f32 {
    if icon_row(buttons) {
        let columns = buttons.len().min(CLASSIC_ICON_COLUMNS);
        columns as f32 * ICON_BUTTON_SIZE + columns.saturating_sub(1) as f32 * GAP
    } else {
        let width = button_width(buttons, compact);
        buttons.len() as f32 * width + (buttons.len().saturating_sub(1)) as f32 * GAP
    }
}

/// Width every button in a row shares: enough for the widest one.
pub(super) fn button_width(buttons: &[ButtonSpec], compact: bool) -> f32 {
    buttons
        .iter()
        .map(|b| {
            if compact {
                return single_line_button_width(&b.label, &b.hint);
            }
            let label = font::ui(BODY).width(&b.label);
            let hint = font::ui(SMALL).width(&b.hint);
            label.max(hint) + 2.0 * BUTTON_PADDING
        })
        .fold(if compact { 0.0 } else { BUTTON_MIN_WIDTH }, f32::max)
        .ceil()
}

pub(super) fn single_line_button_width(label: &str, hint: &str) -> f32 {
    (font::ui(BODY).width(label) + GAP + font::ui(SMALL).width(hint) + 2.0 * BUTTON_PADDING).ceil()
}

/// Adds a line of text with its capital letters centered vertically on
/// `middle`, starting at `left`.
pub(super) fn push_text_row(layout: &mut Layout, left_middle: Vec2, px: u32, line: Line) {
    let face = font::ui(px);
    let origin = Vec2::new(left_middle.x, left_middle.y - face.cap_height / 2.0);
    layout.shapes.push(Shape::Text { origin, px, line });
}

pub(super) fn line_width(face: &Face, line: &Line) -> f32 {
    line.iter().map(|(span, _)| face.width(span)).sum()
}

#[cfg(test)]
mod hint_tests {
    use super::visible_button_hint;

    #[test]
    fn shortcuts_wait_for_hover_but_costs_and_work_times_stay_visible() {
        for (hint, idle) in [
            ("SPACE", ""),
            ("V / ESC", ""),
            ("CTRL-RMB", ""),
            ("X · RMB", ""),
            ("7 · 15 PROD", "15 PROD"),
            ("8 | 20 PROD", "20 PROD"),
            ("R · 2T", "2T"),
            ("HORSES · 16", "HORSES · 16"),
            ("3T", "3T"),
            ("SEND IT HOME", "SEND IT HOME"),
        ] {
            assert_eq!(visible_button_hint(hint, false), idle, "{hint}");
            assert_eq!(visible_button_hint(hint, true), hint, "{hint}");
        }
    }
}
