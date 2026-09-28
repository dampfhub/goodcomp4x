mod app;
mod cli;
mod game;
mod icon;
mod icon_art;
mod net;
mod persist;
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

    // A network game is set up before the window opens: joining waits for
    // the host's game.
    let network = match &options.network {
        Some(cli::Network::Host(port)) => Some(net::Session::host(*port)?),
        Some(cli::Network::Join(address, code)) => Some(net::Session::join(address, code)?),
        None => None,
    };
    icon::claim_taskbar_identity();
    let event_loop = EventLoop::new()?;
    let mut app = app::App::new(options, network);
    event_loop.run_app(&mut app)?;
    app.into_result()
}
