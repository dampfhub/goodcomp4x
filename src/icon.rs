//! The window and taskbar icon, from the pixels `icon_art.rs` draws.

use winit::window::Icon;

use crate::icon_art;

/// The icon rendered at `size` by `size` pixels.
pub fn icon(size: u32) -> Icon {
    Icon::from_rgba(icon_art::rgba(size), size, size).expect("icon pixels match its size")
}
