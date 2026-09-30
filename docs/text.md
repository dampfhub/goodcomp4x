# Game text: the files in `text/`

The words the game shows live in plain-text files under `text/`, one file per area, so they
can be read and changed without touching Rust. The files are built into the executable
(`include_str!`), so after editing one, rebuild (`cargo build`, or `cargo run`) to see it.

This is being done in stages (#341). So far the settings menu and its Multiplayer page read
their text from here (`text/menus.ini`); everything else is still written in the code, and
moves over area by area.

| File | What it holds |
|---|---|
| `text/menus.ini` | The settings menu (its title, headings, each setting's name, tooltip and values, its buttons and their tooltips, and the notice when a setting changes) and its Multiplayer page (its rows, fields, buttons, tooltips and what it says when a field isn't filled in) |

## An entry

Each entry is a tag in square brackets, then its fields:

```ini
# A comment: a line starting with #.
[setting_turn_transition]
text = TURN TRANSITION
tooltip = UNITS GLIDE TO THEIR NEW HEXES AND THE NEW TURN FLASHES AS A TURN
  RESOLVES.

[setting_queue_limit_many]
text = {turns} TURNS
```

- **The tag** (`setting_turn_transition`) is what the code asks for. It's lower-case letters,
  digits and `_`, starting with a letter, and it's unique across all the files. Don't rename
  one without renaming it in the code: the tests fail until the two match.
- **`text`** (needed): what the game shows, such as a label or a heading.
- **`tooltip`** (optional): what shows when the cursor rests on the thing.
- **`hover_text`** (optional): what a label changes to while the cursor is on it. Nothing in
  the game shows one yet; the first area that does adds it to both UI presentations.

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

## Checking a change

Run `cargo test`. It fails, saying the file and line, if a file doesn't read (a line that's
neither a `[tag]`, a `key = value` nor a comment, an unknown key, a tag or key given twice, an
entry with no `text`, a stray brace), if the code asks for a tag or field no file has, if a file
has a tag or field no code uses, if a placeholder in a file doesn't match what the code fills
in, or if a character isn't one the fonts have (`ui_text_uses_only_the_shared_glyphs`).
The game itself never stops over text: a tag it can't find shows as the tag.

## For code

`src/game/strings.rs` reads the files. Code asks for an entry's fields by tag with the macros
`text!`, `tooltip!` and `hover_text!`:

```rust
use crate::game::keys::Command;
use crate::game::strings::{text, tooltip};

let title: &'static str = text!("settings_title");
let limit: String = text!("setting_queue_limit_many", turns = value);
let about: &'static str = tooltip!("setting_fog");
let playback: String = tooltip!("setting_turn_playback", key = Command::Playback.key());
```

The tag is always a string literal at the call, never built at runtime: the tests find every
call by reading the source. To choose between entries, match and call the macro in each arm
(`Setting::name`, `src/game/settings.rs`). A new file is a line in `FILES` (`strings.rs`) and a
row in the table above. A key placeholder is filled with the key's name from the key map,
`Command::key` (`src/game/keys.rs`), never with a literal.
