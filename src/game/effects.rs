//! Short animations of attacks resolving during turn playback, so it's clear
//! whether each one landed: an arrow shoots from attacker to target, then a
//! burst on a hit, a grey "MISS" if the hex was empty, or "OUT OF RANGE" if
//! the target moved out of reach first. Every unit hurt shows a damage number
//! rising from it, retaliation included.

use glam::Vec2;

use super::draw::{attack_arc_points, push_arrow};
use super::hex::Hex;
use super::{GameState, font, mesh};
use crate::renderer::Vertex;

use super::mesh::Color;

/// Seconds the arrow takes to reach its target.
const SHOT_TRAVEL: f32 = 0.3;
/// Seconds the arrow and its outcome take to fade once it arrives.
const SHOT_FADE: f32 = 0.5;
/// Seconds a damage number rises and fades, starting when the arrow lands.
const DAMAGE_TIME: f32 = 1.0;
/// How far, in world units, a damage number rises.
const DAMAGE_RISE: f32 = 0.5;

const HIT_COLOR: Color = [1.00, 0.45, 0.30, 1.0];
const MISS_COLOR: Color = [0.62, 0.62, 0.66, 1.0];
const OUTLINE_COLOR: Color = [0.08, 0.03, 0.02, 1.0];
const BURST_COLOR: Color = [1.00, 0.85, 0.55, 1.0];
const DAMAGE_COLOR: Color = [1.00, 0.35, 0.28, 1.0];
const TEXT_SHADOW: Color = [0.0, 0.0, 0.0, 0.9];
const LABEL_HEIGHT: f32 = 0.24;
const DAMAGE_HEIGHT: f32 = 0.3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Outcome {
    /// Something was there to hit.
    Hit,
    /// The target hex was empty when the attack landed.
    Miss,
    /// The target was out of range after the moves, so it was called off.
    OutOfRange,
}

#[derive(Clone, Debug)]
pub(super) enum Effect {
    Shot {
        from: Vec2,
        to: Vec2,
        outcome: Outcome,
    },
    Damage {
        at: Vec2,
        amount: f32,
        fatal: bool,
    },
}

impl Effect {
    /// Seconds from start to finish.
    fn lifetime(&self) -> f32 {
        match self {
            Effect::Shot { .. } => SHOT_TRAVEL + SHOT_FADE,
            Effect::Damage { .. } => SHOT_TRAVEL + DAMAGE_TIME,
        }
    }
}

impl GameState {
    /// Starts an effect only when the local player can see it.
    pub(super) fn play(&mut self, effect: Effect) {
        let fog = self.fog();
        let visible = match &effect {
            Effect::Shot { from, to, .. } => {
                fog.sees(Hex::from_world(*from)) && fog.sees(Hex::from_world(*to))
            }
            Effect::Damage { at, .. } => fog.sees(Hex::from_world(*at)),
        };
        if visible {
            self.effects.push((effect, 0.0));
        }
    }

    /// Moves every effect `dt` seconds on, dropping finished ones.
    pub(super) fn age_effects(&mut self, dt: f32) {
        for (_, age) in &mut self.effects {
            *age += dt;
        }
        self.effects
            .retain(|(effect, age)| *age < effect.lifetime());
    }

    /// Draws every playing effect, on top of the units.
    pub(super) fn push_effects(&self, out: &mut Vec<Vertex>) {
        for (effect, age) in &self.effects {
            match *effect {
                Effect::Shot { from, to, outcome } => push_shot(from, to, outcome, *age, out),
                Effect::Damage { at, amount, fatal } => push_damage(at, amount, fatal, *age, out),
            }
        }
    }
}

fn push_shot(from: Vec2, to: Vec2, outcome: Outcome, age: f32, out: &mut Vec<Vertex>) {
    let arrived = (age - SHOT_TRAVEL).max(0.0) / SHOT_FADE;
    let alpha = 1.0 - arrived;
    if outcome == Outcome::OutOfRange {
        let rise = Vec2::new(0.0, 0.45 + 0.2 * (age / (SHOT_TRAVEL + SHOT_FADE)));
        let fade = 1.0 - age / (SHOT_TRAVEL + SHOT_FADE);
        push_label(from + rise, "OUT OF RANGE", MISS_COLOR, fade, out);
        return;
    }

    let color = if outcome == Outcome::Hit {
        HIT_COLOR
    } else {
        MISS_COLOR
    };
    let points = attack_arc_points(from, to, age / SHOT_TRAVEL);
    push_arrow(&points, fade(color, alpha), fade(OUTLINE_COLOR, alpha), out);
    if arrived <= 0.0 {
        return;
    }
    match outcome {
        // A ring bursting outward from the target.
        Outcome::Hit => mesh::polygon_outline(
            to,
            0.25 + 0.45 * arrived,
            0.08 * (1.0 - arrived) + 0.02,
            24,
            0.0,
            fade(BURST_COLOR, alpha),
            out,
        ),
        Outcome::Miss => push_label(to + Vec2::new(0.0, 0.3), "MISS", MISS_COLOR, alpha, out),
        Outcome::OutOfRange => {}
    }
}

/// A damage number rising from `at`, once the arrow has landed.
fn push_damage(at: Vec2, amount: f32, fatal: bool, age: f32, out: &mut Vec<Vertex>) {
    let t = (age - SHOT_TRAVEL) / DAMAGE_TIME;
    if t < 0.0 {
        return;
    }
    let alpha = (1.0 - t * t).clamp(0.0, 1.0);
    let text = if fatal {
        format!("-{amount:.0} KILLED")
    } else {
        format!("-{amount:.0}")
    };
    let position = at + Vec2::new(0.0, 0.55 + DAMAGE_RISE * t);
    push_shadowed_text(position, DAMAGE_HEIGHT, &text, DAMAGE_COLOR, alpha, out);
}

fn push_label(center: Vec2, text: &str, color: Color, alpha: f32, out: &mut Vec<Vertex>) {
    push_shadowed_text(center, LABEL_HEIGHT, text, color, alpha, out);
}

/// World text centered on `center`, over a dark shadow so it reads on any
/// terrain.
fn push_shadowed_text(
    center: Vec2,
    cap_height: f32,
    text: &str,
    color: Color,
    alpha: f32,
    out: &mut Vec<Vertex>,
) {
    let width = font::world_text_width(text, cap_height);
    let origin = center - Vec2::new(width / 2.0, cap_height / 2.0);
    let shadow = Vec2::new(0.02, -0.02);
    let shadow_color = fade(TEXT_SHADOW, alpha);
    font::push_text(origin + shadow, cap_height, text, shadow_color, out);
    font::push_text(origin, cap_height, text, fade(color, alpha), out);
}

fn fade(color: Color, alpha: f32) -> Color {
    let [r, g, b, a] = color;
    [r, g, b, a * alpha.clamp(0.0, 1.0)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_play_out_and_are_dropped() {
        let mut game = GameState::new();
        game.play(Effect::Damage {
            at: Vec2::ZERO,
            amount: 20.0,
            fatal: false,
        });
        let quiet = {
            let mut out = Vec::new();
            game.push_effects(&mut out);
            out.len()
        };
        game.age_effects(SHOT_TRAVEL + 0.1);
        let mut out = Vec::new();
        game.push_effects(&mut out);
        assert!(out.len() > quiet, "the number shows once the shot lands");
        game.age_effects(10.0);
        assert!(game.effects.is_empty());
    }
}
