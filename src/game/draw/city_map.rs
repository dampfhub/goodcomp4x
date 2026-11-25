//! Cities on the map: roads, improvements, buildings, worked tiles and the
//! open city's labor and delivery shares (`MapView`: what to draw, as data).

use super::*;

/// The armed worker job's preview on a tile where it can't go.
pub(super) const BLOCKED_JOB_COLOR: Color = [0.90, 0.30, 0.25, 1.0];

/// A tile's delivery share, atop the hex: its baseline's left end from the
/// hex's center, and its capital height.
pub(super) const SHARE_LABEL_OFFSET: Vec2 = Vec2::new(-0.3, 0.52);
pub(super) const SHARE_LABEL_HEIGHT: f32 = 0.18;
pub(super) const SHARE_LABEL_COLOR: Color = [0.65, 0.85, 0.65, 1.0];
/// A road's dot and the segments joining it to its neighbors.
pub(super) const ROAD_COLOR: Color = [0.65, 0.45, 0.24, 1.0];

pub(super) const WORKED_OUTLINE_WIDTH: f32 = 0.06;
pub(super) const WORKED_OUTLINE_RIM_WIDTH: f32 = 0.10;
pub(super) const WORKED_OUTLINE_RIM_COLOR: Color = [0.02, 0.05, 0.03, 1.0];
/// The dotted line along the open city's manager's delivery route, shown
/// while the cursor is on the manager.
pub(super) const MANAGER_ROUTE_WIDTH: f32 = 0.06;
pub(super) const MANAGER_ROUTE_COLOR: Color = [0.35, 0.82, 1.0, 0.9];

impl GameState {
    /// Roads, improvements, cities and barracks, plus the open city's labor.
    /// Hexes in sight show what's there now; remembered hexes out of sight
    /// show what was there when last seen.
    pub(super) fn push_city_map(&self, fog: &Fog, out: &mut Vec<Vertex>) {
        let view = self.map_view(fog);
        for &h in &view.roads {
            mesh::regular_polygon(h.to_world(), 0.12, 8, 0.0, ROAD_COLOR, out);
        }
        // Road segments join roads to each other and to cities, even though a
        // city center is drawn as its larger marker.
        let joins = |h: Hex| view.roads.contains(&h) || view.cities.iter().any(|c| c.0 == h);
        for &h in &view.roads {
            for n in h.neighbors() {
                let city = !view.roads.contains(&n);
                if joins(n) && (city || (h.q, h.r) < (n.q, n.r)) {
                    mesh::segment(h.to_world(), n.to_world(), 0.09, ROAD_COLOR, out);
                }
            }
        }
        // After the roads, which run through where a share sits (#215).
        let shares = self.push_delivery_shares(fog, out);
        if let Some(i) = self
            .hovered_city
            .or(self.selected_city)
            .filter(|&i| self.cities[i].team == self.local_team || fog.sees(self.cities[i].pos))
        {
            // Delivery labels show routes as the player knows them; the
            // worked-tile rings below show whether goods really arrive. Of
            // another side's city, only the tiles it works in sight show,
            // and not whether their goods arrive: its routes aren't known.
            let own = self.cities[i].team == fog.team();
            let shown = |h: Hex| own || fog.sees(h);
            let routes = own.then(|| self.routes(i));
            let known_routes = self.known_routes(i, fog);
            // A manager picked up takes its cluster with it until it's
            // placed: none show.
            let clusters = &self.cities[i].clusters;
            let several = clusters.len() > 1;
            for (k, cluster) in clusters.iter().enumerate() {
                if self.moving_manager == Some((i, k)) {
                    continue;
                }
                for (worker_index, h) in cluster.tiles().enumerate() {
                    if !shown(h) {
                        continue;
                    }
                    let arrives = routes.as_ref().is_none_or(|r| r.costs.contains_key(&h));
                    let color = if arrives {
                        if worker_index == 0 {
                            [1.0, 0.78, 0.20, 1.0]
                        } else {
                            [0.25, 1.0, 0.4, 1.0]
                        }
                    } else {
                        [1.0, 0.25, 0.2, 1.0]
                    };
                    // A colored ring on a slightly wider dark one, just inside
                    // the hex's fill so it doesn't blur into the grid lines.
                    let center = h.to_world();
                    let radius = WORKED_OUTLINE_RADIUS;
                    let rim = WORKED_OUTLINE_RIM_COLOR;
                    mesh::polygon_outline(
                        center,
                        radius,
                        WORKED_OUTLINE_RIM_WIDTH,
                        6,
                        0.0,
                        rim,
                        out,
                    );
                    mesh::polygon_outline(center, radius, WORKED_OUTLINE_WIDTH, 6, 0.0, color, out);
                }
                // The manager's mark: M, or with several clusters M1 to M4.
                if !shown(cluster.manager) {
                    continue;
                }
                let center = cluster.manager.to_world();
                if several {
                    let label = crate::game::city::manager_label(k, clusters.len());
                    font::push_text_centered(center, 0.34, &label, LABEL_COLOR, out);
                } else {
                    font::push_glyph(center, 0.34, 'M', LABEL_COLOR, out);
                }
                if self.hovered_tile == Some(cluster.manager) {
                    // Hovering a manager traces the way its goods travel to
                    // the city center, as the player knows the board.
                    for leg in known_routes.path_from(cluster.manager).windows(2) {
                        let (from, to) = (leg[0].to_world(), leg[1].to_world());
                        push_dotted_segment(
                            from,
                            to,
                            MANAGER_ROUTE_WIDTH,
                            MANAGER_ROUTE_COLOR,
                            out,
                        );
                    }
                }
            }
        }
        if let Some(i) = self.selected_barracks
            && let Some(barracks) = self.cities[i].barracks
        {
            let routes = self.known_routes_from(self.cities[i].team, barracks, fog);
            let mut costs: Vec<_> = routes
                .costs
                .iter()
                .filter(|(h, _)| self.is_explored(**h))
                .collect();
            costs.sort_unstable_by_key(|(h, _)| (h.q, h.r));
            for (hex, cost) in costs {
                font::push_text(
                    hex.to_world() + SHARE_LABEL_OFFSET,
                    SHARE_LABEL_HEIGHT,
                    &format!("{}%", crate::game::city::delivered_share(*cost) * 25),
                    [1.0, 0.70, 0.30, 1.0],
                    out,
                );
            }
            for source in self.cities[i].worked() {
                push_dotted_segment(
                    source.to_world(),
                    barracks.to_world(),
                    0.06,
                    [1.0, 0.62, 0.20, 0.9],
                    out,
                );
            }
        }
        for &(h, structure) in &view.structures {
            push_structure(h.to_world(), structure.kind, structure.team.color(), out);
        }
        self.push_planned_jobs(&shares, out);
        self.push_others_construction(&view.construction, &shares, out);
        // Shared corner posts overlap: choose their painter order by edge,
        // independently of hash keys, capacity or insertion order.
        let mut barriers: Vec<_> = view.barriers.iter().collect();
        barriers.sort_unstable_by_key(|((a, b), _)| (a.q, a.r, b.q, b.r));
        for (&(a, b), barrier) in barriers {
            push_barrier(a, b, barrier.kind, barrier.team.color(), out);
        }
        // Where the armed worker job would go: an edge, or a ring on a tile,
        // red where it can't.
        if let (Some((hex, across)), Some(kind)) = (self.hovered_job, self.placing_job) {
            match across {
                Some(across) => {
                    let (start, end) = edge_corners(hex, across);
                    push_rounded_segment(start, end, BARRIER_WIDTH, PLANNED_JOB_COLOR_SOLID, out);
                }
                None => {
                    let color = if self.job_unavailable(hex, kind).is_none()
                        || self.job_taken(self.local_team, WorkerJob::on_tile(hex, kind))
                    {
                        PLANNED_JOB_COLOR_SOLID
                    } else {
                        BLOCKED_JOB_COLOR
                    };
                    push_job_ring(hex, JOB_RING_WIDTH, color, out);
                }
            }
        }
        for &(h, label) in &view.sites {
            let center = h.to_world() + IMPROVEMENT_SPOT;
            match MapIcon::improvement(label) {
                Some(icon) => crate::game::sprites::push_icon(center, icon, 1.0, out),
                None => {
                    let letter = label.chars().next().unwrap_or('?');
                    font::push_glyph(center, 0.2, letter, ICON_OUTLINE_COLOR, out);
                }
            }
        }
        for (hex, barracks) in &view.barracks {
            push_barracks_marker(hex.to_world(), barracks.team.color(), out);
            push_health_bar(hex.to_world(), barracks.health, BARRACKS_BAR_SCALE, out);
        }
        for city in &self.cities {
            for building in crate::game::city::Building::PLACEABLE {
                if building == crate::game::city::Building::Barracks {
                    continue;
                }
                let Some(hex) = city.placed_site(building) else {
                    continue;
                };
                if city.team != self.local_team && !fog.sees(hex) {
                    continue;
                }
                let (badge, color) = building_badge(building);
                mesh::regular_polygon(hex.to_world(), 0.31, 4, FRAC_PI_4, color, out);
                font::push_glyph(hex.to_world(), 0.30, badge, LABEL_COLOR, out);
                if building == crate::game::city::Building::CoastalBattery {
                    let health = city.coastal_battery_hp / COASTAL_BATTERY_MAX_HP;
                    push_health_bar(hex.to_world(), health, BATTERY_BAR_SCALE, out);
                }
            }
        }
        for (hex, city) in &view.cities {
            push_city_marker(hex.to_world(), city, out);
        }
        for city in self.cities.iter().filter(|c| c.team == self.local_team) {
            push_worker_count(city.pos.to_world(), city.workers, out);
        }
        self.push_production_tags(out);
    }

    /// Each tile's delivery share to `shares_city` (the percentage atop
    /// the hex), as the player knows the routes: every explored tile in
    /// reach but those a building covers. Returns the tiles labeled, whose
    /// job names then sit below the share.
    pub(super) fn push_delivery_shares(&self, fog: &Fog, out: &mut Vec<Vertex>) -> HashSet<Hex> {
        let mut labeled = HashSet::default();
        let Some(i) = self.shares_city() else {
            return labeled;
        };
        let known_routes = self.known_routes(i, fog);
        let mut costs: Vec<_> = known_routes.costs.iter().collect();
        costs.sort_unstable_by_key(|(h, _)| (h.q, h.r));
        for (h, cost) in costs {
            // Nothing is known of a tile never seen, delivery included.
            if !self.may_show(*h) || !self.is_explored(*h) || self.known_building_at(*h, fog) {
                continue;
            }
            let food_share = self.mill_food_share(i, *h, *cost);
            let production_share = crate::game::city::delivered_share(*cost);
            let label = if food_share == production_share {
                format!("{}%", food_share * 25)
            } else {
                format!("F{} P{}", food_share * 25, production_share * 25)
            };
            font::push_text(
                h.to_world() + SHARE_LABEL_OFFSET,
                SHARE_LABEL_HEIGHT,
                &label,
                SHARE_LABEL_COLOR,
                out,
            );
            labeled.insert(*h);
        }
        labeled
    }

    /// Whether the player knows of a city or placed building on `hex`, which
    /// shows no yields (`closed_to_citizens`): their own always, anyone's in
    /// sight, and a remembered city or barracks.
    /// `known_building_at` but for city centers, which keep their chips:
    /// their own yield comes in on its own.
    pub(super) fn known_building_at_off_center(&self, hex: Hex, fog: &Fog) -> bool {
        let center = self.cities.iter().any(|c| c.pos == hex)
            || self.remembered(hex).is_some_and(|seen| seen.city.is_some()) && !fog.sees(hex);
        !center && self.known_building_at(hex, fog)
    }

    pub(super) fn known_building_at(&self, hex: Hex, fog: &Fog) -> bool {
        let own = self.cities.iter().any(|c| {
            c.team == self.local_team
                && (c.pos == hex
                    || crate::game::city::Building::PLACEABLE
                        .iter()
                        .any(|&b| c.placed_site(b) == Some(hex)))
        });
        if own || fog.sees(hex) {
            return self.closed_to_citizens(hex);
        }
        self.remembered(hex)
            .is_some_and(|seen| seen.city.is_some() || seen.barracks.is_some())
    }

    /// What `push_city_map` shows: the live map in sight, the memory of it
    /// elsewhere. The player's own cities and barracks always show.
    pub(super) fn map_view(&self, fog: &Fog) -> MapView {
        let mut view = MapView::default();
        view.roads
            .extend(self.roads.iter().filter(|h| fog.sees(**h)));
        for (&h, site) in self.sites.iter().filter(|(h, _)| fog.sees(**h)) {
            view.sites.push((h, site.label));
        }
        view.structures.extend(
            self.structures
                .iter()
                .filter(|(h, _)| fog.sees(**h))
                .map(|(&h, &s)| (h, s)),
        );
        view.barriers.extend(
            self.barriers
                .iter()
                .filter(|((a, b), _)| fog.sees(*a) || fog.sees(*b)),
        );
        for city in &self.cities {
            let own = city.team == self.local_team;
            let seen = |health| SeenBuilding {
                team: city.team,
                id: city.id,
                health,
                population: city.population,
            };
            if own || fog.sees(city.pos) {
                view.cities.push((city.pos, seen(1.0)));
            }
            if let Some(hex) = city.barracks.filter(|h| own || fog.sees(*h)) {
                let health = city.barracks_hp / crate::game::city::BARRACKS_MAX_HP;
                view.barracks.push((hex, seen(health)));
            }
        }
        view.construction.extend(
            self.field_workers
                .iter()
                .filter(|w| fog.sees(w.pos))
                .filter_map(|w| self.others_construction(w, self.local_team)),
        );
        if !self.fog_of_war {
            return view;
        }
        for (&h, seen) in self.memory.iter().filter(|(h, _)| !fog.sees(**h)) {
            if seen.road {
                view.roads.push(h);
            }
            if let Some((label, _)) = seen.site {
                view.sites.push((h, label));
            }
            if let Some(structure) = seen.structure {
                view.structures.push((h, structure));
            }
            for &(across, barrier) in seen.barriers.iter().filter(|(n, _)| !fog.sees(*n)) {
                view.barriers.insert(edge(h, across), barrier);
            }
            if let Some(city) = seen.city.filter(|c| c.team != self.local_team) {
                view.cities.push((h, city));
            }
            if let Some(barracks) = seen.barracks.filter(|b| b.team != self.local_team) {
                view.barracks.push((h, barracks));
            }
            view.construction.extend(
                seen.construction
                    .iter()
                    .filter(|seen| seen.team != self.local_team),
            );
        }
        view
    }
}

/// Roads, improvements, structures, cities and barracks to draw.
#[derive(Default)]
pub(super) struct MapView {
    roads: Vec<Hex>,
    /// Improvements, by label.
    sites: Vec<(Hex, &'static str)>,
    structures: Vec<(Hex, Structure)>,
    /// Walls and gates, by edge.
    barriers: HashMap<(Hex, Hex), Structure>,
    cities: Vec<(Hex, SeenBuilding)>,
    barracks: Vec<(Hex, SeenBuilding)>,
    /// Other sides' construction, by the tile its worker stands on.
    construction: Vec<SeenJob>,
}

pub(super) fn building_badge(building: crate::game::city::Building) -> (char, Color) {
    match building {
        crate::game::city::Building::Barracks => ('B', [0.72, 0.35, 0.18, 1.0]),
        crate::game::city::Building::Mill => ('M', [0.35, 0.65, 0.28, 1.0]),
        crate::game::city::Building::Workshop => ('W', [0.38, 0.52, 0.82, 1.0]),
        crate::game::city::Building::CanoeHouse => ('C', [0.30, 0.65, 0.82, 1.0]),
        crate::game::city::Building::Forge => ('F', [0.82, 0.43, 0.22, 1.0]),
        crate::game::city::Building::Stable => ('S', [0.67, 0.49, 0.27, 1.0]),
        crate::game::city::Building::Watchpost => ('V', [0.78, 0.76, 0.35, 1.0]),
        crate::game::city::Building::FieldHospital => ('H', [0.80, 0.33, 0.36, 1.0]),
        crate::game::city::Building::Cannery => ('N', [0.44, 0.72, 0.47, 1.0]),
        crate::game::city::Building::WorkCamp => ('K', [0.67, 0.55, 0.36, 1.0]),
        crate::game::city::Building::Smelter => ('T', [0.82, 0.43, 0.29, 1.0]),
        crate::game::city::Building::Railhead => ('R', [0.47, 0.68, 0.79, 1.0]),
        crate::game::city::Building::Harbor => ('P', [0.34, 0.67, 0.90, 1.0]),
        crate::game::city::Building::CoastalBattery => ('D', [0.85, 0.57, 0.28, 1.0]),
    }
}

/// Short dashes show where goods go without looking like a road.
pub(super) fn push_dotted_segment(
    a: Vec2,
    b: Vec2,
    width: f32,
    color: Color,
    out: &mut Vec<Vertex>,
) {
    const DASHES: usize = 7;
    for index in 0..DASHES {
        let start = (index as f32 + 0.18) / DASHES as f32;
        let end = (index as f32 + 0.62) / DASHES as f32;
        mesh::segment(a.lerp(b, start), a.lerp(b, end), width, color, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::PLAYER_TEAM;
    use crate::game::draw::tests::{count_color, scene};
    use crate::game::fog::tests::{remembered_route_hex, worked_tile_behind_the_mountain};
    use crate::game::unit::{Unit, UnitType};

    #[test]
    fn delivery_labels_show_only_on_explored_tiles() {
        let mut game = GameState::city_scenario();
        game.explore();
        game.select_city();
        let city = game.selected_city.unwrap();
        let label = SHARE_LABEL_COLOR;
        let seen = count_color(&game.build_vertices(), label);
        assert!(seen > 0, "labels on the explored tiles");
        // Forget an explored tile the known routes reach: its label goes.
        let fog = game.fog();
        let forgotten = game
            .known_routes(city, &fog)
            .costs
            .keys()
            .copied()
            .find(|&h| h != game.cities[city].pos && game.is_explored(h))
            .expect("an explored routed tile");
        game.memory.remove(&forgotten);
        assert!(!game.is_explored(forgotten));
        let fewer = count_color(&game.build_vertices(), label);
        assert!(fewer < seen, "{fewer} vs {seen}");
    }

    /// #215: a road through a tile runs where its share label sits, so the
    /// label is drawn after the road, whole on top of it.
    #[test]
    fn a_share_label_is_drawn_over_a_road_through_its_tile() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.explore();
        game.open_city(0);
        // A road through a tile beside the city, on to the hex above it.
        let center = game.cities[0].pos;
        let tile = Hex::new(center.q + 1, center.r);
        let above = Hex::new(tile.q, tile.r + 1);
        game.roads.extend([tile, above]);
        let fog = game.fog();
        let mut out = Vec::new();
        game.push_city_map(&fog, &mut out);

        // The label's box, and the triangles of each color that touch it.
        let cost = game.known_routes(0, &fog).costs[&tile];
        let text = format!("{}%", crate::game::city::delivered_share(cost) * 25);
        let min = tile.to_world() + SHARE_LABEL_OFFSET;
        let size = Vec2::new(
            font::world_text_width(&text, SHARE_LABEL_HEIGHT),
            SHARE_LABEL_HEIGHT,
        );
        let max = min + size;
        let touches = |t: &[Vertex; 3]| {
            let at = |v: &Vertex| Vec2::new(v.pos[0], v.pos[1]);
            let lo = t.iter().fold(Vec2::MAX, |m, v| m.min(at(v)));
            let hi = t.iter().fold(Vec2::MIN, |m, v| m.max(at(v)));
            lo.x < max.x && hi.x > min.x && lo.y < max.y && hi.y > min.y
        };
        let triangles = out.as_chunks::<3>().0;
        let indices = |color: Color| -> Vec<usize> {
            (0..triangles.len())
                .filter(|&i| triangles[i][0].color == color && touches(&triangles[i]))
                .collect()
        };
        let (road, label) = (indices(ROAD_COLOR), indices(SHARE_LABEL_COLOR));
        assert!(!road.is_empty(), "the road runs through the label");
        assert!(!label.is_empty(), "the tile shows its share");
        assert!(
            road.iter().max() < label.iter().min(),
            "the road's triangles {road:?} come before the label's {label:?}"
        );
    }

    #[test]
    fn a_picked_up_manager_takes_its_citizens_off_the_map() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.explore();
        game.open_city(0);
        assert!(game.cities[0].working() > 1);
        let citizen_ring = [0.25, 1.0, 0.4, 1.0];
        assert!(count_color(&game.build_vertices(), citizen_ring) > 0);
        let manager = game.cities[0].clusters[0].manager;
        game.city_click(manager);
        assert_eq!(game.moving_manager, Some((0, 0)));
        assert_eq!(count_color(&game.build_vertices(), citizen_ring), 0);
        // Cancelling puts them back.
        game.city_click(manager);
        assert!(count_color(&game.build_vertices(), citizen_ring) > 0);
    }

    #[test]
    fn only_hovering_the_manager_draws_a_line_and_it_follows_the_route_home() {
        let mut game = GameState::city_scenario();
        let city = game
            .cities
            .iter()
            .position(|c| c.team == PLAYER_TEAM)
            .unwrap();
        game.selected_city = Some(city);
        let fog = game.fog();
        let routes = game.known_routes(city, &fog);
        // A manager at least two legs from the city, so the route bends
        // through other hexes rather than being one straight hop.
        let manager = routes
            .costs
            .keys()
            .copied()
            .filter(|&h| {
                !game.grid.terrain(h).is_water()
                    && !game.cities[city].works(h)
                    && routes.path_from(h).len() >= 3
            })
            .min_by_key(|h| (h.q, h.r))
            .expect("a land tile two legs from the city");
        game.cities[city].clusters[0].manager = manager;
        assert!(game.cities[city].working() > 1, "the manager has workers");
        let path = routes.path_from(manager);
        assert_eq!(path.first(), Some(&manager));
        assert_eq!(path.last(), Some(&game.cities[city].pos));
        assert!(path.windows(2).all(|leg| {
            leg[0].distance(leg[1]) == 1 && routes.costs[&leg[1]] < routes.costs[&leg[0]]
        }));

        let city_map = |game: &GameState| {
            let mut out = Vec::new();
            game.push_city_map(&fog, &mut out);
            out
        };
        let route_vertices = |out: &[Vertex]| {
            out.iter()
                .filter(|v| v.color == MANAGER_ROUTE_COLOR)
                .map(|v| Vec2::new(v.pos[0], v.pos[1]))
                .collect::<Vec<_>>()
        };

        // Not hovering: no route line, and workers add only their rings,
        // no links to the manager.
        game.hovered_tile = None;
        let with_workers = city_map(&game);
        assert!(route_vertices(&with_workers).is_empty());
        let workers = game.cities[city].working() - 1;
        let alone = {
            let mut alone = game.clone();
            alone.cities[city].clusters[0].workers.clear();
            city_map(&alone)
        };
        let mut ring = Vec::new();
        mesh::polygon_outline(Vec2::ZERO, 1.0, 0.1, 6, 0.0, LABEL_COLOR, &mut ring);
        mesh::polygon_outline(Vec2::ZERO, 1.0, 0.1, 6, 0.0, LABEL_COLOR, &mut ring);
        assert_eq!(with_workers.len() - alone.len(), workers * ring.len());

        // Hovering another worked tile draws nothing either.
        game.hovered_tile = Some(game.cities[city].clusters[0].workers[0]);
        assert!(route_vertices(&city_map(&game)).is_empty());

        // Hovering the manager: dashes along each leg of its route and
        // nowhere else.
        game.hovered_tile = Some(manager);
        let dashes = route_vertices(&city_map(&game));
        let mut one_leg = Vec::new();
        push_dotted_segment(Vec2::ZERO, Vec2::X, 0.06, LABEL_COLOR, &mut one_leg);
        assert_eq!(dashes.len(), (path.len() - 1) * one_leg.len());
        let near_leg = |p: Vec2, a: Hex, b: Hex| {
            let (a, b) = (a.to_world(), b.to_world());
            let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            p.distance(a.lerp(b, t)) <= MANAGER_ROUTE_WIDTH
        };
        assert!(
            dashes
                .iter()
                .all(|&p| path.windows(2).any(|leg| near_leg(p, leg[0], leg[1])))
        );
        for leg in path.windows(2) {
            assert!(dashes.iter().any(|&p| near_leg(p, leg[0], leg[1])));
        }

        // Carrying the manager to a new tile hides the line.
        game.moving_manager = Some((city, 0));
        assert!(route_vertices(&city_map(&game)).is_empty());
    }

    #[test]
    fn barracks_delivery_labels_do_not_react_to_unseen_units() {
        let (mut game, city, far) = remembered_route_hex();
        // A barracks two hexes from the remembered hex: it sees only 1.
        let fog = game.fog();
        let site = game
            .grid
            .all_hexes()
            .filter(|&h| {
                h.distance(far) == 2
                    && game.site_available(city, crate::game::city::Building::Barracks, h)
            })
            .find(|&h| !fog.sees(far) && game.routes_from(PLAYER_TEAM, h).costs.contains_key(&far))
            .expect("a barracks site whose goods reach the hex");
        game.cities[city].barracks = Some(site);
        game.selected_city = None;
        game.selected_barracks = Some(city);
        assert!(!game.fog().sees(far));
        let before = scene(&game);
        game.units
            .push(Unit::new(51, far, Team::Red, UnitType::Melee));
        assert_eq!(scene(&game), before);
    }

    #[test]
    fn a_hovered_enemy_city_rings_only_its_tiles_in_sight_and_not_their_routes() {
        // The city of `worked_tile_behind_the_mountain`, now Red's, seen by
        // two Blue scouts: one west of its center, one by the tile it works
        // east of the mountain. `cut` puts Green, out of their sight, on
        // both hexes that start the road home, so no goods arrive.
        let drawn = |worked: Hex, cut: bool| {
            let (mut game, city, _) = worked_tile_behind_the_mountain();
            game.cities[city].team = Team::Red;
            game.cities[city].clusters = crate::game::city::one_cluster(&[worked]);
            game.cities[city].remembered = game.cities[city].clusters.clone();
            for (id, pos) in [(50, Hex::new(-3, 0)), (51, Hex::new(6, 1))] {
                game.units
                    .push(Unit::new(id, pos, Team::Blue, UnitType::Scout));
            }
            let blockers = [Hex::new(1, 0), Hex::new(1, -1)];
            if cut {
                for (id, pos) in [(52, blockers[0]), (53, blockers[1])] {
                    game.units
                        .push(Unit::new(id, pos, Team::Green, UnitType::Melee));
                }
            }
            game.explore();
            let fog = game.fog();
            assert!(fog.sees(game.cities[city].pos));
            assert!(blockers.iter().all(|&h| !fog.sees(h)));
            let arrives = game.routes(city).costs.contains_key(&worked);
            assert!(arrives != cut || !fog.sees(worked));
            game.hovered_city = Some(city);
            game.hovered_tile = Some(game.cities[city].pos);
            (fog.sees(worked), scene(&game))
        };
        let (seen, in_sight) = drawn(Hex::new(5, 0), false);
        assert!(seen);
        assert!(drawn(Hex::new(5, 0), true).1 == in_sight, "its routes");

        let (seen, out_of_sight) = drawn(Hex::new(0, 5), false);
        assert!(!seen && out_of_sight != in_sight);
        assert!(
            drawn(Hex::new(0, -5), false).1 == out_of_sight,
            "which tile"
        );
        assert!(drawn(Hex::new(0, 5), true).1 == out_of_sight);
    }
}
