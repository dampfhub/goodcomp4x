//! The game's text, from the files in `text/` (how to edit them:
//! `docs/text.md`). Each entry is a tag, such as `settings_close_button`,
//! with its `text` and, optionally, a `tooltip` and a `hover_text` (what a
//! label changes to while hovered). A `{name}` in any of them is a
//! placeholder the code fills in; `{food_icon}`, `{wood_icon}`,
//! `{metal_icon}` and `{time_icon}` are the map's icon characters
//! (`map_icons.rs`), put in as the file is read.
//!
//! Code asks for text by tag, always a literal, through the macros:
//!
//! - `text!("tag")` is the entry's text, a `&'static str`;
//!   `text!("tag", food = 3, turns = t)` fills its placeholders into a `String`;
//! - `tooltip!(..)` and `hover_text!(..)` are the same for those fields.
//!
//! The files are embedded (`include_str!`, `FILES`) and read once, the first
//! time any text is asked for. The tests below read the source for every
//! call of the macros and check the files against them: each tag used
//! exists and has the field asked for, each tag and field in the files is
//! used, and a call's placeholders are exactly the entry's. A file that
//! doesn't parse fails them too. At runtime nothing panics: a tag the files
//! lack shows as the tag itself, and a placeholder with no value as itself.

use std::fmt::{Display, Write as _};
use std::sync::{LazyLock, Mutex};

use super::fast_hash::HashMap;
use super::map_icons::{FOOD_ICON, METAL_ICON, TIME_ICON, WOOD_ICON};

/// The text files, by name, embedded in the executable. A new file is a
/// line here.
const FILES: [(&str, &str); 1] = [("menus.ini", include_str!("../../text/menus.ini"))];

/// The icon names a file may write in braces, and the icon characters they
/// stand for.
const ICONS: [(&str, char); 4] = [
    ("food_icon", FOOD_ICON),
    ("wood_icon", WOOD_ICON),
    ("metal_icon", METAL_ICON),
    ("time_icon", TIME_ICON),
];

/// One tag's text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::game) struct Entry {
    pub text: String,
    /// What a label changes to while the cursor is on it.
    pub hover_text: Option<String>,
    pub tooltip: Option<String>,
}

/// A field of an `Entry`, as a file names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::game) enum Field {
    Text,
    HoverText,
    Tooltip,
}

impl Field {
    const ALL: [Field; 3] = [Field::Text, Field::HoverText, Field::Tooltip];

    /// Its key in a file, and the name of its macro.
    fn key(self) -> &'static str {
        match self {
            Field::Text => "text",
            Field::HoverText => "hover_text",
            Field::Tooltip => "tooltip",
        }
    }

    fn of(self, entry: &Entry) -> Option<&str> {
        match self {
            Field::Text => Some(&entry.text),
            Field::HoverText => entry.hover_text.as_deref(),
            Field::Tooltip => entry.tooltip.as_deref(),
        }
    }

    fn slot(self, entry: &mut Entry) -> &mut Option<String> {
        match self {
            Field::HoverText => &mut entry.hover_text,
            Field::Tooltip => &mut entry.tooltip,
            // Parsing keeps the text in an `Option` of its own until the
            // entry is complete.
            Field::Text => unreachable!("text is kept apart while parsing"),
        }
    }
}

/// Every entry of a set of files, by tag.
#[derive(Debug, Default)]
pub(in crate::game) struct Texts {
    entries: HashMap<String, Entry>,
}

/// What's wrong with a line of a text file.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::game) struct ParseError {
    pub file: String,
    pub line: usize,
    pub message: String,
}

impl Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "text/{}:{}: {}", self.file, self.line, self.message)
    }
}

/// An entry being read: its tag, where it starts, its fields' raw values
/// so far (continuation lines join them before they're checked), and
/// whether a line of it was wrong, which drops it.
struct Pending {
    tag: String,
    line: usize,
    fields: Vec<(Field, String, usize)>,
    broken: bool,
}

impl Texts {
    /// Reads `files` (name, contents), keeping every entry that reads
    /// cleanly, and says what's wrong with the rest.
    pub(in crate::game) fn parse(files: &[(&str, &str)]) -> (Texts, Vec<ParseError>) {
        let mut texts = Texts::default();
        let mut errors = Vec::new();
        // Where each tag was first seen, so a repeat can say so.
        let mut seen: HashMap<String, (String, usize)> = HashMap::default();
        for &(file, source) in files {
            let mut error = |line: usize, message: String| {
                errors.push(ParseError {
                    file: file.into(),
                    line,
                    message,
                })
            };
            let mut pending: Option<Pending> = None;
            // Whether the line before was a field's, which an indented line
            // continues.
            let mut continues = false;
            for (index, line) in source.lines().enumerate() {
                let number = index + 1;
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continues = false;
                    continue;
                }
                let problem = if line.starts_with([' ', '\t']) {
                    match pending.as_mut().and_then(|p| p.fields.last_mut()) {
                        Some((_, value, _)) if continues => {
                            value.push(' ');
                            value.push_str(trimmed);
                            None
                        }
                        _ => Some(
                            "an indented line continues the value on the line before it, \
                             and there is none"
                                .to_string(),
                        ),
                    }
                } else if let Some(rest) = trimmed.strip_prefix('[') {
                    continues = false;
                    if let Some(done) = pending.take() {
                        texts.finish(done, &mut error);
                    }
                    let tag = rest.strip_suffix(']').unwrap_or(rest);
                    let problem = if !rest.ends_with(']') {
                        Some(format!("{trimmed:?} opens a [tag] and doesn't close it"))
                    } else if !is_name(tag) {
                        Some(format!(
                            "[{tag}]: a tag is lower-case letters, digits and _, \
                             starting with a letter"
                        ))
                    } else if let Some((first_file, first_line)) = seen.get(tag) {
                        Some(format!(
                            "[{tag}] again: first at text/{first_file}:{first_line}"
                        ))
                    } else {
                        seen.insert(tag.into(), (file.into(), number));
                        None
                    };
                    pending = Some(Pending {
                        tag: tag.into(),
                        line: number,
                        fields: Vec::new(),
                        broken: false,
                    });
                    problem
                } else {
                    continues = false;
                    match trimmed.split_once('=') {
                        None => Some(format!(
                            "{trimmed:?}: expected a [tag], a `key = value` line or a # comment"
                        )),
                        Some((key, value)) => {
                            let key = key.trim();
                            let field = Field::ALL.into_iter().find(|f| f.key() == key);
                            match (field, pending.as_mut()) {
                                (None, _) => {
                                    Some(format!("{key:?}: a key is text, hover_text or tooltip"))
                                }
                                (_, None) => Some(format!("{key} before any [tag]")),
                                (Some(field), Some(entry)) => {
                                    if entry.fields.iter().any(|(f, ..)| *f == field) {
                                        Some(format!("[{}] has {key} twice", entry.tag))
                                    } else {
                                        entry.fields.push((field, value.trim().into(), number));
                                        continues = true;
                                        None
                                    }
                                }
                            }
                        }
                    }
                };
                if let Some(message) = problem {
                    error(number, message);
                    if let Some(entry) = pending.as_mut() {
                        entry.broken = true;
                    }
                }
            }
            if let Some(done) = pending.take() {
                texts.finish(done, &mut error);
            }
        }
        (texts, errors)
    }

    /// Checks a finished entry's values and keeps it if it reads cleanly.
    fn finish(&mut self, pending: Pending, error: &mut impl FnMut(usize, String)) {
        let mut entry = Entry::default();
        let mut text = None;
        let mut clean = !pending.broken;
        for (field, raw, line) in pending.fields {
            match value(&raw) {
                Ok(value) if field == Field::Text => text = Some(value),
                Ok(value) => *field.slot(&mut entry) = Some(value),
                Err(why) => {
                    error(line, format!("[{}] {}: {why}", pending.tag, field.key()));
                    clean = false;
                }
            }
        }
        if !clean {
            return;
        }
        match text {
            Some(text) => entry.text = text,
            None => return error(pending.line, format!("[{}] has no text", pending.tag)),
        }
        self.entries.insert(pending.tag, entry);
    }

    /// The entry `tag` names.
    pub(in crate::game) fn get(&self, tag: &str) -> Option<&Entry> {
        self.entries.get(tag)
    }

    /// Every tag and its entry, in no particular order.
    #[cfg(test)]
    pub(in crate::game) fn entries(&self) -> impl Iterator<Item = (&str, &Entry)> {
        self.entries
            .iter()
            .map(|(tag, entry)| (tag.as_str(), entry))
    }
}

/// Whether `name` is a tag or placeholder name: a lower-case letter, then
/// lower-case letters, digits and underscores.
fn is_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// A field's value from its raw text: without the double quotes around it,
/// if it has them (they keep spaces at its ends), and with the icon names
/// in braces made icon characters. Every other brace must be a
/// placeholder's.
fn value(raw: &str) -> Result<String, String> {
    let unquoted = match raw.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        Some(inner) => inner,
        None => raw,
    };
    let mut out = String::with_capacity(unquoted.len());
    let mut rest = unquoted;
    while let Some(open) = rest.find(['{', '}']) {
        out.push_str(&rest[..open]);
        if rest[open..].starts_with('}') {
            return Err("a } with no { before it".into());
        }
        let Some(close) = rest[open..].find('}') else {
            return Err("a { with no } after it".into());
        };
        let name = &rest[open + 1..open + close];
        if !is_name(name) {
            return Err(format!(
                "{{{name}}}: a placeholder is lower-case letters, digits and _, starting with a letter"
            ));
        }
        match ICONS.iter().find(|(icon, _)| *icon == name) {
            Some(&(_, ch)) => out.push(ch),
            None => out.push_str(&rest[open..=open + close]),
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The placeholders in `template`, in order, each once.
#[cfg(test)]
pub(in crate::game) fn placeholders(template: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let name = &rest[open + 1..open + close];
        if !names.contains(&name) {
            names.push(name);
        }
        rest = &rest[open + close + 1..];
    }
    names
}

/// `template` with each `{name}` in `values` filled in. A placeholder with
/// no value stays as it is (the tests make sure none is missed).
pub(in crate::game) fn fill(template: &str, values: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len() + 8);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let name = &rest[open + 1..open + close];
        match values.iter().find(|(n, _)| *n == name) {
            Some((_, value)) => {
                let _ = write!(out, "{value}");
            }
            None => out.push_str(&rest[open..=open + close]),
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

/// The game's text, read from `FILES` the first time it's asked for. A
/// file that doesn't parse is a test failure; if one slipped through, its
/// errors are logged and the entries that did parse are used.
static TEXTS: LazyLock<Texts> = LazyLock::new(|| {
    let (texts, errors) = Texts::parse(&FILES);
    for error in errors {
        log::error!("{error}");
    }
    texts
});

/// Every entry of the game's text files, for the tests.
#[cfg(test)]
pub(in crate::game) fn texts() -> &'static Texts {
    &TEXTS
}

/// `tag`'s `field`, or, if the files lack it, the tag itself (logged once).
pub(in crate::game) fn lookup(tag: &'static str, field: Field) -> &'static str {
    match TEXTS.get(tag).and_then(|entry| field.of(entry)) {
        Some(text) => text,
        None => {
            static REPORTED: Mutex<Vec<(&str, Field)>> = Mutex::new(Vec::new());
            if let Ok(mut reported) = REPORTED.lock()
                && !reported.contains(&(tag, field))
            {
                reported.push((tag, field));
                log::error!("no {} for [{tag}] in text/", field.key());
            }
            tag
        }
    }
}

/// An entry's text: `text!("tag")`, or `text!("tag", name = value, ..)` to
/// fill its placeholders. The tag is always a literal, so the tests can
/// find it (module comment).
macro_rules! text {
    ($tag:literal) => {
        $crate::game::strings::lookup($tag, $crate::game::strings::Field::Text)
    };
    ($tag:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::game::strings::fill(
            $crate::game::strings::lookup($tag, $crate::game::strings::Field::Text),
            &[$((stringify!($name), &$value as &dyn std::fmt::Display)),+],
        )
    };
}

/// An entry's tooltip, as `text!` gives its text.
macro_rules! tooltip {
    ($tag:literal) => {
        $crate::game::strings::lookup($tag, $crate::game::strings::Field::Tooltip)
    };
    ($tag:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::game::strings::fill(
            $crate::game::strings::lookup($tag, $crate::game::strings::Field::Tooltip),
            &[$((stringify!($name), &$value as &dyn std::fmt::Display)),+],
        )
    };
}

/// An entry's hover text, as `text!` gives its text: what a label shows
/// while the cursor is on it. Nothing shows one yet (stage 1 of #341 moved
/// only the settings menu, whose labels don't change on hover).
#[allow(unused_macros)]
macro_rules! hover_text {
    ($tag:literal) => {
        $crate::game::strings::lookup($tag, $crate::game::strings::Field::HoverText)
    };
    ($tag:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::game::strings::fill(
            $crate::game::strings::lookup($tag, $crate::game::strings::Field::HoverText),
            &[$((stringify!($name), &$value as &dyn std::fmt::Display)),+],
        )
    };
}

// Unused until a label changes on hover (see `hover_text!`).
#[allow(unused_imports)]
pub(crate) use hover_text;
pub(crate) use {text, tooltip};

#[cfg(test)]
mod tests {
    use super::*;

    /// A call of one of the text macros in the source.
    #[derive(Debug)]
    struct Call {
        file: String,
        line: usize,
        field: Field,
        /// The tag, if the call's first argument is a plain string literal.
        tag: Option<String>,
        /// The placeholders it fills.
        names: Vec<String>,
    }

    /// Every call of `text!`, `tooltip!` and `hover_text!` in Rust source
    /// `code`, outside comments and literals.
    fn macro_calls(file: &str, code: &str) -> Vec<Call> {
        let chars: Vec<char> = code.chars().collect();
        let line_at = |i: usize| chars[..i].iter().filter(|&&c| c == '\n').count() + 1;
        let is_ident = |c: char| c.is_alphanumeric() || c == '_';
        let mut calls = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            if let Some(end) = skip_comment(&chars, i).or_else(|| skip_literal(&chars, i)) {
                i = end;
            } else if is_ident(chars[i]) && (i == 0 || !is_ident(chars[i - 1])) {
                let start = i;
                while i < chars.len() && is_ident(chars[i]) {
                    i += 1;
                }
                let name: String = chars[start..i].iter().collect();
                let field = Field::ALL.into_iter().find(|f| f.key() == name);
                if let Some(field) = field
                    && chars[i..].starts_with(&['!', '('])
                {
                    let (tag, names, end) = call_arguments(&chars, i + 2);
                    calls.push(Call {
                        file: file.into(),
                        line: line_at(start),
                        field,
                        tag,
                        names,
                    });
                    i = end;
                }
            } else {
                i += 1;
            }
        }
        calls
    }

    /// Where a comment starting at `i` ends, if one does.
    fn skip_comment(chars: &[char], i: usize) -> Option<usize> {
        let rest = &chars[i..];
        if rest.starts_with(&['/', '/']) {
            Some(i + rest.iter().position(|&c| c == '\n').unwrap_or(rest.len()))
        } else if rest.starts_with(&['/', '*']) {
            Some(
                i + rest
                    .windows(2)
                    .position(|pair| pair == ['*', '/'])
                    .map_or(rest.len(), |at| at + 2),
            )
        } else {
            None
        }
    }

    /// Where a string, raw string or character literal starting at `i`
    /// ends, if one does.
    fn skip_literal(chars: &[char], i: usize) -> Option<usize> {
        let rest = &chars[i..];
        let ident_before = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
        let to = |mut j: usize, end: &[char], raw: bool| {
            while j < chars.len() && !chars[j..].starts_with(end) {
                j += if !raw && chars[j] == '\\' { 2 } else { 1 };
            }
            (j + end.len()).min(chars.len())
        };
        if rest[0] == '"' {
            Some(to(i + 1, &['"'], false))
        } else if rest[0] == 'r'
            && !ident_before
            && let Some(hashes) = rest[1..].iter().position(|&c| c != '#')
            && rest[1 + hashes] == '"'
        {
            let end: Vec<char> = std::iter::once('"')
                .chain(std::iter::repeat_n('#', hashes))
                .collect();
            Some(to(i + hashes + 2, &end, true))
        } else if rest[0] == '\'' && (rest.get(1) == Some(&'\\') || rest.get(2) == Some(&'\'')) {
            Some(to(i + 1, &['\''], false))
        } else {
            None
        }
    }

    /// A macro call's arguments from just inside its `(`: the tag if the
    /// first is a string literal, the names of the `name = value` ones, and
    /// where the call ends.
    fn call_arguments(chars: &[char], mut i: usize) -> (Option<String>, Vec<String>, usize) {
        let mut arguments = Vec::new();
        let mut current = String::new();
        let mut depth = 0;
        while i < chars.len() {
            if let Some(end) = skip_comment(chars, i) {
                i = end;
                continue;
            }
            if let Some(end) = skip_literal(chars, i) {
                current.extend(&chars[i..end]);
                i = end;
                continue;
            }
            let c = chars[i];
            i += 1;
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' if depth > 0 => depth -= 1,
                ')' => break,
                ',' if depth == 0 => {
                    arguments.push(std::mem::take(&mut current));
                    continue;
                }
                _ => {}
            }
            current.push(c);
        }
        arguments.push(current);
        let mut arguments = arguments.iter().map(|a| a.trim()).filter(|a| !a.is_empty());
        let tag = arguments.next().and_then(|first| {
            let inner = first.strip_prefix('"')?.strip_suffix('"')?;
            (!inner.contains(['"', '\\'])).then(|| inner.to_string())
        });
        let names = arguments
            .map(|a| a.split_once('=').map_or(a, |(name, _)| name).trim().into())
            .collect();
        (tag, names, i)
    }

    /// Every call of the text macros in `src/`.
    fn source_calls() -> Vec<Call> {
        fn sources(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    sources(&path, files);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    files.push(path);
                }
            }
        }
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        sources(&src, &mut files);
        let mut calls = Vec::new();
        for path in files {
            let code = std::fs::read_to_string(&path).unwrap();
            let name = path
                .strip_prefix(&src)
                .unwrap()
                .display()
                .to_string()
                .replace('\\', "/");
            calls.extend(macro_calls(&name, &code));
        }
        calls
    }

    #[test]
    fn the_text_files_parse() {
        let (texts, errors) = Texts::parse(&FILES);
        let errors: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert!(errors.is_empty(), "{}", errors.join("\n"));
        assert!(texts.entries().count() > 0);
    }

    #[test]
    fn every_tag_the_code_uses_is_in_the_files_with_its_placeholders() {
        let calls = source_calls();
        // The settings menu's text, at least (a check on the scan itself).
        assert!(calls.len() > 20, "{calls:?}");
        let mut wrong = Vec::new();
        for call in &calls {
            let at = format!("src/{}:{}", call.file, call.line);
            let Some(tag) = &call.tag else {
                wrong.push(format!("{at}: the tag must be a plain string literal"));
                continue;
            };
            let Some(entry) = texts().get(tag) else {
                wrong.push(format!("{at}: no [{tag}] in text/"));
                continue;
            };
            let Some(template) = call.field.of(entry) else {
                wrong.push(format!("{at}: [{tag}] has no {}", call.field.key()));
                continue;
            };
            let mut wanted: Vec<&str> = placeholders(template);
            wanted.sort_unstable();
            let mut given: Vec<&str> = call.names.iter().map(String::as_str).collect();
            given.sort_unstable();
            if wanted != given {
                wrong.push(format!(
                    "{at}: [{tag}] {} has placeholders {wanted:?}, the code fills {given:?}",
                    call.field.key()
                ));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn every_tag_and_field_in_the_files_is_used() {
        let calls = source_calls();
        let mut unused = Vec::new();
        for (tag, entry) in texts().entries() {
            for field in Field::ALL {
                let used = calls
                    .iter()
                    .any(|call| call.field == field && call.tag.as_deref() == Some(tag));
                if field.of(entry).is_some() && !used {
                    unused.push(format!("[{tag}] {}", field.key()));
                }
            }
        }
        unused.sort();
        assert!(
            unused.is_empty(),
            "in text/ but no code shows it:\n{}",
            unused.join("\n")
        );
    }

    #[test]
    fn a_file_reads_entries_fields_quotes_continuations_and_icons() {
        let file = "# A comment.\n\
                    [end_turn_button]\n\
                    text = END TURN\n\
                    hover_text = END TURN {turn}\n\
                    tooltip = ENDS YOUR PLANNING. THE TURN RESOLVES\n  \
                      WHEN EVERY SIDE HAS ENDED.\n\
                    \n\
                    [price]\n\
                    text = \"  {food}{food_icon} \"\n";
        let (texts, errors) = Texts::parse(&[("ui.ini", file)]);
        assert_eq!(errors, []);
        let end_turn = texts.get("end_turn_button").unwrap();
        assert_eq!(end_turn.text, "END TURN");
        assert_eq!(end_turn.hover_text.as_deref(), Some("END TURN {turn}"));
        assert_eq!(
            end_turn.tooltip.as_deref(),
            Some("ENDS YOUR PLANNING. THE TURN RESOLVES WHEN EVERY SIDE HAS ENDED.")
        );
        // Quotes keep the spaces at the ends; the icon name is the icon.
        let price = texts.get("price").unwrap();
        assert_eq!(price.text, format!("  {{food}}{FOOD_ICON} "));
        assert_eq!(price.hover_text, None);
        assert_eq!(placeholders(&price.text), ["food"]);
        assert_eq!(
            fill(&end_turn.hover_text.clone().unwrap(), &[("turn", &12)]),
            "END TURN 12"
        );
    }

    #[test]
    fn a_malformed_file_says_where_and_why() {
        let lines = |file: &str| -> Vec<(usize, String)> {
            let (_, errors) = Texts::parse(&[("bad.ini", file)]);
            errors.into_iter().map(|e| (e.line, e.message)).collect()
        };
        let one = |file: &str, line: usize, says: &str| {
            let errors = lines(file);
            assert!(
                errors.len() == 1 && errors[0].0 == line && errors[0].1.contains(says),
                "{file:?}: {errors:?}"
            );
        };
        one("text = A\n", 1, "before any [tag]");
        one("[a]\ntext = A\n[a]\ntext = B\n", 3, "again");
        one("[a]\ntext = A\ntext = B\n", 3, "twice");
        one("[a]\nlabel = A\n", 2, "a key is");
        one("[a]\ntooltip = A\n", 1, "has no text");
        one("[A]\ntext = A\n", 1, "a tag is");
        one("[a\ntext = A\n", 1, "doesn't close");
        one("[a]\ntext = {food\n", 2, "no }");
        one("[a]\ntext = food}\n", 2, "no {");
        one("[a]\ntext = {Food}\n", 2, "a placeholder is");
        one("[a]\ntext A\n", 2, "expected");
        one("[a]\n\n  more\n", 3, "continues");
        // An entry with an error isn't kept; the others are.
        let (texts, _) = Texts::parse(&[("bad.ini", "[a]\ntext = {\n[b]\ntext = B\n")]);
        assert!(texts.get("a").is_none());
        assert_eq!(texts.get("b").unwrap().text, "B");
        // A tag in two files is one too many.
        let (_, errors) =
            Texts::parse(&[("a.ini", "[a]\ntext = A\n"), ("b.ini", "[a]\ntext = B\n")]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("text/a.ini:1"), "{errors:?}");
    }

    #[test]
    fn filling_leaves_a_placeholder_without_a_value_as_it_is() {
        assert_eq!(
            fill("{a} AND {b}, {a}", &[("a", &"ONE")]),
            "ONE AND {b}, ONE"
        );
        assert_eq!(placeholders("{a} AND {b}, {a}"), ["a", "b"]);
    }

    #[test]
    fn a_missing_tag_shows_as_itself() {
        assert_eq!(
            lookup("no_such_tag_anywhere", Field::Text),
            "no_such_tag_anywhere"
        );
    }

    #[test]
    fn the_scan_finds_calls_and_their_placeholders() {
        // Written as a string, so the scan of this file doesn't count it.
        let code = "// text!(\"in_a_comment\")\n\
                    let a = text!(\"plain\");\n\
                    let s = \"text!(\\\"in_a_string\\\")\";\n\
                    let b = tooltip!(\"filled\", food = f(1, 2), turns = [3, 4].len(),);\n\
                    let c = hover_text!(TAG);\n\
                    let d = rich_text!(\"not_ours\");\n";
        let calls = macro_calls("x.rs", code);
        let found: Vec<_> = calls
            .iter()
            .map(|c| (c.line, c.field, c.tag.clone(), c.names.clone()))
            .collect();
        assert_eq!(
            found,
            [
                (2, Field::Text, Some("plain".into()), vec![]),
                (
                    4,
                    Field::Tooltip,
                    Some("filled".into()),
                    vec!["food".to_string(), "turns".to_string()]
                ),
                (5, Field::HoverText, None, vec![]),
            ]
        );
    }
}
