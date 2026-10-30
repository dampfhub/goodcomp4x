# Game text: the files in `text/`

The words the game shows live in plain-text files under `text/`, one file per area, so they
can be read and changed without touching Rust. The files are built into the executable
(`include_str!`), so after editing one, rebuild (`cargo build`, or `cargo run`) to see it.

This is being done in stages (#341). So far the settings menu and its Multiplayer page
(`text/menus.ini`), the status bar, top bar, Debug panel, view controls and window titles
(`text/ui.ini`), and the units (`text/units.ini`) read their text from here; everything else
is still written in the code, and moves over area by area.

| File | What it holds |
|---|---|
| `text/menus.ini` | The settings menu (its title, headings, each setting's name, tooltip and values, its buttons and their tooltips, and the notice when a setting changes) and its Multiplayer page (its rows, fields, buttons, tooltips and what it says when a field isn't filled in) |
| `text/ui.ini` | The screen's frame: the status bar (ImGui) and top bar (classic) with the turn, supply, Menu and End Turn in all its forms and its tooltip; the Debug panel's buttons, scenarios and tooltips; ImGui's view controls; and the panels' window titles |
| `text/units.ini` | The units: their names, the unit and group trays, each order's button, hint, tooltip and why it's off, the abilities, and what the game says as orders are given (arming, queueing, disbanding, a queue that stops) |

## An entry

Each entry is a tag in square brackets, then its fields:

```ini
# A comment: a line starting with #.
[net_host_button]
text = HOST GAME
tooltip = STARTS A NEW WORLD FOR THIS MANY PLAYERS AND SHOWS ITS JOIN CODE.
  THEY ALSO NEED YOUR ADDRESS AND THE PORT OPEN.

[setting_queue_limit_many]
text = {turns} TURNS
```

- **The tag** (`net_host_button`) is what the code asks for. It's lower-case letters,
  digits and `_`, starting with a letter, and it's unique across all the files. Don't rename
  one without renaming it in the code: the tests fail until the two match.
- **`text`** (needed): what the game shows, such as a label or a heading.
- **`tooltip`** (optional): what shows when the cursor rests on the thing.
- **`hover_text`** (optional): what a button's label changes to while the cursor is on it, in
  both UI presentations. The game's own files never set one (a test checks), so by default no
  label changes on hover: it's there for you to add. It works on the entries of buttons'
  labels (the tests fail on one the code shows no hover text for), and the button keeps its
  size, so keep it about as long as the label.

A field takes the rest of its line, spaces at the ends trimmed. A long value can go on over
the next lines: indent them, and each joins the one before with a space. To keep spaces at a
value's ends, put it in double quotes: `text = "  NAME  "`.

A tooltip is a title line, with the key that does the thing, over a description. For a button,
the entry's `text` is its label and its `tooltip` the description, and the title is the label.
Where the title isn't the label (the CLOSE button's tooltip says CLOSE SETTINGS), an entry
whose tag ends in `_title` holds the title as its text, and the description as its tooltip.

## Placeholders and icons

A name in braces, such as `{turns}` or `{food}`, is a **placeholder**: the code puts a number or
a name there. Keep every placeholder an entry has, spelled the same; you can move them around
in the sentence. Adding, removing or renaming one needs a change in the code as well (the tests
catch a mismatch).

These four names in braces aren't placeholders: they draw the map's icons in the text.

| Write | Shows |
|---|---|
| `{food_icon}` | the food icon |
| `{wood_icon}` | the wood icon |
| `{metal_icon}` | the metal icon |
| `{time_icon}` | the clock (a number of turns) |

Other braces aren't allowed. Characters: printable ASCII, plus the few in
`font::UI_PUNCTUATION` (`src/game/font.rs`: the middle dot, em dash, times sign and ellipsis),
which both UI presentations' fonts carry.

## Keys

A text file never spells out a key. Where an entry names the key that does something, it has a
placeholder, such as `{key}` (the key for the thing the entry is about) or `{world_key}`, and
the game fills in that key's name from its key map (`src/game/keys.rs`), so the text always
names the key the game really uses:

```ini
[setting_turn_playback]
text = INSTANT PLAYBACK
tooltip = PLAY EACH TURN OUT AT ONCE ({key}).
```

The game shows that tooltip as "PLAY EACH TURN OUT AT ONCE (F8)."

Mouse clicks aren't keys: CLICK, RIGHT-CLICK, RMB, and CTRL or SHIFT with a click, are written
out in the files (`CTRL-RIGHT-CLICK`), as the game names them.

## Writing it: short and plain

Every agent, and anyone else, reads over each piece of player-facing text they add or change
before submitting. That covers labels, names, tooltips, hints, notices, disabled reasons and
panel lines, whether the text is in `text/` or still in the code. Cut it until nothing more
can go, and make it read like a terse strategy-game manual, not like a chatbot. List the
strings you checked in the PR.

- **Short.** A label is one to three words. A tooltip is one or two short lines: what it does,
  and its numbers. A notice is one line. Don't repeat the label in the tooltip, and don't
  describe what the icon or the number already shows.
- **Numbers, not adjectives.** `+2 FOOD`, not `GREATLY BOOSTS FOOD`. `3 TURNS` (the clock icon),
  not `TAKES A WHILE`.
- **Plain words.** Avoid these: `SIMPLY`, `EFFECTIVELY`, `SEAMLESSLY`, `ENSURE`, `UTILIZE`,
  `LEVERAGE`, `ENHANCE`, `ROBUST`, `CRUCIAL`, `VITAL`, `KEY` (as an adjective), `POWERFUL`,
  `STRATEGIC`, `UNLEASH`, `EMBARK`, `ELEVATE`, `STREAMLINE` and `DELVE`. Avoid `ALLOWS YOU
  TO`, `IN ORDER TO`, `IT'S IMPORTANT TO` and `PLEASE`.
- **No AI rhythm.** These read as machine-written:
  - "not just X, but Y";
  - lists of three for rhythm;
  - clauses joined by dashes for effect (use a full stop or a colon);
  - rhetorical questions;
  - "whether you're...";
  - exclamation marks;
  - a closing line that sums up or cheers the player on.
- **No hype or flavour padding.** State the rule. A unit's description says what it's good and
  bad at, not how legendary it is.
- **Speak to the player as the game does.** Use orders and facts: `CLICK A HEX TO MOVE`,
  `SHORT OF 4 {metal_icon}`. Don't use first person, and don't talk about yourself.
- **Use the game's own words.** Name things the way the rest of the game does, such as
  STOCKPILE, SUPPLY, QUEUE and GATHER, and use one word for one thing everywhere. Check the
  nearby entries before you name something.

For example:

| Not this | This |
|---|---|
| `PIKEMEN ARE A VERSATILE AND COST-EFFECTIVE UNIT THAT EXCELS AT COUNTERING MOUNTED THREATS!` | `CHEAP. STRONG AGAINST CAVALRY, WEAK OTHERWISE.` |
| `THIS ALLOWS YOU TO SEAMLESSLY GATHER RESOURCES WHEN YOUR CITY ISN'T BUSY.` | `AN IDLE CITY GATHERS.` |
| `NOT ONLY DOES A FARM BOOST FOOD — IT ALSO HELPS YOUR CITY GROW FASTER.` | `+2 FOOD. NEEDS FRESH WATER.` |

## Checking a change

Run `cargo test`. It fails, saying the file and line, if a file doesn't read (a line that's
neither a `[tag]`, a `key = value` nor a comment, an unknown key, a tag or key given twice, an
entry with no `text`, a stray brace), if the code asks for a tag or field no file has, if a file
has a tag or field no code uses, if a placeholder in a file doesn't match what the code fills
in, or if a character isn't one the fonts have (`ui_text_uses_only_the_shared_glyphs`).
It also fails if an entry has a `hover_text` (`the_shipped_files_never_change_a_label_on_hover`):
the files the game ships never change a label on hover, so a hover text is for your own copy,
not for a change to the game.
The game itself never stops over text: a tag it can't find shows as the tag.

## For code

`src/game/strings.rs` reads the files. Code asks for an entry's fields by tag with the macros
`text!`, `tooltip!` and `hover_text!`:

```rust
use crate::game::keys::Command;
use crate::game::strings::{hover_text, text, tooltip};

let title: &'static str = text!("settings_title");
let limit: String = text!("setting_queue_limit_many", turns = value);
let about: &'static str = tooltip!("setting_fog");
let playback: String = tooltip!("setting_turn_playback", key = Command::Playback.key());
// None unless a file gives the entry a hover text (the shipped ones never do):
let hover: Option<&'static str> = hover_text!("menu_button");
```

The tag is always a string literal at the call, never built at runtime: the tests find every
call by reading the source. To choose between entries, match and call the macro in each arm
(`Setting::name`, `src/game/settings.rs`). A new file is a line in `FILES` (`strings.rs`) and a
row in the table above. A key placeholder is filled with the key's name from the key map,
`Command::key` (`src/game/keys.rs`), never with a literal.
