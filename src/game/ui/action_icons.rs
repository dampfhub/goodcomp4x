//! Small vector pictograms shared by classic and ImGui action toolbars.

use glam::Vec2;

use super::{Target, UnitAction};
use crate::game::city::{Build, Good};
use crate::game::mesh;
use crate::game::strings::text;
use crate::game::unit_icons::{self, UnitIcon};
use crate::renderer::Vertex;

pub(super) const ICON_BUTTON_SIZE: f32 = 40.0;
pub(super) const CLASSIC_ICON_COLUMNS: usize = 5;

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
    Alert,
    Clear,
    Disband,
    Settle,
    Food,
    Wood,
    Metal,
}

pub(super) fn for_button(target: Target, label: &str) -> Option<ActionIcon> {
    Some(match target {
        Target::Unit(UnitAction::Move) => ActionIcon::Move,
        Target::Unit(UnitAction::Attack) => ActionIcon::Attack,
        Target::Unit(UnitAction::Swap) => ActionIcon::Swap,
        Target::Unit(UnitAction::Hold) => ActionIcon::Hold,
        Target::Unit(UnitAction::Guard) => ActionIcon::Guard,
        Target::Unit(UnitAction::Alert) => ActionIcon::Alert,
        Target::Unit(UnitAction::ClearOrders) => ActionIcon::Clear,
        Target::Unit(UnitAction::Disband) => ActionIcon::Disband,
        Target::Unit(UnitAction::Settle) => ActionIcon::Settle,
        // The ability's name, as `ability_text` gives it.
        Target::Unit(UnitAction::Ability) => match label.split(" (").next().unwrap_or(label) {
            name if name == text!("ability_shield_wall") => ActionIcon::ShieldWall,
            name if name == text!("ability_volley") => ActionIcon::Volley,
            name if name == text!("ability_charge") => ActionIcon::Charge,
            name if name == text!("ability_deploy") || name == text!("ability_pack_up") => {
                ActionIcon::Deploy
            }
            name if name == text!("ability_lookout") => ActionIcon::Lookout,
            _ => return None,
        },
        Target::Priority(Good::Food) => ActionIcon::Food,
        Target::Priority(Good::Wood) => ActionIcon::Wood,
        Target::Priority(Good::Metal) => ActionIcon::Metal,
        _ => return None,
    })
}

/// What an icon button shows in its corner, from the end of its label,
/// "NAME (3)": an ability's cooldown, or a priority chip's rank.
pub(super) fn badge(label: &str) -> Option<&str> {
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
        // A sight's reticle, as alert units wear on the map (`draw.rs`).
        ActionIcon::Alert => {
            mesh::polygon_outline(center, 0.52 * radius, 0.13 * radius, 24, 0.0, color, out);
            for (inner, outer) in [
                ((0.0, 0.26), (0.0, 0.86)),
                ((0.0, -0.26), (0.0, -0.86)),
                ((0.26, 0.0), (0.86, 0.0)),
                ((-0.26, 0.0), (-0.86, 0.0)),
            ] {
                line(out, inner, outer, 0.13);
            }
            mesh::regular_polygon(center, 0.1 * radius, 12, 0.0, color, out);
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
        ActionIcon::Wood => {
            // A log, leaning right, with rounded ends.
            let (a, b) = (Vec2::new(-0.5, -0.45), Vec2::new(0.5, 0.45));
            let width = 0.38 * radius;
            mesh::segment(center + a * radius, center + b * radius, width, color, out);
            for end in [a, b] {
                mesh::regular_polygon(center + end * radius, width / 2.0, 12, 0.0, color, out);
            }
        }
        ActionIcon::Metal => {
            // An ingot, side on.
            let bar = [(-0.72, -0.34), (0.72, -0.34), (0.48, 0.34), (-0.48, 0.34)]
                .map(|(x, y)| center + Vec2::new(x, y) * radius);
            mesh::polygon(&bar, color, out);
        }
    }
}

/// A wheat ear: a stem and rounded kernels leaning out from it.
fn food(center: Vec2, r: f32, color: [f32; 4], out: &mut Vec<Vertex>) {
    let at = |x: f32, y: f32| center + Vec2::new(x, y) * r;
    let stem_width = 0.12 * r;
    mesh::segment(at(0.0, -0.78), at(0.0, 0.2), stem_width, color, out);
    mesh::regular_polygon(at(0.0, -0.78), stem_width / 2.0, 8, 0.0, color, out);
    let kernel = |x: f32, y: f32, degrees: f32, out: &mut Vec<Vertex>| {
        let turn = Vec2::from_angle(degrees.to_radians());
        let points: Vec<Vec2> = (0..12)
            .map(|i| {
                let angle = std::f32::consts::TAU * i as f32 / 12.0;
                at(x, y) + turn.rotate(Vec2::new(0.13 * angle.cos(), 0.24 * angle.sin())) * r
            })
            .collect();
        mesh::polygon(&points, color, out);
    };
    for y in [-0.26_f32, 0.16] {
        for side in [-1.0_f32, 1.0] {
            kernel(0.3 * side, y, -32.0 * side, out);
        }
    }
    kernel(0.0, 0.56, 0.0, out);
}

/// City-production card pictogram, shared by both UI presentations.
pub(super) fn production_unit_icon(target: Target) -> Option<UnitIcon> {
    UnitIcon::of_build(match target {
        Target::Build(unit) => Build::Unit(unit),
        Target::BuildWorker => Build::Worker,
        Target::BuildScout => Build::Scout,
        Target::BuildSettler => Build::Settler,
        _ => return None,
    })
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
            ActionIcon::Alert,
            ActionIcon::Clear,
            ActionIcon::Disband,
            ActionIcon::Settle,
            ActionIcon::Food,
            ActionIcon::Wood,
            ActionIcon::Metal,
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
