//! Measuring a panel's rows as `render.rs` lays them out, so the layout
//! can size a window before it's drawn.

use super::*;

pub(super) fn measure_panel(ui: &Ui, panel: &PanelBuilder, fonts: &[FontId; 3], width: f32) -> f32 {
    let inner = (width - 45.0).max(100.0);
    let mut height = 26.0; // border and padding
    for row in flat_rows(&panel.rows) {
        height += match row {
            Row::Text(px, line) => {
                let font = match *px {
                    TITLE => fonts[2],
                    SMALL => fonts[0],
                    _ => fonts[1],
                };
                let _font = ui.push_font(font);
                let rows = wrap_spans(ui, line, inner).len();
                ui.text_line_height() * rows as f32 + 7.0
            }
            Row::Gap(gap) => gap + 6.0,
            Row::Bar(_) => 18.0,
            Row::Buttons(buttons, compact) => {
                measure_buttons(ui, buttons, *compact, panel.faded, inner)
            }
            Row::Reorder(_, buttons) => measure_buttons(ui, buttons, true, panel.faded, inner),
            Row::QueueItem(item) => {
                let width = (inner - QUEUE_X_ROOM).max(QUEUE_ROW_MIN);
                queue_row_height(ui, queue_row_lines(ui, item, width).len()) + 7.0
            }
            Row::TitleWithButton(line, spec) => {
                let label = title_button_label(spec, panel.faded);
                if title_button_fits(ui, line, &label, fonts[0], inner) {
                    ui.frame_height_with_spacing()
                } else {
                    // The title's rows, then the button under them.
                    let _font = ui.push_font(fonts[0]);
                    let rows = wrap_spans(ui, line, inner).len() as f32;
                    let title = rows * ui.text_line_height() + SPACING_Y;
                    drop(_font);
                    title + ui.frame_height_with_spacing()
                }
            }
            Row::BuildingCatalog(_, buttons, ..) => {
                (buttons.len().clamp(1, 5) as f32 * 34.0) + 18.0
            }
            Row::Roster(_) => ROSTER_CHIP + 6.0,
            Row::TechTree(tree) => tree_height(ui, fonts, tree, inner) + 2.0 * SPACING_Y,
            Row::ScrollList(_) => unreachable!("flattened by flat_rows"),
            Row::LabeledButtons(..) => unreachable!("classic only"),
            Row::Heading(_) => {
                let _font = ui.push_font(fonts[0]);
                // The text, the rule under it, and the spacing after each.
                ui.calc_text_size("A")[1] + 2.0 * SPACING_Y + 1.0
            }
            Row::Setting(..) | Row::Field(..) => {
                let _font = ui.push_font(fonts[1]);
                let label_width = setting_label_width(ui, panel, fonts[1]);
                let lines = match row {
                    Row::Setting(setting, _) => {
                        setting_fit(ui, *setting, label_width, inner).stacked
                    }
                    _ => field_stacked(label_width, inner),
                };
                let line = ui.calc_text_size("A")[1] + 2.0 * FRAME_PADDING_Y + SPACING_Y;
                line * if lines { 2.0 } else { 1.0 }
            }
        };
    }
    height + 12.0
}

/// How a setting's row fits its width: whether its control goes on a line
/// of its own under its name, and whether a choice shows as a drop-down
/// list rather than a button per value.
#[derive(Clone, Copy)]
pub(super) struct SettingFit {
    pub(super) stacked: bool,
    pub(super) combo: bool,
}

/// How `setting`'s row fits in `available` of width, its name taking
/// `label_width`: the control beside the name if it fits there, else under
/// it; a choice's buttons, if they fit either way, before a drop-down list.
pub(super) fn setting_fit(
    ui: &Ui,
    setting: Setting,
    label_width: f32,
    available: f32,
) -> SettingFit {
    let count = setting.range().count();
    let buttons = setting.control() == Control::Choice && count <= Control::MAX_BUTTONS;
    let mut tries = Vec::new();
    if buttons {
        tries.push(false);
    }
    tries.push(setting.control() == Control::Choice);
    for combo in tries {
        let needs = control_width(ui, setting, combo);
        for stacked in [false, true] {
            let room = if stacked {
                available
            } else {
                available - label_width
            };
            if needs <= room {
                return SettingFit { stacked, combo };
            }
        }
    }
    SettingFit {
        stacked: true,
        combo: setting.control() == Control::Choice,
    }
}

/// The least width `setting`'s control shows its values in: a drop-down
/// list with `combo`.
fn control_width(ui: &Ui, setting: Setting, combo: bool) -> f32 {
    let style = ui.clone_style();
    let pad = style.frame_padding[0];
    let widest = setting
        .range()
        .map(|value| ui.calc_text_size(setting.value_text(value))[0])
        .fold(0.0, f32::max);
    match setting.control() {
        Control::Toggle => ui.frame_height() + style.item_inner_spacing[0] + widest,
        Control::Slider => widest + 4.0 * pad,
        Control::Choice if combo => widest + 2.0 * pad + ui.frame_height(),
        Control::Choice => {
            let count = setting.range().count() as f32;
            count * (widest + 2.0 * pad) + (count - 1.0) * style.item_spacing[0]
        }
    }
}

/// The least width of a typed field's text box beside its name.
const FIELD_MIN_WIDTH: f32 = 120.0;

/// Whether a typed field's text box goes under its name, `label_width`
/// wide, for want of room beside it in `available`.
pub(super) fn field_stacked(label_width: f32, available: f32) -> bool {
    available - label_width < FIELD_MIN_WIDTH
}

/// The room a queue row leaves its X button, and the least width of the
/// row itself.
pub(super) const QUEUE_X_ROOM: f32 = 39.0;
pub(super) const QUEUE_ROW_MIN: f32 = 50.0;

/// A queue row's lines in a button `width` wide: the drag handle and its
/// label, broken between words when they don't fit on one line.
pub(super) fn queue_row_lines(ui: &Ui, item: &QueueItemSpec, width: f32) -> Vec<String> {
    let text = format!("::  {}", item.label.trim_start_matches("> ").trim());
    // The text starts a pad in from the left, and must end as far in.
    let pad = ui.clone_style().frame_padding[0].max(width * 0.03);
    wrap_spans(ui, &vec![(text, TEXT)], width - 2.0 * pad)
        .into_iter()
        .map(|row| row.into_iter().map(|(text, _)| text).collect())
        .collect()
}

/// A queue row's height with `lines` of text.
pub(super) fn queue_row_height(ui: &Ui, lines: usize) -> f32 {
    (lines as f32 * ui.text_line_height() + 6.0).max(30.0)
}

/// The text on a `Row::TitleWithButton`'s button: its label and hint.
pub(super) fn title_button_label(spec: &ButtonSpec, faded: bool) -> String {
    let hint = visible_button_hint(&spec.hint, faded);
    if hint.is_empty() {
        spec.label.clone()
    } else {
        format!("{}  {hint}", spec.label)
    }
}

/// Whether a `Row::TitleWithButton`'s title (`line`, in `font`) and its
/// button (`label`) fit side by side in `available` of width; if not, the
/// button goes on the line below.
pub(super) fn title_button_fits(
    ui: &Ui,
    line: &Line,
    label: &str,
    font: FontId,
    available: f32,
) -> bool {
    let title: f32 = {
        let _font = ui.push_font(font);
        line.iter().map(|(text, _)| rich_width(ui, text)).sum()
    };
    let style = ui.clone_style();
    let button = rich_width(ui, label) + 2.0 * style.frame_padding[0];
    title + style.item_spacing[0] + button <= available
}

/// A row of buttons' height, as `render_buttons` lays it out in `inner` of
/// width.
fn measure_buttons(ui: &Ui, buttons: &[ButtonSpec], compact: bool, faded: bool, inner: f32) -> f32 {
    let grid = button_grid(ui, buttons, compact, faded, inner);
    buttons.len().div_ceil(grid.columns) as f32 * (grid.height + SPACING_Y)
}

/// How `render_buttons` lays out a row of buttons: in columns of equal
/// buttons, each with its lines of text.
pub(super) struct ButtonGrid {
    pub(super) columns: usize,
    pub(super) width: f32,
    pub(super) height: f32,
    /// Each button's lines, in the order of the buttons.
    pub(super) lines: Vec<Vec<String>>,
    /// For each button whose hint has lines of its own (under its label),
    /// the first of them.
    pub(super) hint_from: Vec<Option<usize>>,
}

/// The least width of a button with text.
const MIN_BUTTON_WIDTH: f32 = 128.0;

/// Lays out `buttons` in `available` of width: as many columns as the
/// widest button's text leaves room for, down to one. A compact button's
/// label and hint share a line, and a full one's go on two; text that
/// still doesn't fit its button breaks between words (a compact button's
/// hint going under its label first), and the buttons grow to hold it.
/// Icon buttons (`icon_row`) are squares.
pub(super) fn button_grid(
    ui: &Ui,
    buttons: &[ButtonSpec],
    compact: bool,
    faded: bool,
    available: f32,
) -> ButtonGrid {
    let style = ui.clone_style();
    let spacing = style.item_spacing[0];
    let columns_of = |width: f32| {
        (((available + spacing) / (width + spacing)).floor() as usize)
            .clamp(1, buttons.len().max(1))
    };
    if icon_row(buttons) {
        return ButtonGrid {
            columns: columns_of(ICON_BUTTON_SIZE),
            width: ICON_BUTTON_SIZE,
            height: ICON_BUTTON_SIZE,
            lines: vec![vec![String::new()]; buttons.len()],
            hint_from: vec![None; buttons.len()],
        };
    }
    let pad = style.frame_padding[0];
    let text: Vec<(String, String)> = buttons
        .iter()
        .map(|spec| {
            let hint = visible_button_hint(&spec.hint, faded).to_string();
            (spec.label.clone(), hint)
        })
        .collect();
    let one_line = |(label, hint): &(String, String)| {
        if hint.is_empty() {
            vec![label.clone()]
        } else if compact {
            vec![format!("{label}  {hint}")]
        } else {
            vec![label.clone(), hint.clone()]
        }
    };
    let needs = |lines: &[String]| {
        lines
            .iter()
            .map(|line| rich_width(ui, line))
            .fold(0.0, f32::max)
            + 2.0 * pad
    };
    let widest = text
        .iter()
        .map(|text| needs(&one_line(text)))
        .fold(MIN_BUTTON_WIDTH, f32::max);
    let columns = columns_of(widest);
    let width = ((available - spacing * (columns - 1) as f32) / columns as f32).max(1.0);
    let wrap = |line: &String| -> Vec<String> {
        wrap_spans(ui, &vec![(line.clone(), TEXT)], width - 2.0 * pad)
            .into_iter()
            .map(|row| row.into_iter().map(|(text, _)| text).collect::<String>())
            .collect()
    };
    let (lines, hint_from): (Vec<Vec<String>>, Vec<Option<usize>>) = text
        .iter()
        .map(|text| {
            let mut lines = one_line(text);
            if compact && needs(&lines) > width {
                lines = vec![text.0.clone(), text.1.clone()];
            }
            // The hint on lines of its own: where they start.
            let hint_from = (lines.len() == 2).then(|| wrap(&lines[0]).len());
            (lines.iter().flat_map(wrap).collect(), hint_from)
        })
        .unzip();
    let most = lines.iter().map(Vec::len).max().unwrap_or(1);
    let least = if compact { 28.0 } else { 48.0 };
    ButtonGrid {
        columns,
        width,
        height: (most as f32 * ui.text_line_height()).max(least),
        lines,
        hint_from,
    }
}

/// The style's vertical item spacing and frame padding (`style_imgui`), as
/// `measure_panel` counts them for the settings menu's rows.
const SPACING_Y: f32 = 6.0;
const FRAME_PADDING_Y: f32 = 6.0;

/// The width of the settings menu's label column: its longest setting name,
/// and a gap before the controls.
pub(super) fn setting_label_width(ui: &Ui, panel: &PanelBuilder, font: FontId) -> f32 {
    let _font = ui.push_font(font);
    panel
        .rows
        .iter()
        .filter_map(|row| match row {
            Row::Setting(setting, _) => Some(ui.calc_text_size(setting.name())[0]),
            Row::Field(field, ..) => Some(ui.calc_text_size(field.name())[0]),
            _ => None,
        })
        .fold(0.0, f32::max)
        + 18.0
}

/// How wide the unit strip's window wants to be: its widest row of tokens,
/// plus the same allowance for padding and border `measure_panel` takes off.
pub(super) fn roster_width(panel: &PanelBuilder) -> f32 {
    let widest = panel
        .rows
        .iter()
        .filter_map(|row| match row {
            Row::Roster(chips) => Some(chips.len()),
            _ => None,
        })
        .max()
        .unwrap_or(0) as f32;
    (widest * ROSTER_CHIP + (widest - 1.0).max(0.0) * ROSTER_CHIP_GAP + 45.0).max(210.0)
}
