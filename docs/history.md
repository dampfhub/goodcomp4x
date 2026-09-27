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
32. City interior battle prototype (`codex/city-interior-battle`): each city has a 19-hex tactical
    grid. Adjacent combat troops project independently controlled copies into its gates; breaching
    and occupying the command post captures the city. F12 opens a ready-made siege setup.
33. City interior map interaction: clicking the city center from management enters a separate
    rendered hex map. Troops and command post are drawn in-world; map clicks issue interior orders,
    and exiting restores the exterior camera and city view.
