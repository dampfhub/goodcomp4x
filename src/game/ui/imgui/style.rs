//! The fonts and style ImGui draws the game with.

use super::*;

/// How many of the punctuation characters game text uses
/// (`font::UI_PUNCTUATION`) are past Latin-1.
const PAST_LATIN_1: usize = {
    let mut count = 0;
    let mut i = 0;
    while i < font::UI_PUNCTUATION.len() {
        if font::UI_PUNCTUATION[i] as u32 > 0xFF {
            count += 1;
        }
        i += 1;
    }
    count
};

/// What ImGui's fonts draw, as pairs of first and last code points ending
/// in 0: Basic Latin and Latin-1 (ImGui's default), and each character past
/// them of the punctuation game text uses, the list the classic font has
/// too (`font::UI_PUNCTUATION`). The ranges may not overlap.
static IMGUI_GLYPHS: [u32; 3 + 2 * PAST_LATIN_1] = {
    let mut ranges = [0; 3 + 2 * PAST_LATIN_1];
    ranges[0] = 0x20;
    ranges[1] = 0xFF;
    let mut next = 2;
    let mut i = 0;
    while i < font::UI_PUNCTUATION.len() {
        let ch = font::UI_PUNCTUATION[i] as u32;
        if ch > 0xFF {
            ranges[next] = ch;
            ranges[next + 1] = ch;
            next += 2;
        }
        i += 1;
    }
    ranges
};

/// Gives `imgui` the game's fonts and style: `App`'s context, and the
/// tests' headless one, so they measure text as the game does. Returns the
/// small, body and title fonts; the body font is the default.
pub fn style_imgui(imgui: &mut ::imgui::Context) -> [FontId; 3] {
    use ::imgui::{FontConfig, FontGlyphRanges, FontSource};
    // The host's UI font when it has one, or else the classic UI's, which
    // has every glyph game text uses (ImGui's own has no punctuation past
    // Latin-1). ImGui copies the bytes into its atlas.
    let system_font = std::fs::read("C:\\Windows\\Fonts\\segoeui.ttf").ok();
    let data = system_font.as_deref().unwrap_or(font::FONT_DATA);
    let mut add_font = |size| {
        imgui.fonts().add_font(&[FontSource::TtfData {
            data,
            size_pixels: size,
            config: Some(FontConfig {
                glyph_ranges: FontGlyphRanges::from_slice(&IMGUI_GLYPHS),
                ..FontConfig::default()
            }),
        }])
    };
    let body_font = add_font(18.0);
    let small_font = add_font(15.0);
    let title_font = add_font(22.0);
    let style = imgui.style_mut();
    style.window_padding = [12.0, 10.0];
    style.frame_padding = [10.0, 6.0];
    style.item_spacing = [7.0, 6.0];
    style.window_rounding = 0.0;
    style.frame_rounding = 0.0;
    style.scrollbar_rounding = 0.0;
    style.popup_rounding = 0.0;
    style.child_rounding = 0.0;
    style.grab_rounding = 0.0;
    style.tab_rounding = 0.0;
    style.window_border_size = 1.0;
    style.frame_border_size = 1.0;
    style.window_title_align = [0.0, 0.5];
    style.button_text_align = [0.5, 0.5];
    style.colors[StyleColor::Text as usize] = [0.91, 0.92, 0.91, 1.0];
    style.colors[StyleColor::TextDisabled as usize] = [0.46, 0.48, 0.50, 1.0];
    style.colors[StyleColor::WindowBg as usize] = [0.018, 0.022, 0.030, 0.96];
    style.colors[StyleColor::PopupBg as usize] = [0.025, 0.030, 0.041, 0.98];
    style.colors[StyleColor::Border as usize] = [0.29, 0.32, 0.38, 0.95];
    style.colors[StyleColor::TitleBg as usize] = [0.030, 0.036, 0.050, 1.0];
    style.colors[StyleColor::TitleBgActive as usize] = [0.055, 0.065, 0.086, 1.0];
    style.colors[StyleColor::TitleBgCollapsed as usize] = [0.030, 0.036, 0.050, 0.96];
    style.colors[StyleColor::FrameBg as usize] = [0.032, 0.039, 0.052, 1.0];
    style.colors[StyleColor::FrameBgHovered as usize] = [0.073, 0.084, 0.108, 1.0];
    style.colors[StyleColor::FrameBgActive as usize] = [0.12, 0.14, 0.18, 1.0];
    style.colors[StyleColor::Button as usize] = [0.045, 0.053, 0.070, 1.0];
    style.colors[StyleColor::ButtonHovered as usize] = [0.085, 0.10, 0.13, 1.0];
    style.colors[StyleColor::ButtonActive as usize] = [0.13, 0.15, 0.19, 1.0];
    style.colors[StyleColor::Header as usize] = [0.075, 0.090, 0.12, 1.0];
    style.colors[StyleColor::HeaderHovered as usize] = [0.12, 0.15, 0.19, 1.0];
    style.colors[StyleColor::ScrollbarBg as usize] = [0.024, 0.029, 0.039, 1.0];
    style.colors[StyleColor::ScrollbarGrab as usize] = [0.21, 0.24, 0.28, 1.0];
    style.colors[StyleColor::ScrollbarGrabHovered as usize] = [0.31, 0.35, 0.39, 1.0];
    style.colors[StyleColor::PlotHistogram as usize] = [0.80, 0.69, 0.35, 1.0];
    style.colors[StyleColor::DragDropTarget as usize] = [0.91, 0.77, 0.38, 1.0];
    [small_font, body_font, title_font]
}
