# City system: first design draft

Status: proposal for discussion, not implemented. All numbers below are starting
points for playtesting. Branch: `codex/city-system`.

## Direction

Keep the city-centered loop: population works tiles, food grows the city,
production builds things, science unlocks better options, and strategic deposits
support particular military units. Leave luxuries out of the first version.

Replace fixed territorial borders with access through the map. A city owns its
center and its improvements; it does not own every tile around them. Resources
reach the city along usable routes. Nearby land is naturally useful, roads make
farther land useful, and military occupation can interrupt those connections.

The central decision is: **how much productive land can I afford to defend?**

Pure distance-based collection would be simple but make roads less interesting.
Requiring roads for everything would make the opening slow and a single raid too
punishing. Use proximity and roads together, with local gathering as a fallback.

## City and tile basics

- A city occupies one passable hex and has an owner, population, food reserve,
  health, buildings, citizen assignments, and a persistent production queue.
- The center produces a small guaranteed yield without using a citizen. Start
  with 2 food and 1 production; external tiles are needed for meaningful growth.
- Each population provides one citizen assignment and consumes 2 food per turn.
  One citizen works one tile. Unassigned citizens provide no tile yield initially.
- Terrain supplies basic food and production. A resource is a separate layer:
  for example, grain improves food and an iron deposit enables iron extraction.
- Improvements are a third layer: farms, mines, and pastures increase yields or
  enable extraction. Roads are separate, so a mine can also have a road.
- Food and production belong to a city. Science and gold pool across the team.
  Begin science at 1 per population; buildings can add more later.
- Iron and horses are strategic resources, separate from ordinary tile yields.

Do not multiply population science by road efficiency. Logistics affects goods
gathered from tiles, not every output of the city.

## Access without borders

A tile is workable when it has a passable route to the city and lies within its
logistics budget. Mountains and enemy city centers cannot carry routes. Friendly
units do not block gathering. Enemy-occupied and contested hexes block it.

Use a weighted shortest path, not straight-line distance. Roads reduce the cost
of transporting goods; they do not directly multiply the underlying harvest.

Suggested initial route costs, per entered hex:

| Surface | Cost | Purpose |
| --- | ---: | --- |
| Unroaded plains | 1.0 | Local gathering |
| Unroaded hills | 1.5 | Terrain matters economically |
| Dirt road | 0.5 | Connect early farms and mines |
| Stone road | 0.25 | Support distant or larger settlements |
| Rail | 0.1 | Late-game long-distance industry |

The city center costs zero. Each other hex uses its own terrain or intact road
cost, including the worked tile. Mixed routes add their costs normally; a road
segment still helps even if the entire route is not paved. Any team can use an
unblocked road. Road technology gates construction, not use of captured roads.

Start with a logistics budget of 4 and these deliberately legible efficiency bands:

| Total route cost | Delivered share of tile output |
| --- | ---: |
| At most 1 | 100% |
| Greater than 1, at most 2 | 75% |
| Greater than 2, at most 3 | 50% |
| Greater than 3, at most 4 | 25% |
| Greater than 4, or no route | Unavailable |

Example: a mine yielding 4 production three plains away delivers 2 production
without roads. Dirt roads on all three route hexes cost 1.5 and deliver 3.
Stone roads cost 0.75 and deliver the full 4. If a raider blocks that road,
recalculate using the best alternate path: delivery drops or stops.

Use fixed-point arithmetic and preserve fractional output in city stores.
Do not round each tile's harvest down independently. Scale iron and horse
extraction with the same delivery rate if stockpiles are used.

This creates a changeable economic reach, not a movement border. A long road
can reach beyond the city's nearby tiles. The logistics budget still limits
reach and prevents a city from collecting arbitrary resources across the map.

## Who gets a tile's output?

Access alone does not grant ownership. Avoid both automatic harvesting from every
reachable tile and duplicate harvesting by overlapping cities.

- An improvement has an owning team. Only that team may work it.
- A tile can have only one city assignment globally. Friendly cities can transfer
  an assignment during planning; the transfer replaces the old assignment.
- Unimproved tiles can be assigned when reachable. If opposing teams both request
  the same unassigned tile in one turn, neither receives it that turn. An existing
  assignment persists until released or taken; it is not overwritten by proximity.
- Enemy occupation stops delivery immediately at the economic tick. It does not
  automatically redirect output to the occupier's nearest city.
- Taking an improvement or an assigned unimproved tile requires a queued Secure
  Site action from a unit already standing there. The unit must hold position,
  forgo attacking, survive, and remain uncontested through combat resolution.
  This transfers the site and clears its old assignment; harvesting can begin
  next turn if the new owner assigns it and has a route.
- Blocked citizen assignments remain in place, flagged as disrupted. They resume
  when access returns, avoiding mandatory reassignment after every raid.

These are claims on individual sites, not an expanding colored territory.

## Permanent war and roads

For the first prototype, enemy presence blocks only the occupied hex. Do not add
an invisible radius of economic denial around every unit. A road chokepoint is
valuable because the geography makes it one; open terrain permits detours.

Later, add a Pillage action distinct from Secure Site. Pillaging damages an
improvement or road; securing preserves it for capture. Damaged roads fall back
to terrain cost until repaired. A damaged improvement loses its added output
and strategic extraction. Do not remove underlying terrain yield.

No moving caravans or per-road throughput limits initially. Those could make a
larger simulation interesting, but route cost, labor, and interdiction already
give players several interacting constraints. Likewise, defer road movement
bonuses so transport balancing does not accidentally change combat initiative.

Stone roads and rail are future science unlocks. Their costs and maintenance
should make upgrading an important route preferable to upgrading every tile.
Start by testing dirt roads with an upfront construction cost; tune maintenance
only once gold has a useful economy.

## Growth, production, and strategic resources

Food after population upkeep fills a reserve. A provisional growth threshold is
`10 + 5 * population`; reaching it adds one population and consumes that amount.
If the reserve would fall below zero, lose at most one population that turn,
never below one, and reset food to zero. Growth and starvation change next
turn's citizen capacity and upkeep. Starting reserves need enough buffer that
one interrupted harvest does not immediately shrink a city.

Production advances the queue's first item; unused production carries forward.
Complete at most one item per turn initially. Buildings and new citizens begin
contributing next turn. A completed unit waits for a legal deployment hex rather
than disappearing or stacking illegally. Queues are editable during planning,
and investment stays with a paused item instead of being refunded on reorder.

Recommendation for iron and horses: small team stockpiles, consumed when the
relevant unit completes. Losing a mine stops replenishment but does not delete
or disable existing armies. That gives raids a delayed military payoff.
This is a deliberate design choice, not a requirement of the Civ template.

If materials are missing, keep the item ready but blocked and show why. Reserve
stockpile spending for a deterministic team allocation pass (stable city ID
order initially), so simultaneous completions cannot spend the same iron twice.
Recurring resource upkeep for existing units is deferred.

## Queued planning and turn resolution

The current prototype auto-resolves after the last unit finishes orders. Cities
need time for assignment and queue edits even when no unit needs attention.

Recommended change: retain automatic selection of the next unit, but add an
explicit End Planning button/key. Empty city queues are allowed, with a warning.
An unfinished unit order must be completed or explicitly held before ending
planning. This also allows turns to advance when a team has cities but no units.

Use the same rules for both teams. At the end of the existing eight combat steps:

1. Resolve site actions and city captures against the surviving board state.
2. Validate planned assignments together, including competing claims.
3. Calculate all routes and deliveries from one shared board snapshot.
4. Apply food upkeep, growth/starvation, production, science, and stockpile income.
5. Allocate strategic spending and finish builds deterministically. New units
   cannot act during the turn that created them.
6. Activate finished infrastructure for the next turn, then begin planning.

Economy runs exactly once per full turn, including turns without any combat.
Planning previews show expected yield on the current board; enemy orders may
change the result. Routes do not deliver between individual combat steps.

## City combat boundary

Cities should have their own health and accept a friendly garrison without
counting the city as a second unit. Enemy melee occupation can capture a city
only after its defenses are reduced to zero; ranged attacks alone cannot capture.
The exact siege damage and garrison targeting rules need a separate combat pass.
Do not give cities automatic ranged attacks in the first economic experiment.

On capture, discard the old owner's queued plans, clear external assignments,
and give the city no harvest during that resolution. The new owner replans next
turn. External improvements retain their owners until individually secured.
Capture bonuses, razing, resistance, and victory conditions are later decisions.

## First playable experiment

Build in small steps so we can test the resource-access idea before committing
to a complete city game:

1. Economy slice: two preplaced cities on a larger test map, food/production,
   population assignments, preplaced farms/mines and dirt roads, route previews,
   end-of-turn income, and explicit End Planning. Include a basic AI assignment
   policy; both sides obey the same labor and access rules.
2. Production slice: persistent queues, one building and one recruitable unit,
   blocked deployment handling, and road construction as a queued project.
   Road projects must connect to currently reachable friendly infrastructure;
   later decide whether workers must physically build them.
3. Warfare slice: Secure Site, pillage/repair, city defenses/capture, and iron or
   horses with a unit that consumes them. Extend the AI to use these systems.
4. Progression slice: science unlocks, stone roads, additional improvements,
   founding cities, and eventually rail. No luxuries in this scope.

Keep the original combat layout as a separate scenario. Its radius-three map is
too small to judge distance, alternate routes, or several settlements.

## Fit with the current code

- Add stable city IDs and city state alongside units in `GameState`.
- Keep terrain, resources, improvements, and transport as separate map layers;
  `HexGrid` currently stores terrain only.
- Put pure yield/route calculations in an economy module. Use weighted path
  search rather than the combat movement BFS, since route costs differ.
- Add an economic finalization hook in `turn.rs` after all combat steps and
  before new planning. Do not trigger income from rendering or selection.
- Revise `orders.rs` turn readiness so finishing unit orders does not close
  city planning, and no-unit city turns remain possible.
- Add city selection and panels in `ui.rs`/`draw.rs`. Show raw yield, delivered
  yield, assigned citizens, the chosen route, and the reason for disruption.
- Keep road transport cost independent from unit movement until explicitly
  designing that interaction.

Useful verification cases: road versus off-road delivery; a mountain detour;
alternate routes after interdiction; contested routes; shared tile assignment;
fractional yield accumulation; once-only economy ticks; growth and starvation;
simultaneous strategic spending; full deployment hexes; and cities without units.

## Decisions to discuss first

1. Does claiming individual sites feel right, or should military presence alone
   determine who may harvest? Recommendation: persistent site claims, with
   occupation blocking access and a deliberate action transferring ownership.
2. Should strategic resources be stockpiles or a connected-deposit unit cap?
   Recommendation: stockpiles, to soften the immediate impact of raids.
3. Should road construction consume city production or require physical workers?
   Recommendation: city projects for the initial experiment; workers if building
   and protecting infrastructure becomes a central part of play.
