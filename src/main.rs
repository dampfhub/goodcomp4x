mod app;
mod cli;
mod game;
mod icon;
mod renderer;
mod screenshot;

use winit::event_loop::EventLoop;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let options = cli::Options::parse(std::env::args().skip(1))?;
    if options.help {
        println!("{}", cli::USAGE);
        return Ok(());
    }

    let event_loop = EventLoop::new()?;
    let mut app = app::App::new(options);
    event_loop.run_app(&mut app)?;
    app.into_result()
}
