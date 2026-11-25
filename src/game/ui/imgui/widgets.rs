//! The pieces panels are drawn with: text and buttons with the map's icons
//! in them, the turn strip's chips, and tooltips.

use super::*;

/// A unit strip token drawn with ImGui's draw list: the same token as on the
/// map, in a framed square (bright while selected, white while hovered).
pub(super) fn draw_roster_chip(
    ui: &Ui,
    min: [f32; 2],
    max: [f32; 2],
    chip: &RosterChip,
    hovered: bool,
) {
    let draw = ui.get_window_draw_list();
    // An idle queue's chip is rimmed in the waiting red, like classic's.
    let (bg, border) = if chip.selected {
        (BUTTON_HOVER_BG, ARMED_BORDER_COLOR)
    } else if chip.warning {
        (SHORT_BG, WAITING_TEXT)
    } else {
        (BUTTON_BG, BORDER_COLOR)
    };
    let edge = if hovered { TEXT } else { border };
    let thickness = if chip.selected || chip.warning {
        ARMED_BORDER
    } else {
        BORDER
    };
    draw.add_rect(min, max, bg).filled(true).build();
    draw.add_rect(min, max, edge).thickness(thickness).build();
    let mut vertices = Vec::new();
    let radius = (max[0] - min[0]) * ROSTER_TOKEN_SHARE / 2.0;
    crate::game::ui::paint::push_chip_icon(
        Vec2::ZERO,
        chip.icon,
        radius,
        chip.color,
        &mut vertices,
    );
    // The token is built Y-up around the origin; ImGui's Y points down.
    let center = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
    let at = |v: &Vertex| [center[0] + v.pos[0], center[1] - v.pos[1]];
    for triangle in vertices.as_chunks::<3>().0 {
        draw.add_triangle(
            at(&triangle[0]),
            at(&triangle[1]),
            at(&triangle[2]),
            triangle[0].color,
        )
        .filled(true)
        .build();
    }
    if chip.count > 1 {
        let text = chip.count.to_string();
        let size = ui.calc_text_size(&text);
        let inset = crate::game::ui::paint::CHIP_COUNT_INSET;
        let origin = [max[0] - size[0] - inset, max[1] - size[1] - inset];
        draw.add_rect(
            [origin[0] - 2.0, origin[1] - 1.0],
            [max[0] - inset + 2.0, max[1] - inset + 1.0],
            PANEL_BG,
        )
        .filled(true)
        .build();
        draw.add_text(origin, TEXT, &text);
    }
}

pub(super) fn draw_action_icon(
    ui: &Ui,
    min: [f32; 2],
    max: [f32; 2],
    icon: action_icons::ActionIcon,
    color: Color,
    cooldown: Option<&str>,
    small_font: FontId,
) {
    let draw = ui.get_window_draw_list();
    let mut center = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
    let mut vertices = Vec::new();
    action_icons::push_icon(Vec2::ZERO, 14.0, icon, color, &mut vertices);
    // The badge in the top-right corner: where it goes, and its text's size.
    let badge = cooldown.map(|turns| {
        let _font = ui.push_font(small_font);
        let size = [ui.calc_text_size(turns)[0], ui.text_line_height()];
        (turns, [max[0] - size[0] - 3.0, min[1] + 2.0], size)
    });
    if let Some((_, pos, size)) = badge {
        // The icon moves left, then down, as far as it must and can to
        // clear the badge.
        let (mut low, mut high) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for vertex in &vertices {
            let at = Vec2::new(vertex.pos[0], vertex.pos[1]);
            low = low.min(at);
            high = high.max(at);
        }
        let gap = 1.0;
        let right = center[0] + high.x;
        let top = center[1] - high.y;
        let bottom = pos[1] + size[1];
        if right + gap > pos[0] && top < bottom {
            let room = (center[0] + low.x - min[0] - 3.0).max(0.0);
            center[0] -= (right + gap - pos[0]).min(room);
            if center[0] + high.x + gap > pos[0] {
                let room = (max[1] - 3.0 - (center[1] - low.y)).max(0.0);
                center[1] += (bottom + gap - top).min(room);
            }
        }
    }
    fill_shapes(&draw, center, &vertices);
    if let Some((turns, pos, size)) = badge {
        let _font = ui.push_font(small_font);
        draw.add_rect(
            [pos[0] - 2.0, pos[1] - 1.0],
            [max[0] - 1.0, pos[1] + 13.0],
            PANEL_BG,
        )
        .filled(true)
        .build();
        draw.add_text(pos, color, turns);
        note_mark(turns, pos, [pos[0] + size[0], pos[1] + size[1]], None);
    }
}

pub(super) fn draw_production_icon(
    ui: &Ui,
    min: [f32; 2],
    max: [f32; 2],
    icon: crate::game::unit_icons::UnitIcon,
) {
    let center = [min[0] + CARD_ICON_CENTER, (min[1] + max[1]) / 2.0];
    let mut vertices = Vec::new();
    crate::game::unit_icons::push_pictogram(Vec2::ZERO, 12.0, icon, TEXT, &mut vertices);
    fill_shapes(&ui.get_window_draw_list(), center, &vertices);
}

/// Fills shapes built Y-up around the origin (triangle lists, like the
/// map's) into `draw`, the current window's draw list (ImGui allows one
/// handle on it at a time), centered on `center` (ImGui's Y
/// points down). ImGui feathers every filled triangle's edges by a pixel,
/// the inner ones too, which leaves a small silhouette fuzzy and seamed:
/// these are drawn unfeathered, crisp and whole.
fn fill_shapes(draw: &::imgui::DrawListMut, center: [f32; 2], vertices: &[Vertex]) {
    let at = |v: &Vertex| [center[0] + v.pos[0], center[1] - v.pos[1]];
    unfeathered(|| {
        for triangle in vertices.as_chunks::<3>().0 {
            draw.add_triangle(
                at(&triangle[0]),
                at(&triangle[1]),
                at(&triangle[2]),
                triangle[0].color,
            )
            .filled(true)
            .build();
        }
    });
    #[cfg(test)]
    if !vertices.is_empty() {
        let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
        for [x, y] in vertices.iter().map(at) {
            min = [min[0].min(x), min[1].min(y)];
            max = [max[0].max(x), max[1].max(y)];
        }
        note_mark("icon", min, max, None);
    }
}

pub(super) fn text_line(ui: &Ui, line: &Line) {
    for (index, (text, color)) in line.iter().enumerate() {
        if index != 0 {
            ui.same_line_with_spacing(0.0, 0.0);
        }
        let readable = if *color == LABEL_TEXT {
            [0.66, 0.70, 0.72, 1.0]
        } else if *color == DIM_TEXT {
            [0.72, 0.75, 0.76, 1.0]
        } else {
            *color
        };
        rich_text(ui, text, readable);
    }
}

/// A panel's line of text: `text_line`, broken between words into as many
/// rows as it takes to fit the panel's width (`wrap_spans`).
pub(super) fn panel_text_line(ui: &Ui, line: &Line) {
    let rows = wrap_spans(ui, line, ui.content_region_avail()[0]);
    let spacing = ui.clone_style().item_spacing;
    for (index, row) in rows.iter().enumerate() {
        // The rows of one line sit as close as a wrapped paragraph's.
        let _tight = (index + 1 < rows.len())
            .then(|| ui.push_style_var(StyleVar::ItemSpacing([spacing[0], 0.0])));
        text_line(ui, row);
    }
}

/// `line`'s spans broken between words into rows no wider than `width`,
/// each span keeping its color; a word wider than `width` has a row of its
/// own. A span ending in one space runs into the next, as a label into its
/// value ("DEFENSE " and "20"), so the two stay on one row. The rows after
/// the first drop the spaces they would start with.
pub(super) fn wrap_spans(ui: &Ui, line: &Line, width: f32) -> Vec<Line> {
    fn push(row: &mut Line, text: &str, color: Color) {
        match row.last_mut() {
            Some((last, last_color)) if *last_color == color => last.push_str(text),
            _ => row.push((text.to_string(), color)),
        }
    }
    // The words, each its pieces of spans.
    let mut words: Vec<Line> = Vec::new();
    let mut word = Line::new();
    for (text, color) in line {
        let pieces: Vec<&str> = text.split_inclusive(' ').collect();
        for (index, piece) in pieces.iter().enumerate() {
            push(&mut word, piece, *color);
            let joins_next = index + 1 == pieces.len() && piece.len() > 1;
            if piece.ends_with(' ') && !joins_next {
                words.push(std::mem::take(&mut word));
            }
        }
    }
    words.push(word);
    let width_of =
        |pieces: &Line| -> f32 { pieces.iter().map(|(text, _)| rich_width(ui, text)).sum() };
    let mut rows: Vec<Line> = vec![Vec::new()];
    let mut x = 0.0;
    for mut word in words {
        let mut shown = word.clone();
        if let Some((last, _)) = shown.last_mut() {
            *last = last.trim_end().to_string();
        }
        if x > 0.0 && x + width_of(&shown) > width + 0.5 {
            rows.push(Vec::new());
            x = 0.0;
        }
        if x == 0.0 && rows.len() > 1 {
            // A new row starts at its first letter.
            while let Some((first, _)) = word.first_mut() {
                *first = first.trim_start().to_string();
                if !first.is_empty() {
                    break;
                }
                word.remove(0);
            }
        }
        x += width_of(&word);
        let row = rows.last_mut().expect("a row");
        for (text, color) in word {
            if !text.is_empty() {
                push(row, &text, color);
            }
        }
    }
    // A row ends at its last letter: the spaces after it take no room.
    for row in &mut rows {
        while let Some((last, _)) = row.last_mut() {
            let kept = last.trim_end().len();
            last.truncate(kept);
            if !last.is_empty() {
                break;
            }
            row.pop();
        }
        // A line of nothing still takes its line.
        if row.is_empty() {
            row.push((String::new(), TEXT));
        }
    }
    rows
}

/// Whether `text` holds any inline icon characters (`map_icons::inline_icon`).
fn has_icons(text: &str) -> bool {
    text.chars().any(|ch| map_icons::inline_icon(ch).is_some())
}

/// The room one inline icon takes, at the current font.
fn icon_box(ui: &Ui) -> f32 {
    ui.current_font_size() * 1.1
}

/// How wide `text` draws with its icons.
pub(super) fn rich_width(ui: &Ui, text: &str) -> f32 {
    let mut width = 0.0;
    let mut run = String::new();
    for ch in text.chars() {
        if map_icons::inline_icon(ch).is_some() {
            width += ui.calc_text_size(&run)[0] + icon_box(ui);
            run.clear();
        } else {
            run.push(ch);
        }
    }
    width + ui.calc_text_size(&run)[0]
}

/// Draws `text` into the window's draw list with its top-left at `pos`, each
/// icon character as its icon, the same pictures as the map's yield chips.
/// A dimmed line's icons fade with it.
fn draw_rich(ui: &Ui, pos: [f32; 2], text: &str, color: [f32; 4], dim: bool) {
    let draw = ui.get_window_draw_list();
    let size = icon_box(ui);
    let middle = pos[1] + ui.current_font_size() / 2.0;
    let mut x = pos[0];
    let mut run = String::new();
    let flush = |run: &mut String, x: &mut f32| {
        if !run.is_empty() {
            draw.add_text([*x, pos[1]], color, run.as_str());
            let size = ui.calc_text_size(run.as_str());
            note_mark(run, [*x, pos[1]], [*x + size[0], pos[1] + size[1]], None);
            *x += size[0];
            run.clear();
        }
    };
    for ch in text.chars() {
        if map_icons::inline_icon(ch).is_none() {
            run.push(ch);
            continue;
        }
        flush(&mut run, &mut x);
        let mut vertices = Vec::new();
        map_icons::push_inline_icon(Vec2::ZERO, size * 0.85, ch, false, &mut vertices);
        // Centered on the line by what it draws: the wheat stands a little
        // higher than low, and would reach into the line above.
        let (low, high) = vertices
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), v| {
                (low.min(v.pos[1]), high.max(v.pos[1]))
            });
        let lift = if vertices.is_empty() {
            0.0
        } else {
            (low + high) / 2.0
        };
        let center = [x + size / 2.0, middle + lift];
        // The icon is built Y-up around the origin; ImGui's Y points down.
        if dim {
            for vertex in &mut vertices {
                vertex.color[3] *= 0.4;
            }
        }
        fill_shapes(&draw, center, &vertices);
        x += size;
    }
    flush(&mut run, &mut x);
}

/// `text` as a widget, with its icons drawn in: plain text when it has none.
pub(super) fn rich_text(ui: &Ui, text: &str, color: [f32; 4]) {
    if !has_icons(text) {
        ui.text_colored(color, text);
        note_text_item(ui, text);
        return;
    }
    let pos = ui.cursor_screen_pos();
    ui.dummy([rich_width(ui, text), ui.text_line_height()]);
    draw_rich(ui, pos, text, color, false);
}

/// Shows `lines` (`subject_tooltip_lines`) as the hovered item's tooltip,
/// the same lines classic shows; nothing if there are none.
pub(super) fn show_tooltip(ui: &Ui, lines: &[(u32, Line)]) {
    if lines.is_empty() {
        return;
    }
    note_shown_tooltip(lines);
    ui.tooltip(|| {
        for (_, line) in lines {
            text_line(ui, line);
        }
    });
}

/// A button whose lines may hold icons: ImGui's own button when they
/// don't; otherwise a blank button with the lines drawn over it, centered,
/// or from the left with `left` set.
pub(super) fn rich_button(ui: &Ui, id: &str, lines: &[String], size: [f32; 2], left: bool) -> bool {
    rich_button_hinted(ui, id, lines, size, left, None)
}
/// `spec` as drawn now: with its hover text for its label while the cursor
/// is on it (`ButtonSpec::hover_text`), or `None` to draw it as it is.
/// ImGui knows it as `id`.
pub(super) fn hovered_spec(spec: &ButtonSpec, id: &str) -> Option<ButtonSpec> {
    let hover = spec.hover_label.as_ref()?;
    hovered_last_frame(id).then(|| ButtonSpec {
        label: hover.clone(),
        hover_label: None,
        ..spec.clone()
    })
}

/// `rich_button`, with its lines from the first of `hint` on (a card's
/// price, under its label) in `hint`'s color.
pub(super) fn rich_button_hinted(
    ui: &Ui,
    id: &str,
    lines: &[String],
    size: [f32; 2],
    left: bool,
    hint: Option<(usize, Color)>,
) -> bool {
    if hint.is_none() && !lines.iter().any(|line| has_icons(line)) {
        let label = lines.join("\n");
        let clicked = ui.button_with_size(format!("{label}###{id}"), size);
        note_button_label(ui, &label);
        return clicked;
    }
    let clicked = ui.button_with_size(format!("###{id}"), size);
    let (min, max) = (ui.item_rect_min(), ui.item_rect_max());
    let disabled = ui.clone_style().alpha < 1.0;
    let color = ui.style_color(StyleColor::Text);
    let line_height = ui.text_line_height();
    let total = line_height * lines.len() as f32;
    let top = min[1] + (max[1] - min[1] - total) / 2.0;
    // Kept inside the button, as ImGui keeps its own labels.
    clipped_to(min, max, || {
        for (index, line) in lines.iter().enumerate() {
            let width = rich_width(ui, line);
            let x = if left {
                min[0] + (max[0] - min[0]) * 0.03
            } else {
                min[0] + (max[0] - min[0] - width) / 2.0
            };
            let color = match hint {
                Some((from, hint)) if index >= from => hint,
                _ => color,
            };
            let faded = [
                color[0],
                color[1],
                color[2],
                if disabled { 0.45 } else { 1.0 },
            ];
            draw_rich(
                ui,
                [x, top + line_height * index as f32],
                line,
                faded,
                disabled,
            );
        }
    });
    clicked
}
/// A one-line button with `left` from its left edge, after `indent` of room
/// for an icon drawn there, and `right` flush with its right edge, so a list
/// of them lines their right parts up (the building catalog's prices).
/// When the two don't fit side by side with a gap between, `right` is left
/// out (the button's tooltip gives it) and `left` shortened to what fits.
/// `right` comes with its own color, if it has one (a short card's price).
pub(super) fn split_button(
    ui: &Ui,
    id: &str,
    left: &str,
    indent: f32,
    (right, right_color): (&str, Option<Color>),
    size: [f32; 2],
) -> bool {
    let clicked = ui.button_with_size(format!("###{id}"), size);
    let (min, max) = (ui.item_rect_min(), ui.item_rect_max());
    let disabled = ui.clone_style().alpha < 1.0;
    let color = ui.style_color(StyleColor::Text);
    let color = [
        color[0],
        color[1],
        color[2],
        if disabled { 0.45 } else { 1.0 },
    ];
    let pad = ((max[0] - min[0]) * 0.03).max(SPLIT_PAD);
    let left_x = min[0] + indent.max(pad);
    let room = max[0] - pad - left_x;
    let right_width = rich_width(ui, right);
    let fits = rich_width(ui, left) + icon_box(ui) + right_width <= room;
    let (left, right) = if fits {
        (left.to_string(), right)
    } else {
        (fit_text(left, room, |t| rich_width(ui, t)), "")
    };
    let top = min[1] + (max[1] - min[1] - ui.text_line_height()) / 2.0;
    clipped_to(min, max, || {
        draw_rich(ui, [left_x, top], &left, color, disabled);
        if !right.is_empty() {
            let color = right_color.map_or(color, |c| [c[0], c[1], c[2], color[3]]);
            draw_rich(
                ui,
                [max[0] - pad - right_width, top],
                right,
                color,
                disabled,
            );
        }
    });
    clicked
}

/// The style of a button drawn short (`ButtonSpec::drawn_short`), pushed
/// until the tokens drop: a red rim, and a dark red ground unless it's
/// queued (gold). Its price goes red too (`short_price`).
pub(super) fn short_style<'ui>(
    ui: &'ui Ui,
    spec: &ButtonSpec,
) -> (
    Vec<::imgui::ColorStackToken<'ui>>,
    Option<::imgui::StyleStackToken<'ui>>,
) {
    if !spec.drawn_short() {
        return (Vec::new(), None);
    }
    let mut colors = vec![ui.push_style_color(StyleColor::Border, SHORT_BORDER_COLOR)];
    if spec.state() != ButtonState::Queued {
        colors.push(ui.push_style_color(StyleColor::Button, IMGUI_SHORT_BG));
        colors.push(ui.push_style_color(StyleColor::ButtonHovered, IMGUI_SHORT_HOVER_BG));
    }
    note_short_button(spec.target);
    (
        colors,
        Some(ui.push_style_var(StyleVar::FrameBorderSize(2.0))),
    )
}

/// The color of a short button's price (`short_style`), if it's short.
pub(super) fn short_price(spec: &ButtonSpec) -> Option<Color> {
    spec.drawn_short().then(|| {
        if spec.state() == ButtonState::Queued {
            SHORT_QUEUED_PRICE
        } else {
            SHORT_PRICE
        }
    })
}

/// A short button's ground in ImGui, whose buttons are lighter than
/// classic's: a faint warm red, and a little brighter hovered.
const IMGUI_SHORT_BG: Color = [0.068, 0.046, 0.046, 1.0];
const IMGUI_SHORT_HOVER_BG: Color = [0.12, 0.08, 0.08, 1.0];

/// The least room between a split button's text and its edges.
const SPLIT_PAD: f32 = 6.0;
/// A catalogue card's room for its unit's pictogram, centered
/// `CARD_ICON_CENTER` from the card's left edge, before the name.
const CARD_ICON_CENTER: f32 = 19.0;
pub(super) const CARD_ICON_ROOM: f32 = 2.0 * CARD_ICON_CENTER + 2.0;
