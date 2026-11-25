//! Orders on the map: ghosts at move targets, swap links, attack arcs and
//! arrows, queued plans with their numbered turns; where each unit is drawn
//! (`unit_layout`).

use super::*;

/// The two units sharing a contested hex are drawn at this scale, stacked
/// vertically with Blue on top.
pub(super) const CONTESTED_SCALE: f32 = 0.5;
pub(super) const CONTESTED_OFFSET_Y: f32 = 0.38;

pub(super) const GHOST_FAN_RADIUS: f32 = HEX_SIZE * 0.3;

/// A unit following a queue (Shift-click) shows its plan instead of a ghost,
/// while selected or hovered: a line along its moves, each turn's number on
/// the hex it moves to (and on each attack's arrow), all in pills rimmed in
/// team color (orange for attacks). A small tag beside the unit says a queue
/// is there, with the number of turns it has left.
pub(super) const QUEUE_LINE_WIDTH: f32 = 0.07;
pub(super) const QUEUE_LINE_ALPHA: f32 = 0.7;
pub(super) const QUEUE_DIGIT_HEIGHT: f32 = 0.2;
pub(super) const QUEUE_BADGE_HALF_HEIGHT: f32 = 0.18;
pub(super) const QUEUE_BADGE_PADDING: f32 = 0.09;
pub(super) const QUEUE_BADGE_RIM: f32 = 0.04;
pub(super) const QUEUE_TEXT_COLOR: Color = [0.95, 0.95, 0.95, 1.0];

/// Where a plan's last number sits from its ghost: below it, clear of the
/// pictogram.
pub(super) const QUEUE_END_BADGE_OFFSET: Vec2 = Vec2::new(0.0, -0.36);
/// The orange hex outline on a click waiting to be repeated to replace a queue.
pub(super) const QUEUE_REPLACE_OUTLINE_RADIUS: f32 = 0.9;
pub(super) const QUEUE_REPLACE_OUTLINE_WIDTH: f32 = 0.08;

/// A queued swap is drawn as a link between the two allies.
pub(super) const SWAP_LINK_WIDTH: f32 = 0.1;
pub(super) const SWAP_LINK_ALPHA: f32 = 0.8;

/// A queued attack is drawn as a curved arrow from the attacker (or its ghost,
/// if it's moving first) to the hex it's attacking.
pub(super) const ATTACK_ARC_COLOR: Color = [1.00, 0.45, 0.30, 1.0];
pub(super) const ATTACK_ARC_OUTLINE_COLOR: Color = [0.08, 0.03, 0.02, 1.0];
pub(super) const ATTACK_ARC_WIDTH: f32 = 0.07;
pub(super) const ATTACK_ARC_OUTLINE_WIDTH: f32 = 0.13;
/// How far the arc bows sideways, as a share of its length.
pub(super) const ATTACK_ARC_BOW: f32 = 0.25;
/// The arc starts at the edge of the attacker's icon and stops short of the
/// target's center, so a unit drawn there doesn't hide the arrowhead.
pub(super) const ATTACK_ARC_START: f32 = UNIT_ICON_RADIUS;
pub(super) const ATTACK_ARC_STOP: f32 = HEX_SIZE * 0.55;
pub(super) const ATTACK_ARC_SEGMENTS: usize = 20;
pub(super) const ATTACK_ARROW_LENGTH: f32 = 0.28;
pub(super) const ATTACK_ARROW_WIDTH: f32 = 0.26;

impl GameState {
    /// Ghosts at queued move destinations, links between allies queued to
    /// swap, and diamonds on attacked hexes. Markers for units sharing a target
    /// hex are fanned out so each order stays visible. Units whose queue
    /// reaches past this turn show their numbered plan instead, and only while
    /// selected or hovered; a queue of this turn alone draws like plain orders.
    pub(super) fn push_order_markers(&self, fog: &Fog, out: &mut Vec<Vertex>) {
        let swapping: HashSet<u32> = (0..self.units.len())
            .filter(|&i| self.swap_partner(i).is_some())
            .map(|i| self.units[i].id)
            .collect();

        // Where each moving unit's ghost is drawn, for its attack arc.
        let mut ghosts: HashMap<u32, Vec2> = HashMap::default();
        // Another side's move shows only in sight at both ends: its units
        // carry their moves while a turn plays out.
        let plain_moves = group_by_target(&self.units, |u| {
            u.planned_move.filter(|&dest| {
                !swapping.contains(&u.id)
                    && !u.plans_later_turns()
                    && fog.shows(u)
                    && (u.team == fog.team() || fog.sees(dest))
            })
        });
        for (hex, movers) in plain_moves {
            for (i, unit) in movers.iter().enumerate() {
                let pos = fan_position(hex.to_world(), i, movers.len(), GHOST_FAN_RADIUS);
                let color = with_alpha(unit.team.color(), GHOST_ALPHA);
                push_unit_icon(pos, self.unit_look(unit), 1.0, color, out);
                ghosts.insert(unit.id, pos);
            }
        }

        for (idx, unit) in self.units.iter().enumerate() {
            if let Some(partner) = self
                .swap_partner(idx)
                .filter(|&p| p > idx && fog.shows(unit) && fog.shows(&self.units[p]))
            {
                let color = with_alpha(unit.team.color(), SWAP_LINK_ALPHA);
                let (a, b) = (unit.pos.to_world(), self.units[partner].pos.to_world());
                mesh::segment(a, b, SWAP_LINK_WIDTH, color, out);
            }
        }

        // Only the player's own attacks: the AI's plans aren't theirs to see.
        for (idx, unit) in self.units.iter().enumerate() {
            let Some(target) = unit
                .planned_attack
                .filter(|_| self.is_player_controlled(idx) && !unit.plans_later_turns())
            else {
                continue;
            };
            // Swapping units attack from their partner's hex.
            let from = match ghosts.get(&unit.id) {
                Some(&ghost) => ghost,
                None if unit.planned_move.is_some() => unit.planned_pos().to_world(),
                None => self.unit_layout(idx).0,
            };
            push_attack_arc(from, target.to_world(), out);
        }

        let selection = self.selection();
        let hovered = self
            .hovered_tile
            .and_then(|hex| self.controlled_unit_at(hex));
        let plans: Vec<(&Unit, UnitLook)> = (0..self.units.len())
            .filter(|&i| selection.contains(&i) || hovered == Some(i))
            .filter(|&i| self.units[i].plans_later_turns() && self.is_player_controlled(i))
            .map(|i| (&self.units[i], self.unit_look(&self.units[i])))
            .collect();
        push_queue_plans(&plans, out);
        // A click waiting to be repeated to replace the selection's queue.
        if let Some(hex) = self.queue_replace_hex() {
            let rim = HEX_SIZE * QUEUE_REPLACE_OUTLINE_RADIUS;
            mesh::polygon_outline(
                hex.to_world(),
                rim,
                QUEUE_REPLACE_OUTLINE_WIDTH,
                6,
                0.0,
                ATTACK_ARC_COLOR,
                out,
            );
        }
    }

    /// Where to draw a unit and at what scale: full size in the middle of its
    /// hex, or half size and offset when it shares a contested hex.
    pub(in crate::game) fn unit_layout(&self, idx: usize) -> (Vec2, f32) {
        let unit = &self.units[idx];
        let center = unit.pos.to_world();
        if self.rival_of(idx).is_none() {
            return (center, 1.0);
        }
        // The team earlier in `Team::ALL` (the player, if either is) draws
        // on top.
        let rival = self.rival_of(idx).map(|r| self.units[r].team);
        let offset_y = if rival.is_none_or(|rival| unit.team < rival) {
            CONTESTED_OFFSET_Y
        } else {
            -CONTESTED_OFFSET_Y
        };
        (center + Vec2::new(0.0, offset_y), CONTESTED_SCALE)
    }

    /// Whether `point` (world space) is on unit `idx`'s token as
    /// `build_vertices` draws it: at `unit_layout`'s center and scale (larger
    /// while it's lit as having just acted), its disc or civilian hexagon
    /// out to the edge of its dark outline. The city view uses it to tell a
    /// click on a unit from a click on the tile under it.
    pub(in crate::game) fn unit_token_contains(&self, idx: usize, point: Vec2) -> bool {
        let unit = &self.units[idx];
        let (center, mut scale) = self.unit_layout(idx);
        if self.recent_actors.contains(&unit.id) {
            scale *= ACTED_SCALE;
        }
        let (sides, rotation, size) = token_shape(self.unit_look(unit));
        // As `mesh::polygon_outline` draws it: centered on the rim, its
        // corners pushed out further than its edges.
        let half_step = TAU / 2.0 / sides as f32;
        let outer =
            UNIT_ICON_RADIUS * scale * size + ICON_OUTLINE_WIDTH * scale / 2.0 / half_step.cos();
        // Inside a regular polygon: within its apothem along the normal of
        // the edge the point faces.
        let offset = point - center;
        let from_corner = (offset.to_angle() - rotation).rem_euclid(2.0 * half_step);
        offset.length() * (from_corner - half_step).cos() <= outer * half_step.cos()
    }
}

/// Groups units by the hex `target` picks out, in first-seen order so marker
/// layout stays stable from frame to frame.
pub(super) fn group_by_target(
    units: &[Unit],
    target: impl Fn(&Unit) -> Option<Hex>,
) -> Vec<(Hex, Vec<&Unit>)> {
    let mut groups: Vec<(Hex, Vec<&Unit>)> = Vec::new();
    for unit in units {
        let Some(hex) = target(unit) else { continue };
        match groups.iter_mut().find(|(h, _)| *h == hex) {
            Some((_, members)) => members.push(unit),
            None => groups.push((hex, vec![unit])),
        }
    }
    groups
}

/// Spreads `total` markers evenly around a small circle at `base`, or returns
/// `base` itself when there's only one.
pub(super) fn fan_position(base: Vec2, index: usize, total: usize, radius: f32) -> Vec2 {
    if total <= 1 {
        return base;
    }
    base + Vec2::from_angle(TAU * index as f32 / total as f32) * radius
}

/// A queued attack's arrow from `from` toward `to`.
pub(super) fn push_attack_arc(from: Vec2, to: Vec2, out: &mut Vec<Vertex>) {
    let points = attack_arc_points(from, to, 1.0);
    push_arrow(&points, ATTACK_ARC_COLOR, ATTACK_ARC_OUTLINE_COLOR, out);
}

/// What `push_queue_plans` draws, worked out first: the plan lines, the ghosts
/// where plans end, the attack arrows, and the numbered badges (their text is
/// the turn numbers, "2,4" for one unit using a hex on two turns).
pub(super) struct QueuePlans {
    lines: Vec<(Vec2, Vec2, Color)>,
    ghosts: Vec<(Vec2, UnitLook, Color)>,
    arrows: Vec<Vec<Vec2>>,
    badges: Vec<(Vec2, String, Color)>,
}

pub(super) fn queue_plans(units: &[(&Unit, UnitLook)]) -> QueuePlans {
    // Which units stop on each hex, in order, to fan them apart.
    let mut visitors: Vec<(Hex, Vec<usize>)> = Vec::new();
    for (u, (unit, _)) in units.iter().enumerate() {
        for turn in 0..unit.plan_len() {
            let to = unit.pos_after(turn + 1);
            if unit.pos_after(turn) != to {
                match visitors.iter_mut().find(|(hex, _)| *hex == to) {
                    Some((_, list)) if list.contains(&u) => {}
                    Some((_, list)) => list.push(u),
                    None => visitors.push((to, vec![u])),
                }
            }
        }
    }
    let spot = |hex: Hex, u: usize| match visitors.iter().find(|(h, _)| *h == hex) {
        Some((_, list)) => {
            let slot = list.iter().position(|&v| v == u).unwrap_or(0);
            fan_position(hex.to_world(), slot, list.len(), GHOST_FAN_RADIUS)
        }
        None => hex.to_world(),
    };

    let mut plans = QueuePlans {
        lines: Vec::new(),
        ghosts: Vec::new(),
        arrows: Vec::new(),
        badges: Vec::new(),
    };
    let label = |turns: Vec<usize>| {
        let mut turns = turns;
        turns.sort_unstable();
        turns.dedup();
        let text: Vec<String> = turns.iter().map(usize::to_string).collect();
        text.join(",")
    };
    for (u, &(unit, look)) in units.iter().enumerate() {
        let color = unit.team.color();
        let line = with_alpha(color, QUEUE_LINE_ALPHA);
        let stand = |hex: Hex| {
            if hex == unit.pos {
                unit.pos.to_world()
            } else {
                spot(hex, u)
            }
        };
        let mut at = unit.pos.to_world();
        let mut stops: Vec<(Hex, Vec<usize>)> = Vec::new();
        let mut strikes: Vec<((Hex, Hex), Vec<usize>)> = Vec::new();
        for turn in 0..unit.plan_len() {
            let (from, to) = (unit.pos_after(turn), unit.pos_after(turn + 1));
            if from != to {
                let next = stand(to);
                plans.lines.push((at, next, line));
                at = next;
                add_label(&mut stops, to, turn + 1);
            }
            if let Some(target) = unit.attack_on_turn(turn) {
                add_label(&mut strikes, (to, target), turn + 1);
            }
        }
        let end = unit.plan_end();
        if end != unit.pos {
            plans
                .ghosts
                .push((stand(end), look, with_alpha(color, GHOST_ALPHA)));
        }
        for ((from, target), turns) in strikes {
            let points = attack_arc_points(stand(from), target.to_world(), 1.0);
            let middle = points
                .get(points.len() / 2)
                .copied()
                .unwrap_or(target.to_world());
            plans.badges.push((middle, label(turns), ATTACK_ARC_COLOR));
            plans.arrows.push(points);
        }
        for (hex, turns) in stops {
            // The number at the end sits under the ghost, not on it.
            let offset = if hex == end {
                QUEUE_END_BADGE_OFFSET
            } else {
                Vec2::ZERO
            };
            plans
                .badges
                .push((stand(hex) + offset, label(turns), color));
        }
    }
    plans
}

/// The plans of units following a queue, each unit's on its own: a line from
/// the unit through each hex it moves to, numbered by turn (1 is this turn),
/// ending in a faded ghost of the unit where its plan leaves it; its attacks
/// are arrows from where it stands that turn, numbered at their middle. Where
/// several units' plans stop on one hex, each unit's stop is fanned out
/// around it (as ghosts are), so a number never belongs to two units. A hex
/// or arrow one unit uses on several turns lists them ("2,4").
pub(super) fn push_queue_plans(units: &[(&Unit, UnitLook)], out: &mut Vec<Vertex>) {
    let plans = queue_plans(units);
    for (from, to, color) in plans.lines {
        mesh::segment(from, to, QUEUE_LINE_WIDTH, color, out);
    }
    for (at, look, color) in plans.ghosts {
        push_unit_icon(at, look, 1.0, color, out);
    }
    for points in plans.arrows {
        push_arrow(&points, ATTACK_ARC_COLOR, ATTACK_ARC_OUTLINE_COLOR, out);
    }
    for (at, text, rim) in plans.badges {
        push_turn_badge(at, &text, 1.0, rim, out);
    }
}

/// Adds `turn` to the turns labeled at `key`, in first-seen order.
pub(super) fn add_label<K: PartialEq>(labels: &mut Vec<(K, Vec<usize>)>, key: K, turn: usize) {
    match labels.iter_mut().find(|(k, _)| *k == key) {
        Some((_, turns)) => turns.push(turn),
        None => labels.push((key, vec![turn])),
    }
}

/// A dark pill rimmed in `rim` with `text` on it, centered on `center`.
pub(super) fn push_turn_badge(
    center: Vec2,
    text: &str,
    scale: f32,
    rim: Color,
    out: &mut Vec<Vertex>,
) {
    let height = QUEUE_DIGIT_HEIGHT * scale;
    let radius = QUEUE_BADGE_HALF_HEIGHT * scale;
    // How far the round ends' centers sit from the middle: none for a
    // single digit, which makes the pill a disc.
    let reach = (font::world_text_width(text, height) / 2.0 + QUEUE_BADGE_PADDING * scale - radius)
        .max(0.0);
    push_pill(center, reach, radius + QUEUE_BADGE_RIM * scale, rim, out);
    push_pill(center, reach, radius, BADGE_BG_COLOR, out);
    font::push_text_centered(center, height, text, QUEUE_TEXT_COLOR, out);
}

/// Points along an attack's curve from `from` toward `to`: a quadratic curve
/// bowing to the left of the direction of travel (so opposing attacks
/// between two hexes don't overlap), from the edge of the attacker's icon to
/// just short of the target. Only the first `progress` (0 to 1) of it, for an
/// arrow still in flight. Empty if the two are too close for an arrow.
pub(in crate::game) fn attack_arc_points(from: Vec2, to: Vec2, progress: f32) -> Vec<Vec2> {
    let chord = to - from;
    let length = chord.length();
    if length <= ATTACK_ARC_START + ATTACK_ARC_STOP || progress <= 0.0 {
        return Vec::new();
    }
    let control = (from + to) / 2.0 + chord.perp() * ATTACK_ARC_BOW;
    let point = |t: f32| from.lerp(control, t).lerp(control.lerp(to, t), t);
    let start = ATTACK_ARC_START / length;
    let end = start + (1.0 - ATTACK_ARC_STOP / length - start) * progress.min(1.0);
    (0..=ATTACK_ARC_SEGMENTS)
        .map(|i| point(start + (end - start) * i as f32 / ATTACK_ARC_SEGMENTS as f32))
        .collect()
}

/// Draws an arrow along `points` over a darker outline, its head at the last
/// point.
pub(in crate::game) fn push_arrow(
    points: &[Vec2],
    color: Color,
    outline: Color,
    out: &mut Vec<Vertex>,
) {
    let &[.., before, tip] = points else {
        return;
    };
    let direction = (tip - before).normalize_or_zero();
    for (width, color) in [
        (ATTACK_ARC_OUTLINE_WIDTH, outline),
        (ATTACK_ARC_WIDTH, color),
    ] {
        mesh::polyline(points, width, color, out);
        // The outline pass draws its arrowhead a little bigger all round.
        let grow = (width - ATTACK_ARC_WIDTH) / 2.0;
        let base = tip - direction * grow;
        let side = direction.perp() * (ATTACK_ARROW_WIDTH / 2.0 + grow);
        let point = tip + direction * (ATTACK_ARROW_LENGTH + grow);
        mesh::triangle(point, base + side, base - side, color, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::draw::tests::{count_color, scene};
    use crate::game::fog::tests::{behind_the_mountain, glance_at};
    use crate::game::unit::{Unit, UnitType};

    #[test]
    fn a_queue_shows_numbered_turns_only_while_selected_or_hovered() {
        let mut game = GameState::new();
        game.fog_of_war = false;
        let melee = game
            .units
            .iter()
            .position(|u| u.team == Team::Blue && u.unit_type == crate::game::unit::UnitType::Melee)
            .unwrap();
        game.selected = Some(melee);
        game.hovered_tile = None;
        let start = game.units[melee].pos;
        let first = start.neighbors()[0];
        let second = first.neighbors()[0];
        assert!(game.queue_move(first) && game.queue_move(second));
        let line = with_alpha(Team::Blue.color(), QUEUE_LINE_ALPHA);
        let ghost = with_alpha(Team::Blue.color(), GHOST_ALPHA);

        let selected = game.build_vertices();
        assert!(count_color(&selected, line) > 0, "the path shows");
        assert!(
            count_color(&selected, ghost) > 0,
            "a ghost where the plan ends"
        );

        game.selected = None;
        let deselected = game.build_vertices();
        assert_eq!(count_color(&deselected, line), 0, "hidden once let go of");
        assert_eq!(count_color(&deselected, ghost), 0);

        game.hovered_tile = Some(start);
        let hovered = game.build_vertices();
        assert_eq!(count_color(&hovered, line), count_color(&selected, line));
    }

    #[test]
    fn two_units_passing_one_hex_get_their_own_numbers() {
        use crate::game::unit::TurnOrder;
        let (shared, a_first, b_last) = (Hex::new(0, 0), Hex::new(-1, 0), Hex::new(1, 0));
        // A reaches the shared hex on turn 2, B on turn 1 and moves on.
        let mut a = Unit::new(1, Hex::new(-2, 0), Team::Blue, UnitType::Melee);
        a.planned_move = Some(a_first);
        a.following_queue = true;
        a.queued.push(TurnOrder {
            from: a_first,
            move_to: Some(shared),
            attack: None,
        });
        let mut b = Unit::new(2, Hex::new(0, 2), Team::Blue, UnitType::Cavalry);
        b.planned_move = Some(shared);
        b.following_queue = true;
        b.queued.push(TurnOrder {
            from: shared,
            move_to: Some(b_last),
            attack: None,
        });
        let look = |unit: &Unit| UnitLook {
            icon: UnitIcon::of(unit.unit_type),
            civilian: false,
        };
        let plans = queue_plans(&[(&a, look(&a)), (&b, look(&b))]);

        let texts: Vec<&str> = plans.badges.iter().map(|b| b.1.as_str()).collect();
        assert!(!texts.contains(&"1,2"), "no number shared by two units");
        let near_shared: Vec<(Vec2, &str)> = plans
            .badges
            .iter()
            .filter(|b| b.0.distance(shared.to_world()) < HEX_SIZE * 0.8)
            .map(|b| (b.0, b.1.as_str()))
            .collect();
        assert_eq!(near_shared.len(), 2, "{near_shared:?}");
        assert_ne!(near_shared[0].0, near_shared[1].0, "fanned apart");
        // Each plan ends in its own ghost.
        assert_eq!(plans.ghosts.len(), 2);
        assert_ne!(plans.ghosts[0].0, plans.ghosts[1].0);
    }

    #[test]
    fn a_queue_of_this_turn_alone_draws_a_plain_ghost_and_arrow() {
        let mut game = GameState::new();
        game.fog_of_war = false;
        let melee = game
            .units
            .iter()
            .position(|u| u.team == Team::Blue && u.unit_type == crate::game::unit::UnitType::Melee)
            .unwrap();
        game.selected = Some(melee);
        game.hovered_tile = None;
        let first = game.units[melee].pos.neighbors()[0];
        let target = first.neighbors()[0];
        let line = with_alpha(Team::Blue.color(), QUEUE_LINE_ALPHA);
        let ghost = with_alpha(Team::Blue.color(), GHOST_ALPHA);
        let arrows = |game: &GameState| count_color(&game.build_vertices(), ATTACK_ARC_COLOR);
        let before = arrows(&game);

        // Shift-clicks that only fill this turn: a move, then an attack from there.
        assert!(game.queue_move(first) && game.queue_attack(target));
        assert!(
            game.units[melee].queued.is_empty(),
            "nothing past this turn"
        );
        let vertices = game.build_vertices();
        assert!(
            count_color(&vertices, ghost) > 0,
            "a ghost, not a numbered 1"
        );
        assert_eq!(count_color(&vertices, line), 0, "no plan line");
        assert!(arrows(&game) > before, "the attack's plain arrow");

        // A second turn switches to the numbered plan.
        let next = first
            .neighbors()
            .into_iter()
            .find(|&h| game.grid.is_passable(h) && game.units.iter().all(|u| u.pos != h))
            .expect("an open hex next to the first move");
        assert!(game.queue_move(next));
        let vertices = game.build_vertices();
        assert!(count_color(&vertices, line) > 0);
        assert!(
            count_color(&vertices, ghost) > 0,
            "the ghost moves to the plan's end"
        );
    }

    #[test]
    fn turn_order_numbers_show_only_while_alt_is_held() {
        let mut game = GameState::world_scenario(3);
        game.explore();
        assert_eq!(count_color(&game.build_vertices(), MOVE_ORDER_COLOR), 0);
        game.set_details(true);
        assert!(count_color(&game.build_vertices(), MOVE_ORDER_COLOR) > 0);
        game.set_details(false);
        assert_eq!(count_color(&game.build_vertices(), MOVE_ORDER_COLOR), 0);
    }

    #[test]
    fn another_sides_moves_show_only_in_sight_at_both_ends() {
        // A Red unit in sight next to the hex behind the mountain, and two
        // Red units out of sight: what they plan out of sight draws nothing.
        let setup = || {
            let (mut game, idx, hidden) = behind_the_mountain();
            glance_at(&mut game, idx, hidden);
            let fog = game.fog();
            let near = hidden
                .neighbors()
                .into_iter()
                .filter(|&h| fog.sees(h) && game.grid.terrain(h).is_passable())
                .min_by_key(|h| (h.q, h.r))
                .expect("a hex in sight next to the hidden one");
            let unseen = hidden
                .neighbors()
                .into_iter()
                .filter(|&h| !fog.sees(h) && game.grid.terrain(h).is_passable())
                .min_by_key(|h| (h.q, h.r))
                .expect("a hex out of sight next to the hidden one");
            for (id, pos) in [(2, near), (3, hidden), (4, unseen)] {
                game.units
                    .push(Unit::new(id, pos, Team::Red, UnitType::Melee));
            }
            (game, near, hidden, unseen)
        };
        let (quiet, ..) = setup();
        let quiet = scene(&quiet);

        // Into the hidden hex, and two unseen units swapping places.
        let (mut moving, _, hidden, unseen) = setup();
        moving.units[1].planned_move = Some(hidden);
        moving.units[2].planned_move = Some(unseen);
        moving.units[3].planned_move = Some(hidden);
        assert!(moving.swap_partner(2).is_some());
        assert!(scene(&moving) == quiet);

        // A move in sight at both ends still shows its ghost.
        let (mut moving, near, _, _) = setup();
        moving.units[1].planned_move = Some(Hex::new(0, 1));
        assert!(moving.fog().sees(Hex::new(0, 1)) && moving.fog().sees(near));
        assert!(scene(&moving) != quiet);
    }
}
