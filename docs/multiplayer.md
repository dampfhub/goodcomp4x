# Multiplayer

Two to seven people play one game over the network: one hosts, the others join. They share a
newly generated world, each on their own side, and the AI plays the world's other sides. This
is the first cut, built to test multiplayer features.

## Playing

From the game: Escape opens the settings menu, and its **Multiplayer** button the page to host
or join from (`src/game/ui/network_menu.rs`). To host, pick how many people play (you
included, 2-7) and the port (7777 unless changed), then **Host Game**. To join, type the
host's address (`HOST`, or `HOST:PORT` for another port than 7777) and the join code it
shows, then **Join Game**; the page says how joining goes, or why it couldn't. The port,
address and player count are kept for next time. In a network game the page shows your side
and, on the host, the join code and the seats still open, and **Leave Game** leaves it for a
new world of your own (the host leaving ends the game for everyone).

The same from the command line:

```
vulkan_engine --host --players 3    # 2-7 people, you included (2 unless given)
                                    # listens on port 7777; --port N for another
vulkan_engine --join 192.168.1.20 --code K7M2QX    # HOST or HOST:PORT
```

The world is made as the F4 world is, from the host's settings (AI Players, Start With), with
at least a side for every player. The host plays Blue; guests take Red, Green and the sides
after as they join. The End Turn button (and the log) shows the **join code** and how many are
still to come, like `JOIN CODE K7M2QX - 2 TO COME`, until every seat is filled. Guests need
to reach the host's port: on a home network, forward it on the router and let the game through
the host's firewall.

Everyone plays their turn at once, as ever; the first turn waits until every seat is filled.
Ending the turn sends your plan, and the End Turn button waits (the host's names who it's
waiting for) until everyone has ended theirs; then the turn plays out on every machine. While
you wait you can look around as you like: select units and cities, open city, Barracks and
interior views, and read their panels and tooltips; everything that would change your orders
is disabled, since they're sent. To change them, click the End Turn button while it waits (TAKE
BACK): you plan on, and end the turn again. That works until the host has everyone's plan; a
take-back that reaches it later is too late, and the turn plays out with the orders you sent
(the top bar says so). Nobody can join once the first turn has played. A guest who leaves before it starts to play out frees
their seat for someone else (the turn waits for them); one who leaves after hands their side to
the AI, which plays it from the next turn on, and the rest play on. If the host leaves, the
game can't go on. A guest the host drops for a message it refused sees why. Debug actions that
change the game on one machine only (F1-F4 and the scenario buttons, F6/F7, F9, PROD SPEEDUP,
UNIT CAP) are off in a network game.

## How it works: lockstep

Every machine runs the whole game (`src/game/multiplayer.rs`). Turns are simultaneous, so a
player's planning stays on their machine until they end it. Then their side's **plan**
(`TeamPlan`) goes to the host: their units' orders and queues, their cities' queues, citizens
and priority order, their placed jobs and recalled workers, their troops' orders inside city interiors,
and their stockpile; a unit missing from it was disbanded, and a city new since the turn began
was founded by the settler that stood there. A city founded too near one another side's plan
founded earlier in side order (each was checked only against the turn's start) isn't founded,
and its settler stays. Once the host has every human side's plan, it
sends them all to every guest (`Resolve`). Each machine applies them, in side order, to a copy of
the game as it stood when the turn's planning began (`turn_start`), keeps its own view (camera,
fog memory, settings), and resolves the turn. The same plans on the same game resolve the same
way: the simulation is deterministic, and the host sends the world's seed and settings and the
RNG seed when a guest joins. The AI sides plan on what each has seen (`side_fog`, `fog.rs`),
which is part of the game, never of a machine's own view, so every machine plans them alike.
After every turn each guest sends a checksum of the game
(`GameState::checksum`, which covers how much each side remembers); a mismatch shows DESYNC WITH RED AFTER TURN N on the host. When a guest
leaves mid-game the host sends `SeatLeft` before the turn's `Resolve`, so every machine hands
that side to the AI at the same point.

Only planning may differ between machines, and only in what the plan carries. Anything else a
player's machine does must leave the game alone, or the plans it makes stop matching the game
the others apply them to. So in a network game, troops beside a city stand in its interior from
the start of each turn, on every machine (`begin_lockstep_turn`), rather than when a player
first looks inside, as in a game of one's own. (An early build added them on looking, and the
host refused the orders a guest then gave them.) A slow test lets the AI play every seat for
many turns, looking inside every city, and checks the host takes every plan and every machine
plays the same game: `cargo test plans_the_ai_makes -- --ignored`.

Messages (`Message`): `Hello` (guest, with the protocol version), `Welcome` (host: the seat,
the human sides, the world's seed and settings, the RNG seed and debug toggles) or `Refused`,
`Plan`, `Resolve`, `SeatLeft`, `Checksum`, `Withdraw` (guest: its player took back ending the
turn). The host also sends `Refused`, with the reason, to a guest it drops for a bad message.
`PROTOCOL_VERSION` changes whenever one changes shape, or the rules a turn plays out by, or the
map a seed generates (`mapgen.rs`: each machine builds the world from the seed), so
mismatched builds refuse each other.

Taking a turn back (`take_back_turn`): a guest sends `Withdraw { turn }` and plans on; the host
drops that side's plan and waits for its next `Plan`. The host takes its own back locally. The
host alone decides the order of events: once it has every plan it resolves at once, so a
`Withdraw` still on its way then has lost the race. The host ignores it, and the `Plan` the
guest sends after it (`came_too_late`), and the guest, when the `Resolve` comes, plays the turn
out with the plan the host had, as every machine does.

## Transport

`src/net/`: TCP, each message encoded with postcard, sealed (below), and framed as a
little-endian `u32` length and the sealed bytes. A reader thread per connection opens incoming
messages onto a bounded channel; the frame loop polls it (`Session::pump`) and never waits on
the network. The guest connects, runs the handshake and receives the game before its window
opens; the host runs each join on its own thread. The host takes nothing from guests while a
turn plays out (`GameState::takes_messages`): a guest that played it out sooner may already
send its next plan, which waits in the queue until the host's next turn begins and there's a
game to check it against.

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
  expects, in order; a plan only for the sender's own side and this turn, and only one at a
  time (a second needs a `Withdraw` between); a `Withdraw` only of a plan the host holds for
  this turn; for a turn already resolved, a late `Withdraw` and `Plan` only alternately (a
  `Withdraw` first) until that guest's checksum for it, and never changing anything; every
  unit, city, worker and interior troop a plan names existing and its side's; every hex on the
  map (or inside the city's interior); moves and attacks within the unit's range; the alert
  stance only on a troop that can take it (not a settler, scout or ship; siege set up); an ability
  only when it's ready; every list short (`MAX_PLAN_LIST`). The guest checks the host's
  `Resolve` the same way, and that it carries a plan the guest sent for its own side, and the
  seat and scenario in its `Welcome`.
- **Cheats the checks catch**: spending is accounted exactly (the stockpile plus every paid
  queue item and placed job must be worth what it was when the turn began, so nothing is free);
  planning pays for no queued item, since a build is paid as the turn's economy starts work on
  it, the same on every machine (`work_queues`), so a plan's queues hold unpaid items with no
  work and the paid items the city had, each once, with the work it had (no item marked paid,
  none copied, none refunded without being taken off); the work kept on a worker job can't be
  added (or copied onto another job), only kept or cleared; a city queues only what it can train (no Cavalry
  or Armored, ships only with a Harbor, no growing past the cap, one Scout at a time, and no new
  Settler below population 3), and founds only where the founding rules allow (open land, no
  ruins, 6 hexes from every city, those it founds that turn included), a Barracks no more Cavalry or
  Armored than its deposits allow; citizens work only tiles in their city's reach, never a city
  or a building; workers out on the map can only be recalled, and workers held at home only
  released.
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

A lobby to wait in before the game, a turn timer, rejoining after a drop, and
other scenarios.
