# Multiplayer

Two to seven people play one game over the network: one hosts, the others join. They share a
newly generated world, each on their own side, and the AI plays the world's other sides. This
is the first cut, built to test multiplayer features.

## Playing

On the host:

```
vulkan_engine --host --players 3    # 2-7 people, you included (2 unless given)
                                    # listens on port 7777; --port N for another
```

The world is made as the F4 world is, from the host's settings (AI Players, Start With), with
at least a side for every player. The host plays Blue; guests take Red, Green and the sides
after as they join. The End Turn button (and the log) shows a **join code** and how many are
still to come, like `JOIN CODE K7M2QX - 2 TO COME`, until every seat is filled. On each other
machine:

```
vulkan_engine --join 192.168.1.20 --code K7M2QX    # HOST or HOST:PORT
```

Everyone plays their turn at once, as ever; the first turn waits until every seat is filled.
Ending the turn sends your plan, and the End Turn button waits (the host's names who it's
waiting for) until everyone has ended theirs; then the turn plays out on every machine. Nobody
can join once the first turn has played. A guest who leaves before then frees their seat for
someone else; one who leaves after hands their side to the AI, which plays it from the next
turn on, and the rest play on. If the host leaves, the game can't go on. Debug actions that
change the game on one machine only (F1-F4 and the scenario buttons, F6/F7, F9, PROD SPEEDUP,
UNIT CAP) are off in a network game.

## How it works: lockstep

Every machine runs the whole game (`src/game/multiplayer.rs`). Turns are simultaneous, so a
player's planning stays on their machine until they end it. Then their side's **plan**
(`TeamPlan`) goes to the host: their units' orders and queues, their cities' queues, citizens
and focus, their placed jobs and recalled workers, their troops' orders inside city interiors,
and their stockpile; a unit missing from it was disbanded, and a city new since the turn began
was founded by the settler that stood there. Once the host has every human side's plan, it
sends them all to every guest (`Resolve`). Each machine applies them, in side order, to a copy of
the game as it stood when the turn's planning began (`turn_start`), keeps its own view (camera,
fog memory, settings), and resolves the turn. The same plans on the same game resolve the same
way: the simulation is deterministic, and the host sends the world's seed and settings and the
RNG seed when a guest joins. After every turn each guest sends a checksum of the game
(`GameState::checksum`); a mismatch shows DESYNC WITH RED AFTER TURN N on the host. When a guest
leaves mid-game the host sends `SeatLeft` before the turn's `Resolve`, so every machine hands
that side to the AI at the same point.

Messages (`Message`): `Hello` (guest, with the protocol version), `Welcome` (host: the seat,
the human sides, the world's seed and settings, the RNG seed and debug toggles) or `Refused`,
`Plan`, `Resolve`, `SeatLeft`, `Checksum`.
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
- **Seats are the host's to give.** A guest's messages are tied to its seat: a plan for any other
  side drops it. A guest checks the host's `Welcome` (the host first among the human sides, its
  own seat among them, a world with a side for each) and every `SeatLeft` (only another guest,
  and only once).
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
- **A player who never ends their turn holds everyone up**: there's no turn timer yet, and the
  host can't hand a connected player's side to the AI.
- The join code shows in the host's log.

## Next

A lobby in the game instead of command-line flags, a turn timer, rejoining after a drop, and
other scenarios.
