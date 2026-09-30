# docs/

| File | What it answers | Status |
|---|---|---|
| [architecture.md](architecture.md) | How the code is layered, what happens in a frame and in a turn | current |
| [game-rules.md](game-rules.md) | What the game does: map, units, orders, resolution, combat, cities, AI | current; update with every behavior change |
| [controls.md](controls.md) | Every key and mouse action, for players | current; update with every input change |
| [ui-system.md](ui-system.md) | How screen-space panels are built, docked and hit-tested | current; read before adding UI |
| [text.md](text.md) | The game's text files (`text/`): the format, placeholders and icons, and the tests that check them | current; stages 1-3 of #341 (the menus, and the status bar, top bar, Debug panel and window titles) |
| [city-system.md](city-system.md) | The design direction for cities | proposal; parts are implemented, see game-rules.md |
| [multiplayer.md](multiplayer.md) | Network play: hosting and joining, lockstep, the transport, and its security | first cut; 2-7 players on a generated world, AI on the rest |
| [rts-economy.md](rts-economy.md) | The stockpile economy experiment: model, numbers, findings, next tests | experiment; built on `claude/rts-economy`, rules in game-rules.md |
| [economy/round6-tempo-report.md](economy/round6-tempo-report.md) | The Round 6 tempo report (#239): is the game too fast, and the tuning options measured; with [the rules then](economy/round6-how-it-works.md) and [every knob](economy/round6-knobs.md) | record of `main` at 6f5c4c8; option B was chosen (rts-economy.md, Round 7) |
| [economy/supply-237-tempo.md](economy/supply-237-tempo.md) | What the supply limit (#237) does to the tempo, and why its values | record of `main` at 5663dbc with the limit |
| [history.md](history.md) | How the prototype got here | append an entry per milestone |

Agent instructions are not here: they live in `AGENTS.md` at the repo root and in `src/`,
`src/game/` and `src/renderer/`.
