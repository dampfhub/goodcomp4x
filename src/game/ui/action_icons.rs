//! Small vector pictograms shared by classic and ImGui action toolbars.

use glam::Vec2;

use super::{Target, UnitAction};
use crate::game::city::LaborFocus;
use crate::game::mesh;
use crate::game::unit_icons::{self, UnitIcon};
use crate::renderer::Vertex;

pub(super) const ICON_BUTTON_SIZE: f32 = 40.0;
pub(super) const CLASSIC_ICON_COLUMNS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActionIcon {
    Move,
    Attack,
    Swap,
    ShieldWall,
    Volley,
    Charge,
    Deploy,
    Lookout,
    Hold,
    Guard,
    Clear,
    Disband,
    Settle,
    Food,
    Production,
    Balanced,
}

pub(super) fn for_button(target: Target, label: &str) -> Option<ActionIcon> {
    Some(match target {
        Target::Unit(UnitAction::Move) => ActionIcon::Move,
        Target::Unit(UnitAction::Attack) => ActionIcon::Attack,
        Target::Unit(UnitAction::Swap) => ActionIcon::Swap,
        Target::Unit(UnitAction::Hold) => ActionIcon::Hold,
        Target::Unit(UnitAction::Guard) => ActionIcon::Guard,
        Target::Unit(UnitAction::ClearOrders) => ActionIcon::Clear,
        Target::Unit(UnitAction::Disband) => ActionIcon::Disband,
        Target::Unit(UnitAction::Settle) => ActionIcon::Settle,
        Target::Unit(UnitAction::Ability) => match label.split(" (").next().unwrap_or(label) {
            "SHIELD WALL" => ActionIcon::ShieldWall,
            "VOLLEY" => ActionIcon::Volley,
            "CHARGE" => ActionIcon::Charge,
            "DEPLOY" | "PACK UP" => ActionIcon::Deploy,
            "LOOKOUT" => ActionIcon::Lookout,
            _ => return None,
        },
        Target::Focus(LaborFocus::Food) => ActionIcon::Food,
        Target::Focus(LaborFocus::Production) => ActionIcon::Production,
        Target::Focus(LaborFocus::Balanced) => ActionIcon::Balanced,
        _ => return None,
    })
}

pub(super) fn cooldown(label: &str) -> Option<&str> {
    label.rsplit_once(" (")?.1.strip_suffix(')')
}

/// Append a 24 px pictogram in the renderer's Y-up coordinates. ImGui flips
/// these vertices while copying them into its window draw list.
pub(super) fn push_icon(
    center: Vec2,
    radius: f32,
    icon: ActionIcon,
    color: [f32; 4],
    out: &mut Vec<Vertex>,
) {
    let line = |out: &mut Vec<Vertex>, a: (f32, f32), b: (f32, f32), width: f32| {
        mesh::segment(
            center + Vec2::new(a.0, a.1) * radius,
            center + Vec2::new(b.0, b.1) * radius,
            width * radius,
            color,
            out,
        );
    };
    let rect = |out: &mut Vec<Vertex>, a: (f32, f32), b: (f32, f32)| {
        mesh::quad(
            center + Vec2::new(a.0, a.1) * radius,
            center + Vec2::new(b.0, b.1) * radius,
            color,
            out,
        );
    };
    let triangle = |out: &mut Vec<Vertex>, a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
        mesh::triangle(
            center + Vec2::new(a.0, a.1) * radius,
            center + Vec2::new(b.0, b.1) * radius,
            center + Vec2::new(c.0, c.1) * radius,
            color,
            out,
        );
    };
    match icon {
        ActionIcon::Attack => {
            unit_icons::push_pictogram(center, radius, UnitIcon::Sword, color, out)
        }
        ActionIcon::ShieldWall => {
            for offset in [-0.48_f32, 0.0, 0.48] {
                unit_icons::push_pictogram(
                    center + Vec2::new(offset * radius, 0.0),
                    radius * 0.58,
                    UnitIcon::Shield,
                    color,
                    out,
                );
            }
        }
        ActionIcon::Guard => {
            unit_icons::push_pictogram(center, radius, UnitIcon::Shield, color, out)
        }
        ActionIcon::Volley => unit_icons::push_pictogram(center, radius, UnitIcon::Bow, color, out),
        ActionIcon::Charge => {
            unit_icons::push_pictogram(center, radius, UnitIcon::HorseHead, color, out)
        }
        ActionIcon::Deploy => {
            unit_icons::push_pictogram(center, radius, UnitIcon::Catapult, color, out)
        }
        ActionIcon::Lookout => {
            unit_icons::push_pictogram(center, radius, UnitIcon::Spyglass, color, out)
        }
        ActionIcon::Settle => {
            unit_icons::push_pictogram(center, radius, UnitIcon::Flag, color, out)
        }
        ActionIcon::Move => {
            line(out, (-0.72, 0.0), (0.35, 0.0), 0.19);
            triangle(out, (0.12, 0.60), (0.83, 0.0), (0.12, -0.60));
        }
        ActionIcon::Swap => {
            line(out, (-0.72, 0.38), (0.56, 0.38), 0.15);
            triangle(out, (0.30, 0.70), (0.86, 0.38), (0.30, 0.06));
            line(out, (0.72, -0.38), (-0.56, -0.38), 0.15);
            triangle(out, (-0.30, -0.70), (-0.86, -0.38), (-0.30, -0.06));
        }
        ActionIcon::Hold => {
            rect(out, (-0.50, -0.70), (-0.18, 0.70));
            rect(out, (0.18, -0.70), (0.50, 0.70));
        }
        ActionIcon::Clear => {
            line(out, (-0.72, 0.45), (0.22, 0.45), 0.14);
            line(out, (-0.72, 0.0), (0.22, 0.0), 0.14);
            line(out, (-0.72, -0.45), (0.22, -0.45), 0.14);
            line(out, (0.30, -0.65), (0.75, 0.65), 0.15);
            line(out, (0.75, -0.65), (0.30, 0.65), 0.15);
        }
        ActionIcon::Disband => {
            rect(out, (-0.48, -0.62), (0.48, 0.50));
            rect(out, (-0.66, 0.55), (0.66, 0.72));
            line(out, (-0.17, -0.40), (-0.17, 0.29), 0.10);
            line(out, (0.17, -0.40), (0.17, 0.29), 0.10);
        }
        ActionIcon::Food => food(center, radius, color, out),
        ActionIcon::Production => production(center, radius, color, out),
        ActionIcon::Balanced => {
            food(
                center + Vec2::new(-0.38 * radius, 0.0),
                radius * 0.62,
                color,
                out,
            );
            production(
                center + Vec2::new(0.38 * radius, 0.0),
                radius * 0.62,
                color,
                out,
            );
        }
    }
}

fn food(center: Vec2, r: f32, color: [f32; 4], out: &mut Vec<Vertex>) {
    mesh::segment(
        center + Vec2::new(0.0, -0.75) * r,
        center + Vec2::new(0.0, 0.75) * r,
        0.13 * r,
        color,
        out,
    );
    for y in [-0.32_f32, 0.0, 0.32] {
        mesh::segment(
            center + Vec2::new(0.0, y - 0.18) * r,
            center + Vec2::new(-0.46, y + 0.12) * r,
            0.17 * r,
            color,
            out,
        );
        mesh::segment(
            center + Vec2::new(0.0, y - 0.18) * r,
            center + Vec2::new(0.46, y + 0.12) * r,
            0.17 * r,
            color,
            out,
        );
    }
}

fn production(center: Vec2, r: f32, color: [f32; 4], out: &mut Vec<Vertex>) {
    mesh::segment(
        center + Vec2::new(-0.54, -0.66) * r,
        center + Vec2::new(0.38, 0.37) * r,
        0.17 * r,
        color,
        out,
    );
    mesh::segment(
        center + Vec2::new(-0.08, 0.49) * r,
        center + Vec2::new(0.42, 0.02) * r,
        0.40 * r,
        color,
        out,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_icon_fits_its_button() {
        for icon in [
            ActionIcon::Move,
            ActionIcon::Attack,
            ActionIcon::Swap,
            ActionIcon::ShieldWall,
            ActionIcon::Volley,
            ActionIcon::Charge,
            ActionIcon::Deploy,
            ActionIcon::Lookout,
            ActionIcon::Hold,
            ActionIcon::Guard,
            ActionIcon::Clear,
            ActionIcon::Disband,
            ActionIcon::Settle,
            ActionIcon::Food,
            ActionIcon::Production,
            ActionIcon::Balanced,
        ] {
            let mut vertices = Vec::new();
            push_icon(Vec2::ZERO, 14.0, icon, [1.0; 4], &mut vertices);
            assert!(!vertices.is_empty(), "{icon:?}");
            assert!(
                vertices
                    .iter()
                    .all(|v| v.pos[0].abs() <= 19.0 && v.pos[1].abs() <= 19.0),
                "{icon:?}"
            );
        }
    }
}
