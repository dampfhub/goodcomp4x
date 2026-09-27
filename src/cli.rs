//! Command-line flags (`cargo run -- --help` lists them).

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};

use crate::game::Scenario;

pub const USAGE: &str = "\
Usage: vulkan_engine [OPTIONS]

Options:
  --scenario <NAME>    start in this scenario: combat, cities (the default),
                       frontier or world (a new random map)
  --seed <N>           generate the world scenario's map from seed N (the
                       debug panel shows a map's seed), so it's the same
                       map every run
  --screenshot <FILE>  render the scenario in a hidden window, write one frame
                       to FILE as a PNG, and exit
  --size <WxH>         window size in pixels, e.g. 1280x720 (screenshots
                       default to 1600x900)
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
    pub help: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            scenario: Scenario::Cities,
            seed: None,
            screenshot: None,
            size: None,
            help: false,
        }
    }
}

impl Options {
    /// Parses the arguments after the program name.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut options = Self::default();
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
                            "no scenario called {name:?}: try combat, cities, frontier or world"
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
                "-h" | "--help" => options.help = true,
                _ => bail!("unknown argument {arg:?}\n\n{USAGE}"),
            }
        }
        if options.seed.is_some() && options.scenario != Scenario::World {
            bail!("--seed only applies to --scenario world");
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
