//! Markers on the map: city towers and their production tags, worker
//! counts, walls and gates, outposts and forts, Barracks.

use super::*;

pub(super) const WORKER_TAG_MIN: Vec2 = Vec2::new(-0.78, -0.58);
pub(super) const WORKER_TAG_MAX: Vec2 = Vec2::new(-0.3, -0.32);
pub(super) const WORKER_TAG_COLOR: Color = [0.03, 0.03, 0.04, 0.92];
/// A production tag (`push_production_tag`): over a city's tower from this
/// height up, and over a Barracks from above its health bar; how tall it
/// is, and its text's capital height.
pub(super) const CITY_TAG_BOTTOM: f32 = 0.44;
pub(super) const BARRACKS_TAG_BOTTOM: f32 = 0.62;
pub(super) const PRODUCTION_TAG_HEIGHT: f32 = 0.3;
pub(super) const PRODUCTION_TAG_TEXT: f32 = 0.15;
/// A tag for an item that waits, rimmed red, its shortfall in a light red;
/// one for an item worked (Alt), rimmed in gold, its turns in white.
pub(super) const WAITING_TAG_RIM: Color = [0.60, 0.25, 0.21, 1.0];
pub(super) const WAITING_TAG_TEXT: Color = [0.86, 0.46, 0.40, 1.0];
pub(super) const WORKING_TAG_RIM: Color = [0.62, 0.52, 0.26, 1.0];
pub(super) const WORKING_TAG_TEXT: Color = [0.95, 0.95, 0.95, 1.0];
/// Structures workers build.
pub(super) const STONE_COLOR: Color = [0.24, 0.23, 0.21, 1.0];

pub(super) const MORTAR_COLOR: Color = [0.09, 0.085, 0.08, 1.0];
pub(super) const WOOD_COLOR: Color = [0.40, 0.20, 0.07, 1.0];

impl GameState {
    /// A tag over each of the player's cities and Barracks: while the first
    /// item of its queue waits (`head_wait`, the panels' account too), that
    /// item and what it waits for, with how much the stockpile is short
    /// (#342), always; with Alt held, otherwise, the item it works and its
    /// turns left (#339). Nothing for other sides': their queues hang on a
    /// stockpile the player can't see, and fog.
    pub(super) fn push_production_tags(&self, out: &mut Vec<Vertex>) {
        let at = |i: usize, lane: Lane| {
            let city = &self.cities[i];
            let (hex, bottom) = match lane {
                Lane::City => (city.pos, CITY_TAG_BOTTOM),
                Lane::Barracks => (city.barracks?, BARRACKS_TAG_BOTTOM),
            };
            (self.lane_len(i, lane) > 0 && self.may_show(hex))
                .then(|| hex.to_world() + Vec2::new(0.0, bottom))
        };
        let shown: Vec<(usize, Lane, Vec2)> = (0..self.cities.len())
            .filter(|&i| self.cities[i].team == self.local_team)
            .flat_map(|i| [Lane::City, Lane::Barracks].map(|lane| (i, lane)))
            .filter_map(|(i, lane)| at(i, lane).map(|at| (i, lane, at)))
            .collect();
        if shown.is_empty() {
            return;
        }
        let forecast = self.forecast(self.local_team);
        for (i, lane, at) in shown {
            let Some(queue) = forecast.lane(i, lane) else {
                continue;
            };
            if let Some(wait) = self.head_wait(queue) {
                let text = map_wait_text(wait.waits_for);
                push_production_tag(
                    at,
                    wait.build,
                    &text,
                    WAITING_TAG_RIM,
                    WAITING_TAG_TEXT,
                    out,
                );
            } else if self.show_details
                && let Some(index) = queue.worked
            {
                let build = self.lane_build(i, lane, index);
                let turns = turns_icon(self.item_turns_left(i, lane, index));
                push_production_tag(at, build, &turns, WORKING_TAG_RIM, WORKING_TAG_TEXT, out);
            }
        }
    }
}

/// Axis-aligned rectangles in `color` with a dark border, the border drawn
/// first under all of them so shapes built from several rectangles get one
/// clean outline.
pub(super) fn push_outlined_rects(
    rects: &[(Vec2, Vec2)],
    scale: f32,
    color: Color,
    out: &mut Vec<Vertex>,
) {
    let outline = with_alpha(ICON_OUTLINE_COLOR, color[3]);
    let grow = Vec2::splat(ICON_OUTLINE_WIDTH * scale);
    for &(min, max) in rects {
        mesh::quad(min - grow, max + grow, outline, out);
    }
    for &(min, max) in rects {
        mesh::quad(min, max, color, out);
    }
}

/// A city: a crenellated tower in its team's color with its population on
/// it.
pub(super) fn push_city_marker(pos: Vec2, city: &SeenBuilding, out: &mut Vec<Vertex>) {
    push_city_tower(pos, 1.0, city.team.color(), out);
    // Centered in the tower's body, below the merlons.
    font::push_text_centered(
        pos + Vec2::new(0.0, -0.08),
        0.3,
        &city.population.to_string(),
        LABEL_COLOR,
        out,
    );
}

/// A city's crenellated tower in `color`, `scale` times its size on the map
/// (0.84 wide, 0.8 tall), in whatever space `pos` is in: the turn strip
/// draws it too.
pub(in crate::game) fn push_city_tower(pos: Vec2, scale: f32, color: Color, out: &mut Vec<Vertex>) {
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| {
        (
            pos + Vec2::new(x0, y0) * scale,
            pos + Vec2::new(x1, y1) * scale,
        )
    };
    let rects = [
        rect(-0.42, -0.4, 0.42, 0.24),
        // Three merlons along the top.
        rect(-0.42, 0.24, -0.24, 0.4),
        rect(-0.09, 0.24, 0.09, 0.4),
        rect(0.24, 0.24, 0.42, 0.4),
    ];
    push_outlined_rects(&rects, scale, color, out);
}

/// How many workers a city has at home: a dark tag at the tower's lower
/// left with a shovel and the count.
pub(super) fn push_worker_count(city: Vec2, count: u32, out: &mut Vec<Vertex>) {
    let (min, max) = (city + WORKER_TAG_MIN, city + WORKER_TAG_MAX);
    let edge = Vec2::splat(ICON_OUTLINE_WIDTH / 2.0);
    mesh::quad(min - edge, max + edge, ICON_OUTLINE_COLOR, out);
    mesh::quad(min, max, WORKER_TAG_COLOR, out);
    let middle = (min.y + max.y) / 2.0;
    let height = max.y - min.y;
    unit_icons::push_pictogram(
        Vec2::new(min.x + height / 2.0, middle),
        height * 0.55,
        UnitIcon::Shovel,
        PLANNED_JOB_COLOR_SOLID,
        out,
    );
    font::push_text_centered(
        Vec2::new(max.x - height / 2.0, middle),
        height * 0.55,
        &count.to_string(),
        PLANNED_JOB_COLOR_SOLID,
        out,
    );
}

/// What an item that waits lacks, for its map tag: "-[wood]8 -[metal]2",
/// the icon and amount of each resource the stockpile is short of; SUPPLY;
/// or the population a Settler needs.
pub(super) fn map_wait_text(waits_for: WaitsFor) -> String {
    match waits_for {
        WaitsFor::Stock(short) => stock_icons(short)
            .split(' ')
            .map(|part| format!("-{part}"))
            .collect::<Vec<_>>()
            .join(" "),
        WaitsFor::Supply => "SUPPLY".into(),
        WaitsFor::Citizens => format!("POP {SETTLER_MIN_POPULATION}"),
    }
}

/// A dark tag, its bottom centered on `bottom`, rimmed in `rim`: `build`'s
/// pictogram (or, for a Grow or Gather, its name), then `text` in
/// `text_color`: what it waits for, or its turns left. A few quads and
/// glyphs, and only over the player's own buildings.
pub(super) fn push_production_tag(
    bottom: Vec2,
    build: Build,
    text: &str,
    rim: Color,
    text_color: Color,
    out: &mut Vec<Vertex>,
) {
    let (height, cap) = (PRODUCTION_TAG_HEIGHT, PRODUCTION_TAG_TEXT);
    let pad = height * 0.2;
    let icon = UnitIcon::of_build(build);
    let mark = match icon {
        Some(_) => height * 0.8,
        None => font::world_text_width(build.name(), cap),
    };
    let width = pad + mark + pad + font::world_text_width(text, cap) + pad;
    let min = bottom - Vec2::new(width / 2.0, 0.0);
    let max = min + Vec2::new(width, height);
    let edge = Vec2::splat(ICON_OUTLINE_WIDTH / 2.0);
    mesh::quad(min - edge, max + edge, rim, out);
    mesh::quad(min, max, WORKER_TAG_COLOR, out);
    let middle = (min.y + max.y) / 2.0;
    let baseline = middle - cap / 2.0;
    match icon {
        Some(icon) => unit_icons::push_pictogram(
            Vec2::new(min.x + pad + mark / 2.0, middle),
            mark / 2.0,
            icon,
            WORKING_TAG_TEXT,
            out,
        ),
        None => font::push_text(
            Vec2::new(min.x + pad, baseline),
            cap,
            build.name(),
            WORKING_TAG_TEXT,
            out,
        ),
    }
    let text_left = min.x + pad + mark + pad;
    font::push_text(Vec2::new(text_left, baseline), cap, text, text_color, out);
}

/// A wall or gate along the edge between `a` and `b`: a band of stone with
/// mortar joints and posts in its team's color at both ends; a gate's middle
/// is a door in the team's color.
pub(super) fn push_barrier(
    a: Hex,
    b: Hex,
    kind: StructureKind,
    team: Color,
    out: &mut Vec<Vertex>,
) {
    let (start, end) = edge_corners(a, b);
    let along = |t: f32| start.lerp(end, t);
    mesh::segment(
        start,
        end,
        BARRIER_WIDTH + ICON_OUTLINE_WIDTH,
        ICON_OUTLINE_COLOR,
        out,
    );
    mesh::segment(start, end, BARRIER_WIDTH, STONE_COLOR, out);
    let across = (end - start).perp().normalize_or_zero() * (BARRIER_WIDTH / 2.0);
    if kind == StructureKind::Gate {
        let door = [along(0.3), along(0.7)];
        mesh::segment(
            door[0],
            door[1],
            BARRIER_WIDTH + ICON_OUTLINE_WIDTH,
            ICON_OUTLINE_COLOR,
            out,
        );
        mesh::segment(door[0], door[1], BARRIER_WIDTH, team, out);
        mesh::segment(
            along(0.5) - across,
            along(0.5) + across,
            0.02,
            ICON_OUTLINE_COLOR,
            out,
        );
        for t in [0.15, 0.85] {
            mesh::segment(
                along(t) - across,
                along(t) + across,
                0.02,
                MORTAR_COLOR,
                out,
            );
        }
    } else {
        for t in [0.25, 0.5, 0.75] {
            mesh::segment(
                along(t) - across,
                along(t) + across,
                0.02,
                MORTAR_COLOR,
                out,
            );
        }
    }
    for p in [start, end] {
        let half = Vec2::splat(BARRIER_WIDTH * 0.62);
        let edge = Vec2::splat(ICON_OUTLINE_WIDTH / 2.0);
        mesh::quad(p - half - edge, p + half + edge, ICON_OUTLINE_COLOR, out);
        mesh::quad(p - half, p + half, team, out);
    }
}

/// A structure on a tile, in its team's color: an outpost (a watchtower) or
/// a fort (a palisade of stakes).
pub(super) fn push_structure(
    center: Vec2,
    kind: StructureKind,
    team: Color,
    out: &mut Vec<Vertex>,
) {
    let at = |x: f32, y: f32| center + Vec2::new(x, y);
    match kind {
        // Walls and gates stand on hex edges (`push_barrier`).
        StructureKind::Wall | StructureKind::Gate => {}
        StructureKind::Outpost => {
            for (foot, top) in [
                ((-0.16, -0.36), (-0.09, 0.05)),
                ((0.16, -0.36), (0.09, 0.05)),
            ] {
                let (foot, top) = (at(foot.0, foot.1), at(top.0, top.1));
                mesh::segment(foot, top, 0.09, ICON_OUTLINE_COLOR, out);
                mesh::segment(foot, top, 0.05, WOOD_COLOR, out);
            }
            push_outlined_rects(&[(at(-0.15, 0.03), at(0.15, 0.24))], 1.0, WOOD_COLOR, out);
            let roof = [at(-0.22, 0.24), at(0.22, 0.24), at(0.0, 0.44)];
            mesh::polygon(
                &[at(-0.27, 0.21), at(0.27, 0.21), at(0.0, 0.48)],
                ICON_OUTLINE_COLOR,
                out,
            );
            mesh::polygon(&roof, team, out);
        }
        StructureKind::Fort => {
            // Stakes around the hex, points outward, over a ring in the
            // team's color.
            mesh::polygon_outline(center, 0.56, 0.05, 6, 0.0, team, out);
            for i in 0..12 {
                let out_dir = Vec2::from_angle(i as f32 * std::f32::consts::TAU / 12.0);
                let side = out_dir.perp() * 0.06;
                let base = center + out_dir * 0.56;
                let tip = center + out_dir * 0.74;
                mesh::polygon(
                    &[
                        base - side * 1.6 - out_dir * 0.03,
                        base + side * 1.6 - out_dir * 0.03,
                        tip + out_dir * 0.03,
                    ],
                    ICON_OUTLINE_COLOR,
                    out,
                );
                mesh::polygon(&[base - side, base + side, tip], WOOD_COLOR, out);
            }
        }
    }
}

/// A barracks: a small house (walls and a pitched roof) in `color`, its
/// team's, marked B.
pub(super) fn push_barracks_marker(pos: Vec2, color: Color, out: &mut Vec<Vertex>) {
    let alpha = color[3];
    push_barracks_house(pos, 1.0, color, out);
    font::push_glyph(
        pos + Vec2::new(0.0, -0.09),
        0.26,
        'B',
        with_alpha(LABEL_COLOR, alpha),
        out,
    );
}

/// A barracks' house, unmarked: walls and a pitched roof in `color`, `scale`
/// times its size on the map (0.76 wide, 0.72 tall), in whatever space
/// `pos` is in: the turn strip draws it too.
pub(in crate::game) fn push_barracks_house(
    pos: Vec2,
    scale: f32,
    color: Color,
    out: &mut Vec<Vertex>,
) {
    let outline = with_alpha(ICON_OUTLINE_COLOR, color[3]);
    let at = |x: f32, y: f32| pos + Vec2::new(x, y) * scale;
    let (eave, peak, half) = (0.1, 0.4, 0.38);
    let roof = |grow: f32| {
        (
            at(-half - grow, eave - grow / 2.0),
            at(half + grow, eave - grow / 2.0),
            at(0.0, peak + grow),
        )
    };
    let (a, b, c) = roof(ICON_OUTLINE_WIDTH * 1.6);
    mesh::triangle(a, b, c, outline, out);
    let walls = (at(-0.28, -0.32), at(0.28, eave));
    push_outlined_rects(&[walls], scale, color, out);
    let (a, b, c) = roof(0.0);
    mesh::triangle(a, b, c, color, out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::draw::tests::count_color;

    #[test]
    fn breached_post_gains_a_distinct_world_marker() {
        let mut game = GameState::siege_scenario();
        let intact = count_color(&game.build_vertices(), SELECTED_COLOR);
        game.cities[1].interior.core_hp = 0.0;
        let breached = count_color(&game.build_vertices(), SELECTED_COLOR);
        assert!(breached > intact, "breached post needs a visible gold ring");
    }

    #[test]
    fn queued_walls_are_drawn_with_round_ends() {
        let mut game = GameState::city_scenario();
        let city = game.cities[0].pos;
        let (a, b) = (city.neighbors()[0], city.neighbors()[1]);
        game.cities[0]
            .worker_jobs
            .push(crate::game::workers::WorkerJob {
                hex: a,
                kind: crate::game::workers::JobKind::Wall,
                across: Some(b),
                done: 0,
            });
        let vertices = game.build_vertices();
        // A segment is two triangles; each round end is twelve more.
        assert_eq!(count_color(&vertices, PLANNED_EDGE_COLOR), 6 + 2 * 12 * 3);
    }

    #[test]
    fn a_city_waiting_for_the_stockpile_shows_what_it_lacks_over_its_tower() {
        use crate::game::city::{Build, Queued, Stock, WaitsFor};
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.selected = None;
        let rim = |game: &GameState| count_color(&game.build_vertices(), WAITING_TAG_RIM);
        assert_eq!(rim(&game), 0, "nothing queued");
        // Broke: a Grow at the head of the queue waits for food.
        game.stockpiles[Team::Blue.index()] = Stock::default();
        game.cities[0].queue = vec![Queued::new(Build::Grow)];
        let forecast = game.forecast(Team::Blue);
        let wait = game.head_waits(&forecast)[0];
        let WaitsFor::Stock(short) = wait.waits_for else {
            panic!("{wait:?}")
        };
        assert!(short.food > 0, "{short:?}");
        assert!(rim(&game) > 0, "the tag shows");
        // Its text is the shortfall, each resource's icon and amount.
        let text = map_wait_text(wait.waits_for);
        assert!(
            text.starts_with(&format!("-{}", crate::game::map_icons::FOOD_ICON)),
            "{text}"
        );
        let one = rim(&game);
        // A Gather behind it doesn't stop it waiting: the head still does.
        game.cities[0].queue.push(Queued::new(Build::Gather));
        assert_eq!(rim(&game), one);
        // Paid for, the head no longer waits.
        game.cities[0].queue[0].paid = true;
        assert_eq!(rim(&game), 0);
        // Another side's waiting city shows nothing: its stockpile isn't
        // the player's to see.
        game.stockpiles[Team::Red.index()] = Stock::default();
        game.cities[1].queue = vec![Queued::new(Build::Worker)];
        game.explore();
        assert_eq!(rim(&game), 0);
    }

    /// #342: a Barracks whose first item waits shows the tag over its own
    /// hex, with the item's pictogram, without Alt; #339: with Alt, what
    /// the city and the Barracks work, with their turns left, and still
    /// nothing for another side.
    #[test]
    fn the_players_buildings_show_what_waits_always_and_what_they_work_with_alt() {
        use crate::game::city::{Build, BuildUnit, Queued, Stock};
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.selected = None;
        game.fog_of_war = false;
        let barracks = Hex::new(-2, 0);
        game.cities[0].barracks = Some(barracks);
        game.cities[0].built.push(Building::Barracks);
        let tags = |game: &GameState, rim: Color| {
            let vertices = game.build_vertices();
            // A tag's rim is one quad: six vertices.
            count_color(&vertices, rim) / 6
        };
        // Broke: a Cavalry waits in the Barracks; the city works a Gather.
        game.stockpiles[Team::Blue.index()] = Stock::default();
        game.cities[0].barracks_queue = vec![Queued::new(BuildUnit::Cavalry)];
        game.cities[0].queue = vec![Queued::new(Build::Gather)];
        assert_eq!(tags(&game, WAITING_TAG_RIM), 1, "the Barracks' tag");
        assert_eq!(tags(&game, WORKING_TAG_RIM), 0, "only with Alt");
        // The tag sits over the Barracks, not the city: its rim's quad.
        let over = |game: &GameState, rim: Color, hex: Hex| {
            let bottom = hex.to_world().y + BARRACKS_TAG_BOTTOM;
            game.build_vertices().iter().any(|v| {
                v.color == rim
                    && (v.pos[0] - hex.to_world().x).abs() < 1.0
                    && (v.pos[1] - bottom).abs() < PRODUCTION_TAG_HEIGHT + 0.05
            })
        };
        assert!(over(&game, WAITING_TAG_RIM, barracks));
        // Its pictogram is drawn: plain triangles (no glyph's texture) in
        // the tag's light; a Gather's tag names it in glyphs instead.
        let mut with = Vec::new();
        push_production_tag(
            Vec2::ZERO,
            Build::Unit(BuildUnit::Cavalry),
            "-",
            WAITING_TAG_RIM,
            WAITING_TAG_TEXT,
            &mut with,
        );
        let mut without = Vec::new();
        push_production_tag(
            Vec2::ZERO,
            Build::Gather,
            "-",
            WAITING_TAG_RIM,
            WAITING_TAG_TEXT,
            &mut without,
        );
        let pictogram = |vertices: &[Vertex]| {
            vertices
                .iter()
                .filter(|v| v.color == WORKING_TAG_TEXT && v.uv == crate::renderer::SOLID_UV)
                .count()
        };
        assert!(pictogram(&with) > 0);
        assert_eq!(pictogram(&without), 0);
        assert!(with.len() < 400, "{} vertices for a tag", with.len());

        // Alt: the city's Gather gets a tag with its turns left; the
        // Barracks still shows what it waits for.
        game.set_details(true);
        assert_eq!(tags(&game, WORKING_TAG_RIM), 1);
        assert_eq!(tags(&game, WAITING_TAG_RIM), 1);
        // Paid for, the Cavalry is worked: a working tag over each.
        game.cities[0].barracks_queue[0].paid = true;
        assert_eq!(tags(&game, WORKING_TAG_RIM), 2);
        assert_eq!(tags(&game, WAITING_TAG_RIM), 0);
        assert!(over(&game, WORKING_TAG_RIM, barracks));
        // Released, only waits show.
        game.set_details(false);
        assert_eq!(tags(&game, WORKING_TAG_RIM), 0);

        // Another side's buildings show nothing, Alt or not, in sight or not.
        game.cities[0].queue.clear();
        game.cities[0].barracks_queue.clear();
        game.set_details(true);
        game.cities[1].queue = vec![Queued::prepaid(Build::Worker)];
        game.stockpiles[Team::Red.index()] = Stock::default();
        game.cities[1].barracks = Some(game.cities[1].pos);
        game.cities[1].barracks_queue = vec![Queued::new(BuildUnit::Melee)];
        assert_eq!(tags(&game, WORKING_TAG_RIM), 0);
        assert_eq!(tags(&game, WAITING_TAG_RIM), 0);
    }

    #[test]
    fn shared_corner_walls_have_identical_unsorted_vertices_on_every_frame() {
        let mut game = GameState::city_scenario();
        game.fog_of_war = false;
        let a = Hex::new(0, 0);
        let b = Hex::new(1, 0);
        let c = Hex::new(0, 1);
        for (neighbor, team) in [(b, Team::Blue), (c, Team::Red)] {
            game.barriers.insert(
                edge(a, neighbor),
                Structure {
                    kind: StructureKind::Wall,
                    team,
                },
            );
        }
        let vertices = |game: &GameState| {
            game.build_vertices()
                .into_iter()
                .map(|v| (v.pos, v.color, v.uv))
                .collect::<Vec<_>>()
        };
        let expected = vertices(&game);
        for _ in 0..20 {
            assert_eq!(vertices(&game), expected);
        }
        // Rebuild with a different backing capacity and reversed insertion order.
        let entries: Vec<_> = game.barriers.drain().collect();
        game.barriers.reserve(100);
        game.barriers.extend(entries.into_iter().rev());
        assert_eq!(vertices(&game), expected);
    }
}
