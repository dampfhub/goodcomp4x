//! The research view (T, or a city's Research button): the tech tree in
//! the Selection panel (`research_tray`), its tiers, each tech's state,
//! price and what it unlocks, and the side's research queue in the queue
//! panel (`research_queue_panel`); and the research the top bar shows
//! (`research_spans`). The rules are `game/research.rs`.

use super::builder::{ButtonSpec, PanelBuilder};
use super::text::{cost_hint, turns_text};
use super::{
    BOOSTED_TEXT, DIM_TEXT, GAP, GOLD_TEXT, LABEL_TEXT, Line, QueueItemSpec, QueueKind,
    REDUCED_TEXT, SMALL, TEXT, TITLE, Target, WAITING_TEXT,
};
use crate::game::GameState;
use crate::game::city::stock_icons;
use crate::game::keys::Command;
use crate::game::research::{ResearchForecast, Tech, names};
use crate::game::strings::text;

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
    /// (opening anything else closes it, `close_research_behind`).
    pub(in crate::game) fn research_view(&self) -> bool {
        self.research_open
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

    /// The research view's Selection panel: what's researched now, then the
    /// tree by tier, a card for each tech and a line of what it unlocks and
    /// needs.
    pub(super) fn research_tray(&self, panel: &mut PanelBuilder) {
        let team = self.local_team;
        panel.text(
            TITLE,
            vec![(text!("research_title").to_string(), team.color())],
        );
        match self.research_forecast(team) {
            ResearchForecast::Idle => {
                panel.text(SMALL, vec![(text!("research_idle").to_string(), GOLD_TEXT)]);
            }
            ResearchForecast::Works { tech, .. } => {
                let left = self.research_turns_left(team).unwrap_or(0).max(0) as u32;
                panel.text(
                    SMALL,
                    vec![(
                        text!(
                            "research_working",
                            tech = tech.name(),
                            turns = turns_text(left)
                        ),
                        GOLD_TEXT,
                    )],
                );
                let head = self.research(team).queue[0];
                panel.bar((head.progress as f32 / tech.work() as f32).clamp(0.0, 1.0));
            }
            ResearchForecast::Waits { tech, short } => {
                panel.text(
                    SMALL,
                    vec![(
                        text!(
                            "research_waits",
                            tech = tech.name(),
                            short = stock_icons(short)
                        ),
                        WAITING_TEXT,
                    )],
                );
            }
        }
        panel.text(SMALL, vec![(text!("research_how").to_string(), DIM_TEXT)]);
        let tiers = Tech::all().map(tier).max().unwrap_or(0);
        for t in 1..=tiers {
            let techs: Vec<Tech> = Tech::all().filter(|&tech| tier(tech) == t).collect();
            panel.gap(GAP);
            panel.heading(&text!("research_tier", tier = numeral(t)));
            panel.buttons(techs.iter().map(|&tech| self.tech_card(tech)).collect());
            for &tech in &techs {
                panel.text(SMALL, self.tech_line(tech));
            }
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

    /// The line under the tier's cards for `tech`: its name, what it
    /// unlocks, and the techs it needs that the side hasn't researched.
    fn tech_line(&self, tech: Tech) -> Line {
        let team = self.local_team;
        let done = self.researched(team, tech);
        let mut line = vec![
            (format!("{}: ", tech.name()), LABEL_TEXT),
            (tech.unlocks_text(), if done { BOOSTED_TEXT } else { TEXT }),
        ];
        let missing: Vec<&str> = tech
            .requires()
            .filter(|&r| !self.researched(team, r))
            .map(|r| r.name())
            .collect();
        if !missing.is_empty() {
            line.push((
                format!(" · {}", text!("research_needs", techs = names(missing))),
                DIM_TEXT,
            ));
        }
        line
    }

    /// The research queue panel: its title with Clear, and a row for each
    /// tech, the first marked while it's worked, red while it waits.
    pub(super) fn research_queue_panel(&self, visible: usize, panel: &mut PanelBuilder) {
        let team = self.local_team;
        let queue = &self.research(team).queue;
        if queue.is_empty() {
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
