//! Formatting helpers for numbers, stats and descriptions shown in the UI.

use super::{
    BOOSTED_TEXT, Color, DIM_TEXT, FOOD_TEXT, LABEL_TEXT, Line, METAL_TEXT, REDUCED_TEXT, TEXT,
    WOOD_TEXT,
};
use crate::game::ability::Ability;
use crate::game::city::{Stock, resource_icon, stock_icons, turns_icon};
use crate::game::unit::Unit;

/// A price in short form for a build card's hint: each resource's icon and
/// whole amount, zeros left out (`stock_icons`), or FREE.
pub(super) fn price_hint(price: Stock) -> String {
    let icons = stock_icons(price);
    if icons.is_empty() {
        "FREE".into()
    } else {
        icons
    }
}

/// A build card's hint: its price and, after the clock icon, its turns.
pub(super) fn cost_hint(price: Stock, turns: i32) -> String {
    format!("{} {}", price_hint(price), turns_icon(turns))
}

/// Each resource's color.
pub(super) fn resource_color(name: &str) -> Color {
    match name {
        "FOOD" => FOOD_TEXT,
        "WOOD" => WOOD_TEXT,
        _ => METAL_TEXT,
    }
}

/// A stockpile, with its change a turn if given: each resource's icon,
/// amount and change, changes green, red or dim.
pub(super) fn stock_spans(stock: Stock, change: Option<Stock>) -> Line {
    let mut line = Vec::new();
    for (i, (name, amount)) in stock.parts().into_iter().enumerate() {
        let separator = if i == 0 { "" } else { "   " };
        line.push((
            format!("{separator}{}", resource_icon(name)),
            resource_color(name),
        ));
        line.push((quantity(amount), TEXT));
        let Some(change) = change else {
            continue;
        };
        let delta = change.parts()[i].1;
        let delta_color = match delta.signum() {
            1 => BOOSTED_TEXT,
            -1 => REDUCED_TEXT,
            _ => DIM_TEXT,
        };
        line.push((format!(" {}", signed_quantity(delta)), delta_color));
    }
    line
}

/// "LABEL value" pairs on one line, labels dim and values in their own color.
pub(super) fn stat_spans(stats: &[(&str, String, Color)]) -> Line {
    let mut line = Vec::new();
    for (i, (label, value, color)) in stats.iter().enumerate() {
        let separator = if i == 0 { "" } else { "   " };
        line.push((format!("{separator}{label} "), LABEL_TEXT));
        line.push((value.clone(), *color));
    }
    line
}

/// Green if a stat is above its base value, red if below.
pub(super) fn compare(value: f32, base: f32) -> Color {
    if value > base {
        BOOSTED_TEXT
    } else if value < base {
        REDUCED_TEXT
    } else {
        TEXT
    }
}

/// A city amount in quarter units, without decimals when it's whole.
pub(super) fn quantity(quarters: i32) -> String {
    if quarters % 4 == 0 {
        (quarters / 4).to_string()
    } else {
        format!("{:.2}", quarters as f32 / 4.0)
    }
}

pub(super) fn signed_quantity(quarters: i32) -> String {
    let sign = if quarters >= 0 { "+" } else { "" };
    format!("{sign}{}", quantity(quarters))
}

/// The End Turn button's label: the next thing the turn is waiting on, in
/// the turn strip's order (production, then units, as that's what clicking
/// selects first), or "END TURN" once nothing is, flagged with how many of
/// the player's queues sit idle this turn (`idle`, from `idle_queues`).
pub(super) fn end_turn_label((units, cities): (usize, usize), idle: usize) -> String {
    match (units, cities) {
        (_, 1) => "CHOOSE PRODUCTION".into(),
        (_, cities) if cities > 1 => format!("{cities} CITIES NEED PRODUCTION"),
        (0, _) if idle > 0 => format!("END TURN · {idle} WAITING"),
        (0, _) => "END TURN".into(),
        (1, _) => "UNIT NEEDS ORDERS".into(),
        (units, _) => format!("{units} UNITS NEED ORDERS"),
    }
}

/// What the turn is waiting on, like "2 UNITS AND 1 CITY NEED ORDERS", from
/// `GameState::pending`; `None` once nothing is.
pub(super) fn pending_text((units, cities): (usize, usize)) -> Option<String> {
    let plural = |count: usize, one: &str, many: &str| {
        let word = if count == 1 { one } else { many };
        format!("{count} {word}")
    };
    let mut parts = Vec::new();
    if units > 0 {
        parts.push(plural(units, "UNIT", "UNITS"));
    }
    if cities > 0 {
        parts.push(plural(cities, "CITY", "CITIES"));
    }
    let verb = if units + cities == 1 { "NEEDS" } else { "NEED" };
    (!parts.is_empty()).then(|| format!("{} {verb} ORDERS", parts.join(" AND ")))
}

pub(super) fn turns_text(turns: u32) -> String {
    crate::game::city::turns_icon(turns as i32)
}

/// `text` if it fits in `max_width` (as `measure` measures a string);
/// otherwise as many of its words as fit with "…" after them (a word cut
/// short only when not even the first fits), or nothing if not even the
/// ellipsis does. Both presentations' top bars fit the notice with it.
pub(super) fn fit_text(text: &str, max_width: f32, measure: impl Fn(&str) -> f32) -> String {
    if measure(text) <= max_width {
        return text.to_string();
    }
    let with_ellipsis = |end: usize| {
        // No dangling separator (" - ", ",", ":") before the ellipsis.
        let kept = text[..end].trim_end_matches(|c: char| c.is_whitespace() || "-·,;:".contains(c));
        format!("{kept}…")
    };
    // The longest cut at a char boundary that fits: widths only grow with
    // the text, so search the boundaries.
    let ends: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let longest = ends.partition_point(|&end| measure(&with_ellipsis(end)) <= max_width);
    if longest == 0 {
        return String::new();
    }
    let cut = ends[longest - 1];
    // Back to the end of the last whole word, if there is one.
    let word_end = if text[cut..].starts_with(char::is_whitespace) {
        Some(cut)
    } else {
        text[..cut].rfind(char::is_whitespace)
    };
    match word_end {
        Some(end) if !text[..end].trim().is_empty() => with_ellipsis(end),
        _ => with_ellipsis(cut),
    }
}

/// Splits `text` into lines of at most `max_chars`, breaking between words.
pub(super) fn wrap(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > max_chars {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The ability's name and a short description of what it does.
pub(super) fn ability_text(unit: &Unit) -> (&'static str, &'static str) {
    match unit.ability() {
        Ability::ShieldWall => ("SHIELD WALL", "+50% DEFENSE THIS TURN, NO MOVING"),
        Ability::Volley => ("VOLLEY", "ALSO HITS ENEMIES NEXT TO THE TARGET, ALL AT 60%"),
        Ability::Charge => ("CHARGE", "+1 MOVE AND +50% ATTACK THIS TURN"),
        Ability::Deploy if unit.deployed => ("PACK UP", "A TURN PACKING UP, THEN IT CAN MOVE"),
        Ability::Deploy => ("DEPLOY", "A TURN SETTING UP, THEN +1 RANGE BUT NO MOVING"),
        Ability::Lookout => ("LOOKOUT", "NO MOVING THIS TURN, +2 SIGHT NEXT TURN"),
    }
}
