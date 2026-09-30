//! The key map: the key each command is on. `App` (`src/app.rs`) turns a
//! key press into a `Key`, asks `command_for` what it does here and carries
//! that out; the UI names a command's key from here too (`Command::key`),
//! so a key moved in the map is moved wherever the game names it. The text
//! files never spell out a key: an entry that names one has a placeholder
//! such as `{key}` that the code fills from here (`docs/text.md`).
//!
//! What each key does, for players: `docs/controls.md`.

use std::fmt::{self, Display};

use super::scenario::Scenario;

/// A key, as the game tells keys apart. `App` makes one of each key press
/// (the numeric keypad's Enter is `Enter`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Escape,
    Space,
    Tab,
    Enter,
    Backspace,
    Delete,
    PageDown,
    /// A letter key, as its upper-case letter.
    Letter(char),
    /// A digit key on the main row, as its digit.
    Digit(char),
    /// A function key: F1 is `F(1)`.
    F(u8),
}

impl Display for Key {
    /// The key's name, as the UI shows it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Escape => f.write_str("ESC"),
            Key::Space => f.write_str("SPACE"),
            Key::Tab => f.write_str("TAB"),
            Key::Enter => f.write_str("ENTER"),
            Key::Backspace => f.write_str("BACKSPACE"),
            Key::Delete => f.write_str("DEL"),
            Key::PageDown => f.write_str("PAGE DOWN"),
            Key::Letter(c) | Key::Digit(c) => write!(f, "{c}"),
            Key::F(n) => write!(f, "F{n}"),
        }
    }
}

/// A key with the modifiers a binding needs held with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
}

impl Chord {
    const fn key(key: Key) -> Self {
        Self {
            key,
            ctrl: false,
            shift: false,
        }
    }

    const fn ctrl(key: Key) -> Self {
        Self {
            key,
            ctrl: true,
            shift: false,
        }
    }

    const fn ctrl_shift(key: Key) -> Self {
        Self {
            key,
            ctrl: true,
            shift: true,
        }
    }

    fn modifiers(self) -> usize {
        usize::from(self.ctrl) + usize::from(self.shift)
    }
}

impl Display for Chord {
    /// The chord's name, as the UI shows it: `CTRL+V`, say.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ctrl {
            f.write_str("CTRL+")?;
        }
        if self.shift {
            f.write_str("SHIFT+")?;
        }
        self.key.fmt(f)
    }
}

/// What a key does. `App` carries each out, most by calling one
/// `GameState` method.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Close the settings menu, a view or the selection; with nothing to
    /// close, open the settings menu.
    Back,
    /// Close a structure menu, or else hold the selected unit or end the
    /// turn.
    HoldOrEndTurn,
    NextUnit,
    Ability,
    SelectCity,
    CityInterior,
    AutoAssign,
    Move,
    Attack,
    Road,
    Improve,
    FoundCity,
    BuildMelee,
    BuildRanged,
    BuildSiege,
    BuildScout,
    BuildSettler,
    BuildBarracks,
    BuildMill,
    BuildWorkshop,
    BuildWorker,
    Grow,
    Gather,
    /// In the city interior, clear its orders; else take the open city's
    /// first build off its queue.
    Remove,
    Disband,
    /// Move the open city's first build down its queue.
    QueueHeadDown,
    /// Start this scenario afresh.
    Scenario(Scenario),
    Yields,
    Guard,
    Alert,
    Fullscreen,
    Save,
    Load,
    Playback,
    FinishBuild,
    Fog,
    /// Switch between the ImGui and classic presentations.
    Presentation,
    /// ImGui: put the view's panels back where they start.
    ResetLayout,
    // While typing into a text field (the Multiplayer page, classic):
    Paste,
    Copy,
    Cut,
    /// Take the last character off.
    Erase,
    StopTyping,
}

/// The keys while playing.
pub const PLAYING: &[(Chord, Command)] = &[
    (Chord::key(Key::Escape), Command::Back),
    (Chord::key(Key::Space), Command::HoldOrEndTurn),
    (Chord::key(Key::Tab), Command::NextUnit),
    (Chord::key(Key::Letter('Q')), Command::Ability),
    (Chord::key(Key::Letter('C')), Command::SelectCity),
    (Chord::key(Key::Letter('V')), Command::CityInterior),
    (Chord::key(Key::Letter('A')), Command::AutoAssign),
    (Chord::key(Key::Letter('M')), Command::Move),
    (Chord::key(Key::Letter('X')), Command::Attack),
    (Chord::key(Key::Letter('R')), Command::Road),
    (Chord::key(Key::Letter('I')), Command::Improve),
    (Chord::key(Key::Letter('F')), Command::FoundCity),
    (Chord::key(Key::Digit('1')), Command::BuildMelee),
    (Chord::key(Key::Digit('2')), Command::BuildRanged),
    (Chord::key(Key::Digit('3')), Command::BuildSiege),
    (Chord::key(Key::Digit('4')), Command::BuildScout),
    (Chord::key(Key::Letter('S')), Command::BuildSettler),
    (Chord::key(Key::Digit('5')), Command::BuildBarracks),
    (Chord::key(Key::Digit('6')), Command::BuildMill),
    (Chord::key(Key::Digit('7')), Command::BuildWorkshop),
    (Chord::key(Key::Digit('8')), Command::BuildWorker),
    (Chord::key(Key::Digit('9')), Command::Grow),
    (Chord::key(Key::Digit('0')), Command::Gather),
    (Chord::key(Key::Backspace), Command::Remove),
    (Chord::key(Key::Delete), Command::Disband),
    (Chord::key(Key::PageDown), Command::QueueHeadDown),
    (Chord::key(Key::F(1)), Command::Scenario(Scenario::Combat)),
    (Chord::key(Key::F(2)), Command::Scenario(Scenario::Cities)),
    (Chord::key(Key::F(3)), Command::Scenario(Scenario::Frontier)),
    (Chord::key(Key::F(4)), Command::Scenario(Scenario::World)),
    (Chord::key(Key::F(12)), Command::Scenario(Scenario::Siege)),
    (Chord::key(Key::Letter('Y')), Command::Yields),
    (Chord::key(Key::Letter('G')), Command::Guard),
    (Chord::key(Key::Letter('E')), Command::Alert),
    (Chord::key(Key::F(5)), Command::Fullscreen),
    (Chord::key(Key::F(6)), Command::Save),
    (Chord::key(Key::F(7)), Command::Load),
    (Chord::key(Key::F(8)), Command::Playback),
    (Chord::key(Key::F(9)), Command::FinishBuild),
    (Chord::key(Key::F(10)), Command::Fog),
    (Chord::key(Key::F(11)), Command::Presentation),
    (Chord::ctrl_shift(Key::Letter('R')), Command::ResetLayout),
];

/// The keys while typing into a text field; any other key types its text.
pub const TYPING: &[(Chord, Command)] = &[
    (Chord::ctrl(Key::Letter('V')), Command::Paste),
    (Chord::ctrl(Key::Letter('C')), Command::Copy),
    (Chord::ctrl(Key::Letter('X')), Command::Cut),
    (Chord::key(Key::Backspace), Command::Erase),
    (Chord::key(Key::Enter), Command::StopTyping),
    (Chord::key(Key::Tab), Command::StopTyping),
    (Chord::key(Key::Escape), Command::StopTyping),
];

/// What `key` does in `map` with Ctrl and Shift held as given: of the
/// bindings of `key` whose modifiers are all held, the one needing the
/// most. A modifier a binding doesn't need doesn't stop it (Shift+M still
/// arms Move).
pub fn command_for(map: &[(Chord, Command)], key: Key, ctrl: bool, shift: bool) -> Option<Command> {
    map.iter()
        .filter(|(chord, _)| chord.key == key && (ctrl || !chord.ctrl) && (shift || !chord.shift))
        .max_by_key(|(chord, _)| chord.modifiers())
        .map(|&(_, command)| command)
}

impl Command {
    /// The chord it's on, the first if it has more than one.
    fn chord(self) -> Option<Chord> {
        PLAYING
            .iter()
            .chain(TYPING)
            .find(|&&(_, command)| command == self)
            .map(|&(chord, _)| chord)
    }

    /// Its key's name, as the UI shows it (`F8`, `CTRL+V`), to fill a text
    /// file's key placeholder; empty for a command no key gives.
    pub fn key(self) -> String {
        self.chord()
            .map(|chord| chord.to_string())
            .unwrap_or_default()
    }

    /// `key` for text in lower case, each part capitalized: `Ctrl+Shift+R`.
    pub fn key_in_words(self) -> String {
        self.key()
            .split('+')
            .map(|part| {
                let mut chars = part.chars();
                chars.next().map_or_else(String::new, |first| {
                    first.to_string() + &chars.as_str().to_lowercase()
                })
            })
            .collect::<Vec<_>>()
            .join("+")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_two_bindings_share_a_chord() {
        for map in [PLAYING, TYPING] {
            for (i, (chord, _)) in map.iter().enumerate() {
                assert!(
                    !map[i + 1..].iter().any(|(other, _)| other == chord),
                    "{chord} is bound twice"
                );
            }
        }
    }

    #[test]
    fn a_key_does_what_its_most_specific_binding_says() {
        let r = Key::Letter('R');
        assert_eq!(command_for(PLAYING, r, false, false), Some(Command::Road));
        // A modifier the binding doesn't need is ignored...
        assert_eq!(command_for(PLAYING, r, true, false), Some(Command::Road));
        assert_eq!(
            command_for(PLAYING, Key::Letter('M'), false, true),
            Some(Command::Move)
        );
        // ...and one that needs it wins when it's held.
        assert_eq!(
            command_for(PLAYING, r, true, true),
            Some(Command::ResetLayout)
        );
        // Typing: V types a V; Ctrl+V pastes.
        let v = Key::Letter('V');
        assert_eq!(command_for(TYPING, v, false, false), None);
        assert_eq!(command_for(TYPING, v, true, false), Some(Command::Paste));
        assert_eq!(
            command_for(TYPING, Key::Escape, false, false),
            Some(Command::StopTyping)
        );
        assert_eq!(
            command_for(PLAYING, Key::F(11), false, false),
            Some(Command::Presentation)
        );
    }

    #[test]
    fn a_command_is_named_by_its_key() {
        assert_eq!(Command::Back.key(), "ESC");
        assert_eq!(Command::Playback.key(), "F8");
        assert_eq!(Command::Scenario(Scenario::World).key(), "F4");
        assert_eq!(Command::Paste.key(), "CTRL+V");
        assert_eq!(Command::StopTyping.key(), "ENTER");
        assert_eq!(Command::ResetLayout.key(), "CTRL+SHIFT+R");
        assert_eq!(Command::ResetLayout.key_in_words(), "Ctrl+Shift+R");
        assert_eq!(Command::Playback.key_in_words(), "F8");
        assert_eq!(Command::ResetLayout.key_in_words(), "Ctrl+Shift+R");
        assert_eq!(Command::Playback.key_in_words(), "F8");
        assert_eq!(Command::Disband.key(), "DEL");
        // The Naval scenario has no key.
        assert_eq!(Command::Scenario(Scenario::Naval).key(), "");
    }
}
