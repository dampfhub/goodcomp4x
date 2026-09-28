//! Formatting helpers for numbers, stats and descriptions shown in the UI.

use super::{BOOSTED_TEXT, Color, LABEL_TEXT, Line, REDUCED_TEXT, TEXT};
use crate::game::ability::Ability;
use crate::game::unit::Unit;

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

/// Turns required to finish a queue item at the delivery-adjusted rate shown
/// in its structure panel. Completion is resolved on the next economy tick.
pub(super) fn turns_at_rate(remaining: i32, per_turn: i32) -> String {
    if per_turn <= 0 {
        return "—".into();
    }
    let turns = (remaining.max(1) + per_turn - 1) / per_turn;
    format!("{turns}T")
}

/// The End Turn button's label: the next thing the turn is waiting on, in
/// the turn strip's order (production, idle workers, then units, as that's
/// what clicking selects first), or "END TURN" once nothing is.
pub(super) fn end_turn_label((units, cities, workers): (usize, usize, usize)) -> String {
    match (units, cities, workers) {
        (_, 1, _) => "CHOOSE PRODUCTION".into(),
        (_, cities, _) if cities > 1 => format!("{cities} CITIES NEED PRODUCTION"),
        (_, _, 1) => "WORKER NEEDS A JOB".into(),
        (_, _, workers) if workers > 1 => format!("{workers} WORKERS NEED JOBS"),
        (0, _, _) => "END TURN".into(),
        (1, _, _) => "UNIT NEEDS ORDERS".into(),
        (units, _, _) => format!("{units} UNITS NEED ORDERS"),
    }
}

/// What the turn is waiting on, like "2 UNITS AND 1 CITY NEED ORDERS", from
/// `GameState::pending`; `None` once nothing is.
pub(super) fn pending_text((units, cities, workers): (usize, usize, usize)) -> Option<String> {
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
    if workers > 0 {
        parts.push(plural(workers, "WORKER", "WORKERS"));
    }
    let verb = if units + cities + workers == 1 {
        "NEEDS"
    } else {
        "NEED"
    };
    (!parts.is_empty()).then(|| format!("{} {verb} ORDERS", parts.join(" AND ")))
}

pub(super) fn turns_text(turns: u32) -> String {
    match turns {
        1 => "1 TURN".to_string(),
        _ => format!("{turns} TURNS"),
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
