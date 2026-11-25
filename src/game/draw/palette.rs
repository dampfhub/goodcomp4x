//! Colors several parts of the map share, and mixing them.

use super::*;

pub(super) const BORDER_COLOR: Color = [0.10, 0.10, 0.13, 1.0];

pub(super) const SELECTED_COLOR: Color = [0.80, 0.78, 0.30, 1.0];

pub(super) const MOVE_RANGE_COLOR: Color = [0.24, 0.42, 0.26, 1.0];
pub(super) const ATTACK_RANGE_COLOR: Color = [0.45, 0.22, 0.22, 1.0];

pub(super) const LABEL_COLOR: Color = [0.05, 0.05, 0.05, 1.0];

/// The dark edge around unit icons and map markers, so they stand out on any
/// terrain.
pub(super) const ICON_OUTLINE_COLOR: Color = [0.03, 0.03, 0.04, 1.0];

pub(super) const PLANNED_JOB_COLOR_SOLID: Color = [0.95, 0.78, 0.42, 1.0];

pub(super) const BADGE_BG_COLOR: Color = [0.06, 0.06, 0.08, 0.9];

pub(super) fn mix(a: Color, b: Color, t: f32) -> Color {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

pub(super) fn brighten([r, g, b, a]: Color) -> Color {
    [
        (r + 0.35).min(1.0),
        (g + 0.35).min(1.0),
        (b + 0.35).min(1.0),
        a,
    ]
}

pub(super) fn with_alpha([r, g, b, _]: Color, a: f32) -> Color {
    [r, g, b, a]
}
