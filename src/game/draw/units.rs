//! Units: tokens and pictograms, status rings, order badges, health bars
//! and the attack preview's marks on them.

use super::*;

pub(super) const HEALTH_BAR_BG_COLOR: Color = [0.08, 0.08, 0.08, 1.0];

/// Sides of the disc military units stand on.
pub(super) const TOKEN_SIDES: u32 = 32;
/// Civilian hexagons, relative to the military disc.
pub(super) const CIVILIAN_TOKEN_SIZE: f32 = 0.95;

/// Rings drawn behind a unit's icon: gold while its ability is queued, steel
/// while a siege engine is deployed.
pub(super) const ABILITY_RING_COLOR: Color = [0.95, 0.78, 0.25, 1.0];
pub(super) const ABILITY_RING_RADIUS: f32 = UNIT_ICON_RADIUS * 1.32;
pub(super) const DEPLOYED_RING_COLOR: Color = [0.62, 0.66, 0.72, 1.0];
pub(super) const DEPLOYED_RING_RADIUS: f32 = UNIT_ICON_RADIUS * 1.18;
/// A white hex outline around a guarding unit, outside any status rings.
pub(super) const GUARD_OUTLINE_COLOR: Color = [0.92, 0.94, 0.98, 1.0];
pub(super) const GUARD_OUTLINE_RADIUS: f32 = UNIT_ICON_RADIUS * 1.5;
pub(super) const GUARD_OUTLINE_WIDTH: f32 = 0.05;
/// A red reticle around a unit on alert: a ring outside any status rings,
/// with four ticks on the diagonals (clear of the turn-order badges and the
/// health bar).
pub(super) const ALERT_COLOR: Color = [0.96, 0.26, 0.16, 1.0];
pub(super) const ALERT_RING_RADIUS: f32 = UNIT_ICON_RADIUS * 1.45;
pub(super) const ALERT_TICK_SPAN: (f32, f32) = (UNIT_ICON_RADIUS * 1.2, UNIT_ICON_RADIUS * 1.95);
pub(super) const ALERT_WIDTH: f32 = 0.05;

/// Turn-order badges beside each unit: move order on the left, attack order on the right.
pub(super) const BADGE_OFFSET_X: f32 = 0.62;
pub(super) const BADGE_RADIUS: f32 = 0.17;
pub(super) const BADGE_DIGIT_HEIGHT: f32 = 0.2;

pub(super) const MOVE_ORDER_COLOR: Color = [0.45, 0.70, 1.00, 1.0];
pub(super) const ATTACK_ORDER_COLOR: Color = [1.00, 0.40, 0.35, 1.0];

pub(super) const HEALTH_BAR_WIDTH: f32 = 0.9;
pub(super) const HEALTH_BAR_HEIGHT: f32 = 0.14;
pub(super) const HEALTH_BAR_OFFSET_Y: f32 = 0.78;

/// The attack preview on a health bar (`push_health_loss`): the HP it would
/// lose pulses between these two, a lethal loss rims the bar in red, and the
/// number sits over the bar, red for the target, gold for retaliation.
pub(super) const PREVIEW_LOSS_BRIGHT: Color = [1.0, 0.93, 0.80, 1.0];
pub(super) const PREVIEW_LOSS_DARK: Color = [0.80, 0.10, 0.06, 1.0];
pub(super) const PREVIEW_LETHAL_COLOR: Color = [1.0, 0.16, 0.10, 1.0];
pub(super) const PREVIEW_RIM: f32 = 0.05;
pub(super) const PREVIEW_DEALT_COLOR: Color = [1.0, 0.45, 0.36, 1.0];
pub(super) const PREVIEW_RETALIATION_COLOR: Color = [1.0, 0.82, 0.32, 1.0];
pub(super) const PREVIEW_LABEL_HEIGHT: f32 = 0.2;
/// Pulses a second.
pub(super) const PREVIEW_PULSE_RATE: f32 = 1.6;

/// How a unit is drawn, beyond its type: its pictogram, and whether it's a
/// civilian (settler or worker), drawn hollow.
#[derive(Clone, Copy)]
pub(in crate::game) struct UnitLook {
    pub icon: UnitIcon,
    pub civilian: bool,
}

/// A unit's token (as on the map) sized so a military unit's disc has
/// `radius`, in whatever space `center` is in: the UI draws the unit strip
/// with it.
pub(in crate::game) fn push_unit_token(
    center: Vec2,
    look: UnitLook,
    radius: f32,
    color: Color,
    out: &mut Vec<Vertex>,
) {
    push_unit_icon(center, look, radius / UNIT_ICON_RADIUS, color, out);
}

/// A token's outline: its sides, rotation and size relative to a military
/// unit's disc. Drawing and the city view's click test share it.
pub(super) fn token_shape(look: UnitLook) -> (u32, f32, f32) {
    if look.civilian {
        (6, FRAC_PI_2, CIVILIAN_TOKEN_SIZE)
    } else {
        (TOKEN_SIDES, 0.0, 1.0)
    }
}

/// The unit's token in its team color with a dark outline, so it reads on
/// any terrain, and its pictogram on top. Military units stand on a disc;
/// civilians on a hollow pointy-top hexagon (a pale center inside a
/// team-colored rim), unlike both the disc and the flat-top map hexes.
pub(super) fn push_unit_icon(
    center: Vec2,
    look: UnitLook,
    scale: f32,
    color: Color,
    out: &mut Vec<Vertex>,
) {
    let (sides, rotation, size) = token_shape(look);
    let radius = UNIT_ICON_RADIUS * scale * size;
    let alpha = color[3];
    mesh::regular_polygon(center, radius, sides, rotation, color, out);
    if look.civilian {
        let pale = with_alpha(mix(color, [1.0, 1.0, 1.0, 1.0], 0.7), alpha);
        mesh::regular_polygon(center, radius * 0.68, sides, rotation, pale, out);
    }
    let outline = with_alpha(ICON_OUTLINE_COLOR, alpha);
    mesh::polygon_outline(
        center,
        radius,
        ICON_OUTLINE_WIDTH * scale,
        sides,
        rotation,
        outline,
        out,
    );
    unit_icons::push_pictogram(
        center,
        radius,
        look.icon,
        with_alpha(LABEL_COLOR, alpha),
        out,
    );
}

/// The reticle of a unit on alert (`Unit::alert`): a red ring with a tick on
/// each diagonal, unlike Guard's white hex.
pub(super) fn push_alert_reticle(center: Vec2, scale: f32, out: &mut Vec<Vertex>) {
    mesh::polygon_outline(
        center,
        ALERT_RING_RADIUS * scale,
        ALERT_WIDTH * scale,
        32,
        0.0,
        ALERT_COLOR,
        out,
    );
    let (inner, outer) = ALERT_TICK_SPAN;
    for degrees in [45.0_f32, 135.0, 225.0, 315.0] {
        let direction = Vec2::from_angle(degrees.to_radians());
        mesh::segment(
            center + direction * inner * scale,
            center + direction * outer * scale,
            ALERT_WIDTH * 1.4 * scale,
            ALERT_COLOR,
            out,
        );
    }
}

/// Status rings behind the icon. They're filled discs, so only the rim shows
/// once the icon is drawn on top; the larger one goes first so both stay visible.
/// A guarding unit also gets a hex outline around everything, and a unit on
/// alert a red reticle.
pub(super) fn push_status_rings(center: Vec2, unit: &Unit, scale: f32, out: &mut Vec<Vertex>) {
    if unit.guarding {
        mesh::polygon_outline(
            center,
            GUARD_OUTLINE_RADIUS * scale,
            GUARD_OUTLINE_WIDTH * scale,
            6,
            0.0,
            GUARD_OUTLINE_COLOR,
            out,
        );
    }
    if unit.alert {
        push_alert_reticle(center, scale, out);
    }
    if unit.ability_queued {
        mesh::regular_polygon(
            center,
            ABILITY_RING_RADIUS * scale,
            24,
            0.0,
            ABILITY_RING_COLOR,
            out,
        );
    }
    if unit.deployed {
        mesh::regular_polygon(
            center,
            DEPLOYED_RING_RADIUS * scale,
            24,
            0.0,
            DEPLOYED_RING_COLOR,
            out,
        );
    }
}

/// Small numbered badges showing where the unit's type moves and attacks in
/// the turn order.
pub(super) fn push_order_badges(
    center: Vec2,
    unit: &Unit,
    look: UnitLook,
    scale: f32,
    out: &mut Vec<Vertex>,
) {
    let badges = [
        (-BADGE_OFFSET_X, Phase::Move, MOVE_ORDER_COLOR),
        (BADGE_OFFSET_X, Phase::Attack, ATTACK_ORDER_COLOR),
    ];
    // Civilians never attack, so they get no attack badge.
    let shown = if look.civilian { 1 } else { 2 };
    for (offset_x, phase, color) in badges.into_iter().take(shown) {
        let pos = center + Vec2::new(offset_x * scale, 0.0);
        push_rank_badge(pos, step_rank(unit.unit_type, phase), scale, color, out);
    }
}

/// Preserve single-digit ink centering; future ranks also support multiple digits.
pub(super) fn push_rank_badge(
    pos: Vec2,
    rank: u32,
    scale: f32,
    color: Color,
    out: &mut Vec<Vertex>,
) {
    let text = rank.to_string();
    let height = BADGE_DIGIT_HEIGHT * scale;
    let radius = (BADGE_RADIUS * scale).max(font::world_text_width(&text, height) / 2.0);
    mesh::regular_polygon(pos, radius, 12, 0.0, BADGE_BG_COLOR, out);
    if rank < 10 {
        font::push_glyph(pos, height, char::from(b'0' + rank as u8), color, out);
    } else {
        font::push_text_centered(pos, height, &text, color, out);
    }
}

impl GameState {
    /// The attack preview's marks on the map's health bars: units' where
    /// they're drawn, and the Barracks' and coastal batteries'.
    pub(super) fn push_attack_preview(&self, preview: &AttackPreview, out: &mut Vec<Vertex>) {
        let pulse = self.preview_pulse();
        for loss in &preview.losses {
            let (at, scale, max_hp) = match loss.hurt {
                Hurt::Unit(i) => {
                    let (at, scale) = self.drawn_unit_layout(i);
                    (at, scale, self.units[i].max_hp())
                }
                Hurt::Barracks(city) => match self.cities[city].barracks {
                    Some(hex) => (hex.to_world(), BARRACKS_BAR_SCALE, BARRACKS_MAX_HP),
                    None => continue,
                },
                Hurt::Battery(city) => {
                    match self.cities[city].placed_site(Building::CoastalBattery) {
                        Some(hex) => (hex.to_world(), BATTERY_BAR_SCALE, COASTAL_BATTERY_MAX_HP),
                        None => continue,
                    }
                }
                // Drawn with the interior (`build_interior_vertices`).
                Hurt::Fighter(_) | Hurt::Post => continue,
            };
            push_health_loss(at, loss, max_hp, scale, pulse, out);
        }
    }

    /// The preview's pulse, 0 to 1, on the presentation clock (still in a
    /// screenshot, like the clouds).
    pub(super) fn preview_pulse(&self) -> f32 {
        0.5 + 0.5 * (self.cloud_time * PREVIEW_PULSE_RATE * TAU).sin()
    }
}

/// The color of the HP a preview shows lost, at `pulse` (0 to 1).
pub(super) fn preview_loss_color(pulse: f32) -> Color {
    mix(PREVIEW_LOSS_DARK, PREVIEW_LOSS_BRIGHT, pulse)
}

/// `loss` marked on the health bar `push_health_bar` draws for `max_hp` at
/// `unit_center`: the HP it would lose as a pulsing segment, a flashing red
/// rim if it's lethal, and the number over the bar.
pub(super) fn push_health_loss(
    unit_center: Vec2,
    loss: &Loss,
    max_hp: f32,
    scale: f32,
    pulse: f32,
    out: &mut Vec<Vertex>,
) {
    let size = Vec2::new(HEALTH_BAR_WIDTH, HEALTH_BAR_HEIGHT) * scale;
    let min = unit_center + Vec2::new(0.0, HEALTH_BAR_OFFSET_Y * scale) - size / 2.0;
    let max = min + size;
    let from = ((loss.hp - loss.amount) / max_hp).clamp(0.0, 1.0);
    let to = (loss.hp / max_hp).clamp(0.0, 1.0);
    mesh::quad(
        Vec2::new(min.x + size.x * from, min.y),
        Vec2::new(min.x + size.x * to, max.y),
        preview_loss_color(pulse),
        out,
    );
    let lethal = loss.lethal();
    if lethal {
        let rim = PREVIEW_RIM * scale;
        let color = with_alpha(PREVIEW_LETHAL_COLOR, 0.35 + 0.65 * pulse);
        let (outer_min, outer_max) = (min - Vec2::splat(rim), max + Vec2::splat(rim));
        mesh::quad(outer_min, Vec2::new(outer_max.x, min.y), color, out);
        mesh::quad(Vec2::new(outer_min.x, max.y), outer_max, color, out);
        mesh::quad(
            Vec2::new(outer_min.x, min.y),
            Vec2::new(min.x, max.y),
            color,
            out,
        );
        mesh::quad(
            Vec2::new(max.x, min.y),
            Vec2::new(outer_max.x, max.y),
            color,
            out,
        );
    }
    let text = if lethal {
        format!("-{:.0} LETHAL", loss.amount)
    } else {
        format!("-{:.0}", loss.amount)
    };
    let color = if loss.target_side {
        PREVIEW_DEALT_COLOR
    } else {
        PREVIEW_RETALIATION_COLOR
    };
    // Legible however small the unit is drawn (a contested hex).
    let height = PREVIEW_LABEL_HEIGHT * scale.max(0.75);
    let radius = height * 0.85;
    let at = Vec2::new(unit_center.x, max.y + PREVIEW_RIM * scale + radius);
    let reach = (font::world_text_width(&text, height) / 2.0 + radius * 0.4 - radius).max(0.0);
    push_pill(at, reach, radius, BADGE_BG_COLOR, out);
    font::push_text_centered(at, height, &text, color, out);
}

pub(super) fn push_health_bar(
    unit_center: Vec2,
    hp_fraction: f32,
    scale: f32,
    out: &mut Vec<Vertex>,
) {
    let hp = hp_fraction.clamp(0.0, 1.0);
    let size = Vec2::new(HEALTH_BAR_WIDTH, HEALTH_BAR_HEIGHT) * scale;
    let min = unit_center + Vec2::new(0.0, HEALTH_BAR_OFFSET_Y * scale) - size / 2.0;

    mesh::quad(min, min + size, HEALTH_BAR_BG_COLOR, out);
    mesh::quad(
        min,
        min + size * Vec2::new(hp, 1.0),
        [1.0 - hp, hp, 0.08, 1.0],
        out,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::draw::tests::count_color;
    use crate::game::unit::{Unit, UnitType};

    #[test]
    fn order_badges_render_multidigit_ranks_without_panicking() {
        for rank in [0, 9, 10, 100, u32::MAX] {
            let mut vertices = Vec::new();
            push_rank_badge(Vec2::ZERO, rank, 1.0, LABEL_COLOR, &mut vertices);
            let glyphs = vertices.iter().filter(|v| v.color == LABEL_COLOR).count();
            assert_eq!(glyphs, rank.to_string().len() * 6);
            assert!(vertices.iter().all(|v| v.pos.iter().all(|x| x.is_finite())));
        }
    }

    #[test]
    fn the_attack_preview_marks_health_bars_only_while_its_target_is_hovered() {
        let target = Hex::new(1, 0);
        let mut game = GameState::new();
        game.cities.clear();
        game.units = vec![
            Unit::new(1, Hex::new(0, 0), Team::Blue, UnitType::Melee),
            Unit::new(2, target, Team::Red, UnitType::Melee),
        ];
        game.selected = Some(0);
        let loss = preview_loss_color(game.preview_pulse());
        let rim = with_alpha(PREVIEW_LETHAL_COLOR, 0.35 + 0.65 * game.preview_pulse());
        let marks = |game: &GameState| {
            let scene = game.build_vertices();
            (count_color(&scene, loss), count_color(&scene, rim))
        };

        game.hovered_tile = None;
        assert_eq!(marks(&game), (0, 0));
        game.hovered_tile = Some(Hex::new(0, 1));
        assert_eq!(marks(&game), (0, 0), "an empty hex is no target");
        // The target's loss and the attacker's retaliation, neither lethal.
        game.hovered_tile = Some(target);
        let (segments, rims) = marks(&game);
        assert!(segments > 0);
        assert_eq!(rims, 0);
        let one_bar = segments / 2;
        // A lethal hit rims the target's bar, and draws no retaliation.
        game.units[1].hp = 5.0;
        let (segments, rims) = marks(&game);
        assert_eq!(segments, one_bar);
        assert!(rims > 0);
        game.hovered_tile = None;
        assert_eq!(marks(&game), (0, 0));
    }
}
