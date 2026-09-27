# History

How the prototype got here, oldest first. Git history has the detail; this is the narrative.

1. Vulkan framework from scratch: window, instance, device, swapchain, triangle pipeline, resize
   handling, frame sync.
2. Spinning triangle, then a 3D prism with depth buffer and mouse rotation; 165 FPS cap.
3. Pivot to the hex combat prototype: dynamic per-frame vertex buffers, 2D orthographic camera,
   depth buffer removed. Hex grid, four unit types, Civ-style combat.
4. Single-letter unit labels (bitmap font), mouse-wheel zoom, middle-drag pan.
5. We-go turns: queued orders, then resolution. Moved from "all moves then all attacks" to the
   per-type resolution order. Grid shrunk to radius 3.
6. Undo by re-clicking; attacks target hexes (shift-click) so you can anticipate moves; staggered
   playback with highlight flashes.
7. Simple Red AI; player controls Blue only.
8. Fixes: fanned-out markers when several units target one hex; ghost previews for queued moves
   (RGBA vertex colors); cancelling a move drops the attack it enabled; SPIR-V alignment bug.
9. Path-blocking movement (BFS); units can't hop over each other.
10. Codebase cleanup: game module split into camera/turn/ai/draw; dead code removed; drop-order bug
    fixed.
11. Terrain: hills (+25% defense), mountains (impassable), center pass; AI uses walking distance.
12. Simultaneous resolution within a step; move/attack order badges.
13. Contested hexes replace enemy bounces; ally swaps (Ctrl-click); allies can't target the same
    hex; input moved into `orders.rs`.
14. Abilities (Shield Wall, Volley, Charge, Deploy) with an on-screen button; renderer draws
    multiple batches (world + UI); full bitmap font.
15. Project record (the old CLAUDE.md). Auto-select the next unit once the current one has moved
    and attacked; first unit selected each turn; Space to skip (ability moved to Q); camera glides
    to the unit the game selects.
16. Turns end automatically once every unit has acted; Enter removed. Space now holds a unit
    (keeps queued orders, forfeits the rest); Tab browses.
17. City system (`codex/city-system`): cities, logistics, growth, builds, settlers, workers; Enter
    ends planning again so cities can be planned.
18. UI merge and text: antialiased TrueType text through a glyph atlas (renderer gained textures);
    self-sizing panels fix overlapping city text; action buttons (Move/Attack/Swap/ability/Hold)
    with tooltips and armed states; hover info box; End Turn button; larger centered window,
    zoomed out.
19. Window/taskbar icon drawn in code; F5 toggles borderless fullscreen.
20. Worked-tile rings, click-a-unit to leave the city view, no route preview; yields only in the
    city view with a Y toggle; tile tooltip on hover.
21. Space ends the turn once nothing's waiting; the turn also waits for city builds, not
    citizens; Guard (G) skips a unit every turn. The End Turn button names what's waiting. Hold
    Escape to quit. MSAA, at the highest sample count the GPU supports.
22. Group orders: Alt-drag/Alt-click to select several units; they converge on a clicked hex or
    all attack a clicked enemy. Attack arcs replace the little target markers.
23. Testing aids: a savestate (F6 save, F7 load) and a faded DEBUG panel with buttons for the
    scenario pages and savestate.
24. Attack arrows as clean ribbons, player's only; resolved attacks animate (shot, hit burst /
    miss / out of range, damage numbers).
25. Debug toggle (F8) for instant turn playback.
26. City buildings and labor (`codex/city-buildings-labor`): Granary and Barracks, the city
    manager and its work group, labor focus, worker roads and improvements, build queue controls,
    Horses and Iron with the Horse and Armored units they unlock, and city and barracks combat.
27. Map generation, tile modifiers, scouts and fog of war (`claude/mapgen-fog-scouts`): ten ground
    types with their own yields, with hills and forest/jungle as modifiers; rivers along hex edges
    (fresh water +1 food); workable water; a seeded Pangea world on a 61x36 rectangular map (F4),
    each side starting with a settler, worker and scout; the Scout unit (Lookout ability); fog of
    war; a vertex buffer that grows as needed.
28. Mill and Workshop buildings, building sites chosen on the map, and reusable queue panels
    (scroll, drag to reorder, X to remove) docked by screen zone (`ui/dock.rs`).
29. Agent-focused repo: AGENTS.md (root and nested) replaces CLAUDE.md, readable by Claude Code
    and Codex; rules, controls, architecture and history in `docs/`, checked against the code;
    the GitHub Projects work board (`tools/board/`, skill in `.agents/skills/`), a commit-message
    lint, CI (fmt, clippy, tests, MSRV 1.92, tool self-tests), a PR template, and an AI-vs-AI
    simulation test of board invariants.
30. Unit pictograms and sharper text (`claude/unit-icons-font`): units show a pictogram built from
    simple shapes (sword, bow, horse head, catapult, spyglass, shield; flag and shovel for
    civilians) instead of a letter; text is IBM Plex Mono, with world text as signed distance
    fields; the Horse unit merged into Cavalry, which now needs a barracks on Horses.
31. Map icons (`claude/resource-site-icons`): resources and improvements are small dark-edged
    icons straight on the tile instead of badges: a horse's head and an ingot for Horses and
    Iron in the top-right corner; a wheat stalk, ore cart, fence and stacked logs for farm, mine,
    pasture and lumber mill in the top-left. Unit tokens shrank (radius 0.42 to 0.36) to make
    room. Holding Alt shows extra map info: units' turn-order numbers (no longer always shown)
    and every explored tile's yields. Yields are die-face pips of wheat and hammers (one icon
    and a number past six) instead of a strip with numbers.
32. City workers (`claude/city-workers`): workers are no longer units. Each city keeps a pool
    at home (built with 8, one with every new city) and a list of jobs queued from a tile panel:
    roads, improvements, outposts (sight 2) and forts (placeholder +50% defense), plus walls and
    gates placed on hex edges by clicking or dragging along them. Workers walk out (1 hex a
    turn) and work in a new last step of the turn, after every unit; an enemy stepping onto one
    captures it and an attack kills it. A Recall button sends a worker home. The AI queues
    improvements and roads and hunts enemy workers. The World scenario has no AI opponent. Units
    can be disbanded (Delete or Disband, twice).
33. Order queues (`claude/order-queues`): left click moves and right click attacks (any hex in
    range; Shift-click no longer attacks). Shift-left-click queues one more turn moving toward a
    hex and Shift-right-click an attack, so units carry orders over several turns; a group's
    queues always have the same number of turns. Queued units don't hold up ending the turn, any
    other order cancels the queue, and a queued turn that no longer fits drops the queue. Plans
    show as turn numbers along a line, only while the unit is selected or hovered.
34. Far Shift-clicks (`claude/queue-far-moves`): one Shift-left-click queues every turn it takes
    to walk to the hex, around terrain and known walls, instead of one turn per click.
35. City interior battle prototype (`codex/city-interior-battle`): each city has a 19-hex tactical
    grid. Adjacent combat troops project independently controlled copies into its gates; breaching
    and occupying the command post captures the city. F12 opens a ready-made siege setup.
36. City interior map interaction: clicking the city center from management enters a separate
    rendered hex map. Troops and command post are drawn in-world; map clicks issue interior orders,
    and exiting restores the exterior camera and city view.
37. Shared unit deaths across the exterior and interior maps: units retain separate HP bars,
    interior wounds persist across gate exits, and fatal damage in either layer removes the unit
    from both. The command post retaliates at range two.
38. City centers no longer accept exterior attacks or have an exterior HP bar. Units standing
    there and barracks remain attackable; the tactical command post is the capture target.
39. Selection and the unit strip (`claude/selection-roster`): a left-drag on the map draws a
    selection box (Shift-drag adds) instead of panning, which is now middle-drag only; Shift-click
    adds a unit to the selection and Ctrl-click takes a group member out; Alt no longer selects.
    A unit strip panel (top-left, dockable) shows every unit still needing orders, settlers too,
    with the selection framed: click to jump to one, Shift- or Ctrl-click to add or remove it.
    The unit and group panels gained a Clear Orders button; Hold toggles, and any order ends a
    hold. Queued plans are drawn per unit (fanned numbers, a ghost at each plan's end) and the
    turns-left tag reads `3T`. A plain click that would replace a multi-turn queue must be
    repeated on the same hex, so viewing a plan and clicking away can't wipe it.
40. Settings menu (`claude/settings-menu`): Escape with nothing left to close opens a settings
    menu (top-right, dockable, in both presentations), and closes it again; holding Escape from
    there still quits. Player options live in one `Settings` struct (`settings.rs`) kept across
    scenario switches and loads, each an integer the menu steps with < and > buttons, so a new
    option is a field and an entry in `Setting::ALL`. Turn playback (F8) is the first.
41. Move queue limit (`claude/queue-length-setting`): the second setting caps how many turns of
    moves one Shift-click queues for each unit (6 by default, 1 to 20), replacing the fixed
    64-turn safety net. A hex farther away is queued as far as the limit goes, with a notice
    saying so, and the same Shift-click again carries on; in a group the limit counts from the
    end of each member's own plan.
42. Settings menu follow-ups (`claude/settings-quit-and-queue-cap`): the menu opens centered and
    has a Quit button, which replaces holding Escape to quit. The queue limit now caps a unit's
    whole plan (this turn included) rather than each Shift-click, so repeated clicks can't queue
    past it; a full queue refuses more turns until turns are played.
43. Fog clouds (`claude/fog-clouds`): the grey octagons over unexplored hexes became banks of
    muted cumulus, each puff one quad the shader rounds and feathers (`soft_disc_uv`, a new
    renderer primitive) and shades from a lit top to a dark underside, over a soft shadow per
    bank and a dark fill, so unexplored land is cloud nearly all the way through, in about a
    quarter of the vertices the old sampled mesh took. Each bank grows its own irregular clump
    of 4 to 9 puffs. The clouds drift on a slow wind and billow, fading out past the map's edge.
    A third setting, Fog, swaps them for solid grey.
44. More sides and contested ground (`claude/world-players`): `Team` grew to seven sides, each
    at war with every other, and the AI plays all but Blue. The F4 world now seats Blue and 4-6 AI
    sides (the World AI setting) on starts scattered and evened out for spacing and land, each
    with horses and iron a few hexes away, and starting with a city or a settler (the World Start
    setting). Between the starts it places special tiles (Orchard, Quarry) that yield more, and
    ruins a side claims by holding them for three turns. The AI heads for ruins as well as
    enemies, and its searches stop early, so turns with many sides stay quick.
45. Room for more sides (`claude/world-players`): the world grows in proportion to its sides
    (about 93 by 57 hexes for six), so neighboring starts sit about a fifth farther apart. The fog
    draws only what the camera can show, and its fill only on unexplored hexes, so a big map costs
    no more per frame than a small one.
46. The turn strip (`claude/turn-tasks`): the unit strip now lists everything needing the
    player this turn, civilian tasks first (cities with nothing to build, cities with idle
    workers, settlers), then military units grouped by kind with a count. Selecting a group lists
    its units one by one below it. It starts at the bottom center, in a new
    `Zone::BottomCenter` that slides aside for other panels.
47. Sessions remembered (`claude/turn-tasks`): settings are saved as they change, and on quitting
    the window size, the presentation and the ImGui panel layout (with ImGui's docking data) are
    saved too, in the config folder, so the next session starts the same way.
48. Production first (`claude/turn-tasks`): the game moves through a turn in the turn strip's
    order, cities needing a build before units and settlers before the military, and a new
    world opens on the city.
49. Workers apart (`claude/turn-tasks`): idle workers left the turn strip (they never held up
    the turn). A tile takes one worker job at a time, so a road and an improvement can't be
    queued on it together, and the tile the tile panel shows has a white ring.
50. Worker reach and worker mode (`claude/worker-reach`): workers only work within 3 tiles of a
    city, or next to a road, so roads extend their reach. W (or Worker Jobs in the city panel)
    turns on worker mode, which lights the reachable tiles, dims the rest, explains itself in a
    panel, and opens the tile panel for any clicked tile. Queued walls and gates have rounded
    ends, so a run of them joins into one line.
51. The worker menu (`claude/worker-reach`): the only way to give workers orders. Pick a job,
    then place it on the map by clicking or dragging over tiles or edges, with a preview ring
    under the cursor; the menu also lists each city's workers and jobs (moved from the city
    panel). The tile panel is gone. Idle workers are back in the turn strip and the turn order,
    after production, and can sleep for a turn.
