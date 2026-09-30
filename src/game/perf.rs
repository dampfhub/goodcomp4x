//! Tests only: a timing report of the frame's and the turn's stages on a busy
//! generated world. Ignored by default; run it optimized:
//! `cargo test --release perf_report -- --ignored --nocapture`.
use std::time::{Duration, Instant};

use glam::Vec2;

use super::scenario::Scenario;
use super::ui::ImGuiLayoutState;
use super::*;

const SCREEN: Vec2 = Vec2::new(1600.0, 900.0);

/// Reproduce F4, F10, then Alt, at home and at maximum zoom-out.
#[test]
#[ignore]
fn world_overlay_report() {
    let mut game = GameState::world_scenario(3);
    game.fog_of_war = false;
    let mut buffer = Vec::new();
    for half in [game.camera.half_height, super::camera::MAX_HALF_HEIGHT] {
        game.camera.half_height = half;
        for details in [false, true] {
            game.set_details(details);
            let elapsed = time(50, || game.build_vertices_into(&mut buffer));
            println!(
                "half_height={half} Alt={details}: {} vertices, {:.3} ms",
                buffer.len(),
                elapsed.as_secs_f64() * 1000.0
            );
        }
    }
}

/// Mean time of `f` over `runs` calls.
fn time(runs: u32, mut f: impl FnMut()) -> Duration {
    f();
    let start = Instant::now();
    for _ in 0..runs {
        f();
    }
    start.elapsed() / runs
}

fn report(name: &str, took: Duration) {
    println!("{name:<40} {:>10.3} ms", took.as_secs_f64() * 1000.0);
}

/// A world from `seed` played `turns` turns by the AI on every side.
fn busy_world(seed: u64, turns: u32) -> GameState {
    let mut game = GameState::new();
    game.settings.instant_playback = true;
    game.settings.world_ai = 6;
    game.seed_rng(seed);
    game.switch_scenario(Scenario::World);
    for _ in 0..turns {
        play_turn(&mut game);
    }
    game
}

fn play_turn(game: &mut GameState) {
    game.selected = None;
    game.group.clear();
    game.plan_ai_turn(PLAYER_TEAM);
    game.resolve_turn();
    game.update(0.0);
}

fn frame_report(game: &mut GameState, label: &str) {
    println!("-- frame: {label}");
    let cursor = Some(SCREEN * 0.5);
    report("fog()", time(50, || drop(game.fog())));
    report(
        "update_hover_imgui",
        time(50, || game.update_hover_imgui(cursor, SCREEN, 0.006)),
    );
    report("animate_clouds", time(50, || game.animate_clouds(0.006)));
    if let Some(city) = game.selected_city {
        let fog = game.fog();
        report(
            "known_routes(selected city)",
            time(50, || drop(game.known_routes(city, &fog))),
        );
        report(
            "routes(selected city)",
            time(50, || drop(game.routes(city))),
        );
        println!(
            "  show_yields {} show_details {}",
            game.show_yields, game.show_details
        );
    }
    let count = game.build_vertices().len();
    let mut buffer = Vec::new();
    report(
        "build_vertices_into (kept buffer)",
        time(20, || game.build_vertices_into(&mut buffer)),
    );
    println!("{:<40} {count:>10} vertices", "  world");
    report(
        "build_ui (classic)",
        time(20, || drop(game.build_ui(SCREEN, cursor))),
    );
    let mut context = ::imgui::Context::create();
    context.io_mut().display_size = SCREEN.to_array();
    context
        .io_mut()
        .config_flags
        .insert(::imgui::ConfigFlags::DOCKING_ENABLE);
    let font = context
        .fonts()
        .add_font(&[::imgui::FontSource::DefaultFontData { config: None }]);
    context.fonts().build_rgba32_texture();
    let fonts = [font, font, font];
    let mut layout = ImGuiLayoutState::default();
    report(
        "draw_imgui + render",
        time(20, || {
            context.io_mut().delta_time = 0.006;
            let ui = context.frame();
            game.draw_imgui(ui, SCREEN, cursor, &fonts, &mut layout);
            context.render();
        }),
    );
}

#[test]
#[ignore]
fn perf_report() {
    let start = Instant::now();
    let mut game = busy_world(2, 0);
    report("world setup", start.elapsed());
    frame_report(&mut game, "turn 0, home");

    let mut turns = Duration::ZERO;
    for _ in 0..40 {
        let start = Instant::now();
        play_turn(&mut game);
        turns += start.elapsed();
    }
    report("turn (mean of 40, AI on every side)", turns / 40);
    println!(
        "units {}, cities {}, workers {}",
        game.units.len(),
        game.cities.len(),
        game.field_workers.len()
    );
    report(
        "plan_ai_turn (one side)",
        time(10, || game.clone().plan_ai_turn(Team::Red)),
    );
    // No time has passed since the last turn: its transition is playing.
    frame_report(&mut game, "turn 40, home, turn transition playing");
    game.age_transition(1.0);
    frame_report(&mut game, "turn 40, home");
    game.camera.half_height = super::camera::MAX_HALF_HEIGHT;
    frame_report(&mut game, "turn 40, zoomed all the way out");
}
