//! Yield chips: food, wood and metal on a tile, as pips or numbers
//! (`YieldRow`: their layout, as data).

use super::*;

/// Tile yields: below the unit spot, food, wood then metal as die-face pips
/// on a dark see-through pill.
pub(super) const YIELD_ROW_OFFSET: Vec2 = Vec2::new(0.0, -0.56);
/// Distance between neighboring pips, across and up.
pub(super) const YIELD_PIP_PITCH: Vec2 = Vec2::new(0.11, 0.15);
pub(super) const YIELD_ICON_SCALE: f32 = 0.8;
/// How far a pip icon, at that scale, reaches from its center.
pub(super) const YIELD_ICON_HALF: Vec2 = Vec2::new(0.05, 0.076);
/// Space between neighboring groups (food, wood, metal).
pub(super) const YIELD_GROUP_GAP: f32 = 0.08;
pub(super) const YIELD_ROW_PADDING: Vec2 = Vec2::new(0.06, 0.035);
/// The widest a yield pill may be, from its center, and stay inside the hex.
pub(super) const YIELD_ROW_MAX_HALF_WIDTH: f32 = 0.46;
pub(super) const YIELD_PIP_CORNER: f32 = 0.1;
pub(super) const YIELD_ROW_COLOR: Color = [0.005, 0.006, 0.007, 0.85];
/// Past six, a yield is one icon and its amount.
pub(super) const YIELD_DIGIT_HEIGHT: f32 = 0.12;
pub(super) const YIELD_DIGIT_COLOR: Color = [0.95, 0.95, 0.95, 1.0];
pub(super) const YIELD_NUMBER_GAP: f32 = 0.03;
/// Zoomed out past this (the camera's half height, in world units), yield
/// chips are drawn plain: flat pip silhouettes (`push_map_icon_silhouette`)
/// on a square-cornered pill. A pip is then about 5 pixels tall in a
/// 1080-pixel window, too small for the icons' detail, and on the biggest
/// map with Alt held the full icons took millions of vertices (#332).
pub(super) const YIELD_DETAIL_MAX_HALF_HEIGHT: f32 = 12.0;

impl GameState {
    /// Alt: holding it shows extra map info (units' turn order, every tile's
    /// yields); releasing it hides it again.
    pub fn set_details(&mut self, held: bool) {
        self.show_details = held;
    }

    /// Yield chips below the unit spot: while Alt is held, on every explored
    /// tile that can be worked; otherwise, while the open city shows its
    /// yields, on the tiles within its economic reach.
    pub(super) fn push_tile_yields(&self, fog: &Fog, out: &mut Vec<Vertex>) {
        let city = self.yields_city();
        if city.is_none() && !self.show_details {
            return;
        }
        let reach = city.map(|city| (city, self.known_routes(city, fog)));
        let detailed = self.camera.half_height <= YIELD_DETAIL_MAX_HALF_HEIGHT;
        // The cheap tests first: most hexes fail one.
        for hex in self.grid.all_hexes().filter(|&h| {
            self.may_show(h)
                && self.grid.terrain(h).is_workable()
                && (self.show_details
                    || reach.as_ref().is_some_and(|(city, routes)| {
                        routes.costs.contains_key(&h) || self.cities[*city].works(h)
                    }))
                && self.is_explored(h)
                && !self.known_building_at_off_center(h, fog)
        }) {
            let goods = self.known_yield(hex, fog);
            push_yield_row(hex.to_world() + YIELD_ROW_OFFSET, goods, detailed, out);
        }
    }
}

/// A tile's yields on a dark pill: food, wood then metal (`raw_yield`),
/// each laid out like the pips on a die, or as one icon and a number past
/// six. Nothing for a tile yielding nothing.
pub(super) fn push_yield_row(
    center: Vec2,
    goods: (i32, i32, i32),
    detailed: bool,
    out: &mut Vec<Vertex>,
) {
    // Cache geometry by yield value, never by tile, so yield changes are immediate.
    type YieldMeshes = HashMap<((i32, i32, i32), bool), Vec<Vertex>>;
    thread_local! {
        static ROWS: std::cell::RefCell<YieldMeshes> = Default::default();
    }
    let key = (goods, detailed);
    ROWS.with_borrow_mut(|rows| {
        if rows.len() >= 256 && !rows.contains_key(&key) {
            rows.clear();
        }
        let shape = rows.entry(key).or_insert_with(|| {
            let mut shape = Vec::new();
            build_yield_row(Vec2::ZERO, goods, detailed, &mut shape);
            shape
        });
        mesh::place(shape, center, 1.0, None, out);
    });
}

pub(super) fn build_yield_row(
    center: Vec2,
    goods: (i32, i32, i32),
    detailed: bool,
    out: &mut Vec<Vertex>,
) {
    let row = yield_row(goods);
    if row.icons.is_empty() {
        return;
    }
    let pill = if detailed {
        rounded_rect(center, row.half, YIELD_PIP_CORNER)
    } else {
        let h = row.half;
        [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
            .map(|(x, y)| center + h * Vec2::new(x, y))
            .to_vec()
    };
    mesh::convex_polygon(&pill, YIELD_ROW_COLOR, out);
    let scale = YIELD_ICON_SCALE * row.scale;
    for (icon, at) in row.icons {
        if detailed {
            crate::game::sprites::push_icon(center + at, icon, scale, out);
        } else {
            map_icons::push_map_icon_silhouette(center + at, icon, scale, out);
        }
    }
    for (text, at) in row.labels {
        font::push_text_centered(
            center + at,
            YIELD_DIGIT_HEIGHT * row.scale,
            &text,
            YIELD_DIGIT_COLOR,
            out,
        );
    }
}

/// A yield row's contents, as offsets from its center.
pub(super) struct YieldRow {
    /// Icons in drawing order: within a group, higher ones first so lower
    /// ones overlap them.
    icons: Vec<(MapIcon, Vec2)>,
    /// Amounts past six, beside their single icon.
    labels: Vec<(String, Vec2)>,
    /// How far the pill reaches each way.
    half: Vec2,
    /// How much the row was shrunk to fit its hex (1 if it fit).
    scale: f32,
}

/// Where the pips for 1 to 6 go, in pip pitches, top row first: a die's
/// faces, but 2 side by side, 3 a triangle and 6 two rows of three.
pub(super) fn pip_spots(amount: i32) -> &'static [(f32, f32)] {
    match amount {
        1 => &[(0.0, 0.0)],
        2 => &[(-0.5, 0.0), (0.5, 0.0)],
        3 => &[(0.0, 0.5), (-0.5, -0.5), (0.5, -0.5)],
        4 => &[(-0.5, 0.5), (0.5, 0.5), (-0.5, -0.5), (0.5, -0.5)],
        5 => &[
            (-1.0, 0.7),
            (1.0, 0.7),
            (0.0, 0.0),
            (-1.0, -0.7),
            (1.0, -0.7),
        ],
        6 => &[
            (-1.0, 0.5),
            (0.0, 0.5),
            (1.0, 0.5),
            (-1.0, -0.5),
            (0.0, -0.5),
            (1.0, -0.5),
        ],
        _ => &[],
    }
}

/// Lays out a tile's food, wood and metal groups side by side, centered.
pub(super) fn yield_row((food, wood, metal): (i32, i32, i32)) -> YieldRow {
    let mut row = YieldRow {
        icons: Vec::new(),
        labels: Vec::new(),
        half: Vec2::ZERO,
        scale: 1.0,
    };
    // Each group's contents around its own center, and its half extent.
    let groups: Vec<YieldRow> = [
        (MapIcon::Food, food),
        (MapIcon::Wood, wood),
        (MapIcon::Metal, metal),
    ]
    .into_iter()
    .filter(|&(_, amount)| amount > 0)
    .map(|(icon, amount)| {
        let spots = pip_spots(amount);
        if spots.is_empty() {
            // Past six: one icon, then the number.
            let text = amount.to_string();
            let width = font::world_text_width(&text, YIELD_DIGIT_HEIGHT);
            let half_x = YIELD_ICON_HALF.x + (YIELD_NUMBER_GAP + width) / 2.0;
            return YieldRow {
                icons: vec![(icon, Vec2::new(YIELD_ICON_HALF.x - half_x, 0.0))],
                labels: vec![(text, Vec2::new(half_x - width / 2.0, 0.0))],
                half: Vec2::new(half_x, YIELD_ICON_HALF.y),
                scale: 1.0,
            };
        }
        let spots: Vec<Vec2> = spots
            .iter()
            .map(|&(x, y)| Vec2::new(x, y) * YIELD_PIP_PITCH)
            .collect();
        let reach = spots.iter().fold(Vec2::ZERO, |m, s| m.max(s.abs()));
        YieldRow {
            icons: spots.into_iter().map(|at| (icon, at)).collect(),
            labels: Vec::new(),
            half: reach + YIELD_ICON_HALF,
            scale: 1.0,
        }
    })
    .collect();
    if groups.is_empty() {
        return row;
    }
    let width: f32 = groups.iter().map(|g| 2.0 * g.half.x).sum::<f32>()
        + YIELD_GROUP_GAP * (groups.len() - 1) as f32;
    let mut left = -width / 2.0;
    for group in groups {
        let shift = Vec2::new(left + group.half.x, 0.0);
        row.icons
            .extend(group.icons.into_iter().map(|(icon, at)| (icon, at + shift)));
        row.labels.extend(
            group
                .labels
                .into_iter()
                .map(|(text, at)| (text, at + shift)),
        );
        row.half.y = row.half.y.max(group.half.y);
        left += 2.0 * group.half.x + YIELD_GROUP_GAP;
    }
    row.half = Vec2::new(width / 2.0, row.half.y) + YIELD_ROW_PADDING;
    // Three full groups would reach past the hex: shrink the whole row.
    if row.half.x > YIELD_ROW_MAX_HALF_WIDTH {
        let shrink = YIELD_ROW_MAX_HALF_WIDTH / row.half.x;
        for (_, at) in &mut row.icons {
            *at *= shrink;
        }
        for (_, at) in &mut row.labels {
            *at *= shrink;
        }
        row.half *= shrink;
        row.scale = shrink;
    }
    row
}

/// The corners of a rectangle with rounded corners of `radius`, centered on
/// `center` and reaching `half` out each way.
pub(super) fn rounded_rect(center: Vec2, half: Vec2, radius: f32) -> Vec<Vec2> {
    const STEPS: usize = 4;
    let inner = half - Vec2::splat(radius);
    [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
        .into_iter()
        .enumerate()
        .flat_map(|(quarter, (sx, sy))| {
            let corner = center + inner * Vec2::new(sx, sy);
            (0..=STEPS).map(move |step| {
                let angle = (quarter * STEPS + step) as f32 / (4 * STEPS) as f32 * TAU;
                corner + Vec2::from_angle(angle) * radius
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::PLAYER_TEAM;
    use crate::game::draw::tests::{count_color, scene};
    use crate::game::fog::tests::remembered_route_hex;
    use crate::game::unit::{Unit, UnitType};

    #[test]
    fn repeated_yield_rows_and_revealed_world_have_small_geometry_budgets() {
        let mut row = Vec::new();
        push_yield_row(Vec2::ZERO, (2, 1, 0), true, &mut row);
        assert!(row.len() <= 100, "{} yield vertices", row.len());
        // Translation and the cache must not change layout or sprite selection.
        let mut moved = Vec::new();
        push_yield_row(Vec2::new(10.0, -5.0), (2, 1, 0), true, &mut moved);
        assert_eq!(row.len(), moved.len());
        for (a, b) in row.iter().zip(&moved) {
            assert!((a.pos[0] + 10.0 - b.pos[0]).abs() < 1e-5);
            assert!((a.pos[1] - 5.0 - b.pos[1]).abs() < 1e-5);
            assert_eq!((a.color, a.uv), (b.color, b.uv));
        }
        for seed in [3, 42] {
            let mut game = GameState::world_scenario(seed);
            game.fog_of_war = false;
            game.camera.half_height = crate::game::camera::MAX_HALF_HEIGHT;
            game.set_details(true);
            let count = game.build_vertices().len();
            assert!(count < 600_000, "seed {seed}: {count} vertices with Alt");
            game.set_details(false);
            game.camera.half_height = 12.0;
            assert!(game.build_vertices().len() < 100_000);
        }
    }

    #[test]
    fn alt_shows_yields_on_every_explored_tile_outside_the_city_view() {
        let mut game = GameState::city_scenario();
        game.explore();
        let city = game
            .cities
            .iter()
            .position(|c| c.team == PLAYER_TEAM)
            .unwrap();
        // Every pill is the same outline, whatever its width.
        let pill_vertices = count_color(
            &{
                let mut out = Vec::new();
                push_yield_row(Vec2::ZERO, (2, 1, 0), true, &mut out);
                out
            },
            YIELD_ROW_COLOR,
        );
        let rows =
            |game: &GameState| count_color(&game.build_vertices(), YIELD_ROW_COLOR) / pill_vertices;
        game.selected_city = None;
        assert_eq!(rows(&game), 0, "no yields without the city view or Alt");
        game.selected_city = Some(city);
        let in_reach = rows(&game);
        assert!(in_reach > 0, "the open city shows its tiles' yields");
        game.selected_city = None;
        game.set_details(true);
        let fog = game.fog();
        let yielding = game
            .grid
            .all_hexes()
            .filter(|&h| game.grid.terrain(h).is_workable() && game.is_explored(h))
            .filter(|&h| game.known_yield(h, &fog) != (0, 0, 0))
            .count();
        assert!(yielding > in_reach);
        assert_eq!(rows(&game), yielding, "a row on every tile that yields");
    }

    /// Zoomed out, a yield row keeps its pill and a pip for each good, but
    /// each pip is a flat silhouette in a few triangles (#332).
    #[test]
    fn zoomed_out_yield_rows_are_plain_and_cheap() {
        let row = |detailed| {
            let mut out = Vec::new();
            push_yield_row(Vec2::ZERO, (3, 2, 1), detailed, &mut out);
            out
        };
        let (near, far) = (row(true), row(false));
        assert!(near.iter().any(|v| v.uv[0] >= 2.0));
        assert!(far.iter().all(|v| v.uv == crate::renderer::SOLID_UV));
        assert!(
            far.len() < near.len() && near.len() <= 100,
            "{} vertices, {} in full",
            far.len(),
            near.len()
        );
        // One quad for the pill, then a triangle pair per pip: 3 food,
        // 2 wood and 1 metal.
        assert_eq!(far.len(), 6 + 6 * 6);
        // The same pill, square-cornered.
        let bounds = |vertices: &[Vertex]| {
            vertices
                .iter()
                .filter(|v| v.color == YIELD_ROW_COLOR)
                .map(|v| Vec2::new(v.pos[0], v.pos[1]))
                .fold((Vec2::INFINITY, Vec2::NEG_INFINITY), |(lo, hi), p| {
                    (lo.min(p), hi.max(p))
                })
        };
        let ((near_lo, near_hi), (far_lo, far_hi)) = (bounds(&near), bounds(&far));
        assert!(near_lo.abs_diff_eq(far_lo, 1e-4) && near_hi.abs_diff_eq(far_hi, 1e-4));

        // The map switches past YIELD_DETAIL_MAX_HALF_HEIGHT.
        let mut game = GameState::city_scenario();
        game.explore();
        game.set_details(true);
        let yields = |game: &GameState| {
            let stages = game.scene_stages();
            stages
                .iter()
                .find(|(name, _)| *name == "tile yields")
                .unwrap()
                .1
        };
        game.camera.half_height = YIELD_DETAIL_MAX_HALF_HEIGHT;
        let full = yields(&game);
        game.camera.half_height = YIELD_DETAIL_MAX_HALF_HEIGHT + 0.01;
        let plain = yields(&game);
        assert!(plain > 0 && plain < full, "{plain} plain, {full} in full");
    }

    #[test]
    fn yields_up_to_six_are_die_pips_and_more_are_a_number() {
        let pips = |amount: i32| -> Vec<Vec2> {
            let row = yield_row((amount, 0, 0));
            assert!(row.labels.is_empty(), "{amount} needs no number");
            row.icons.into_iter().map(|(_, at)| at).collect()
        };
        let rows = |spots: &[Vec2]| {
            let mut ys: Vec<i32> = spots
                .iter()
                .map(|s| (s.y * 1000.0).round() as i32)
                .collect();
            ys.dedup();
            ys.len()
        };
        for amount in 1..=6 {
            assert_eq!(pips(amount).len(), amount as usize);
        }
        assert_eq!(rows(&pips(2)), 1, "2 side by side");
        // 3: one on top, centered over two.
        let three = pips(3);
        assert_eq!(rows(&three), 2);
        assert!(three[0].x.abs() < 1e-5 && three[0].y > three[1].y);
        // 4: a square.
        let four = pips(4);
        assert_eq!(rows(&four), 2);
        assert!((four[0].x + four[1].x).abs() < 1e-5);
        // 5: a square with one in the middle.
        let five = pips(5);
        assert!(five.iter().any(|s| s.length() < 1e-5));
        // 6: two rows of three.
        let six = pips(6);
        assert_eq!(rows(&six), 2);
        assert_eq!(six.iter().filter(|s| s.y > 0.0).count(), 3);

        let seven = yield_row((7, 0, 0));
        assert_eq!(seven.icons.len(), 1);
        assert_eq!(seven.labels[0].0, "7");
        assert!(
            seven.labels[0].1.x > seven.icons[0].1.x,
            "the number follows the icon"
        );
        assert!(yield_row((0, 0, 0)).icons.is_empty());
    }

    #[test]
    fn a_yield_row_puts_food_wood_and_metal_left_to_right_and_stays_inside_the_hex() {
        for goods in [
            (2, 1, 0),
            (6, 2, 0),
            (12, 1, 5),
            (3, 0, 0),
            (0, 1, 3),
            (1, 2, 1),
            (4, 1, 4),
        ] {
            let row = yield_row(goods);
            let span = |icon: MapIcon| {
                let xs: Vec<f32> = row
                    .icons
                    .iter()
                    .filter(|(i, _)| *i == icon)
                    .map(|(_, at)| at.x)
                    .collect();
                (
                    xs.iter().copied().fold(f32::MAX, f32::min),
                    xs.iter().copied().fold(f32::MIN, f32::max),
                )
            };
            let (food, wood, metal) = (
                span(MapIcon::Food),
                span(MapIcon::Wood),
                span(MapIcon::Metal),
            );
            assert!(food.1 < wood.0 && food.1 < metal.0, "{goods:?}");
            assert!(wood.1 < metal.0, "{goods:?}");
            let count = |icon| row.icons.iter().filter(|(i, _)| *i == icon).count();
            assert_eq!(count(MapIcon::Wood), goods.1.clamp(0, 6) as usize);
            // The pill's corners stay inside the hex's fill.
            let corner = (YIELD_ROW_OFFSET - row.half).abs();
            let fill = HEX_SIZE * HEX_FILL_SCALE * 3f32.sqrt();
            assert!(3f32.sqrt() * corner.x + corner.y <= fill, "{goods:?}");
        }
    }

    #[test]
    fn placing_shows_the_citys_shares_with_yields_on_or_alt() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.explore();
        game.show_yields = true;
        game.open_city(0);
        game.arm_worker_job(crate::game::workers::JobKind::Road);
        let city = 0;
        assert_eq!(game.yields_city(), Some(city));
        assert_eq!(game.shares_city(), Some(city));
        let fog = game.fog();
        let mut out = Vec::new();
        assert!(!game.push_delivery_shares(&fog, &mut out).is_empty());
        // Yields off: the shares only while Alt is held.
        game.show_yields = false;
        assert_eq!(game.shares_city(), None);
        game.set_details(true);
        assert_eq!(game.shares_city(), Some(city));
        game.set_details(false);
        game.press_escape();
        assert_eq!(game.shares_city(), None, "stopped placing");
    }

    #[test]
    fn a_building_hides_its_tiles_yields_and_a_job_name_clears_the_share() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.explore();
        game.open_city(0);
        let fog = game.fog();
        let center = game.cities[0].pos;
        let barracks = center.neighbors()[0];
        let mut out = Vec::new();
        assert!(
            game.push_delivery_shares(&fog, &mut out)
                .contains(&barracks)
        );
        game.cities[0].barracks = Some(barracks);
        let mut out = Vec::new();
        let shares = game.push_delivery_shares(&fog, &mut out);
        assert!(!shares.contains(&barracks), "no share on a building");
        assert!(game.known_building_at_off_center(barracks, &fog));
        assert!(
            !game.known_building_at_off_center(center, &fog),
            "the center keeps its chips"
        );
        // A job's name on a tile with a share sits below it, clear of it.
        let tile = *shares.iter().next().unwrap();
        let label = GameState::job_label_at(tile, &shares);
        let share_bottom = tile.to_world().y + SHARE_LABEL_OFFSET.y;
        assert!(label.y + PLANNED_JOB_LABEL_HEIGHT / 2.0 < share_bottom);
        let bare = GameState::job_label_at(tile, &HashSet::default());
        assert_eq!(bare, tile.to_world() + PLANNED_JOB_LABEL_OFFSET);
    }

    #[test]
    fn yield_chips_split_production_into_wood_and_metal() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        // Plains hills with a mine: a wood from the plains, the hill's and
        // the mine's metal.
        let mine = Hex::new(-2, 2);
        assert_eq!(game.sites[&mine].label, "MINE");
        let (food, wood, metal) = game.raw_yield(mine);
        assert_eq!((food, wood + metal), game.tile_yield(mine));
        assert_eq!(metal, 3);
        assert_eq!(wood, 1);
        // The city center: 2 food and 1 wood, whatever its ground.
        assert_eq!(game.raw_yield(game.cities[0].pos), (2, 1, 0));
        // A farm: food only.
        let farm = Hex::new(-4, 1);
        let (_, wood, metal) = game.raw_yield(farm);
        assert_eq!((wood, metal), (0, 0));
        let row = yield_row(game.raw_yield(mine));
        let metal_pips = row.icons.iter().filter(|(i, _)| *i == MapIcon::Metal);
        assert_eq!(metal_pips.count(), 3);
    }

    #[test]
    fn yield_badges_do_not_react_to_what_happens_out_of_sight() {
        let (mut game, city, far) = remembered_route_hex();
        assert!(game.show_yields && game.yields_city() == Some(city));
        let before = scene(&game);
        game.units
            .push(Unit::new(51, far, Team::Red, UnitType::Melee));
        assert_eq!(scene(&game), before);
        game.sites.insert(
            far,
            crate::game::city::Site {
                team: Team::Red,
                food: 9,
                production: 9,
                label: "FARM",
            },
        );
        assert_eq!(scene(&game), before);
    }
}
