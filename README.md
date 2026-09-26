# riskofcivlike

A prototype for experimenting with combat and city rules for a Civ-like game, written in Rust on
raw Vulkan. You play Blue against an AI-controlled Red on a small hex map. Turns are
simultaneous: both sides queue orders, then the turn resolves step by step in a fixed order set
by unit type.

## Running it

Requirements: Rust 1.92 or newer, a Vulkan driver, and
`glslc` for compiling shaders (it ships with the [Vulkan SDK](https://vulkan.lunarg.com/); the
build looks in `$VULKAN_SDK/bin`, then `PATH`).

```
cargo run --release     # play
cargo run               # debug build with the Vulkan validation layer (from the SDK)
cargo test              # unit tests; no GPU needed
```

The city scenario opens by default; F1-F4 switch between the combat, city, frontier and
generated-world scenarios. See [docs/controls.md](docs/controls.md) for every control.

## Documentation

- [docs/README.md](docs/README.md): index of everything under `docs/`
- [docs/game-rules.md](docs/game-rules.md): the rules as implemented
- [docs/architecture.md](docs/architecture.md): how the code is organized
- [AGENTS.md](AGENTS.md): instructions for coding agents working in this repo

Work is tracked on the [project board](https://github.com/users/dampfhub/projects/1).
