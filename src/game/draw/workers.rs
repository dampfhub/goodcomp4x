//! Workers' jobs (planned, under way, other sides' construction) and the
//! workers out in the field.

use super::*;

/// A queued wall or gate: an opaque, muted gold, so where edges meet the
/// rounded ends blend into one line instead of doubling up.
pub(super) const PLANNED_EDGE_COLOR: Color = [0.52, 0.42, 0.24, 1.0];

/// Workers: tokens out on the map (smaller in a corner when a unit shares
/// their hex), the player's queued jobs as faded named rings, and the tag on
/// each of the player's cities counting the workers at home.
pub(super) const WORKER_SCALE: f32 = 0.7;
pub(super) const WORKER_BESIDE_SCALE: f32 = 0.45;
pub(super) const WORKER_BESIDE_UNIT: Vec2 = Vec2::new(0.45, -0.32);
pub(super) const PLANNED_JOB_COLOR: Color = [0.95, 0.78, 0.42, 0.75];

/// A job a worker is out on, brighter than one still queued.
pub(super) const JOB_UNDER_WAY_COLOR: Color = [1.0, 0.88, 0.52, 1.0];

/// Where the job ring's open top side runs: its apothem.
pub(super) const JOB_RING_TOP: f32 = JOB_RING_RADIUS * 0.866_025_4;

pub(super) const PLANNED_JOB_RING_WIDTH: f32 = 0.05;
/// A job's name sits in its ring's open top...
pub(super) const PLANNED_JOB_LABEL_OFFSET: Vec2 = Vec2::new(0.0, JOB_RING_TOP);
pub(super) const PLANNED_JOB_LABEL_HEIGHT: f32 = 0.12;
/// ...on a dark plate reaching past the text by this much, so it reads over
/// the ring and any ground.
pub(super) const JOB_LABEL_PADDING: Vec2 = Vec2::new(0.05, 0.03);
pub(super) const JOB_LABEL_PLATE_COLOR: Color = [0.01, 0.01, 0.012, 0.8];

/// A job's name on a tile showing its delivery share: just under the share.
pub(super) const JOB_LABEL_UNDER_SHARE: Vec2 = Vec2::new(0.0, 0.40);

impl GameState {
    /// Where a job's name goes on its tile: atop the hex, or just under the
    /// delivery share when the tile shows one.
    pub(super) fn job_label_at(hex: Hex, shares: &HashSet<Hex>) -> Vec2 {
        let offset = if shares.contains(&hex) {
            JOB_LABEL_UNDER_SHARE
        } else {
            PLANNED_JOB_LABEL_OFFSET
        };
        hex.to_world() + offset
    }

    /// The player's worker jobs: those under way (`push_jobs_under_way`),
    /// and those queued, a faded ring on each tile, named. A job partly
    /// built (a worker left it) adds the turns of work it has left.
    pub(super) fn push_planned_jobs(&self, shares: &HashSet<Hex>, out: &mut Vec<Vertex>) {
        self.push_jobs_under_way(shares, out);
        let queued = self
            .cities
            .iter()
            .filter(|c| c.team == self.local_team)
            .flat_map(|c| &c.worker_jobs);
        for job in queued {
            let left = (job.done > 0).then(|| {
                let left = self.job_turns_left(self.local_team, *job);
                format!(
                    "{} {}",
                    self.job_name(*job),
                    crate::game::city::turns_icon(left as i32)
                )
            });
            if let Some(across) = job.across {
                let (start, end) = edge_corners(job.hex, across);
                push_rounded_segment(start, end, BARRIER_WIDTH * 0.6, PLANNED_EDGE_COLOR, out);
                if let Some(label) = left {
                    let at = (start + end) / 2.0 + PLANNED_JOB_LABEL_OFFSET * 0.5;
                    let height = PLANNED_JOB_LABEL_HEIGHT;
                    font::push_text_centered(at, height, &label, PLANNED_JOB_COLOR, out);
                }
                continue;
            }
            push_job_ring(job.hex, PLANNED_JOB_RING_WIDTH, PLANNED_JOB_COLOR, out);
            push_job_label(
                Self::job_label_at(job.hex, shares),
                left.as_deref().unwrap_or(self.job_name(*job)),
                PLANNED_JOB_COLOR,
                out,
            );
        }
    }

    /// The jobs the player's workers are out on: a solid ring on the tile (or
    /// the edge, for a wall or gate) named with the job and, once the worker
    /// is there working, the turns of work left, like "IMPROVE" and the clock with 2.
    pub(super) fn push_jobs_under_way(&self, shares: &HashSet<Hex>, out: &mut Vec<Vertex>) {
        let working = self
            .field_workers
            .iter()
            .filter(|w| w.team == self.local_team && !w.recalled);
        for worker in working {
            let Some(job) = worker.job else { continue };
            let label = match worker.work_left.filter(|_| worker.pos == job.hex) {
                Some(left) => format!(
                    "{} {}",
                    self.job_name(job),
                    crate::game::city::turns_icon(left as i32)
                ),
                None => self.job_name(job).into(),
            };
            let at = match job.across {
                Some(across) => {
                    let (start, end) = edge_corners(job.hex, across);
                    push_rounded_segment(start, end, BARRIER_WIDTH * 0.6, JOB_UNDER_WAY_COLOR, out);
                    (start + end) / 2.0 + PLANNED_JOB_LABEL_OFFSET * 0.5
                }
                None => {
                    push_job_ring(job.hex, JOB_RING_WIDTH, JOB_UNDER_WAY_COLOR, out);
                    Self::job_label_at(job.hex, shares)
                }
            };
            push_job_label(at, &label, JOB_UNDER_WAY_COLOR, out);
        }
    }

    /// Other sides' construction the player knows of (`MapView`): like the
    /// player's own jobs under way, a ring on the tile (or the edge, for a
    /// wall or gate) named with the job, but in the builder's color and
    /// without the turns left, which would give away its Workshop.
    pub(super) fn push_others_construction(
        &self,
        jobs: &[SeenJob],
        shares: &HashSet<Hex>,
        out: &mut Vec<Vertex>,
    ) {
        for seen in jobs.iter().filter(|seen| self.may_show(seen.job.hex)) {
            let color = seen.team.color();
            let at = match seen.job.across {
                Some(across) => {
                    let (start, end) = edge_corners(seen.job.hex, across);
                    push_rounded_segment(start, end, BARRIER_WIDTH * 0.6, color, out);
                    (start + end) / 2.0 + PLANNED_JOB_LABEL_OFFSET * 0.5
                }
                None => {
                    push_job_ring(seen.job.hex, JOB_RING_WIDTH, color, out);
                    Self::job_label_at(seen.job.hex, shares)
                }
            };
            push_job_label(at, seen.name, color, out);
        }
    }

    /// Workers out on the map that the player can see: small hollow tokens
    /// with a shovel, tucked into a corner when a unit shares their hex, and
    /// for the player's own, a dotted line to the job they're walking to.
    pub(super) fn push_field_workers(&self, fog: &Fog, out: &mut Vec<Vertex>) {
        let look = UnitLook {
            icon: UnitIcon::Shovel,
            civilian: true,
        };
        for worker in self.field_workers.iter().filter(|w| fog.shows_worker(w)) {
            let shared = self.units_at(worker.pos).any(|i| fog.shows(&self.units[i]));
            let (center, scale) = if shared {
                (
                    worker.pos.to_world() + WORKER_BESIDE_UNIT,
                    WORKER_BESIDE_SCALE,
                )
            } else {
                (worker.pos.to_world(), WORKER_SCALE)
            };
            let center = center + self.worker_glide(worker);
            if worker.team == self.local_team
                && let Some(job) = worker.job.filter(|job| job.hex != worker.pos)
            {
                push_dotted_segment(center, job.hex.to_world(), 0.05, PLANNED_JOB_COLOR, out);
            }
            let color = if self.recent_actors.contains(&worker.id) {
                brighten(worker.team.color())
            } else {
                worker.team.color()
            };
            push_unit_icon(center, look, scale, color, out);
        }
    }
}

/// A job's ring on `hex` (`JOB_RING_RADIUS`): five sides of a hexagon, open
/// along the top where its name goes.
pub(super) fn push_job_ring(hex: Hex, width: f32, color: Color, out: &mut Vec<Vertex>) {
    let center = hex.to_world();
    // From the top left corner around to the top right one, the long way.
    let corners: [Vec2; 6] = std::array::from_fn(|i| {
        center + Vec2::from_angle((i + 2) as f32 * TAU / 6.0) * JOB_RING_RADIUS
    });
    mesh::polyline(&corners, width, color, out);
}

/// A job's name centered on `at`, on a dark plate.
pub(super) fn push_job_label(at: Vec2, text: &str, color: Color, out: &mut Vec<Vertex>) {
    let height = PLANNED_JOB_LABEL_HEIGHT;
    let radius = height / 2.0 + JOB_LABEL_PADDING.y;
    let reach =
        (font::world_text_width(text, height) / 2.0 + JOB_LABEL_PADDING.x - radius).max(0.0);
    push_pill(at, reach, radius, JOB_LABEL_PLATE_COLOR, out);
    font::push_text_centered(at, height, text, color, out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::draw::tests::{count_color, scene, top_color};
    use crate::game::fog::tests::{behind_the_mountain, glance_at};

    #[test]
    fn work_under_way_is_ringed_and_counts_its_turns_left() {
        let mut game = GameState::city_scenario();
        // Fresh water everywhere, so any open tile takes a farm.
        game.grid = game.grid.clone().with_rivers_everywhere();
        game.explore();
        game.open_city(0);
        let city = game.cities[0].pos;
        // Two hexes out: a turn walking, then at work.
        let hex = game
            .grid
            .all_hexes()
            .filter(|&h| h.distance(city) == 2)
            .find(|&h| {
                game.job_unavailable(h, crate::game::workers::JobKind::Improve)
                    .is_none()
            })
            .unwrap();
        game.placing_job = Some(crate::game::workers::JobKind::Improve);
        assert!(game.place_job_at(hex, None));
        game.placing_job = None;
        let solid = |game: &GameState| count_color(&game.build_vertices(), JOB_UNDER_WAY_COLOR);
        assert_eq!(solid(&game), 0, "only queued, not under way");
        // Out and walking there: its name in solid gold.
        game.resolve_workers();
        let walking = solid(&game);
        assert!(walking > 0);
        assert_ne!(game.field_workers[0].pos, hex, "still walking");
        // At work: the clock and 3 after "IMPROVE", more shapes.
        game.resolve_workers();
        assert_eq!(game.field_workers[0].work_left, Some(3));
        assert!(solid(&game) > walking);
    }

    #[test]
    fn a_job_on_a_worked_tile_nests_inside_the_worked_ring() {
        use crate::game::workers::{FieldWorker, JobKind};
        let mut game = GameState::city_scenario();
        game.explore();
        game.open_city(0);
        let (manager, worker) = (
            game.cities[0].clusters[0].manager,
            game.cities[0].clusters[0].workers[0],
        );
        // A job under way on the manager's tile, one queued on the worker's.
        let job = WorkerJob::on_tile(manager, JobKind::Improve);
        game.field_workers.push(FieldWorker {
            id: 900,
            team: game.cities[0].team,
            home: 0,
            base: game.cities[0].pos,
            pos: manager,
            job: Some(job),
            work_left: Some(2),
            recalled: false,
        });
        game.cities[0]
            .worker_jobs
            .push(WorkerJob::on_tile(worker, JobKind::Road));
        let fog = game.fog();
        let mut out = Vec::new();
        game.push_city_map(&fog, &mut out);

        // Each ring's line, below the tile's center (clear of labels), shows
        // its own color on top: the job's doesn't cover the worked ring's.
        let manager_gold = [1.0, 0.78, 0.20, 1.0];
        let below = |hex: Hex, radius: f32| hex.to_world() - Vec2::new(0.0, radius * 0.866);
        assert_eq!(
            top_color(&out, below(manager, WORKED_OUTLINE_RADIUS)),
            Some(manager_gold)
        );
        assert_eq!(
            top_color(&out, below(manager, JOB_RING_RADIUS)),
            Some(JOB_UNDER_WAY_COLOR)
        );

        // The shapes' radii: every corner of a job's ring (solid, not its
        // name's glyphs) lies inside the worked ring's dark rim, on both tiles.
        let distances = |hex: Hex, color: Color| {
            out.iter()
                .filter(|v| v.color == color && v.uv == crate::renderer::SOLID_UV)
                .map(|v| Vec2::new(v.pos[0], v.pos[1]).distance(hex.to_world()))
                .filter(|&d| d < HEX_SIZE)
                .collect::<Vec<f32>>()
        };
        let rim_inside = |hex: Hex| {
            distances(hex, WORKED_OUTLINE_RIM_COLOR)
                .into_iter()
                .fold(f32::MAX, f32::min)
        };
        for (hex, color) in [(manager, JOB_UNDER_WAY_COLOR), (worker, PLANNED_JOB_COLOR)] {
            let ring = distances(hex, color);
            assert!(!ring.is_empty());
            let outermost = ring.into_iter().fold(0.0, f32::max);
            assert!(
                outermost < rim_inside(hex),
                "{outermost} vs {}",
                rim_inside(hex)
            );
        }
    }

    /// A worker of `team` at `job`'s tile: at work on it, or just arrived.
    fn worker_at(team: Team, job: WorkerJob, working: bool) -> crate::game::workers::FieldWorker {
        crate::game::workers::FieldWorker {
            id: 77,
            team,
            home: 0,
            base: job.hex,
            pos: job.hex,
            job: Some(job),
            work_left: working.then_some(2),
            recalled: false,
        }
    }

    /// Vertices of `color` for a job's ring (or edge) and its name: what
    /// another side's construction adds.
    fn construction_vertices(job: WorkerJob, name: &str, color: Color) -> usize {
        let mut out = Vec::new();
        match job.across {
            Some(across) => {
                let (start, end) = edge_corners(job.hex, across);
                push_rounded_segment(start, end, BARRIER_WIDTH * 0.6, color, &mut out);
            }
            None => push_job_ring(job.hex, JOB_RING_WIDTH, color, &mut out),
        }
        push_job_label(Vec2::ZERO, name, color, &mut out);
        count_color(&out, color)
    }

    #[test]
    fn another_sides_construction_in_sight_shows_in_its_color_named_without_turns() {
        use crate::game::city::Building;
        use crate::game::workers::JobKind;
        let (mut game, _, _) = behind_the_mountain();
        game.camera.center = Vec2::ZERO;
        let near = Hex::new(0, 1);
        assert!(game.fog().sees(near));
        let red = Team::Red.color();
        let colored = |game: &GameState, color: Color| count_color(&game.build_vertices(), color);

        // Red's worker just arrived shows only itself; at work, its
        // Barracks going up too: a red ring named BARRACKS, no turns left.
        let barracks = WorkerJob::on_tile(near, JobKind::Build(Building::Barracks));
        game.field_workers = vec![worker_at(Team::Red, barracks, false)];
        let idle = colored(&game, red);
        game.field_workers = vec![worker_at(Team::Red, barracks, true)];
        assert_eq!(
            colored(&game, red) - idle,
            construction_vertices(barracks, "BARRACKS", red)
        );
        assert_eq!(colored(&game, JOB_UNDER_WAY_COLOR), 0);

        // A wall on one of the tile's edges: the edge, named.
        let wall = WorkerJob {
            hex: near,
            kind: JobKind::Wall,
            across: Some(Hex::new(1, 1)),
            done: 0,
        };
        game.field_workers = vec![worker_at(Team::Red, wall, false)];
        let idle = colored(&game, red);
        game.field_workers = vec![worker_at(Team::Red, wall, true)];
        assert_eq!(
            colored(&game, red) - idle,
            construction_vertices(wall, "WALL", red)
        );

        // The player's own job is drawn as before, in bright gold with its
        // turns left, and not in team color.
        let blue = game.local_team.color();
        game.field_workers = vec![worker_at(game.local_team, barracks, false)];
        let idle = colored(&game, blue);
        game.field_workers = vec![worker_at(game.local_team, barracks, true)];
        assert_eq!(colored(&game, blue), idle);
        assert!(colored(&game, JOB_UNDER_WAY_COLOR) > 0);
    }

    #[test]
    fn another_sides_construction_out_of_sight_shows_only_as_last_seen() {
        use crate::game::workers::JobKind;
        let (mut game, cavalry, hidden) = behind_the_mountain();
        game.camera.center = Vec2::ZERO;
        let red = Team::Red.color();
        let job = WorkerJob::on_tile(hidden, JobKind::Fort);
        let colored = |game: &GameState| count_color(&game.build_vertices(), red);

        // Never seen: nothing of it shows.
        let before = scene(&game);
        game.field_workers = vec![worker_at(Team::Red, job, true)];
        game.explore();
        assert_eq!(scene(&game), before);

        // Seen at work, then out of sight: it stays as last seen, even
        // once the worker has gone.
        glance_at(&mut game, cavalry, hidden);
        let shown = construction_vertices(job, "FORT", red);
        assert_eq!(colored(&game), shown);
        game.field_workers.clear();
        game.explore();
        assert_eq!(colored(&game), shown);

        // Seen again with nobody at work there: gone.
        glance_at(&mut game, cavalry, hidden);
        assert_eq!(colored(&game), 0);
    }

    #[test]
    fn a_building_placed_for_workers_shows_its_name_on_its_tile() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.selected = None;
        game.explore();
        let site = Hex::new(-2, 0);
        let barracks = crate::game::city::Building::Barracks;
        let before = count_color(&game.build_vertices(), PLANNED_JOB_COLOR);
        game.open_city(0);
        game.queue_selected_city_building(barracks);
        assert!(game.place_job_at(site, None));
        game.leave_city_view();
        // A faded ring and its name, like any job waiting for a worker.
        assert!(count_color(&game.build_vertices(), PLANNED_JOB_COLOR) > before);
        let job = game.cities[0].worker_jobs[0];
        assert_eq!(game.job_name(job), "BARRACKS");
    }
}
