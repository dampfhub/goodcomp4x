//! Command-line flags (`cargo run -- --help` lists them).

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};

use crate::game::Scenario;

pub const USAGE: &str = "\
Usage: vulkan_engine [OPTIONS]

Options:
  --scenario <NAME>    start in this scenario: combat, cities (the default),
                       frontier, world (a new random map), siege, or naval
  --seed <N>           generate the world scenario's map from seed N (the
                       debug panel shows a map's seed), so it's the same
                       map every run
  --screenshot <FILE>  render the scenario in a hidden window, write one frame
                       to FILE as a PNG, and exit
  --size <WxH>         window size in pixels, e.g. 1280x720 (screenshots
                       default to 1600x900)
  --host               host a two-player game (the Cities scenario) and
                       wait for a player to join; you play Blue
  --port <N>           the port --host listens on (default 7777)
  --join <ADDRESS>     join a hosted game at HOST or HOST:PORT; you play Red
  --code <CODE>        the join code the host shows (needed with --join)
  -h, --help           print this and exit";

/// What the command line asked for.
#[derive(Debug, PartialEq)]
pub struct Options {
    pub scenario: Scenario,
    /// The world scenario's map seed, if given.
    pub seed: Option<u32>,
    /// Screenshot mode: where to write the PNG.
    pub screenshot: Option<PathBuf>,
    /// Window size in physical pixels, if given.
    pub size: Option<(u32, u32)>,
    /// A multiplayer game to host or join.
    pub network: Option<Network>,
    pub help: bool,
}

/// `--host` (with `--port`) or `--join`.
#[derive(Debug, PartialEq)]
pub enum Network {
    Host(u16),
    /// The host's address, and its join code.
    Join(String, String),
}

impl Default for Options {
    fn default() -> Self {
        Self {
            scenario: Scenario::Cities,
            seed: None,
            screenshot: None,
            size: None,
            network: None,
            help: false,
        }
    }
}

impl Options {
    /// Parses the arguments after the program name.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut options = Self::default();
        let (mut host, mut port, mut join, mut code) = (false, None, None, None);
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let mut value = || {
                args.next()
                    .ok_or_else(|| anyhow!("{arg} needs a value\n\n{USAGE}"))
            };
            match arg.as_str() {
                "--scenario" => {
                    let name = value()?;
                    options.scenario = Scenario::from_name(&name).with_context(|| {
                        format!(
                            "no scenario called {name:?}: try combat, cities, frontier, world, siege or naval"
                        )
                    })?;
                }
                "--seed" => {
                    let seed = value()?;
                    options.seed = Some(seed.parse().with_context(|| {
                        format!(
                            "--seed wants a whole number up to {}, not {seed:?}",
                            u32::MAX
                        )
                    })?);
                }
                "--screenshot" => options.screenshot = Some(value()?.into()),
                "--size" => options.size = Some(parse_size(&value()?)?),
                "--host" => host = true,
                "--port" => {
                    let text = value()?;
                    port =
                        Some(text.parse().with_context(|| {
                            format!("--port wants a port number, not {text:?}")
                        })?);
                }
                "--join" => join = Some(value()?),
                "--code" => code = Some(value()?),
                "-h" | "--help" => options.help = true,
                _ => bail!("unknown argument {arg:?}\n\n{USAGE}"),
            }
        }
        if options.seed.is_some() && options.scenario != Scenario::World {
            bail!("--seed only applies to --scenario world");
        }
        options.network = match (host, join) {
            (true, Some(_)) => bail!("--host and --join can't go together"),
            (true, None) if code.is_some() => bail!("--code only applies to --join"),
            (true, None) => Some(Network::Host(port.unwrap_or(crate::net::DEFAULT_PORT))),
            (false, Some(address)) => {
                let code = code.context("--join needs --code: the join code the host shows")?;
                Some(Network::Join(address, code))
            }
            (false, None) if code.is_some() => bail!("--code only applies to --join"),
            (false, None) if port.is_some() => bail!("--port only applies to --host"),
            (false, None) => None,
        };
        if options.network.is_some() && options.screenshot.is_some() {
            bail!("a screenshot can't be taken of a network game");
        }
        Ok(options)
    }
}

/// `1280x720` as (1280, 720).
fn parse_size(text: &str) -> Result<(u32, u32)> {
    let parsed = text
        .split_once(['x', 'X'])
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .filter(|&(w, h): &(u32, u32)| w > 0 && h > 0);
    parsed.with_context(|| format!("--size wants WIDTHxHEIGHT, like 1280x720, not {text:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options> {
        Options::parse(args.iter().map(|arg| arg.to_string()))
    }

    #[test]
    fn host_and_join_take_a_port_and_a_code() {
        assert_eq!(
            parse(&["--host"]).unwrap().network,
            Some(Network::Host(crate::net::DEFAULT_PORT))
        );
        assert_eq!(
            parse(&["--host", "--port", "9000"]).unwrap().network,
            Some(Network::Host(9000))
        );
        assert_eq!(
            parse(&["--join", "10.0.0.2", "--code", "ABC123"])
                .unwrap()
                .network,
            Some(Network::Join("10.0.0.2".into(), "ABC123".into()))
        );
        for bad in [
            &["--join", "10.0.0.2"][..],
            &["--host", "--join", "x", "--code", "y"],
            &["--port", "9000"],
            &["--host", "--code", "y"],
            &["--host", "--port", "lots"],
            &["--host", "--screenshot", "x.png"],
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn no_arguments_start_the_city_scenario() {
        assert_eq!(parse(&[]).unwrap(), Options::default());
        assert_eq!(Options::default().scenario, Scenario::Cities);
    }

    #[test]
    fn the_screenshot_command_parses() {
        let options = parse(&[
            "--screenshot",
            "out.png",
            "--scenario",
            "frontier",
            "--size",
            "1280x720",
        ])
        .unwrap();
        assert_eq!(options.screenshot, Some(PathBuf::from("out.png")));
        assert_eq!(options.scenario, Scenario::Frontier);
        assert_eq!(options.size, Some((1280, 720)));
        assert!(!options.help);

        let world = parse(&["--seed", "42", "--scenario", "world"]).unwrap();
        assert_eq!((world.scenario, world.seed), (Scenario::World, Some(42)));
    }

    #[test]
    fn bad_arguments_are_errors() {
        assert!(parse(&["--scenario", "moon"]).is_err());
        assert!(parse(&["--screenshot"]).is_err());
        assert!(parse(&["--size", "1280"]).is_err());
        assert!(parse(&["--size", "0x720"]).is_err());
        assert!(parse(&["--frobnicate"]).is_err());
        assert!(parse(&["--scenario", "world", "--seed", "-1"]).is_err());
        assert!(
            parse(&["--seed", "42"]).is_err(),
            "the default scenario has no seed"
        );
        assert!(parse(&["--help"]).unwrap().help);
    }
}
