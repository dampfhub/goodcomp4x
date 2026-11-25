//! The research view (T, or a city's Research button), which takes the
//! screen below the top bar, every other panel hidden: the tech tree
//! (`research_tray`, `tech_tree`: what each card shows and where the lines
//! go; `tech_tree.rs` lays it out) and, at its right, the side's research
//! queue (`research_queue_panel`); classic places the two in
//! `research_screen`, ImGui in `imgui/tech_tree.rs`. And the research the
//! top bar shows (`research_spans`). The rules are `game/research.rs`.

use glam::Vec2;

use super::builder::{ButtonSpec, PanelBuilder, Row};
use super::queue::queue_items_that_fit;
use super::tech_tree::{
    Link, LinkState, TechCard, TechState, TechTree, UnlockIcon, UnlockMark, arrange,
};
use super::text::{cost_hint, turns_text};
use super::{
    Color, DIM_TEXT, GAP, GOLD_TEXT, LABEL_TEXT, Layout, Line, MARGIN, PADDING, QueueItemSpec,
    QueueKind, REDUCED_TEXT, SMALL, Shape, TEXT, TITLE, TOP_BAR_HEIGHT, Target, WAITING_TEXT, Zone,
};
use crate::game::GameState;
use crate::game::city::{Build, stock_icons};
use crate::game::draw::building_badge;
use crate::game::keys::Command;
use crate::game::research::{ResearchForecast, Tech, Unlock, names};
use crate::game::strings::text;
use crate::game::unit_icons::UnitIcon;

/// Classic: the research queue's width, at the screen's right.
const QUEUE_COLUMN: f32 = 320.0;
/// Classic: the research view's ground, under its two panels, which hides
/// the map.
const GROUND: Color = [0.006, 0.007, 0.01, 1.0];

/// Its tier: 1 for a tech that needs none, else one past the highest tier
/// among those it needs.
pub(super) fn tier(tech: Tech) -> usize {
    1 + tech.requires().map(tier).max().unwrap_or(0)
}

/// A tier's numeral, as the tree's headings and names write it.
fn numeral(tier: usize) -> String {
    const NUMERALS: [&str; 10] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
    NUMERALS
        .get(tier.wrapping_sub(1))
        .map_or_else(|| tier.to_string(), |n| (*n).to_string())
}

impl GameState {
    /// Whether the research view shows: opened, and nothing else selected
    /// and the settings menu shut (opening anything else closes it,
    /// `close_research_behind`).
    pub(in crate::game) fn research_view(&self) -> bool {
        self.research_open
            && !self.settings_open
            && self.selected_city.is_none()
            && self.selected_barracks.is_none()
            && self.interior_view.is_none()
            && self.selected.is_none()
            && self.group.is_empty()
    }

    /// T, or a city's Research button: opens the research view, leaving
    /// whatever was open, or closes it.
    pub fn toggle_research(&mut self) {
        if self.research_view() {
            self.close_research();
            return;
        }
        self.leave_city_view();
        self.close_settings();
        self.selected = None;
        self.group.clear();
        self.ui_click_mode = None;
        self.research_open = true;
        self.research_queue_scroll = 0;
        self.notice = text!(
            "research_open_notice",
            key = Command::Research.key(),
            back = Command::Back.key()
        );
    }

    pub(in crate::game) fn close_research(&mut self) {
        self.research_open = false;
        self.queue_drag = None;
    }

    /// Something else opened over the research view closes it, so it
    /// doesn't come back when that closes. Each frame (`update`).
    pub(in crate::game) fn close_research_behind(&mut self) {
        if self.research_open && !self.research_view() {
            self.close_research();
        }
    }

    /// The top bar's research: the tech researched now and the turns it
    /// has left, red while it waits for the stockpile; NO RESEARCH when
    /// nothing is queued. Short, as the classic bar has the notice beside it.
    pub(super) fn research_spans(&self) -> Line {
        let team = self.local_team;
        let value = match self.research_forecast(team) {
            ResearchForecast::Idle => (text!("status_research_none").to_string(), DIM_TEXT),
            ResearchForecast::Works { tech, .. } => {
                let left = self.research_turns_left(team).unwrap_or(0).max(0) as u32;
                (format!("{} {}", tech.name(), turns_text(left)), TEXT)
            }
            ResearchForecast::Waits { tech, .. } => (
                text!("status_research_waits", tech = tech.name()),
                REDUCED_TEXT,
            ),
        };
        vec![("   ".into(), LABEL_TEXT), value]
    }

    /// Classic: the research view over the screen below the top bar, on a
    /// ground that hides the map: the tree's panel (`research_tray`) and,
    /// at the right, the queue's (`research_queue_panel`), as tall as the
    /// room. `layout()` places nothing else while it shows.
    pub(super) fn research_screen(&self, screen: Vec2, layout: &mut Layout) {
        let ground = Vec2::new(screen.x, screen.y - TOP_BAR_HEIGHT);
        layout.shapes.push(Shape::Fill {
            min: Vec2::ZERO,
            max: ground,
            color: GROUND,
        });
        let width = screen.x - 2.0 * MARGIN;
        let height = layout.remaining_height(Zone::TopLeft, width);
        let mut queue = PanelBuilder::default();
        self.research_queue_panel(queue_items_that_fit(height), &mut queue);
        queue.min_size = Vec2::new(QUEUE_COLUMN.min(width * 0.4).round(), height);
        let tree_width = width - queue.size().x - GAP;
        let mut tree = PanelBuilder::default();
        self.research_tray(&mut tree);
        for row in &mut tree.rows {
            if let Row::TechTree(tree) = row {
                tree.room = Some(tree_width - 2.0 * PADDING);
            }
        }
        tree.min_size = Vec2::new(tree_width, height);
        tree.fit_height(height);
        layout.dock_panel(tree, Zone::TopLeft);
        layout.dock_panel(queue, Zone::TopRight);
        // The ground takes the clicks between them too, after the two, so a
        // button's panel is found before it.
        layout.panels.push((Vec2::ZERO, ground));
    }

    /// The research view's tree, under what's researched now.
    pub(super) fn research_tray(&self, panel: &mut PanelBuilder) {
        let team = self.local_team;
        panel.text(
            TITLE,
            vec![(text!("research_title").to_string(), team.color())],
        );
        let status = match self.research_forecast(team) {
            ResearchForecast::Idle => (text!("research_idle").to_string(), GOLD_TEXT),
            ResearchForecast::Works { tech, .. } => {
                let left = self.research_turns_left(team).unwrap_or(0).max(0) as u32;
                let turns = turns_text(left);
                let line = text!("research_working", tech = tech.name(), turns = turns);
                (line, GOLD_TEXT)
            }
            ResearchForecast::Waits { tech, short } => {
                let short = stock_icons(short);
                let line = text!("research_waits", tech = tech.name(), short = short);
                (line, WAITING_TEXT)
            }
        };
        panel.text(SMALL, vec![status]);
        panel.text(SMALL, vec![(text!("research_how").to_string(), DIM_TEXT)]);
        panel.gap(GAP);
        panel.tech_tree(self.tech_tree());
    }

    /// The tree as the side has it: a card for each tech, where it goes
    /// (`tech_tree::arrange`, by tier), and a line from each tech to each
    /// that needs it.
    pub(super) fn tech_tree(&self) -> TechTree {
        let team = self.local_team;
        let techs: Vec<Tech> = Tech::all().collect();
        let index = |tech: Tech| usize::from(tech.0);
        let columns: Vec<usize> = techs.iter().map(|&tech| tier(tech) - 1).collect();
        let needs: Vec<Vec<usize>> = techs
            .iter()
            .map(|tech| tech.requires().map(index).collect())
            .collect();
        let mut links = Vec::new();
        for (to, parents) in needs.iter().enumerate() {
            for &from in parents {
                let state = if self.researched(team, techs[from]) {
                    LinkState::Done
                } else if self.research_place(team, techs[from]).is_some()
                    && self.research_place(team, techs[to]).is_some()
                {
                    LinkState::Planned
                } else {
                    LinkState::Open
                };
                links.push(Link { from, to, state });
            }
        }
        let tiers = columns.iter().max().map_or(0, |&c| c + 1);
        let longest = techs.iter().map(|t| t.turns()).max().unwrap_or(0).max(0) as u32;
        TechTree {
            cards: techs
                .iter()
                .map(|&tech| (self.tech_card(tech), self.tech_card_view(tech)))
                .collect(),
            slots: arrange(&columns, &needs),
            links,
            headings: (1..=tiers)
                .map(|t| text!("research_tier", tier = numeral(t)))
                .collect(),
            tags: vec![
                text!("research_card_queued", place = techs.len()),
                text!("research_card_left", turns = turns_text(longest)),
                text!("research_card_waits").to_string(),
            ],
            names: true,
            room: None,
        }
    }

    /// A tech's card: its name, and its price and turns, or where it
    /// stands (researched, worked, queued). A researched tech's card is
    /// off; any other queues it, and what it needs first.
    fn tech_card(&self, tech: Tech) -> ButtonSpec {
        let team = self.local_team;
        if self.researched(team, tech) {
            let done = text!("research_card_done");
            return ButtonSpec::new(Target::Research(tech), tech.name(), done)
                .unavailable(Some(done.into()));
        }
        let place = self.research_place(team, tech);
        let hint = match place {
            Some(0) => match self.research_forecast(team) {
                ResearchForecast::Waits { .. } => text!("research_card_waits").to_string(),
                _ => {
                    let left = self.research_turns_left(team).unwrap_or(0).max(0) as u32;
                    text!("research_card_left", turns = turns_text(left))
                }
            },
            Some(index) => text!("research_card_queued", place = index + 1),
            None => cost_hint(tech.price(), tech.turns()),
        };
        let short = place.is_none() && !self.expected_stock(team).covers(tech.price());
        ButtonSpec::new(Target::Research(tech), tech.name(), hint)
            .queued(place.is_some())
            .short(short)
    }

    /// What a tech's card shows: where it stands, its price and turns (or
    /// RESEARCHED) and its tag, what it unlocks, and its progress while
    /// it's researched.
    fn tech_card_view(&self, tech: Tech) -> TechCard {
        let team = self.local_team;
        let place = self.research_place(team, tech);
        let state = if self.researched(team, tech) {
            TechState::Researched
        } else {
            match (place, self.research_forecast(team)) {
                (Some(0), ResearchForecast::Waits { .. }) => TechState::Waits,
                (Some(0), _) => TechState::Researching,
                (Some(_), _) => TechState::Queued,
                (None, _) if tech.requires().all(|r| self.researched(team, r)) => {
                    TechState::Available
                }
                (None, _) => TechState::Locked,
            }
        };
        let tag = match state {
            TechState::Researching => {
                let left = self.research_turns_left(team).unwrap_or(0).max(0) as u32;
                Some(text!("research_card_left", turns = turns_text(left)))
            }
            TechState::Waits => Some(text!("research_card_waits").to_string()),
            TechState::Queued => place.map(|i| text!("research_card_queued", place = i + 1)),
            _ => None,
        };
        let detail = match state {
            TechState::Researched => text!("research_card_done").to_string(),
            _ => cost_hint(tech.price(), tech.turns()),
        };
        let progress = (state == TechState::Researching).then(|| {
            let head = self.research(team).queue[0];
            (head.progress as f32 / tech.work() as f32).clamp(0.0, 1.0)
        });
        let unlocks = tech
            .unlocks()
            .iter()
            .map(|&unlock| UnlockMark {
                icon: match unlock {
                    Unlock::Unit(unit) => UnlockIcon::Unit(
                        UnitIcon::of_build(Build::Unit(unit)).expect("a unit has a pictogram"),
                    ),
                    Unlock::Building(building) => {
                        let (letter, color) = building_badge(building);
                        UnlockIcon::Building(letter, color)
                    }
                },
                name: unlock.name(),
            })
            .collect();
        TechCard {
            tech,
            state,
            detail,
            tag,
            unlocks,
            progress,
            short: place.is_none() && !self.expected_stock(team).covers(tech.price()),
            names: true,
        }
    }

    /// The research queue panel: its title with Clear, and a row for each
    /// tech, the first marked while it's worked, red while it waits; a line
    /// saying it's empty when it is.
    pub(super) fn research_queue_panel(&self, visible: usize, panel: &mut PanelBuilder) {
        let team = self.local_team;
        let queue = &self.research(team).queue;
        if queue.is_empty() {
            let empty = text!("research_queue_empty").to_string();
            panel.text(SMALL, vec![(empty, LABEL_TEXT)]);
            return;
        }
        let offset = self
            .research_queue_scroll
            .min(queue.len().saturating_sub(visible));
        if queue.len() > visible {
            panel.scrollbar = Some((QueueKind::Research, offset, queue.len(), visible));
        }
        self.queue_title(
            text!("research_queue_title"),
            Target::ClearResearchQueue,
            panel,
        );
        let forecast = self.research_forecast(team);
        let drag = self
            .queue_drag
            .filter(|drag| drag.kind == QueueKind::Research);
        for (index, item) in queue.iter().enumerate().skip(offset).take(visible) {
            let tech = item.build;
            let head = index == 0;
            let (active, waiting, state) = match forecast {
                ResearchForecast::Works { .. } if head => {
                    let left = self.research_turns_left(team).unwrap_or(0).max(0) as u32;
                    (
                        true,
                        false,
                        text!("research_card_left", turns = turns_text(left)),
                    )
                }
                ResearchForecast::Waits { short, .. } if head => (
                    false,
                    true,
                    text!("research_row_short", short = stock_icons(short)),
                ),
                _ => (false, false, turns_text(tech.turns() as u32)),
            };
            let prefix = if active { "> " } else { "  " };
            panel.queue_item(QueueItemSpec {
                kind: QueueKind::Research,
                index,
                label: format!("{prefix}{} | {state}", tech.name()),
                active,
                waiting,
                dragging: drag.is_some_and(|drag| drag.source == index),
                drop_target: drag
                    .is_some_and(|drag| drag.target == Some(index) && drag.source != index),
            });
        }
    }

    /// A tech card's tooltip: what it unlocks and needs, its price and
    /// turns, and what queuing it queues first.
    pub(super) fn tech_tooltip(&self, tech: Tech) -> (String, String, Option<String>) {
        let team = self.local_team;
        let mut description = text!("research_unlocks", unlocks = tech.unlocks_text());
        let needs: Vec<&str> = tech.requires().map(|r| r.name()).collect();
        if !needs.is_empty() {
            description.push(' ');
            description.push_str(&text!("research_needs_all", techs = names(needs)));
        }
        description.push(' ');
        description.push_str(&cost_hint(tech.price(), tech.turns()));
        let path = self.research_path(team, tech);
        let first: Vec<&str> = path[..path.len().saturating_sub(1)]
            .iter()
            .map(|t| t.name())
            .collect();
        let caution =
            (!first.is_empty()).then(|| text!("research_queues_first", techs = names(first)));
        (description, text!("research_card_key").into(), caution)
    }
}
