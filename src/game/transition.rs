//! The turn transition, so a turn that resolves at once (instant playback, or
//! a network game resolving when the last player ends theirs) doesn't just
//! snap everything into place: every unit and worker the player saw glides
//! from where it was drawn to its new spot (`GLIDE_TIME`), and as the next
//! turn's planning begins the map dims for a moment and the turn number
//! flashes gold (the turn cue, `CUE_TIME`). With step-by-step playback each
//! step's movers glide, and the cue comes once, as the turn ends.
//!
//! Presentation only: the game state still changes at once, and only where
//! things are drawn lags for a fraction of a second. Nothing here is read by
//! resolution, the AI, the lockstep or its checksum; input, hover and hit
//! tests keep using the real positions, so nothing waits on it. Its clock
//! runs in `update(dt)`, so it only plays when time passes: screenshot mode
//! shows the opening position, and tests calling `update(0.0)` keep it
//! still. The Turn Transition setting turns it off.

use glam::Vec2;

use super::GameState;
use super::fast_hash::HashMap;
use super::fog::Fog;
use super::mesh;
use super::workers::FieldWorker;
use crate::renderer::Vertex;

type Color = [f32; 4];

/// Seconds a unit or worker takes to glide to its new spot.
const GLIDE_TIME: f32 = 0.35;
/// Seconds the turn number takes to fade from gold back to its color.
const CUE_TIME: f32 = 0.8;
/// Seconds the map's dimming takes to clear, and how dark it starts.
const DIM_TIME: f32 = 0.4;
const DIM_ALPHA: f32 = 0.18;
/// The turn number's color as a new turn begins.
const CUE_COLOR: Color = [1.0, 0.78, 0.30, 1.0];
// Well under a second: a transition, not a wait.
const _: () = assert!(GLIDE_TIME < 0.5 && CUE_TIME < 1.0 && DIM_TIME < CUE_TIME);

/// Where a unit is drawn, and at what scale.
type Spot = (Vec2, f32);

/// The transition playing. A copy of the game (a savestate, a network turn's
/// start) starts without one: it belongs to what this window shows, not to
/// the game.
#[derive(Default, Debug)]
pub(super) struct Transition {
    /// Units gliding, by id: from where they were drawn to `unit_layout`.
    units: HashMap<u32, Glide>,
    /// Workers gliding, by id: from where they were drawn to their hex.
    workers: HashMap<u32, Glide>,
    /// Seconds since the latest turn cue began, while it plays.
    cue: Option<f32>,
}

impl Clone for Transition {
    fn clone(&self) -> Self {
        Self::default()
    }
}

#[derive(Clone, Copy, Debug)]
struct Glide {
    from: Vec2,
    from_scale: f32,
    age: f32,
}

impl Glide {
    /// How far along it is, 0 to 1, easing out so it slows into place.
    fn progress(&self) -> f32 {
        let t = (self.age / GLIDE_TIME).clamp(0.0, 1.0);
        1.0 - (1.0 - t).powi(3)
    }
}

/// What the player saw as a batch of steps began (`GameState::before_steps`):
/// each unit's and worker's spot and where it was drawn, and the fog then.
pub(super) struct Before {
    fog: Fog,
    /// Id, `unit_layout`, and the drawn center and scale.
    units: Vec<(u32, Spot, Spot)>,
    /// Id, hex center, and the drawn center.
    workers: Vec<(u32, Vec2, Vec2)>,
}

impl GameState {
    /// Before `update` resolves a batch of steps: what the player sees, for
    /// `start_transition` to glide from. `None` with the setting off or
    /// nothing to resolve.
    pub(super) fn before_steps(&self) -> Option<Before> {
        if !self.settings.turn_transition || self.pending_steps.is_empty() {
            return None;
        }
        let fog = self.fog();
        let units = (0..self.units.len())
            .filter(|&i| fog.shows(&self.units[i]))
            .map(|i| {
                (
                    self.units[i].id,
                    self.unit_layout(i),
                    self.drawn_unit_layout(i),
                )
            })
            .collect();
        let workers = self
            .field_workers
            .iter()
            .filter(|w| fog.shows_worker(w))
            .map(|w| {
                let at = w.pos.to_world();
                (w.id, at, at + self.worker_glide(w))
            })
            .collect();
        Some(Before {
            fog,
            units,
            workers,
        })
    }

    /// After `update` resolved a batch of steps: everything the player saw
    /// that has moved glides from where it was drawn, and a finished turn
    /// (`turn_over`) starts the cue. A worker sent out this turn glides from
    /// the base it left, if the player saw it.
    pub(super) fn start_transition(&mut self, before: Option<Before>, turn_over: bool) {
        let Some(before) = before else {
            return;
        };
        let units: HashMap<u32, usize> = self
            .units
            .iter()
            .enumerate()
            .map(|(i, unit)| (unit.id, i))
            .collect();
        let mut glides = std::mem::take(&mut self.transition.units);
        glides.retain(|id, _| units.contains_key(id));
        for (id, was, (from, from_scale)) in before.units {
            let Some(&i) = units.get(&id) else {
                continue;
            };
            if self.unit_layout(i) != was {
                let age = 0.0;
                glides.insert(
                    id,
                    Glide {
                        from,
                        from_scale,
                        age,
                    },
                );
            }
        }
        self.transition.units = glides;

        let was: HashMap<u32, (Vec2, Vec2)> = before
            .workers
            .into_iter()
            .map(|(id, at, drawn)| (id, (at, drawn)))
            .collect();
        let local = self.local_team;
        let workers = &mut self.transition.workers;
        workers.retain(|id, _| self.field_workers.iter().any(|w| w.id == *id));
        for worker in &self.field_workers {
            let at = worker.pos.to_world();
            let from = match was.get(&worker.id) {
                Some(&(was_at, drawn)) => (was_at != at).then_some(drawn),
                None => (worker.team == local || before.fog.sees(worker.base))
                    .then(|| worker.base.to_world())
                    .filter(|&base| base != at),
            };
            if let Some(from) = from {
                let (from_scale, age) = (1.0, 0.0);
                workers.insert(
                    worker.id,
                    Glide {
                        from,
                        from_scale,
                        age,
                    },
                );
            }
        }

        if turn_over {
            self.transition.cue = Some(0.0);
        }
    }

    /// Moves the transition `dt` seconds on (`update`), dropping what's done.
    pub(super) fn age_transition(&mut self, dt: f32) {
        let transition = &mut self.transition;
        for glides in [&mut transition.units, &mut transition.workers] {
            if glides.is_empty() {
                continue;
            }
            for glide in glides.values_mut() {
                glide.age += dt;
            }
            glides.retain(|_, glide| glide.age < GLIDE_TIME);
        }
        if let Some(age) = &mut transition.cue {
            *age += dt;
            if *age >= CUE_TIME {
                transition.cue = None;
            }
        }
    }

    /// Where unit `idx` is drawn and at what scale: `unit_layout`, or part
    /// way there while it glides.
    pub(super) fn drawn_unit_layout(&self, idx: usize) -> (Vec2, f32) {
        let (center, scale) = self.unit_layout(idx);
        if self.transition.units.is_empty() {
            return (center, scale);
        }
        match self.transition.units.get(&self.units[idx].id) {
            Some(glide) => {
                let t = glide.progress();
                let drawn_scale = glide.from_scale + (scale - glide.from_scale) * t;
                (glide.from.lerp(center, t), drawn_scale)
            }
            None => (center, scale),
        }
    }

    /// How far from its hex's center `worker` is drawn while it glides.
    pub(super) fn worker_glide(&self, worker: &FieldWorker) -> Vec2 {
        if self.transition.workers.is_empty() {
            return Vec2::ZERO;
        }
        self.transition
            .workers
            .get(&worker.id)
            .map_or(Vec2::ZERO, |glide| {
                (glide.from - worker.pos.to_world()) * (1.0 - glide.progress())
            })
    }

    /// The turn number's color in the top bar, both presentations: `base`,
    /// flashing gold as a new turn begins and fading back over `CUE_TIME`.
    pub(super) fn turn_number_color(&self, base: Color) -> Color {
        let Some(age) = self.transition.cue else {
            return base;
        };
        // Holds near gold, then fades faster toward the end.
        let gold = 1.0 - (age / CUE_TIME).clamp(0.0, 1.0).powi(2);
        std::array::from_fn(|i| base[i] + (CUE_COLOR[i] - base[i]) * gold)
    }

    /// The turn cue on the map: a moment's dimming over everything the camera
    /// may show, clearing over `DIM_TIME`. Drawn under the units, so they
    /// stand out as they glide.
    pub(super) fn push_turn_dim(&self, out: &mut Vec<Vertex>) {
        let Some(age) = self.transition.cue else {
            return;
        };
        let t = age / DIM_TIME;
        if t >= 1.0 {
            return;
        }
        let alpha = DIM_ALPHA * (1.0 - t).powi(2);
        let (min, max) = self.cloud_view();
        mesh::quad(
            min - Vec2::ONE,
            max + Vec2::ONE,
            [0.0, 0.0, 0.0, alpha],
            out,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::hex::Hex;
    use crate::game::unit::{Team, Unit, UnitType};

    /// A game with one Blue melee ordered one hex east, alone on the map.
    fn one_mover() -> (GameState, u32, Hex, Hex) {
        let mut game = GameState::new();
        game.clear_selection();
        let blue = game
            .units
            .iter()
            .position(|u| u.team == Team::Blue)
            .unwrap();
        let id = game.units[blue].id;
        game.units.retain(|u| u.id == id);
        let from = game.units[0].pos;
        let to = Hex::new(from.q + 1, from.r);
        assert!(game.grid.is_passable(to));
        game.units[0].planned_move = Some(to);
        (game, id, from, to)
    }

    fn resolve(game: &mut GameState) {
        game.resolve_turn();
        game.update(0.0);
        assert!(!game.is_resolving());
    }

    fn drawn(game: &GameState, id: u32) -> Vec2 {
        let idx = game.units.iter().position(|u| u.id == id).unwrap();
        game.drawn_unit_layout(idx).0
    }

    #[test]
    fn a_unit_glides_from_its_old_hex_to_its_new_one() {
        let (mut game, id, from, to) = one_mover();
        resolve(&mut game);
        assert_eq!(game.units[0].pos, to, "the game moved it at once");
        assert_eq!(drawn(&game, id), from.to_world(), "drawn where it was");

        // Part way, eased: past halfway at half the time, and always
        // between the two hexes.
        game.update(GLIDE_TIME / 2.0);
        let half = drawn(&game, id);
        let along = (half - from.to_world()).length() / (to.to_world() - from.to_world()).length();
        assert!(along > 0.5 && along < 1.0, "{along}");
        let mut last = along;
        for _ in 0..3 {
            game.update(GLIDE_TIME / 8.0);
            let now = (drawn(&game, id) - from.to_world()).length()
                / (to.to_world() - from.to_world()).length();
            assert!(now >= last && now <= 1.0, "{now} after {last}");
            last = now;
        }

        game.update(GLIDE_TIME);
        assert_eq!(drawn(&game, id), to.to_world(), "settled");
        assert!(game.transition.units.is_empty());
    }

    #[test]
    fn the_turn_cue_flashes_the_turn_number_and_dims_the_map_briefly() {
        let (mut game, ..) = one_mover();
        let mut dim = Vec::new();
        game.push_turn_dim(&mut dim);
        assert!(dim.is_empty());
        assert_eq!(game.turn_number_color([1.0; 4]), [1.0; 4]);

        resolve(&mut game);
        game.push_turn_dim(&mut dim);
        assert!(!dim.is_empty(), "the map dims as the turn ends");
        assert_eq!(game.turn_number_color([1.0; 4]), CUE_COLOR);

        game.update(DIM_TIME);
        dim.clear();
        game.push_turn_dim(&mut dim);
        assert!(dim.is_empty(), "the dimming clears first");
        let fading = game.turn_number_color([1.0; 4]);
        assert!(fading != CUE_COLOR && fading != [1.0; 4], "{fading:?}");
        game.update(CUE_TIME);
        assert_eq!(game.turn_number_color([1.0; 4]), [1.0; 4]);
    }

    #[test]
    fn step_playback_glides_each_step_and_cues_once_at_the_end() {
        let (mut game, id, from, to) = one_mover();
        game.settings.instant_playback = false;
        game.resolve_turn();
        // The pause before the first step, then the melee's move.
        game.update(0.61);
        assert_eq!(game.units[0].pos, to);
        assert_eq!(drawn(&game, id), from.to_world());
        assert!(game.is_resolving());
        assert!(
            game.transition.cue.is_none(),
            "not while the turn plays out"
        );
        while game.is_resolving() {
            game.update(0.61);
        }
        assert_eq!(drawn(&game, id), to.to_world(), "glided during the steps");
        assert!(game.transition.cue.is_some());
    }

    #[test]
    fn nothing_glides_or_cues_with_the_setting_off() {
        let (mut game, id, _, to) = one_mover();
        game.settings.turn_transition = false;
        resolve(&mut game);
        assert_eq!(drawn(&game, id), to.to_world());
        assert!(game.transition.cue.is_none());
        let mut dim = Vec::new();
        game.push_turn_dim(&mut dim);
        assert!(dim.is_empty());
    }

    #[test]
    fn a_unit_the_player_never_saw_does_not_give_away_where_it_came_from() {
        let (mut game, ..) = one_mover();
        game.units[0].planned_move = None;
        // A Red scout just out of sight steps into it. People play Red too,
        // so the AI leaves its order be.
        game.humans.push(Team::Red);
        let fog = game.fog();
        let hidden = game
            .grid
            .all_hexes()
            .filter(|&h| !fog.sees(h) && game.grid.is_passable(h) && !game.is_occupied(h))
            .find(|&h| {
                h.neighbors()
                    .into_iter()
                    .any(|n| fog.sees(n) && game.grid.is_passable(n) && !game.is_occupied(n))
            })
            .expect("a hidden hex beside sight");
        let seen = hidden
            .neighbors()
            .into_iter()
            .find(|&n| fog.sees(n) && game.grid.is_passable(n) && !game.is_occupied(n))
            .unwrap();
        let id = 900;
        let mut scout = Unit::new(id, hidden, Team::Red, UnitType::Scout);
        scout.planned_move = Some(seen);
        game.units.push(scout);
        resolve(&mut game);
        assert_eq!(game.units[1].pos, seen);
        assert_eq!(drawn(&game, id), seen.to_world(), "appears where it is");
    }

    #[test]
    fn a_worker_sent_out_glides_from_its_city() {
        use crate::game::workers::JobKind;
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.selected = None;
        game.fog_of_war = false;
        let city = game.cities[0].pos;
        let hex = game
            .grid
            .all_hexes()
            .filter(|&h| h.distance(city) == 3 && game.grid.is_passable(h))
            .filter(|&h| !game.sites.contains_key(&h) && !game.roads.contains(&h))
            .min_by_key(|h| (h.q, h.r))
            .expect("a bare tile");
        game.selected_city = Some(0);
        game.placing_job = Some(JobKind::Fort);
        game.place_job_at(hex, None);
        game.placing_job = None;
        game.selected_city = None;
        resolve(&mut game);

        let worker = game.field_workers.iter().find(|w| w.home == 0).unwrap();
        assert_ne!(worker.pos, city, "on its way");
        let glide = game.worker_glide(worker);
        assert_eq!(
            worker.pos.to_world() + glide,
            city.to_world(),
            "drawn at home"
        );
        let id = worker.id;
        game.update(GLIDE_TIME);
        let worker = game.field_workers.iter().find(|w| w.id == id).unwrap();
        assert_eq!(game.worker_glide(worker), Vec2::ZERO);
    }

    #[test]
    fn the_game_is_the_same_with_or_without_the_transition() {
        let play = |transition: bool| {
            let mut game = GameState::city_scenario();
            game.seed_rng(7);
            game.settings.turn_transition = transition;
            for _ in 0..6 {
                game.clear_selection();
                game.plan_ai_turn(Team::Blue);
                game.resolve_turn();
                game.update(0.0);
                game.update(0.1);
            }
            game.checksum()
        };
        assert_eq!(play(true), play(false));
    }

    #[test]
    fn a_copy_of_the_game_starts_settled() {
        let (mut game, id, _, to) = one_mover();
        resolve(&mut game);
        let copy = game.clone();
        assert!(copy.transition.cue.is_none());
        assert_eq!(drawn(&copy, id), to.to_world());
    }
}
