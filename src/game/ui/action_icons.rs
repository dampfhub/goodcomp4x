//! Small vector pictograms shared by classic and ImGui action toolbars.

use glam::Vec2;

use super::{Target, UnitAction};
use crate::game::city::{BuildUnit, LaborFocus};
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
    Wood,
    Metal,
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
            // The salvage-age names, and the ones they had before the theme.
            "BARRICADE" | "SHIELD WALL" => ActionIcon::ShieldWall,
            "ARROW RAIN" | "VOLLEY" => ActionIcon::Volley,
            "RIDE DOWN" | "CHARGE" => ActionIcon::Charge,
            "SET UP" | "DEPLOY" | "PACK UP" => ActionIcon::Deploy,
            "BINOCULARS" | "LOOKOUT" => ActionIcon::Lookout,
            _ => return None,
        },
        Target::Focus(LaborFocus::Food) => ActionIcon::Food,
        Target::Focus(LaborFocus::Wood) => ActionIcon::Wood,
        Target::Focus(LaborFocus::Metal) => ActionIcon::Metal,
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
            // A machete, point up and to the right.
            let (along, across) = (
                Vec2::new(1.0, 1.0).normalize(),
                Vec2::new(-1.0, 1.0).normalize(),
            );
            let blade = |points: &[(f32, f32)]| -> Vec<Vec2> {
                points
                    .iter()
                    .map(|&(s, t)| center + (along * s + across * t) * radius)
                    .collect()
            };
            let handle = blade(&[(-0.98, -0.11), (-0.5, -0.11), (-0.5, 0.11), (-0.98, 0.11)]);
            mesh::polygon(&handle, color, out);
            let guard = blade(&[(-0.44, -0.2), (-0.36, -0.2), (-0.36, 0.2), (-0.44, 0.2)]);
            mesh::polygon(&guard, color, out);
            let edge = blade(&[
                (-0.36, -0.13),
                (0.45, -0.2),
                (0.8, -0.14),
                (0.98, 0.05),
                (0.86, 0.15),
                (-0.36, 0.13),
            ]);
            mesh::polygon(&edge, color, out);
        }
        ActionIcon::ShieldWall => barricade(center, radius, color, out),
        ActionIcon::Guard => {
            // A stop sign held as a shield: its rim, a gap, its face.
            let turn = std::f32::consts::PI / 8.0;
            mesh::polygon_outline(center, 0.72 * radius, 0.15 * radius, 8, turn, color, out);
            mesh::regular_polygon(center, 0.52 * radius, 8, turn, color, out);
        }
        ActionIcon::Volley => arrow_rain(center, radius, color, out),
        ActionIcon::Charge => {
            // The outrider at the gallop, with speed lines behind.
            unit_icons::push_pictogram(
                center + Vec2::new(0.2 * radius, 0.0),
                radius * 0.95,
                UnitIcon::Outrider,
                color,
                out,
            );
            line(out, (-1.15, 0.28), (-0.72, 0.28), 0.09);
            line(out, (-1.25, 0.02), (-0.66, 0.02), 0.09);
            line(out, (-1.15, -0.24), (-0.72, -0.24), 0.09);
        }
        ActionIcon::Deploy => {
            unit_icons::push_pictogram(center, radius, UnitIcon::Trebuchet, color, out)
        }
        ActionIcon::Lookout => binoculars(center, radius, color, out),
        ActionIcon::Settle => {
            // A swallowtail flag planted in a heap of rubble.
            line(out, (-0.4, -0.62), (-0.4, 0.8), 0.13);
            let flag = [
                (-0.34, 0.8),
                (0.64, 0.8),
                (0.44, 0.5),
                (0.64, 0.2),
                (-0.34, 0.2),
            ]
            .map(|(x, y)| center + Vec2::new(x, y) * radius);
            mesh::polygon(&flag, color, out);
            let heap: Vec<Vec2> = (0..=10)
                .map(|i| {
                    let angle = std::f32::consts::PI * i as f32 / 10.0;
                    center
                        + Vec2::new(-0.4 + 0.46 * angle.cos(), -0.78 + 0.26 * angle.sin()) * radius
                })
                .collect();
            mesh::polygon(&heap, color, out);
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
        ActionIcon::Metal => gear(center, radius, color, out),
        ActionIcon::Balanced => {
            // A balance: post and base, beam, and a pan hanging from each end.
            line(out, (0.0, -0.62), (0.0, 0.5), 0.12);
            rect(out, (-0.34, -0.72), (0.34, -0.6));
            line(out, (-0.72, 0.46), (0.72, 0.46), 0.1);
            for side in [-1.0_f32, 1.0] {
                let x = 0.62 * side;
                line(out, (x, 0.46), (x - 0.2, -0.02), 0.06);
                line(out, (x, 0.46), (x + 0.2, -0.02), 0.06);
                let pan: Vec<Vec2> = (0..=8)
                    .map(|i| {
                        let angle = std::f32::consts::PI * (1.0 + i as f32 / 8.0);
                        center + Vec2::new(x + 0.28 * angle.cos(), 0.16 * angle.sin()) * radius
                    })
                    .collect();
                mesh::polygon(&pan, color, out);
            }
        }
    }
}

/// An opened tin can: a ribbed body (its ribs gaps) with a rounded base,
/// and the lid tipped up above it.
fn food(center: Vec2, r: f32, color: [f32; 4], out: &mut Vec<Vertex>) {
    let at = |x: f32, y: f32| center + Vec2::new(x, y) * r;
    mesh::quad(at(-0.44, 0.1), at(0.44, 0.32), color, out);
    mesh::quad(at(-0.44, -0.24), at(0.44, 0.02), color, out);
    let mut base = vec![at(-0.44, -0.32), at(0.44, -0.32)];
    base.extend((0..=8).map(|i| {
        let angle = std::f32::consts::PI * i as f32 / 8.0;
        at(0.44 * angle.cos(), -0.52 - 0.16 * angle.sin())
    }));
    mesh::polygon(&base, color, out);
    let tilt = Vec2::from_angle(22f32.to_radians());
    let lid: Vec<Vec2> = (0..16)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / 16.0;
            let rim = tilt.rotate(Vec2::new(0.44 * angle.cos(), 0.11 * angle.sin()));
            at(0.06 + rim.x, 0.58 + rim.y)
        })
        .collect();
    mesh::polygon(&lid, color, out);
}

/// A gear: a toothed ring around a hole.
fn gear(center: Vec2, r: f32, color: [f32; 4], out: &mut Vec<Vertex>) {
    mesh::polygon_outline(center, 0.41 * r, 0.34 * r, 16, 0.0, color, out);
    for k in 0..8 {
        let along = Vec2::from_angle(std::f32::consts::TAU * (k as f32 + 0.5) / 8.0);
        let across = along.perp();
        let tooth = [
            along * 0.5 - across * 0.15,
            along * 0.8 - across * 0.11,
            along * 0.8 + across * 0.11,
            along * 0.5 + across * 0.15,
        ]
        .map(|p| center + p * r);
        mesh::polygon(&tooth, color, out);
    }
}

/// A road barricade: a board striped in slanted bars, the stripes between
/// them gaps, on an A-frame of legs at each end.
fn barricade(center: Vec2, r: f32, color: [f32; 4], out: &mut Vec<Vertex>) {
    let at = |x: f32, y: f32| center + Vec2::new(x, y) * r;
    let (bottom, top, slant) = (0.18, 0.56, 0.16);
    for side in [-1.0_f32, 1.0] {
        let x = 0.66 * side;
        mesh::segment(at(x, top), at(x - 0.24, -0.72), 0.13 * r, color, out);
        mesh::segment(at(x, top), at(x + 0.24, -0.72), 0.13 * r, color, out);
    }
    let bar = |b0: f32, b1: f32, t0: f32, t1: f32| {
        [at(b0, bottom), at(b1, bottom), at(t1, top), at(t0, top)]
    };
    mesh::polygon(&bar(-0.9, -0.64, -0.9, -0.64 + slant), color, out);
    for b0 in [-0.52_f32, -0.21, 0.1, 0.41] {
        let b1 = b0 + 0.19;
        mesh::polygon(&bar(b0, b1, b0 + slant, (b1 + slant).min(0.9)), color, out);
    }
    mesh::polygon(&bar(0.72, 0.9, 0.9, 0.9), color, out);
}

/// Three arrows falling steeply down and to the right, side by side.
fn arrow_rain(center: Vec2, r: f32, color: [f32; 4], out: &mut Vec<Vertex>) {
    let down = Vec2::new(0.42, -0.91).normalize();
    let across = down.perp();
    for (offset, lead) in [(-0.56_f32, -0.08_f32), (0.0, 0.1), (0.56, -0.08)] {
        let middle = across * offset + down * lead;
        let (tail, tip) = (middle - down * 0.56, middle + down * 0.56);
        let base = tip - down * 0.3;
        mesh::segment(center + tail * r, center + base * r, 0.09 * r, color, out);
        mesh::triangle(
            center + (base + across * 0.17) * r,
            center + tip * r,
            center + (base - across * 0.17) * r,
            color,
            out,
        );
        for side in [-1.0_f32, 1.0] {
            let vane = [
                tail + down * 0.22,
                tail + down * 0.06 + across * 0.14 * side,
                tail - down * 0.06 + across * 0.14 * side,
                tail + down * 0.06,
            ]
            .map(|p| center + p * r);
            mesh::polygon(&vane, color, out);
        }
    }
}

/// Binoculars, eyepieces up: two barrels ending in wide lenses, joined by
/// a bridge and its hinge.
fn binoculars(center: Vec2, r: f32, color: [f32; 4], out: &mut Vec<Vertex>) {
    let at = |x: f32, y: f32| center + Vec2::new(x, y) * r;
    for side in [-1.0_f32, 1.0] {
        let x = 0.41 * side;
        mesh::regular_polygon(at(x, -0.36), 0.33 * r, 16, 0.0, color, out);
        mesh::quad(at(x - 0.26, -0.36), at(x + 0.26, 0.2), color, out);
        mesh::quad(at(x - 0.17, 0.27), at(x + 0.17, 0.6), color, out);
    }
    mesh::quad(at(-0.2, -0.02), at(0.2, 0.22), color, out);
    mesh::regular_polygon(at(0.0, 0.1), 0.15 * r, 12, 0.0, color, out);
}

/// City-production card pictogram, shared by both UI presentations.
pub(super) fn production_unit_icon(target: Target) -> Option<UnitIcon> {
    Some(match target {
        Target::Build(BuildUnit::Melee) => UnitIcon::Scrapper,
        Target::Build(BuildUnit::Ranged) => UnitIcon::Bowman,
        Target::Build(BuildUnit::Cavalry) => UnitIcon::Outrider,
        Target::Build(BuildUnit::Siege) => UnitIcon::Trebuchet,
        Target::Build(BuildUnit::Armored) => UnitIcon::RiotGuard,
        Target::Build(BuildUnit::PatrolGalley) => UnitIcon::Skiff,
        Target::Build(BuildUnit::LandingCraft) => UnitIcon::Barge,
        Target::Build(BuildUnit::BombardShip) => UnitIcon::RustHulk,
        Target::BuildWorker => UnitIcon::Shovel,
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
            ActionIcon::Clear,
            ActionIcon::Disband,
            ActionIcon::Settle,
            ActionIcon::Food,
            ActionIcon::Wood,
            ActionIcon::Metal,
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
