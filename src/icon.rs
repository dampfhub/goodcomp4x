//! The window and taskbar icon, from the pixels `icon_art.rs` draws.

use winit::window::Icon;

use crate::icon_art;

/// The icon rendered at `size` by `size` pixels.
pub fn icon(size: u32) -> Icon {
    Icon::from_rgba(icon_art::rgba(size), size, size).expect("icon pixels match its size")
}

/// The game's application id on Windows.
#[cfg(windows)]
const APP_USER_MODEL_ID: &str = "dampfhub.riskofcivlike";

/// Gives the process its own application id on Windows, before any window
/// exists. Without one, Windows groups the taskbar button by the executable
/// and can show a blank default icon for it, ignoring the icon the window
/// sets; with one, the button shows the window's icon. Does nothing
/// elsewhere, or if Windows refuses (the icon is then no worse than before).
pub fn claim_taskbar_identity() {
    #[cfg(windows)]
    {
        #[link(name = "shell32")]
        unsafe extern "system" {
            fn SetCurrentProcessExplicitAppUserModelID(app_id: *const u16) -> i32;
        }
        let id: Vec<u16> = APP_USER_MODEL_ID.encode_utf16().chain([0]).collect();
        // SAFETY: `id` is a NUL-terminated UTF-16 string that outlives the call.
        let result = unsafe { SetCurrentProcessExplicitAppUserModelID(id.as_ptr()) };
        if result < 0 {
            log::warn!("couldn't set the taskbar application id (HRESULT {result:#x})");
        }
    }
}
