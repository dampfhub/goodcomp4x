//! The research view's tech tree in ImGui (`Row::TechTree`): laid out by
//! `ui/tech_tree.rs` in ImGui's fonts, each card an invisible button with
//! the card drawn over it, so it clicks, says why it's off and shows its
//! tooltip as any panel button does.

use super::*;
use crate::game::ui::tech_tree::{
    BADGE_LETTER, CARD_BAR, CARD_PAD, CardRows, LINE_SPACING, TechCard, TechState, TechTree,
    TreeMetrics, UNLOCK_ICON, UNLOCK_ICON_RADIUS, UnlockIcon, push_badge_square,
};
use crate::game::unit_icons;

/// The room `measure_panel` and the window leave around a panel's rows,
/// with a vertical scrollbar's: what the Selection window needs besides
/// the tree for the tree to fit (`tree_panel_width`).
const PANEL_ROOM: f32 = 45.0 + 16.0;

/// The tree's sizes in ImGui's fonts (`fonts`: small, body, title).
pub(super) fn tree_metrics(ui: &Ui, fonts: &[FontId; 3], tree: &TechTree) -> TreeMetrics {
    let (name_line, line) = {
        let _body = ui.push_font(fonts[1]);
        let name_line = ui.text_line_height();
        let _small = ui.push_font(fonts[0]);
        (name_line, ui.text_line_height() + LINE_SPACING)
    };
    tree.metrics(
        |text| {
            let _font = ui.push_font(fonts[1]);
            rich_width(ui, text)
        },
        |text| {
            let _font = ui.push_font(fonts[0]);
            rich_width(ui, text)
        },
        name_line,
        line,
    )
}

/// The tree's height in `width`, as `render_tech_tree` lays it out.
pub(super) fn tree_height(ui: &Ui, fonts: &[FontId; 3], tree: &TechTree, width: f32) -> f32 {
    let metrics = tree_metrics(ui, fonts, tree);
    tree.place(width, &metrics).size.y
}

/// How wide a panel holding a tech tree wants to be: room for the tree's
/// columns side by side.
pub(super) fn tree_panel_width(ui: &Ui, fonts: &[FontId; 3], panel: &PanelBuilder) -> Option<f32> {
    panel.rows.iter().find_map(|row| match row {
        Row::TechTree(tree) => Some(tree.width(&tree_metrics(ui, fonts, tree)) + PANEL_ROOM),
        _ => None,
    })
}

impl GameState {
    /// Draws `tree` in the width the window has: its headings, its lines,
    /// and its cards, each a button.
    pub(super) fn render_tech_tree(
        &self,
        frame: &mut ImGuiFrame,
        tree: &TechTree,
        PanelScope { pin, subject }: PanelScope,
    ) {
        let (ui, fonts) = (frame.ui, frame.fonts);
        let metrics = tree_metrics(ui, fonts, tree);
        let placed = tree.place(ui.content_region_avail()[0], &metrics);
        let origin = ui.cursor_screen_pos();
        let at = |p: Vec2| [origin[0] + p.x, origin[1] + p.y];
        {
            let draw = ui.get_window_draw_list();
            for &(min, max, state) in &placed.wires {
                draw.add_rect(at(min), at(max), state.color())
                    .filled(true)
                    .build();
            }
        }
        {
            let _font = ui.push_font(fonts[0]);
            for &(corner, column) in &placed.headings {
                draw_rich(ui, at(corner), &tree.headings[column], GOLD_TEXT, false);
            }
        }
        for (index, ((spec, card), &(min, max))) in tree.cards.iter().zip(&placed.cards).enumerate()
        {
            ui.set_cursor_screen_pos(at(min));
            let size = max - min;
            let hovered = {
                let _disabled = ui.begin_disabled(spec.unavailable.is_some());
                if ui.invisible_button(format!("##tech-{index}"), [size.x, size.y]) {
                    frame.actions.push(Action::Button(pin, spec.target));
                }
                note_drawn_button(ui, spec.target);
                self.button_tooltip(ui, spec, subject);
                ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED)
            };
            draw_card(
                ui,
                fonts,
                (at(min), at(max)),
                card,
                tree.names,
                &metrics,
                hovered,
            );
        }
        // The room the tree takes, as one item.
        ui.set_cursor_screen_pos(origin);
        ui.dummy([placed.size.x.max(1.0), placed.size.y.max(1.0)]);
    }
}

/// One card, over its button: its ground and rim by where its tech stands,
/// then its lines as `CardRows` places them.
fn draw_card(
    ui: &Ui,
    fonts: &[FontId; 3],
    (min, max): ([f32; 2], [f32; 2]),
    card: &TechCard,
    names: bool,
    metrics: &TreeMetrics,
    hovered: bool,
) {
    let (rim, width) = card.rim();
    let ground = match (card.state, hovered) {
        (TechState::Researched, _) => [0.07, 0.13, 0.08, 1.0],
        (TechState::Researching, _) => [0.24, 0.21, 0.11, 1.0],
        (TechState::Waits, _) => [0.22, 0.09, 0.07, 1.0],
        (TechState::Locked, false) => [0.06, 0.07, 0.08, 1.0],
        (_, false) => [0.11, 0.14, 0.16, 1.0],
        (_, true) => [0.17, 0.21, 0.24, 1.0],
    };
    {
        let draw = ui.get_window_draw_list();
        draw.add_rect(min, max, ground).filled(true).build();
        let inset = width / 2.0;
        draw.add_rect(
            [min[0] + inset, min[1] + inset],
            [max[0] - inset, max[1] - inset],
            rim,
        )
        .thickness(width)
        .build();
    }
    let rows = CardRows::new(metrics, max[1] - min[1]);
    let left = min[0] + CARD_PAD;
    let room = max[0] - min[0] - 2.0 * CARD_PAD;
    {
        let _font = ui.push_font(fonts[1]);
        let name = fit_text(card.tech.name(), room, |t| rich_width(ui, t));
        let top = min[1] + rows.name + (metrics.name_line - ui.text_line_height()) / 2.0;
        draw_rich(ui, [left, top], &name, card.name_color(), false);
    }
    let _font = ui.push_font(fonts[0]);
    let text_height = ui.text_line_height();
    let line_top = |down: f32| min[1] + down + (metrics.line - text_height) / 2.0;
    let tag_width = card.tag.as_ref().map_or(0.0, |tag| rich_width(ui, tag));
    let detail_room = room
        - if card.tag.is_some() {
            tag_width + crate::game::ui::tech_tree::TAG_GAP
        } else {
            0.0
        };
    let detail = fit_text(&card.detail, detail_room.max(0.0), |t| rich_width(ui, t));
    let top = line_top(rows.detail);
    draw_rich(ui, [left, top], &detail, card.detail_color(), false);
    if let Some(tag) = &card.tag {
        draw_rich(
            ui,
            [max[0] - CARD_PAD - tag_width, top],
            tag,
            card.tag_color(),
            false,
        );
    }
    let color = card.unlock_color();
    for (index, unlock) in card.unlocks.iter().enumerate() {
        let (x, down) = if names {
            (left, rows.unlocks + index as f32 * metrics.line)
        } else {
            (left + index as f32 * UNLOCK_ICON, rows.unlocks)
        };
        let center = [
            x + UNLOCK_ICON / 2.0 - 2.0,
            min[1] + down + metrics.line / 2.0,
        ];
        let mut vertices = Vec::new();
        match unlock.icon {
            UnlockIcon::Unit(icon) => unit_icons::push_pictogram(
                Vec2::ZERO,
                UNLOCK_ICON_RADIUS,
                icon,
                color,
                &mut vertices,
            ),
            UnlockIcon::Building(_, badge) => {
                push_badge_square(Vec2::ZERO, badge, color, &mut vertices)
            }
        }
        fill_shapes(&ui.get_window_draw_list(), center, &vertices);
        if let UnlockIcon::Building(letter, _) = unlock.icon {
            // On its badge, as the map has it; not a mark of its own.
            let letter = letter.to_string();
            let size = ui.calc_text_size(&letter);
            let at = [center[0] - size[0] / 2.0, center[1] - size[1] / 2.0];
            ui.get_window_draw_list()
                .add_text(at, BADGE_LETTER, &letter);
        }
        if names {
            let name = fit_text(unlock.name, room - UNLOCK_ICON, |t| rich_width(ui, t));
            draw_rich(ui, [x + UNLOCK_ICON, line_top(down)], &name, color, false);
        }
    }
    if let Some(progress) = card.progress {
        let top = min[1] + rows.bar;
        let (bar_min, bar_max) = ([left, top], [max[0] - CARD_PAD, top + CARD_BAR]);
        let filled = bar_min[0] + (bar_max[0] - bar_min[0]) * progress.clamp(0.0, 1.0);
        let draw = ui.get_window_draw_list();
        draw.add_rect(bar_min, bar_max, [0.03, 0.04, 0.05, 1.0])
            .filled(true)
            .build();
        draw.add_rect(bar_min, [filled, bar_max[1]], GOLD_TEXT)
            .filled(true)
            .build();
    }
}
