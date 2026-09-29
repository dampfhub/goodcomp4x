//! The system clipboard, as text: the Multiplayer page's copy and paste
//! (`docs/multiplayer.md`). `App` uses it for the classic fields and the
//! COPY buttons, and hands it to ImGui (`ImGuiClipboard`) for its text
//! boxes; the game itself never touches it. Windows only for now, through
//! the Win32 clipboard calls of `windows-sys`: elsewhere there is no
//! clipboard (`get_text` is `None`, `set_text` is `false`).

/// The clipboard's text, if it holds any.
pub fn get_text() -> Option<String> {
    platform::get_text()
}

/// Puts `text` on the clipboard in place of what's there: whether it could.
pub fn set_text(text: &str) -> bool {
    platform::set_text(text)
}

/// ImGui's text boxes' Ctrl+C, Ctrl+X and Ctrl+V, through `get_text` and
/// `set_text` (`imgui::Context::set_clipboard_backend`).
pub struct ImGuiClipboard;

impl imgui::ClipboardBackend for ImGuiClipboard {
    fn get(&mut self) -> Option<String> {
        get_text()
    }

    fn set(&mut self, value: &str) {
        set_text(value);
    }
}

#[cfg(windows)]
mod platform {
    use std::ptr;
    use std::thread;
    use std::time::Duration;

    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
    };
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

    /// The clipboard, open to this thread until dropped. Another program
    /// may hold it for a moment, so opening tries a few times.
    struct Open;

    impl Open {
        fn new() -> Option<Self> {
            for _ in 0..5 {
                // SAFETY: no pointers; a null owner window is allowed and
                // opens the clipboard for this task.
                if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
                    return Some(Open);
                }
                thread::sleep(Duration::from_millis(2));
            }
            None
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            // SAFETY: this thread opened the clipboard (`Open::new`), and
            // closes it once.
            unsafe { CloseClipboard() };
        }
    }

    pub fn get_text() -> Option<String> {
        let _open = Open::new()?;
        // SAFETY: the clipboard is open. The handle stays the clipboard's:
        // it is only read here, never freed.
        let handle = unsafe { GetClipboardData(u32::from(CF_UNICODETEXT)) };
        if handle.is_null() {
            return None;
        }
        // SAFETY: `handle` is the clipboard's global memory for
        // CF_UNICODETEXT, valid while the clipboard is open.
        let data = unsafe { GlobalLock(handle) }.cast::<u16>();
        if data.is_null() {
            return None;
        }
        // SAFETY: `GlobalLock` gave `data`, which points at the handle's
        // `GlobalSize` bytes until `GlobalUnlock`; the text is read no
        // further than that, up to its terminating zero.
        let text = unsafe {
            let units = std::slice::from_raw_parts(data, GlobalSize(handle) / 2);
            let end = units.iter().position(|&unit| unit == 0);
            String::from_utf16_lossy(&units[..end.unwrap_or(units.len())])
        };
        // SAFETY: unlocks the lock taken above; `data` isn't used after.
        unsafe { GlobalUnlock(handle) };
        Some(text)
    }

    pub fn set_text(text: &str) -> bool {
        let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
        let Some(_open) = Open::new() else {
            return false;
        };
        // SAFETY: the clipboard is open.
        if unsafe { EmptyClipboard() } == 0 {
            return false;
        }
        // SAFETY: allocates movable global memory for the text and its
        // terminating zero, as the clipboard requires.
        let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) };
        if memory.is_null() {
            return false;
        }
        // SAFETY: `memory` was just allocated, `wide.len()` units long.
        let data = unsafe { GlobalLock(memory) }.cast::<u16>();
        if data.is_null() {
            // SAFETY: `memory` is ours and not given to the clipboard.
            unsafe { GlobalFree(memory) };
            return false;
        }
        // SAFETY: `data` has room for `wide.len()` units (above), and the
        // two don't overlap; unlocking ends the use of `data`.
        unsafe {
            ptr::copy_nonoverlapping(wide.as_ptr(), data, wide.len());
            GlobalUnlock(memory);
        }
        // SAFETY: the clipboard is open and emptied by this thread; on
        // success it owns `memory`, which must then not be freed here.
        if unsafe { SetClipboardData(u32::from(CF_UNICODETEXT), memory) }.is_null() {
            // SAFETY: the clipboard refused `memory`, so it's still ours.
            unsafe { GlobalFree(memory) };
            return false;
        }
        true
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn get_text() -> Option<String> {
        None
    }

    pub fn set_text(_text: &str) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    /// The real clipboard, by hand on Windows: `cargo test clipboard --
    /// --ignored`. It replaces what's on the clipboard.
    #[test]
    #[ignore = "uses the system clipboard"]
    fn text_goes_onto_the_clipboard_and_back() {
        let text = "192.168.1.20:55741 K7M2QX \u{e9}\u{1F600}";
        assert!(super::set_text(text));
        assert_eq!(super::get_text().as_deref(), Some(text));
        assert!(super::set_text(""));
        assert_eq!(super::get_text().as_deref(), Some(""));
    }
}
