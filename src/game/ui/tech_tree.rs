//! The research view's tech tree (#413): a column for each tier, a card for
//! each tech, and a line from each tech to those that need it. Built from
//! `TECHS` (`GameState::tech_tree`, `research.rs`), so it grows with the
//! tree; where each card goes is worked out here once, for both
//! presentations, which only measure their text (`TechTree::metrics`) and
//! draw: classic places each card as a button that draws itself
//! (`Button::card`, `draw_tech_card`), ImGui in `imgui/tech_tree.rs`.
//!
//! Where the cards go: each tier is a column, and within it the techs keep
//! the order of what they need (`arrange`), each moved up or down toward
//! the techs it's linked to, so the lines stay short and seldom cross.
//! When the panel is too narrow for the columns side by side, the tiers
//! stack instead, each under its heading, without the lines
//! (`TreePlacement::stacked`).

use glam::Vec2;

use super::builder::ButtonSpec;
use super::text::fit_text;
use super::{
    BAR_BG, BODY, BOOSTED_TEXT, BORDER_COLOR, Color, DIM_TEXT, GOLD_TEXT, LINE_GAP, SHORT_PRICE,
    SMALL, TEXT, WAITING_TEXT,
};
use crate::game::font;
use crate::game::mesh;
use crate::game::research::Tech;
use crate::game::unit_icons::{self, UnitIcon};
use crate::renderer::Vertex;

/// Room between two columns, where the lines between them turn.
pub(super) const COLUMN_GAP: f32 = 44.0;
/// Room between two cards of a column.
pub(super) const CARD_GAP: f32 = 12.0;
/// Room between a card's edge and what's in it.
pub(super) const CARD_PAD: f32 = 8.0;
/// Room an unlock's icon takes before its name, or in a row of icons.
pub(super) const UNLOCK_ICON: f32 = 22.0;
/// The progress bar at the foot of the card being researched, and the room
/// above it.
pub(super) const CARD_BAR: f32 = 5.0;
const CARD_BAR_GAP: f32 = 4.0;
/// Room between a card's price and its tag (QUEUED 2).
pub(super) const TAG_GAP: f32 = 12.0;
/// Room under a tier's heading.
const HEADING_GAP: f32 = 6.0;
/// Room above a tier's heading when the tiers are stacked.
const SECTION_GAP: f32 = 14.0;
/// How thick a line between two cards is.
pub(super) const WIRE: f32 = 2.0;

/// Where a tech stands for the side, which its card's colors show.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum TechState {
    Researched,
    /// The queue's first, worked now.
    Researching,
    /// The queue's first, waiting for the stockpile.
    Waits,
    Queued,
    /// Everything it needs is researched.
    Available,
    /// Something it needs isn't researched yet. A click still queues it,
    /// with what it needs first.
    Locked,
}

/// A thing a tech unlocks, as its card shows it: its icon and its name.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct UnlockMark {
    pub(super) icon: UnlockIcon,
    pub(super) name: &'static str,
}

/// An unlock's icon: a unit's pictogram, or a building's badge as the map
/// draws it, a letter on a colored square (`draw::building_badge`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum UnlockIcon {
    Unit(UnitIcon),
    Building(char, Color),
}

/// What a tech's card shows, besides its name.
#[derive(Clone, PartialEq, Debug)]
pub(super) struct TechCard {
    pub(super) tech: Tech,
    pub(super) state: TechState,
    /// Its price and turns, or RESEARCHED.
    pub(super) detail: String,
    /// At the detail line's right end: QUEUED and its place, its turns
    /// left, or WAITS.
    pub(super) tag: Option<String>,
    pub(super) unlocks: Vec<UnlockMark>,
    /// How far its research is, while it's worked.
    pub(super) progress: Option<f32>,
    /// The stockpile can't pay its price this turn (`ButtonSpec::short`):
    /// its price is red.
    pub(super) short: bool,
    /// Its unlocks' names beside their icons (`TechTree::names`), as the
    /// tree it's placed in has them.
    pub(super) names: bool,
}

impl TechCard {
    pub(super) fn name_color(&self) -> Color {
        match self.state {
            TechState::Locked => DIM_TEXT,
            TechState::Researching => GOLD_TEXT,
            _ => TEXT,
        }
    }

    pub(super) fn detail_color(&self) -> Color {
        match self.state {
            TechState::Researched => BOOSTED_TEXT,
            TechState::Waits => SHORT_PRICE,
            TechState::Locked => DIM_TEXT,
            _ if self.short => SHORT_PRICE,
            _ => TEXT,
        }
    }

    pub(super) fn tag_color(&self) -> Color {
        match self.state {
            TechState::Waits => WAITING_TEXT,
            _ => GOLD_TEXT,
        }
    }

    pub(super) fn unlock_color(&self) -> Color {
        match self.state {
            TechState::Locked => DIM_TEXT,
            _ => TEXT,
        }
    }

    /// Its rim's color and width.
    pub(super) fn rim(&self) -> (Color, f32) {
        match self.state {
            TechState::Researched => ([0.22, 0.52, 0.24, 1.0], 2.0),
            TechState::Researching => (GOLD_TEXT, 3.0),
            TechState::Waits => (WAITING_TEXT, 3.0),
            TechState::Queued => ([0.60, 0.48, 0.16, 1.0], 2.0),
            TechState::Available => ([0.42, 0.46, 0.54, 1.0], 2.0),
            TechState::Locked => (BORDER_COLOR, 2.0),
        }
    }
}

/// A line from a tech to one that needs it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Link {
    pub(super) from: usize,
    pub(super) to: usize,
    pub(super) state: LinkState,
}

/// How a line is drawn: dim while the tech it comes from isn't researched,
/// gold while both ends are queued, green once that tech is researched.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(super) enum LinkState {
    Open,
    Planned,
    Done,
}

impl LinkState {
    pub(super) fn color(self) -> Color {
        match self {
            Self::Open => [0.20, 0.22, 0.26, 1.0],
            Self::Planned => [0.60, 0.48, 0.16, 1.0],
            Self::Done => [0.22, 0.52, 0.24, 1.0],
        }
    }
}

/// The tree: a card (a button and what it shows) for each tech in `TECHS`
/// order, the slot each goes in, the lines between them and the tiers'
/// headings.
#[derive(Clone)]
pub(super) struct TechTree {
    pub(super) cards: Vec<(ButtonSpec, TechCard)>,
    /// Each card's column (its tier, from 0) and its place down the column,
    /// in cards (`arrange`).
    pub(super) slots: Vec<(usize, f32)>,
    pub(super) links: Vec<Link>,
    /// A heading for each column.
    pub(super) headings: Vec<String>,
    /// Every tag a card may show, so each card leaves room for the widest
    /// and the cards keep their size as techs are queued.
    pub(super) tags: Vec<String>,
    /// Each unlock's name beside its icon; without, the icons in a row
    /// (classic, when the tree doesn't fit the screen's height:
    /// `PanelBuilder::fit_height`).
    pub(super) names: bool,
}

/// The sizes the tree is laid out from, measured in a presentation's fonts
/// (`TechTree::metrics`).
#[derive(Clone, Copy, Debug)]
pub(super) struct TreeMetrics {
    /// A card's width: its widest content, padded.
    pub(super) card_width: f32,
    /// The height of a card's name line.
    pub(super) name_line: f32,
    /// The height of a line of small text: the detail, an unlock, a heading.
    pub(super) line: f32,
}

/// Where the tree's headings, cards and lines go, from its top left
/// corner, Y down.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct TreePlacement {
    pub(super) size: Vec2,
    /// Each card's corners, by its place in `TechTree::cards`.
    pub(super) cards: Vec<(Vec2, Vec2)>,
    /// Each heading's top left corner and its column.
    pub(super) headings: Vec<(Vec2, usize)>,
    /// The lines, as rectangles, with how each is drawn; drawn in this
    /// order, the brightest last.
    pub(super) wires: Vec<(Vec2, Vec2, LinkState)>,
    /// The tiers stack, as the columns don't fit side by side.
    pub(super) stacked: bool,
}

impl TechTree {
    pub(super) fn columns(&self) -> usize {
        self.headings.len()
    }

    /// The sizes to lay the tree out with, in a presentation's fonts: how
    /// wide `name_width` draws a name and `small_width` the small text, and
    /// the heights of their lines.
    pub(super) fn metrics(
        &self,
        name_width: impl Fn(&str) -> f32,
        small_width: impl Fn(&str) -> f32,
        name_line: f32,
        line: f32,
    ) -> TreeMetrics {
        let tag = self
            .tags
            .iter()
            .map(|tag| small_width(tag))
            .fold(0.0, f32::max);
        let widest = self
            .cards
            .iter()
            .map(|(spec, card)| {
                let name = name_width(&spec.label);
                let detail = small_width(&card.detail) + TAG_GAP + tag;
                let unlocks = if self.names {
                    card.unlocks
                        .iter()
                        .map(|unlock| UNLOCK_ICON + small_width(unlock.name))
                        .fold(0.0, f32::max)
                } else {
                    card.unlocks.len() as f32 * UNLOCK_ICON
                };
                name.max(detail).max(unlocks)
            })
            .fold(0.0, f32::max);
        TreeMetrics {
            card_width: (widest + 2.0 * CARD_PAD).ceil(),
            name_line,
            line,
        }
    }

    /// The sizes in the classic font.
    pub(super) fn classic_metrics(&self) -> TreeMetrics {
        let (name, small) = (font::ui(BODY), font::ui(SMALL));
        self.metrics(
            |text| name.width(text),
            |text| small.width(text),
            name.line_height,
            small.line_height + LINE_GAP,
        )
    }

    /// How many lines of unlocks each card leaves room for.
    fn unlock_lines(&self) -> usize {
        if self.names {
            self.cards
                .iter()
                .map(|(_, card)| card.unlocks.len())
                .max()
                .unwrap_or(0)
        } else {
            1
        }
    }

    /// Every card's height: its name, its detail, its unlocks and room for
    /// the progress bar.
    pub(super) fn card_height(&self, metrics: &TreeMetrics) -> f32 {
        (2.0 * CARD_PAD
            + metrics.name_line
            + metrics.line * (1 + self.unlock_lines()) as f32
            + CARD_BAR_GAP
            + CARD_BAR)
            .ceil()
    }

    /// How wide the tree is with its columns side by side.
    pub(super) fn width(&self, metrics: &TreeMetrics) -> f32 {
        let columns = self.columns() as f32;
        columns * metrics.card_width + (columns - 1.0).max(0.0) * COLUMN_GAP
    }

    /// Where everything goes in `width`: the columns side by side if they
    /// fit, else stacked.
    pub(super) fn place(&self, width: f32, metrics: &TreeMetrics) -> TreePlacement {
        if width + 0.5 >= self.width(metrics) {
            self.place_columns(metrics)
        } else {
            self.place_stacked(width, metrics)
        }
    }

    fn place_columns(&self, metrics: &TreeMetrics) -> TreePlacement {
        let (card_width, card_height) = (metrics.card_width, self.card_height(metrics));
        let column_x = |column: usize| column as f32 * (card_width + COLUMN_GAP);
        let top = metrics.line + HEADING_GAP;
        let cards: Vec<(Vec2, Vec2)> = self
            .slots
            .iter()
            .map(|&(column, slot)| {
                let min = Vec2::new(column_x(column), top + slot * (card_height + CARD_GAP));
                (min.round(), (min + Vec2::new(card_width, card_height)).round())
            })
            .collect();
        let headings = (0..self.columns())
            .map(|column| (Vec2::new(column_x(column), 0.0), column))
            .collect();
        let mut wires = Vec::new();
        for link in &self.links {
            let (from, to) = (cards[link.from], cards[link.to]);
            let start = Vec2::new(from.1.x, (from.0.y + from.1.y) / 2.0);
            let end = Vec2::new(to.0.x, (to.0.y + to.1.y) / 2.0);
            let (first, last) = (self.slots[link.from].0, self.slots[link.to].0);
            // Turning in the room between columns: right after the first
            // card, and, if a column lies between, along the room over the
            // card it goes to, then down in the room just before it.
            let turn = start.x + COLUMN_GAP / 2.0;
            let mut points = vec![start, Vec2::new(turn, start.y)];
            if last > first + 1 {
                let lane = to.0.y - CARD_GAP / 2.0;
                let before = end.x - COLUMN_GAP / 2.0;
                points.push(Vec2::new(turn, lane));
                points.push(Vec2::new(before, lane));
                points.push(Vec2::new(before, end.y));
            } else {
                points.push(Vec2::new(turn, end.y));
            }
            points.push(end);
            for pair in points.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let half = Vec2::splat(WIRE / 2.0);
                wires.push((a.min(b) - half, a.max(b) + half, link.state));
            }
        }
        // The brightest drawn last, over the dim where two lines share a
        // stretch.
        wires.sort_by_key(|&(_, _, state)| state);
        let bottom = cards.iter().map(|&(_, max)| max.y).fold(0.0, f32::max);
        TreePlacement {
            size: Vec2::new(self.width(metrics), bottom).ceil(),
            cards,
            headings,
            wires,
            stacked: false,
        }
    }

    fn place_stacked(&self, width: f32, metrics: &TreeMetrics) -> TreePlacement {
        let card_height = self.card_height(metrics);
        let per_row = (((width + CARD_GAP) / (metrics.card_width + CARD_GAP)).floor() as usize).max(1);
        let card_width = metrics.card_width.min(width.max(0.0));
        let mut cards = vec![(Vec2::ZERO, Vec2::ZERO); self.cards.len()];
        let mut headings = Vec::new();
        let mut y = 0.0;
        let mut right: f32 = 0.0;
        for column in 0..self.columns() {
            if column > 0 {
                y += SECTION_GAP;
            }
            headings.push((Vec2::new(0.0, y), column));
            y += metrics.line + HEADING_GAP;
            let mut members: Vec<usize> = (0..self.cards.len())
                .filter(|&i| self.slots[i].0 == column)
                .collect();
            members.sort_by(|&a, &b| self.slots[a].1.total_cmp(&self.slots[b].1));
            for (index, &card) in members.iter().enumerate() {
                let (row, place) = (index / per_row, index % per_row);
                let min = Vec2::new(
                    place as f32 * (card_width + CARD_GAP),
                    y + row as f32 * (card_height + CARD_GAP),
                );
                let max = min + Vec2::new(card_width, card_height);
                right = right.max(max.x);
                cards[card] = (min.round(), max.round());
            }
            let rows = members.len().div_ceil(per_row) as f32;
            y += rows * card_height + (rows - 1.0).max(0.0) * CARD_GAP;
        }
        TreePlacement {
            size: Vec2::new(right, y).ceil(),
            cards,
            headings,
            wires: Vec::new(),
            stacked: true,
        }
    }
}

/// Where a card's lines go, down from its top: the name's, the detail's,
/// and each unlock's (or the row of unlock icons'); and its progress bar's
/// top.
pub(super) struct CardRows {
    pub(super) name: f32,
    pub(super) detail: f32,
    pub(super) unlocks: f32,
    pub(super) bar: f32,
}

impl CardRows {
    pub(super) fn new(metrics: &TreeMetrics, height: f32) -> Self {
        let detail = CARD_PAD + metrics.name_line;
        Self {
            name: CARD_PAD,
            detail,
            unlocks: detail + metrics.line,
            bar: height - CARD_PAD - CARD_BAR,
        }
    }
}

/// Each card's column and its place down it, in cards: `columns[i]` is card
/// `i`'s column and `needs[i]` the cards it needs (in earlier columns).
///
/// A column takes the order of what its cards need (the first column, the
/// order given); then, the longest column fixed, each card in the others
/// moves toward the middle of the cards it's linked to, keeping its order
/// and a card's room from its neighbors, within the longest column's
/// height. So a card needed by two sits between them, and one needing a
/// card sits level with it.
pub(super) fn arrange(columns: &[usize], needs: &[Vec<usize>]) -> Vec<(usize, f32)> {
    let count = columns.len();
    let column_count = columns.iter().map(|&c| c + 1).max().unwrap_or(0);
    let mut slot = vec![0.0_f32; count];
    let mut order: Vec<Vec<usize>> = vec![Vec::new(); column_count];
    for column in 0..column_count {
        let mut members: Vec<usize> = (0..count).filter(|&i| columns[i] == column).collect();
        let center = |i: usize| {
            let parents = &needs[i];
            if parents.is_empty() {
                f32::MAX
            } else {
                parents.iter().map(|&p| slot[p]).sum::<f32>() / parents.len() as f32
            }
        };
        members.sort_by(|&a, &b| center(a).total_cmp(&center(b)).then(a.cmp(&b)));
        for (place, &i) in members.iter().enumerate() {
            slot[i] = place as f32;
        }
        order[column] = members;
    }
    let tallest = order.iter().map(Vec::len).max().unwrap_or(0);
    let Some(fixed) = order.iter().position(|members| members.len() == tallest) else {
        return Vec::new();
    };
    let last = tallest.saturating_sub(1) as f32;
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (child, parents) in needs.iter().enumerate() {
        for &parent in parents {
            children[parent].push(child);
        }
    }
    // Out from the fixed column, each way, and back again.
    let outward: Vec<usize> = (fixed + 1..column_count).chain((0..fixed).rev()).collect();
    for _ in 0..2 {
        for &column in &outward {
            let members = &order[column];
            let wanted: Vec<f32> = members
                .iter()
                .map(|&i| {
                    let linked: Vec<f32> = needs[i]
                        .iter()
                        .chain(&children[i])
                        .map(|&j| slot[j])
                        .collect();
                    if linked.is_empty() {
                        slot[i]
                    } else {
                        linked.iter().sum::<f32>() / linked.len() as f32
                    }
                })
                .collect();
            let mut placed = wanted.clone();
            for k in 0..placed.len() {
                let floor = if k == 0 { 0.0 } else { placed[k - 1] + 1.0 };
                placed[k] = placed[k].max(floor);
            }
            for k in (0..placed.len()).rev() {
                let ceiling = if k + 1 == placed.len() {
                    last
                } else {
                    placed[k + 1] - 1.0
                };
                placed[k] = placed[k].min(ceiling).max(0.0);
            }
            for (&i, &place) in members.iter().zip(&placed) {
                slot[i] = place;
            }
        }
    }
    (0..count).map(|i| (columns[i], slot[i])).collect()
}

/// Classic: a tech's card, drawn over its button (`paint::draw_button`):
/// its rim and ground by where it stands, its name, its price and turns
/// (or RESEARCHED) with its tag at the right end, its unlocks and, while
/// it's researched, the progress bar.
pub(super) fn draw_tech_card(
    min: Vec2,
    max: Vec2,
    card: &TechCard,
    hovered: bool,
    out: &mut Vec<Vertex>,
) {
    let (rim, width) = card.rim();
    let ground = match (card.state, hovered) {
        (TechState::Researched, _) => [0.016, 0.034, 0.018, 0.95],
        (TechState::Researching, _) => [0.05, 0.043, 0.016, 0.95],
        (TechState::Waits, _) => [0.06, 0.03, 0.028, 0.95],
        (TechState::Queued, false) => [0.036, 0.032, 0.02, 0.95],
        (TechState::Locked, false) => [0.018, 0.019, 0.024, 0.95],
        (_, false) => [0.03, 0.035, 0.05, 0.95],
        (_, true) => [0.08, 0.09, 0.13, 0.95],
    };
    mesh::quad(min - width, max + width, rim, out);
    mesh::quad(min, max, ground, out);
    let (name_face, small) = (font::ui(BODY), font::ui(SMALL));
    let metrics = TreeMetrics {
        card_width: max.x - min.x,
        name_line: name_face.line_height,
        line: small.line_height + LINE_GAP,
    };
    let rows = CardRows::new(&metrics, max.y - min.y);
    // A line `down` from the top, its capitals centered in the line.
    let baseline = |down: f32, line: f32, cap: f32| max.y - down - (line + cap) / 2.0;
    let left = min.x + CARD_PAD;
    let room = max.x - min.x - 2.0 * CARD_PAD;
    let name = fit_text(card.tech.name(), room, |t| name_face.width(t));
    let y = baseline(rows.name, metrics.name_line, name_face.cap_height);
    name_face.push(Vec2::new(left, y), &name, card.name_color(), out);
    let y = baseline(rows.detail, metrics.line, small.cap_height);
    let tag_width = card.tag.as_ref().map_or(0.0, |tag| small.width(tag));
    let detail_room = room - if card.tag.is_some() { tag_width + TAG_GAP } else { 0.0 };
    let detail = fit_text(&card.detail, detail_room.max(0.0), |t| small.width(t));
    small.push(Vec2::new(left, y), &detail, card.detail_color(), out);
    if let Some(tag) = &card.tag {
        let x = max.x - CARD_PAD - tag_width;
        small.push(Vec2::new(x, y), tag, card.tag_color(), out);
    }
    let color = card.unlock_color();
    let names = card.names;
    for (index, unlock) in card.unlocks.iter().enumerate() {
        let (x, down) = if names {
            (left, rows.unlocks + index as f32 * metrics.line)
        } else {
            (left + index as f32 * UNLOCK_ICON, rows.unlocks)
        };
        let middle = max.y - down - metrics.line / 2.0;
        let center = Vec2::new(x + UNLOCK_ICON / 2.0 - 2.0, middle);
        push_unlock_icon(center, unlock.icon, color, out);
        if names {
            let room = room - UNLOCK_ICON;
            let name = fit_text(unlock.name, room, |t| small.width(t));
            let y = middle - small.cap_height / 2.0;
            small.push(Vec2::new(x + UNLOCK_ICON, y), &name, color, out);
        }
    }
    if let Some(progress) = card.progress {
        let top = max.y - rows.bar;
        let (bar_min, bar_max) = (Vec2::new(left, top - CARD_BAR), Vec2::new(max.x - CARD_PAD, top));
        mesh::quad(bar_min, bar_max, BAR_BG, out);
        let filled = bar_min.x + (bar_max.x - bar_min.x) * progress.clamp(0.0, 1.0);
        mesh::quad(bar_min, Vec2::new(filled, bar_max.y), GOLD_TEXT, out);
    }
}

/// How far an unlock's icon reaches from its center.
pub(super) const UNLOCK_ICON_RADIUS: f32 = 8.0;

/// An unlock's icon centered on `center`, Y up: a unit's pictogram in
/// `color`, or a building's badge, its letter drawn on it by the caller
/// in ImGui (`font::push_glyph` here draws the classic font's).
pub(super) fn push_unlock_icon(center: Vec2, icon: UnlockIcon, color: Color, out: &mut Vec<Vertex>) {
    match icon {
        UnlockIcon::Unit(icon) => {
            unit_icons::push_pictogram(center, UNLOCK_ICON_RADIUS, icon, color, out)
        }
        UnlockIcon::Building(letter, badge) => {
            push_badge_square(center, badge, color, out);
            font::push_glyph(center, BADGE_CAP, letter, BADGE_LETTER, out);
        }
    }
}

/// A building badge's square, dimmed with `color`'s dimming (a locked
/// card's).
pub(super) fn push_badge_square(center: Vec2, badge: Color, color: Color, out: &mut Vec<Vertex>) {
    let badge = if color == DIM_TEXT {
        [badge[0] * 0.5, badge[1] * 0.5, badge[2] * 0.5, badge[3]]
    } else {
        badge
    };
    let corner = BADGE_HALF * std::f32::consts::SQRT_2;
    mesh::regular_polygon(center, corner, 4, std::f32::consts::FRAC_PI_4, badge, out);
}

/// A building badge's letter, as the map draws it (`draw/palette.rs`).
pub(super) const BADGE_LETTER: Color = [0.05, 0.05, 0.05, 1.0];
/// Half a building badge's side, and its letter's capital height: in the
/// map's proportions.
pub(super) const BADGE_HALF: f32 = 7.5;
pub(super) const BADGE_CAP: f32 = 8.5;
