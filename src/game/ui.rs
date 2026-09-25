//! Screen-space UI: the selected unit's ability button.
//!
//! UI geometry is laid out in pixels with the origin at the window's
//! bottom-left and Y pointing up, the same orientation as the world, so the
//! shared shape and text helpers draw the same way in both.

use glam::{Mat4, Vec2, Vec3};

use super::ability::Ability;
use super::unit::Unit;
use super::{GameState, font, mesh};
use crate::renderer::Vertex;

type Color = [f32; 4];

const BUTTON_SIZE: Vec2 = Vec2::new(600.0, 52.0);
const BUTTON_BOTTOM: f32 = 28.0;
const BUTTON_BORDER: f32 = 2.0;
/// Text heights are multiples of the font's 7 rows so glyph cells land on whole pixels.
const LABEL_HEIGHT: f32 = 21.0;
const DESCRIPTION_HEIGHT: f32 = 14.0;
const DESCRIPTION_GAP: f32 = 12.0;

const BORDER_COLOR: Color = [0.55, 0.58, 0.66, 1.0];
const READY_BG: Color = [0.16, 0.18, 0.24, 0.95];
const QUEUED_BG: Color = [0.78, 0.64, 0.20, 0.95];
const COOLDOWN_BG: Color = [0.10, 0.10, 0.12, 0.95];
const READY_TEXT: Color = [0.95, 0.95, 0.95, 1.0];
const QUEUED_TEXT: Color = [0.08, 0.07, 0.04, 1.0];
const COOLDOWN_TEXT: Color = [0.50, 0.50, 0.54, 1.0];
const DESCRIPTION_TEXT: Color = [0.80, 0.80, 0.84, 1.0];

/// Maps UI pixels (origin bottom-left, Y up) to clip space.
pub fn ui_projection(screen_size: Vec2) -> Mat4 {
    Mat4::from_translation(Vec3::new(-1.0, 1.0, 0.0))
        * Mat4::from_scale(Vec3::new(2.0 / screen_size.x, -2.0 / screen_size.y, 1.0))
}

impl GameState {
    /// The UI as a triangle list in UI pixels: the ability button for the
    /// selected unit, if any.
    pub fn build_ui(&self, screen_size: Vec2) -> Vec<Vertex> {
        let mut out = Vec::new();
        let Some(idx) = self.selected else { return out };
        let unit = &self.units[idx];
        let (name, description) = ability_text(unit);

        let (label, bg, text_color) = if unit.ability_queued {
            (format!("{name} - ON"), QUEUED_BG, QUEUED_TEXT)
        } else if unit.ability_cooldown > 0 {
            let turns = unit.ability_cooldown;
            let unit_word = if turns == 1 { "TURN" } else { "TURNS" };
            (
                format!("{name} - READY IN {turns} {unit_word}"),
                COOLDOWN_BG,
                COOLDOWN_TEXT,
            )
        } else {
            (format!("{name} - Q"), READY_BG, READY_TEXT)
        };

        let (min, max) = button_rect(screen_size);
        mesh::quad(
            min - BUTTON_BORDER,
            max + BUTTON_BORDER,
            BORDER_COLOR,
            &mut out,
        );
        mesh::quad(min, max, bg, &mut out);

        let label_y = (min.y + max.y - LABEL_HEIGHT) / 2.0;
        push_centered_text(
            screen_size.x,
            label_y,
            LABEL_HEIGHT,
            &label,
            text_color,
            &mut out,
        );
        let description_y = max.y + BUTTON_BORDER + DESCRIPTION_GAP;
        push_centered_text(
            screen_size.x,
            description_y,
            DESCRIPTION_HEIGHT,
            description,
            DESCRIPTION_TEXT,
            &mut out,
        );

        out
    }

    /// Handles a click on the UI, returning whether it hit anything (in which
    /// case it shouldn't also count as a click on the map). `cursor` is in
    /// window pixels with the origin at the top-left.
    pub(super) fn click_ui(&mut self, cursor: Vec2, screen_size: Vec2) -> bool {
        if self.selected.is_none() {
            return false;
        }
        let point = Vec2::new(cursor.x, screen_size.y - cursor.y);
        let (min, max) = button_rect(screen_size);
        let hit = point.cmpge(min).all() && point.cmple(max).all();
        if hit {
            self.toggle_selected_ability();
        }
        hit
    }
}

/// The ability button's bottom-left and top-right corners, centered along the
/// bottom of the window.
fn button_rect(screen_size: Vec2) -> (Vec2, Vec2) {
    let min = Vec2::new(
        ((screen_size.x - BUTTON_SIZE.x) / 2.0).round(),
        BUTTON_BOTTOM,
    );
    (min, min + BUTTON_SIZE)
}

/// The ability's button label and a one-line description of what it does.
fn ability_text(unit: &Unit) -> (&'static str, &'static str) {
    match unit.ability() {
        Ability::ShieldWall => ("SHIELD WALL", "+50% DEFENSE THIS TURN - CANNOT MOVE"),
        Ability::Volley => (
            "VOLLEY",
            "ATTACK ALSO HITS ENEMIES NEXT TO THE TARGET - ALL HITS DEAL 60%",
        ),
        Ability::Charge => ("CHARGE", "+1 MOVE AND +50% ATTACK THIS TURN"),
        Ability::Deploy if unit.deployed => ("PACK UP", "SPEND A TURN PACKING UP TO MOVE AGAIN"),
        Ability::Deploy => (
            "DEPLOY",
            "SPEND A TURN SETTING UP - THEN +1 RANGE BUT CANNOT MOVE",
        ),
    }
}

/// Draws a line of text centered horizontally in the window, snapped to whole
/// pixels so glyph cells stay crisp.
fn push_centered_text(
    screen_width: f32,
    bottom: f32,
    height: f32,
    text: &str,
    color: Color,
    out: &mut Vec<Vertex>,
) {
    let left = ((screen_width - font::text_width(text, height)) / 2.0).round();
    font::push_text(Vec2::new(left, bottom.round()), height, text, color, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_projection_puts_the_origin_at_the_bottom_left() {
        let size = Vec2::new(1280.0, 720.0);
        let clip = |p: Vec2| ui_projection(size).project_point3(p.extend(0.0)).truncate();
        // Vulkan clip space has Y = +1 at the bottom of the window.
        assert!(clip(Vec2::ZERO).abs_diff_eq(Vec2::new(-1.0, 1.0), 1e-5));
        assert!(clip(size).abs_diff_eq(Vec2::new(1.0, -1.0), 1e-5));
    }
}
