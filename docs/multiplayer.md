# Multiplayer

Two people play one game over the network: one hosts, the other joins. This is the first cut,
built to test multiplayer features; it plays the Cities scenario with the host as Blue and the
guest as Red.

## Playing

On the host:

```
vulkan_engine --host            # listens on port 7777; --port N for another
```

The End Turn button (and the log) shows a **join code**, like `JOIN CODE K7M2QX`, until someone joins. On the
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

`src/net/`: TCP, each message encoded with postcard, sealed (below), and framed as a
little-endian `u32` length and the sealed bytes. A reader thread per connection opens incoming
messages onto a bounded channel; the frame loop polls it (`Session::pump`) and never waits on
the network. The guest connects, runs the handshake and receives the game before its window
opens; the host runs each join on its own thread.

## Security

Opening a port means anyone who can reach it can send anything, and anyone on the network path
can watch or change what's sent. What's in place:

- **Encrypted and authenticated by the join code** (`src/net/secure.rs`). Both ends first send
  a magic number and their protocol version in the clear, so another program or build stops
  there. Then they run **SPAKE2** (a password-authenticated key exchange, RustCrypto's `spake2`
  on Ed25519) with the join code: both get the same strong key only if both used the same code,
  and someone watching learns nothing to guess the code with offline, since every guess takes a
  live join at the host. HKDF-SHA256 derives a key per direction, and every message is sealed
  with **ChaCha20-Poly1305** under a nonce counting that direction's messages, so a message that's
  read, changed, replayed, reordered or dropped in transit is either unreadable or fails to open,
  which drops the connection. Keys are new every game (the exchange is ephemeral).
- **Guessing the code doesn't work.** The code is six letters and digits (about a billion codes,
  from the OS's random source). A wrong code shows when the guest's first sealed message won't
  open; after 10 of those the host stops listening, so a guesser gets 10 tries in a billion.
- **Joining can't be blocked by one machine.** The host runs up to 8 joins at once, at most 3 from
  any one address, each on its own thread with 10 seconds to finish; a silent or garbage
  connection holds up nobody, and one address can't fill every slot.
- **Nothing arriving is trusted.** A frame over 1 MiB drops the peer before it's read, and so does
  a message that doesn't open or doesn't decode. The incoming queue is bounded. Every message is
  checked before it touches the game (`GameState::receive`): only the messages its role
  expects; a plan only for the sender's own side and this turn; every unit, city, worker and
  interior troop it names existing and its side's; every hex on the map (or inside the city's
  interior); moves and attacks within the unit's range; an ability only when it's ready; every
  list short (`MAX_PLAN_LIST`). The guest checks the host's `Resolve` the same way, and the
  seat and scenario in its `Welcome`.
- **Cheats the checks catch**: spending is accounted exactly (the stockpile plus everything
  queued and placed must be worth what it was when the turn began, so nothing is free); build
  progress can't be added, only kept or cleared; a city queues only what it can train (no Cavalry
  or Armored, ships only with a Harbor, no growing past the cap), a Barracks no more Cavalry or
  Armored than its deposits allow; citizens work only tiles in their city's reach, never a city
  or a building; workers out on the map can only be recalled.
- **No panics from input.** A randomized test throws thousands of hostile plans at the checks,
  and applies and resolves every one that passes, without a crash; others throw garbage, huge
  frames, early messages, wrong codes and floods of connections at the transport
  (`src/net/`, `multiplayer.rs`).

What's left, knowingly:

- **The host listens on every network interface** (0.0.0.0), so a LAN (or, with the port
  forwarded, the internet) reaches it; the OS firewall may ask the first time. With the join
  code guarding the game that's safe to allow, but only forward the port if you need to.
- **Some cheats are out of reach of any check**: a modified client can see through its own fog
  (every machine holds the whole game in lockstep), and can plan anything a real player could.
- **Many machines at once** could still fill the host's 8 join slots for a while; a real guest
  can retry.
- The join code shows in the host's log.

## Next

More than two players (and AI sides beside them), other scenarios and worlds, a lobby in the game
instead of command-line flags, and rejoining after a drop.
