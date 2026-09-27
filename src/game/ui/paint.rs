//! Turning laid-out shapes and buttons into vertices.

use super::{
    ARMED_BORDER, ARMED_BORDER_COLOR, BAR_BG, BODY, BORDER, BORDER_COLOR, BUTTON_BG, BUTTON_HEIGHT,
    BUTTON_HOVER_BG, BUTTON_PADDING, Button, ButtonState, Color, DEBUG_ALPHA, DIM_TEXT,
    DISABLED_BG, DISABLED_TEXT, GAP, GOLD_TEXT, GROWTH_COLOR, LINE_GAP, PANEL_BG, QUEUED_BG,
    QUEUED_HINT_TEXT, QUEUED_HOVER_BG, QUEUED_TEXT, SMALL, Shape, TEXT,
};
use crate::game::font;
use crate::game::mesh;
use crate::renderer::Vertex;
use glam::Vec2;

pub(super) fn draw_shape(shape: &Shape, out: &mut Vec<Vertex>) {
    match shape {
        Shape::Panel { min, max, faded } => {
            let (bg, border) = (fade(PANEL_BG, *faded), fade(BORDER_COLOR, *faded));
            draw_box(*min, *max, bg, BORDER, border, out)
        }
        Shape::Text { origin, px, line } => {
            let face = font::ui(*px);
            let mut pen = origin.x;
            for (span, color) in line {
                face.push(Vec2::new(pen, origin.y), span, *color, out);
                pen += face.width(span);
            }
        }
        Shape::Bar { min, max, fraction } => {
            mesh::quad(*min, *max, BAR_BG, out);
            let filled = Vec2::new(min.x + (max.x - min.x) * fraction.clamp(0.0, 1.0), max.y);
            mesh::quad(*min, filled, GROWTH_COLOR, out);
        }
        Shape::Scrollbar {
            track_min,
            track_max,
            thumb_min,
            thumb_max,
        } => {
            mesh::quad(*track_min, *track_max, BAR_BG, out);
            mesh::quad(*thumb_min, *thumb_max, GOLD_TEXT, out);
        }
        Shape::QueueItem {
            min,
            max,
            label,
            active,
            dragging,
            drop_target,
            locked,
        } => {
            let bg = if *dragging {
                BUTTON_HOVER_BG
            } else {
                BUTTON_BG
            };
            let edge = if *drop_target {
                ARMED_BORDER_COLOR
            } else if *active {
                GOLD_TEXT
            } else {
                BORDER_COLOR
            };
            draw_box(*min, *max, bg, BORDER, edge, out);
            let face = font::ui(SMALL);
            let origin = Vec2::new(
                min.x + BUTTON_PADDING,
                (min.y + max.y - face.cap_height) / 2.0,
            );
            face.push(
                origin,
                label,
                if *locked {
                    DIM_TEXT
                } else if *active {
                    GOLD_TEXT
                } else {
                    TEXT
                },
                out,
            );
        }
    }
}

pub(super) fn draw_button(button: &Button, hovered: bool, out: &mut Vec<Vertex>) {
    let (bg, text_color, hint_color) = match (button.state, hovered) {
        (ButtonState::Ready, false) => (BUTTON_BG, TEXT, DIM_TEXT),
        (ButtonState::Ready, true) => (BUTTON_HOVER_BG, TEXT, DIM_TEXT),
        (ButtonState::Queued, false) => (QUEUED_BG, QUEUED_TEXT, QUEUED_HINT_TEXT),
        (ButtonState::Queued, true) => (QUEUED_HOVER_BG, QUEUED_TEXT, QUEUED_HINT_TEXT),
        (ButtonState::Disabled, _) => (DISABLED_BG, DISABLED_TEXT, DISABLED_TEXT),
    };
    let (border, border_color) = if button.armed {
        (ARMED_BORDER, ARMED_BORDER_COLOR)
    } else {
        (BORDER, BORDER_COLOR)
    };
    let [bg, border_color, text_color, hint_color] =
        [bg, border_color, text_color, hint_color].map(|color| fade(color, button.faded));
    draw_box(button.min, button.max, bg, border, border_color, out);

    let (label_face, hint_face) = (font::ui(BODY), font::ui(SMALL));
    let center = (button.min + button.max) / 2.0;
    let height = button.max.y - button.min.y;
    if height < BUTTON_HEIGHT {
        // One line: the label, then the hint beside it.
        let gap = if button.hint.is_empty() { 0.0 } else { GAP };
        let width = label_face.width(&button.label) + gap + hint_face.width(&button.hint);
        let left = center.x - width / 2.0;
        let baseline = center.y - label_face.cap_height / 2.0;
        label_face.push(Vec2::new(left, baseline), &button.label, text_color, out);
        let hint_left = left + label_face.width(&button.label) + gap;
        hint_face.push(
            Vec2::new(hint_left, baseline),
            &button.hint,
            hint_color,
            out,
        );
    } else {
        // Two lines: the label, with the hint under it.
        let gap = LINE_GAP + 2.0;
        let block = label_face.cap_height + gap + hint_face.cap_height;
        let label_baseline = center.y + block / 2.0 - label_face.cap_height;
        let hint_baseline = center.y - block / 2.0;
        let label_left = center.x - label_face.width(&button.label) / 2.0;
        let hint_left = center.x - hint_face.width(&button.hint) / 2.0;
        label_face.push(
            Vec2::new(label_left, label_baseline),
            &button.label,
            text_color,
            out,
        );
        hint_face.push(
            Vec2::new(hint_left, hint_baseline),
            &button.hint,
            hint_color,
            out,
        );
    }
}

/// `color` made see-through for the debug panel, or unchanged.
pub(super) fn fade(color: Color, faded: bool) -> Color {
    let [r, g, b, a] = color;
    if faded {
        [r, g, b, a * DEBUG_ALPHA]
    } else {
        color
    }
}

/// A filled box with a border drawn around the outside of `min`..`max`.
fn draw_box(
    min: Vec2,
    max: Vec2,
    bg: Color,
    border: f32,
    border_color: Color,
    out: &mut Vec<Vertex>,
) {
    mesh::quad(min - border, max + border, border_color, out);
    mesh::quad(min, max, bg, out);
}
