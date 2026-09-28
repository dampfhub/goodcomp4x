# Multiplayer

Two people play one game over the network: one hosts, the other joins. This is the first cut,
built to test multiplayer features; it plays the Cities scenario with the host as Blue and the
guest as Red.

## Playing

On the host:

```
vulkan_engine --host            # listens on port 7777; --port N for another
```

The top bar (and the log) shows a **join code**, like `HOSTING - JOIN CODE K7M2QX`. On the
other machine:

```
vulkan_engine --join 192.168.1.20 --code K7M2QX    # HOST or HOST:PORT
```

Both play their turn at once, as ever. Ending the turn sends your plan; the End Turn button
then reads WAITING FOR RED (or BLUE) until the other player ends theirs, and the turn plays out
on both machines. If the other player leaves, the top bar says so and the game can't go on.
Debug actions that change the game on one machine only (F1-F4 and the scenario buttons, F6/F7,
F9, PROD SPEEDUP, UNIT CAP) are off in a network game.

## How it works: lockstep

Every machine runs the whole game (`src/game/multiplayer.rs`). Turns are simultaneous, so a
player's planning stays on their machine until they end it. Then their side's **plan**
(`TeamPlan`) goes to the host: their units' orders and queues, their cities' queues, citizens
and focus, their placed jobs and recalled workers, their troops' orders inside city interiors,
and their stockpile; a unit missing from it was disbanded, and a city new since the turn began
was founded by the settler that stood there. Once the host has every human side's plan, it
sends them all to the guest (`Resolve`). Each machine applies them, in side order, to a copy of
the game as it stood when the turn's planning began (`turn_start`), keeps its own view (camera,
fog memory, settings), and resolves the turn. The same plans on the same game resolve the same
way: the simulation is deterministic, and the host sends the RNG seed when the guest joins. After
every turn the guest sends a checksum of the game (`GameState::checksum`); a mismatch shows
DESYNC AFTER TURN N.

Messages (`Message`): `Hello` (guest, with the protocol version and join code), `Welcome` (host:
the seat, scenario, RNG seed and debug toggles) or `Refused`, `Plan`, `Resolve`, `Checksum`.
`PROTOCOL_VERSION` changes whenever one changes shape, so mismatched builds refuse each other.

## Transport

`src/net.rs`: TCP, each message a little-endian `u32` length and its bincode encoding. A reader
thread per connection feeds a channel; the frame loop polls it (`Session::pump`) and never waits
on the network. The guest connects and receives the game before its window opens.

## Security

Opening a port means anyone who can reach it can send anything. What's in place:

- **Nothing arriving is trusted.** A frame over 1 MiB, or one that doesn't decode (bincode with
  the same limit, whatever lengths a message claims inside), drops the peer. Every message is
  checked before it touches the game (`GameState::receive`): only the messages its role expects,
  in order (nothing before `Hello`); a plan only for the sender's own side and this turn; every
  unit, city, worker and interior troop it names existing and its side's; every hex on the map
  (or inside the city's interior); moves and attacks within the unit's range; every list short
  (`MAX_PLAN_LIST`); a stockpile no larger than the side had plus what its queues could refund. A
  bad message drops the peer, and nothing of it reaches the game. The guest checks the host's
  `Resolve` the same way, and the seat and scenario in its `Welcome`.
- **Joining takes the join code**, six letters and digits (about a billion codes). The host takes
  one guest; a connection that doesn't say hello within 10 seconds is dropped, and after 10
  refused guests the host stops listening, so a code can't be guessed at.
- **No panics from input**: the checks stop what would index out of range or leave the map, so a
  hostile peer can't crash the other machine by what it sends. The tests throw garbage, huge
  frames, early messages and hostile plans at it (`net.rs`, `multiplayer.rs`).

What isn't, yet:

- **No encryption or authentication beyond the code.** Anyone who can watch the traffic can read
  it, and change it. Play on networks you trust (a LAN, or a VPN like Tailscale), and don't
  forward the port to the internet.
- **The host listens on every network interface** (0.0.0.0), so a LAN or a public address reaches
  it; the OS firewall may ask the first time.
- **Cheating isn't fully prevented.** The checks bound what a plan can do, but a modified client
  could still, say, queue what it can't build yet, or plan on what its fog hides.
- A client that connects and stays silent holds the host for up to 10 seconds at a time.

## Next

More than two players (and AI sides beside them), other scenarios and worlds, a lobby in the game
instead of command-line flags, rejoining after a drop, and encryption.
