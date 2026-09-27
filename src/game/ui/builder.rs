//! `PanelBuilder`, the panel content primitive, and its text and button measuring.

use super::settings_menu::classic_setting_rows;
use super::{
    BODY, BUILDING_LIST_VISIBLE, BUTTON_HEIGHT, BUTTON_MIN_WIDTH, BUTTON_PADDING,
    BuildingScrollRegion, Button, ButtonState, END_TURN_HEIGHT, GAP, GOLD_TEXT, GROWTH_BAR_HEIGHT,
    LINE_GAP, Layout, Line, PADDING, QUEUE_ITEM_GAP, QUEUE_ITEM_HEIGHT, QUEUE_REMOVE_WIDTH,
    QueueItemRegion, QueueItemSpec, QueueKind, QueueScrollRegion, ROSTER_CHIP, ROSTER_CHIP_GAP,
    RosterChip, SCROLLBAR_WIDTH, SMALL, Shape, Target, UnitAction,
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

#[derive(Clone)]
pub(super) enum Row {
    Text(u32, Line),
    Gap(f32),
    Bar(f32),
    QueueItem(QueueItemSpec),
    /// Buttons of equal width; compact ones are one line, label then hint.
    Buttons(Vec<ButtonSpec>, bool),
    /// A bounded, independently scrollable list within the city tray.
    BuildingCatalog(usize, Vec<ButtonSpec>, usize),
    /// A row of unit tokens in the unit strip.
    Roster(Vec<RosterChip>),
    /// A heading over a group of rows, with a rule under it in ImGui.
    Heading(String),
    /// A player setting at its current value, changed with the control its
    /// `Setting::control` names: ImGui draws that widget, classic places
    /// the buttons `settings_menu::classic_setting_rows` lays out.
    Setting(Setting, i32),
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
        buttons: Vec<ButtonSpec>,
        offset: usize,
    ) {
        self.rows.push(Row::BuildingCatalog(city, buttons, offset));
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

    /// A row of equally wide buttons.
    pub(super) fn buttons(&mut self, buttons: Vec<ButtonSpec>) {
        self.space_button_rows();
        self.rows.push(Row::Buttons(buttons, false));
    }

    /// A row of equally wide one-line buttons.
    pub(super) fn compact_buttons(&mut self, buttons: Vec<ButtonSpec>) {
        self.space_button_rows();
        self.rows.push(Row::Buttons(buttons, true));
    }

    /// Button borders are drawn just outside their rows, so a row of buttons
    /// right under another needs a gap to keep them from overlapping.
    fn space_button_rows(&mut self) {
        if matches!(self.rows.last(), Some(Row::Buttons(..))) {
            self.rows.push(Row::Gap(GAP));
        }
    }

    fn row_height(row: &Row) -> f32 {
        match row {
            Row::Text(px, _) => font::ui(*px).line_height + LINE_GAP,
            Row::Gap(height) => *height,
            Row::Bar(_) => GROWTH_BAR_HEIGHT,
            Row::QueueItem(_) => QUEUE_ITEM_HEIGHT,
            Row::Buttons(_, false) => BUTTON_HEIGHT,
            Row::Buttons(_, true) => END_TURN_HEIGHT,
            Row::Roster(_) => ROSTER_CHIP,
            Row::BuildingCatalog(_, buttons, _) => {
                let visible = buttons.len().clamp(1, BUILDING_LIST_VISIBLE);
                visible as f32 * 30.0 + (visible + 1) as f32 * 4.0
            }
            Row::Heading(_) | Row::Setting(..) => unreachable!("expanded by classic_rows"),
        }
    }

    fn row_width(row: &Row) -> f32 {
        match row {
            Row::Text(px, line) => line_width(font::ui(*px), line),
            Row::Gap(_) | Row::Bar(_) => 0.0,
            Row::QueueItem(item) => {
                font::ui(SMALL).width(&item.label) + 2.0 * BUTTON_PADDING + QUEUE_REMOVE_WIDTH
            }
            Row::Buttons(buttons, compact) => {
                let width = button_width(buttons, *compact);
                buttons.len() as f32 * width + (buttons.len().saturating_sub(1)) as f32 * GAP
            }
            Row::BuildingCatalog(_, buttons, _) => buttons
                .iter()
                .map(|button| single_line_button_width(&button.label, &button.hint))
                .fold(320.0, f32::max),
            Row::Roster(chips) => {
                chips.len() as f32 * ROSTER_CHIP
                    + chips.len().saturating_sub(1) as f32 * ROSTER_CHIP_GAP
            }
            Row::Heading(_) | Row::Setting(..) => unreachable!("expanded by classic_rows"),
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
            match row {
                Row::Text(px, line) => {
                    // Center capital letters in the line, ignoring the gap.
                    let middle = top - (height - LINE_GAP) / 2.0;
                    push_text_row(layout, Vec2::new(left, middle), px, line);
                }
                Row::Gap(_) => {}
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
                        dragging: item.dragging,
                        drop_target: item.drop_target,
                        locked: item.locked,
                    });
                    layout.buttons.push(Button {
                        target: item.kind.remove_target(item.index),
                        label: "X".into(),
                        hint: String::new(),
                        state: ButtonState::Ready,
                        armed: false,
                        faded: self.faded,
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
                Row::Buttons(buttons, compact) => {
                    let width = button_width(&buttons, compact);
                    for (i, spec) in buttons.into_iter().enumerate() {
                        let min = Vec2::new(left + i as f32 * (width + GAP), top - height);
                        layout.buttons.push(Button {
                            target: spec.target,
                            label: spec.label,
                            hint: spec.hint,
                            state: spec.state,
                            armed: spec.armed,
                            faded: self.faded,
                            min: min.round(),
                            max: (min + Vec2::new(width, height)).round(),
                        });
                    }
                }
                Row::BuildingCatalog(city, buttons, offset) => {
                    let total = buttons.len();
                    let visible = total.min(BUILDING_LIST_VISIBLE);
                    let overflow = buttons.len() > visible;
                    let region_min = Vec2::new(left, top - height).round();
                    let region_max = Vec2::new(left + inner_width, top).round();
                    layout.shapes.push(Shape::Panel {
                        min: region_min,
                        max: region_max,
                        faded: self.faded,
                    });
                    let list_width =
                        inner_width - 8.0 - if overflow { SCROLLBAR_WIDTH + 4.0 } else { 0.0 };
                    let offset = offset.min(buttons.len().saturating_sub(visible));
                    for (index, spec) in buttons.into_iter().enumerate().skip(offset).take(visible)
                    {
                        let y = region_max.y - 4.0 - (index - offset) as f32 * 34.0;
                        let max = Vec2::new(left + 4.0 + list_width, y).round();
                        let min = Vec2::new(left + 4.0, y - 30.0).round();
                        layout.buttons.push(Button {
                            target: spec.target,
                            label: spec.label,
                            hint: spec.hint,
                            state: spec.state,
                            armed: spec.armed,
                            faded: self.faded,
                            min,
                            max,
                        });
                    }
                    if overflow {
                        let track_min =
                            Vec2::new(region_max.x - 4.0 - SCROLLBAR_WIDTH, region_min.y + 4.0);
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
                Row::Heading(_) | Row::Setting(..) => unreachable!("expanded by classic_rows"),
            }
            top -= height;
        }
    }
}

/// `rows` as the classic presentation measures and places them: a heading
/// becomes a line of gold text and a setting its label and buttons
/// (`classic_setting_rows`). Rows of buttons that end up adjacent get a gap
/// between them, as `PanelBuilder::buttons` would give them.
pub(super) fn classic_rows(rows: Vec<Row>) -> Vec<Row> {
    let mut out = Vec::with_capacity(rows.len());
    let push = |out: &mut Vec<Row>, row: Row| {
        if matches!(row, Row::Buttons(..)) && matches!(out.last(), Some(Row::Buttons(..))) {
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
            row => push(&mut out, row),
        }
    }
    out
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
