//! The clouds over unexplored hexes: a fixed field of banks of puffs that
//! drift with the wind and billow, fading out past the map's edge.

use super::*;

/// Distance between cloud banks, in world units (a hex is 1 from center to corner).
pub(super) const CLOUD_SPACING: f32 = 2.8;

/// The most puffs in one cloud bank; each has between 4 and this many.
pub(super) const MAX_BANK_PUFFS: usize = 9;

/// How fast the whole cloud field drifts, in world units a second (a hex is
/// 1 from center to corner): slow enough to read as weather, not motion.
pub(super) const CLOUD_WIND: Vec2 = Vec2::new(0.09, 0.025);
/// Each puff swells and shrinks by this share of its radius...
pub(super) const CLOUD_BILLOW: f32 = 0.06;
/// ...and wanders by this much (world units) around its place in the bank...
pub(super) const CLOUD_WANDER: f32 = 0.12;
/// ...over about this many seconds, each puff at its own pace.
pub(super) const CLOUD_BILLOW_PERIOD: f32 = 14.0;

/// A cloud bank's shadow's radius.
pub(super) const SHADOW_RADIUS: f32 = 0.62 * CLOUD_SPACING;

/// Vertices in one cloud disc: a quad the shader rounds and feathers
/// (`soft_disc_uv`).
pub(super) const CLOUD_PUFF_VERTICES: usize = 6;
/// The dark band under each cloud bank, fading out at its edge.
pub(super) const CLOUD_SHADOW_COLOR: Color = [0.03, 0.03, 0.04, 0.55];
/// A puff's lit top and shaded underside (linear colors): muted, so the
/// clouds stay behind the map rather than competing with it.
pub(super) const CLOUD_LIGHT: Color = [0.25, 0.26, 0.29, 0.96];
pub(super) const CLOUD_DARK: Color = [0.10, 0.105, 0.125, 0.96];

/// Stable pseudo-random number for one cell of the cloud lattice.
pub(super) fn cloud_hash(x: i32, y: i32, salt: u32) -> f32 {
    let mut bits =
        (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77) ^ salt;
    bits ^= bits >> 16;
    bits = bits.wrapping_mul(0x7FEB_352D);
    bits ^= bits >> 15;
    bits as f32 / u32::MAX as f32
}

/// One cloud puff: a soft disc lit from above.
pub(super) struct Puff {
    center: Vec2,
    radius: f32,
    /// Its opacity: less past the map's edge (`edge_fade`).
    alpha: f32,
}

/// The shape of the cloud bank at lattice point (`x`, `y`): its puffs'
/// places around the bank's center and their radii, in units of the bank's
/// scale. The bank grows from one puff: each next one buds off a puff
/// already placed, in any direction and overlapping it, so every bank is
/// its own irregular clump rather than a copy of one template.
pub(super) fn bank_shape(x: i32, y: i32) -> ([(Vec2, f32); MAX_BANK_PUFFS], usize) {
    let hash =
        |i: usize, salt: u32| cloud_hash(x, y, salt ^ (i as u32 + 1).wrapping_mul(0x9E37_79B9));
    let count = 4 + (cloud_hash(x, y, 0x3C6E_F372) * (MAX_BANK_PUFFS - 3) as f32) as usize;
    let count = count.min(MAX_BANK_PUFFS);
    let mut puffs = [(Vec2::ZERO, 0.0); MAX_BANK_PUFFS];
    for i in 0..count {
        let radius = 0.2 + hash(i, 0x68E3_1DA4) * 0.16;
        puffs[i].1 = radius;
        if i > 0 {
            let (parent, parent_radius) = puffs[(hash(i, 0x1B87_3593) * i as f32) as usize % i];
            let angle = hash(i, 0x7E95_761E) * TAU;
            let reach = (parent_radius + radius) * (0.5 + hash(i, 0x2F1D_9A07) * 0.25);
            puffs[i].0 = parent + Vec2::from_angle(angle) * reach;
        }
    }
    // Centered on the bank, so a large bank doesn't lean off its place.
    let mean = puffs[..count].iter().map(|p| p.0).sum::<Vec2>() / count as f32;
    for puff in &mut puffs[..count] {
        puff.0 -= mean;
    }
    (puffs, count)
}

impl GameState {
    /// Moves the fog's clouds on by `dt` seconds (`push_cloud_banks`). The
    /// app calls it every frame, except in screenshot mode.
    pub fn animate_clouds(&mut self, dt: f32) {
        // Wrapped long before f32 loses the precision a frame needs.
        self.cloud_time = (self.cloud_time + dt) % 100_000.0;
    }
}

/// How much of a cloud puff centered at `point` shows: all of it over the
/// map, fading out over the hexes beyond its edge, so puffs drifting past
/// the edge come and go smoothly.
pub(super) fn edge_fade(grid: &HexGrid, point: Vec2) -> f32 {
    let hex = Hex::from_world(point);
    if grid.contains(hex) {
        return 1.0;
    }
    let nearest = hex
        .neighbors()
        .into_iter()
        .flat_map(|n| std::iter::once(n).chain(n.neighbors()))
        .filter(|&n| grid.contains(n))
        .map(|n| n.to_world().distance(point))
        .fold(f32::INFINITY, f32::min);
    1.0 - ((nearest - 1.0) / 1.2).clamp(0.0, 1.0)
}

/// Draws the unexplored map as banks of cumulus, those that reach into
/// `view` (a world rectangle, min and max): one bank per point of a
/// world-space lattice that drifts with `CLOUD_WIND` as `time` (seconds)
/// passes, each bank a soft shadow under an irregular clump of puffs
/// (`bank_shape`) that slowly billow. Every puff fades out at its edge and is lit from above
/// (`push_cloud_puff`); higher puffs are drawn first, so the lit top of a
/// lower billow overlaps the shaded underside of the one behind it. A bank
/// keeps its lattice point's hash as it drifts, so the pattern moves whole.
/// The work per puff is constant and cheap: no noise sampling per vertex.
pub(super) fn push_cloud_banks(
    grid: &HexGrid,
    time: f32,
    view: (Vec2, Vec2),
    out: &mut Vec<Vertex>,
) {
    let (min, max) = grid.bounds();
    // A bank reaches about two spacings from its lattice point.
    let reach = Vec2::splat(2.0 * CLOUD_SPACING);
    let (min, max) = (min.max(view.0 - reach), max.min(view.1 + reach));
    if min.cmpgt(max).any() {
        return;
    }
    let drift = CLOUD_WIND * time;
    let (min, max) = (min - drift, max - drift);
    let min_x = (min.x / CLOUD_SPACING).floor() as i32 - 1;
    let max_x = (max.x / CLOUD_SPACING).ceil() as i32 + 1;
    let min_y = (min.y / CLOUD_SPACING).floor() as i32 - 1;
    let max_y = (max.y / CLOUD_SPACING).ceil() as i32 + 1;
    let mut shadows = Vec::new();
    let mut puffs = Vec::new();
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let look = cached_bank(x, y);
            let (bank, scale) = (look.place + drift, look.scale);
            for puff in &look.puffs[..look.count] {
                let phase = puff.phase + time * puff.pace * TAU / CLOUD_BILLOW_PERIOD;
                let wander = Vec2::new(phase.cos(), (phase * 0.8).sin()) * CLOUD_WANDER;
                let center = bank + puff.place * scale + wander;
                let alpha = edge_fade(grid, center);
                if alpha <= 0.0 {
                    continue;
                }
                let radius = puff.size * scale * 1.5 * (1.0 + CLOUD_BILLOW * (phase * 1.3).sin());
                puffs.push(Puff {
                    center,
                    radius,
                    alpha,
                });
            }
            let shadow = bank + Vec2::new(0.04, -0.12) * scale;
            let alpha = edge_fade(grid, shadow);
            if alpha > 0.0 {
                shadows.push((shadow, alpha));
            }
        }
    }
    puffs.sort_unstable_by(|a, b| b.center.y.total_cmp(&a.center.y));
    out.reserve((shadows.len() + puffs.len()) * CLOUD_PUFF_VERTICES);
    for (center, alpha) in shadows {
        let color = with_alpha(CLOUD_SHADOW_COLOR, CLOUD_SHADOW_COLOR[3] * alpha);
        push_soft_disc(center, Vec2::splat(SHADOW_RADIUS), color, color, out);
    }
    for puff in &puffs {
        push_cloud_puff(puff, out);
    }
}

/// A cloud bank's fixed look, all from its lattice point's hash: where it
/// sits, how big it is, and its puffs.
#[derive(Clone, Copy)]
pub(super) struct Bank {
    /// Its place before the wind moves it.
    place: Vec2,
    scale: f32,
    puffs: [BankPuff; MAX_BANK_PUFFS],
    count: usize,
}

/// A puff of a `Bank`: its place around the bank's center and radius, in
/// units of the bank's scale, and how it billows (its pace, and its phase at
/// time 0).
#[derive(Clone, Copy, Default)]
pub(super) struct BankPuff {
    place: Vec2,
    size: f32,
    pace: f32,
    phase: f32,
}

pub(super) fn bank(x: i32, y: i32) -> Bank {
    // Rows are offset by half a bank, and each bank wanders off its lattice
    // point, so the lattice doesn't show.
    let jitter = Vec2::new(
        cloud_hash(x, y, 0xA341_316C) - 0.5,
        cloud_hash(x, y, 0xC801_3EA4) - 0.5,
    ) * 0.7;
    let row_shift = if y.rem_euclid(2) == 1 { 0.5 } else { 0.0 };
    let (shape, count) = bank_shape(x, y);
    let mut puffs = [BankPuff::default(); MAX_BANK_PUFFS];
    for (i, (puff, &(place, size))) in puffs.iter_mut().zip(&shape[..count]).enumerate() {
        let salt = (i as u32 + 1).wrapping_mul(0x9E37_79B9);
        // Each puff billows at its own phase and pace.
        *puff = BankPuff {
            place,
            size,
            pace: 0.75 + cloud_hash(x, y, 0x2545_F491 ^ salt) * 0.5,
            phase: cloud_hash(x, y, 0x5851_F42D ^ salt) * TAU,
        };
    }
    Bank {
        place: (Vec2::new(x as f32 + row_shift, y as f32) + jitter) * CLOUD_SPACING,
        scale: CLOUD_SPACING * (0.8 + cloud_hash(x, y, 0xAD90_777D) * 0.5),
        puffs,
        count,
    }
}

/// `bank`, worked out once per lattice point: the clouds are rebuilt every
/// frame and it's most of their cost. The wind brings new points into view
/// slowly, so the cache is simply emptied if it ever gets big.
pub(super) fn cached_bank(x: i32, y: i32) -> Bank {
    thread_local! {
        static BANKS: std::cell::RefCell<HashMap<(i32, i32), Bank>> = Default::default();
    }
    BANKS.with_borrow_mut(|banks| {
        if banks.len() > 20_000 {
            banks.clear();
        }
        *banks.entry((x, y)).or_insert_with(|| bank(x, y))
    })
}

pub(super) fn push_cloud_puff(puff: &Puff, out: &mut Vec<Vertex>) {
    push_soft_disc(
        puff.center,
        Vec2::splat(puff.radius),
        with_alpha(CLOUD_LIGHT, CLOUD_LIGHT[3] * puff.alpha),
        with_alpha(CLOUD_DARK, CLOUD_DARK[3] * puff.alpha),
        out,
    );
}

/// An ellipse with half-axes `radii`, solid in the middle and fading to
/// transparent at its rim (`soft_disc_uv`), shaded from `light` at the
/// top to `dark` at the bottom.
pub(super) fn push_soft_disc(
    center: Vec2,
    radii: Vec2,
    light: Color,
    dark: Color,
    out: &mut Vec<Vertex>,
) {
    // The light comes from above and a little to the left.
    let shade = |local: Vec2| {
        let t = (0.5 + 0.5 * local.dot(Vec2::new(-0.35, 0.94))).clamp(0.0, 1.0);
        let mut color = [0.0; 4];
        for (c, (l, d)) in color.iter_mut().zip(light.iter().zip(dark)) {
            *c = d + (l - d) * t;
        }
        color
    };
    let vertex = |x: f32, y: f32| {
        let local = Vec2::new(x, y);
        let p = center + local * radii;
        Vertex {
            pos: [p.x, p.y, 0.0],
            color: shade(local),
            uv: crate::renderer::soft_disc_uv([x, y]),
        }
    };
    let (bl, br, tr, tl) = (
        vertex(-1.0, -1.0),
        vertex(1.0, -1.0),
        vertex(1.0, 1.0),
        vertex(-1.0, 1.0),
    );
    out.extend([bl, br, tr, bl, tr, tl]);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A view taking in any map.
    const WHOLE_WORLD: (Vec2, Vec2) = (Vec2::splat(-1.0e4), Vec2::splat(1.0e4));

    #[test]
    fn unexplored_cloud_geometry_stays_small() {
        let game = GameState::world_scenario(3);
        let clouds = |view| {
            let mut vertices = Vec::new();
            push_cloud_banks(&game.grid, 0.0, view, &mut vertices);
            vertices
        };
        // The view the game starts in: only the banks it takes in.
        let view = clouds(game.cloud_view());
        assert!(view.len() < 25_000, "{} cloud vertices in view", view.len());
        assert!(view.iter().any(|v| v.color[3] < 1.0));
        // Zoomed all the way out, the whole map. The former recursively
        // sampled mesh emitted well over 150,000 fog vertices on a smaller
        // map than this one.
        let whole = clouds(WHOLE_WORLD);
        assert!(whole.len() > view.len());
        assert!(whole.len() < 120_000, "{} cloud vertices", whole.len());
        // Nothing when the view is off the map.
        assert!(clouds((Vec2::splat(500.0), Vec2::splat(600.0))).is_empty());
    }

    #[test]
    fn the_fog_setting_picks_clouds_or_solid_grey() {
        let mut game = GameState::world_scenario(3);
        let soft = |vertices: &[Vertex]| vertices.iter().filter(|v| v.uv[0] <= -2.0).count();
        let flat = |vertices: &[Vertex]| vertices.iter().any(|v| v.color == SOLID_FOG_COLOR);
        assert!(game.settings.cloud_fog, "clouds by default");
        let clouds = game.build_vertices();
        assert!(soft(&clouds) > 0);
        assert!(!flat(&clouds));
        game.set_setting(crate::game::settings::Setting::FogStyle, 0);
        assert!(!game.settings.cloud_fog);
        assert_eq!(game.notice, "FOG: SOLID GREY");
        let solid = game.build_vertices();
        assert_eq!(soft(&solid), 0, "no cloud puffs");
        assert!(flat(&solid));
        // Without fog of war there is neither.
        game.fog_of_war = false;
        let clear = game.build_vertices();
        assert!(!flat(&clear));
        assert_eq!(soft(&clear), 0);
    }

    #[test]
    fn the_clouds_drift_and_billow_but_stay_over_the_map() {
        let game = GameState::world_scenario(3);
        let clouds = |time: f32| {
            let mut vertices = Vec::new();
            push_cloud_banks(&game.grid, time, WHOLE_WORLD, &mut vertices);
            vertices
        };
        let centers: Vec<Vec2> = game.grid.all_hexes().map(Hex::to_world).collect();
        let min = centers.iter().copied().reduce(Vec2::min).unwrap();
        let max = centers.iter().copied().reduce(Vec2::max).unwrap();
        let reach = Vec2::splat(2.5 * CLOUD_SPACING);
        let still = clouds(0.0);
        assert_eq!(
            still.len(),
            clouds(0.0).len(),
            "the same time, the same clouds"
        );
        for time in [0.5, 60.0, 600.0, 5000.0] {
            let later = clouds(time);
            assert!(
                later.len() < 120_000,
                "{} cloud vertices at {time} s",
                later.len()
            );
            let moved = still.iter().zip(&later).any(|(a, b)| a.pos != b.pos);
            assert!(moved, "the clouds move by {time} s");
            // Whatever has drifted past the map's edge fades out: nothing is
            // drawn more than a bank's width beyond it.
            for v in &later {
                let at = Vec2::new(v.pos[0], v.pos[1]);
                let inside = at.cmpge(min - reach).all() && at.cmple(max + reach).all();
                assert!(inside, "a cloud vertex at {at} is far off the map");
            }
        }
        let mut game = game;
        game.animate_clouds(2.0);
        game.animate_clouds(3.0);
        assert_eq!(game.cloud_time, 5.0);
    }

    #[test]
    fn a_puff_fades_out_past_the_map_edge() {
        let game = GameState::world_scenario(3);
        let inside = game.grid.all_hexes().next().unwrap().to_world();
        assert_eq!(edge_fade(&game.grid, inside), 1.0);
        let far = Vec2::new(1.0e4, 0.0);
        assert_eq!(edge_fade(&game.grid, far), 0.0);
    }
}
