//! Keeping the player's settings and layout between sessions: small text
//! files in the game's config folder (`%APPDATA%\riskofcivlike` on Windows,
//! `$XDG_CONFIG_HOME/riskofcivlike` or `~/.config/riskofcivlike` elsewhere).
//! Each file is written whole, through a temporary file and a rename, so a
//! crash mid-write can't leave half of one. Reading a missing or unreadable
//! file gives `None`, and the game starts with its defaults; a failed write
//! is logged and otherwise ignored. What goes in each file is up to its
//! owner: `Settings::to_text` (`settings.txt`), `ImGuiLayoutState::to_text`
//! and the window's size (`layout.txt`), and ImGui's own docking data
//! (`imgui.ini`).

use std::path::PathBuf;

/// The folder the files live in, if the environment names one.
fn config_dir() -> Option<PathBuf> {
    const APP: &str = "riskofcivlike";
    let var = |name| std::env::var_os(name).filter(|value| !value.is_empty());
    if cfg!(windows) {
        return var("APPDATA").map(|dir| PathBuf::from(dir).join(APP));
    }
    var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| var("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|dir| dir.join(APP))
}

/// The contents of config file `name`, if it's there.
pub fn read(name: &str) -> Option<String> {
    std::fs::read_to_string(config_dir()?.join(name)).ok()
}

/// Replaces config file `name` with `contents`.
pub fn write(name: &str, contents: &str) {
    let Some(dir) = config_dir() else {
        return;
    };
    let path = dir.join(name);
    let temporary = dir.join(format!("{name}.tmp"));
    let result = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(&temporary, contents))
        .and_then(|()| std::fs::rename(&temporary, &path))
        .or_else(|_| {
            // Some file systems (a sandbox's, say) won't rename across the
            // layers they keep: write the file in place instead.
            let _ = std::fs::remove_file(&temporary);
            std::fs::write(&path, contents)
        });
    if let Err(err) = result {
        log::warn!("couldn't save {}: {err}", path.display());
    }
}

/// Splits a line of saved text into its key and the rest, skipping blank
/// lines and `#` comments.
pub fn key_and_values(line: &str) -> Option<(&str, Vec<&str>)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let mut words = line.split_whitespace();
    let key = words.next()?;
    Some((key, words.collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_split_into_a_key_and_values() {
        assert_eq!(
            key_and_values("  window 2 10.5 -3 "),
            Some(("window", vec!["2", "10.5", "-3"]))
        );
        assert_eq!(key_and_values("# a comment"), None);
        assert_eq!(key_and_values("   "), None);
        assert_eq!(key_and_values("alone"), Some(("alone", vec![])));
    }
}
