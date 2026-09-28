//! The top bar, the debug panel and the city/Barracks hover panel.

use super::builder::{ButtonSpec, PanelBuilder, push_text_row, single_line_button_width};
use super::dock::Zone;
use super::paint::fade;
use super::text::{end_turn_label, price_hint, stock_spans};
use super::{
    BODY, Button, ButtonState, DIM_TEXT, END_TURN_HEIGHT, GAP, GOLD_TEXT, LABEL_TEXT, Layout, Line,
    MARGIN, NOTICE_TEXT, SMALL, TEXT, TITLE, TOP_BAR_HEIGHT, Target,
};
use crate::game::GameState;
use crate::game::city::{MAX_CITY_POPULATION, Stock, turns_icon};
use crate::game::font;
use crate::game::scenario::Scenario;
use glam::Vec2;

impl GameState {
    /// Testing tools, top-left under the top bar and see-through so they
    /// don't read as game UI: the scenario pages (the current one gold;
    /// pressing it again restarts it) and the savestate.
    pub(super) fn debug_panel(&self, layout: &mut Layout) {
        layout.dock_panel(self.debug_panel_content(), Zone::TopLeft);
    }

    pub(super) fn debug_panel_content(&self) -> PanelBuilder {
        let mut panel = PanelBuilder {
            faded: true,
            ..PanelBuilder::default()
        };
        panel.text(SMALL, vec![("DEBUG".into(), fade(LABEL_TEXT, true))]);
        let debug_button = |target, label: &str, hint: &str, state| ButtonSpec {
            target,
            label: label.into(),
            hint: hint.into(),
            state,
            armed: false,
        };
        panel.compact_buttons(
            Scenario::ALL[..4]
                .iter()
                .copied()
                .map(|scenario| {
                    let current = ButtonState::new(scenario == self.scenario, false);
                    debug_button(
                        Target::Scenario(scenario),
                        scenario.name(),
                        scenario.key(),
                        current,
                    )
                })
                .collect(),
        );
        if let Some(seed) = self.map_seed {
            panel.text(
                SMALL,
                vec![(format!("MAP SEED {seed}"), fade(DIM_TEXT, true))],
            );
        }
        panel.gap(GAP);
        panel.compact_buttons(vec![
            debug_button(
                Target::Scenario(Scenario::Siege),
                "SIEGE",
                "F12",
                ButtonState::new(self.scenario == Scenario::Siege, false),
            ),
            debug_button(
                Target::Scenario(Scenario::Naval),
                "NAVAL",
                "",
                ButtonState::new(self.scenario == Scenario::Naval, false),
            ),
            debug_button(
                Target::SaveState,
                "SAVE",
                "F6",
                ButtonState::new(false, self.is_resolving()),
            ),
            debug_button(
                Target::LoadState,
                "LOAD",
                "F7",
                ButtonState::new(false, self.savestate.is_none()),
            ),
        ]);
        let can_complete = if let Some(city) = self.selected_barracks {
            !self.cities[city].barracks_queue.is_empty()
        } else if let Some(city) = self.selected_city {
            !self.cities[city].queue.is_empty()
        } else {
            false
        };
        // Beside it, how the Cavalry and Armored cap counts
        // (`city/barracks.rs`): those alive, or every one ever trained.
        let cap = if self.lifetime_special_cap {
            "UNIT CAP: EVER"
        } else {
            "UNIT CAP: ALIVE"
        };
        panel.compact_buttons(vec![
            debug_button(
                Target::CompleteProduction,
                "FINISH BUILD",
                "F9",
                ButtonState::new(false, self.is_resolving() || !can_complete),
            ),
            debug_button(Target::ToggleLifetimeCap, cap, "", ButtonState::Ready),
        ]);
        if let Some(saved) = self.saved_summary() {
            panel.text(
                SMALL,
                vec![(format!("SAVED: {saved}"), fade(DIM_TEXT, true))],
            );
        }
        panel.gap(GAP);
        let playback = if self.settings.instant_playback {
            "PLAYBACK: ALL AT ONCE"
        } else {
            "PLAYBACK: STEP BY STEP"
        };
        let fog = if self.fog_of_war {
            "FOG OF WAR: ON"
        } else {
            "FOG OF WAR: OFF"
        };
        panel.compact_buttons(vec![debug_button(
            Target::TogglePlayback,
            playback,
            "F8",
            ButtonState::Ready,
        )]);
        // Beside fog, the economy experiment's variant
        // (`docs/rts-economy.md`): production speeds builds.
        let speedup = if self.production_speedup {
            "PROD SPEEDUP: ON"
        } else {
            "PROD SPEEDUP: OFF"
        };
        panel.compact_buttons(vec![
            debug_button(Target::ToggleFog, fog, "F10", ButtonState::Ready),
            debug_button(
                Target::ToggleProductionSpeedup,
                speedup,
                "",
                ButtonState::Ready,
            ),
        ]);
        panel
    }

    /// The player's stockpile and its change a turn (every city's delivery,
    /// less the citizens' food), for the top bar in both presentations.
    pub(super) fn stockpile_line(&self) -> Line {
        let income = self.side_income(self.local_team);
        let change = Stock {
            food: income.food - self.upkeep(self.local_team),
            ..income
        };
        stock_spans(self.stock(self.local_team), change)
    }

    /// The turn the top bar names: the one playing out, or the one being
    /// planned (still being planned while a network game waits for the
    /// others' plans, before it has begun to play out).
    pub(super) fn shown_turn(&self) -> u32 {
        if self.is_playing_out() {
            self.turn
        } else {
            self.turn + 1
        }
    }

    /// Turn number and the stockpile on the left, the latest notice in the
    /// middle, and on the right the End Turn button, which names whatever the turn is still
    /// waiting on (clicking it selects that).
    pub(super) fn top_bar(&self, size: Vec2, layout: &mut Layout) {
        let min = Vec2::new(0.0, size.y - TOP_BAR_HEIGHT);
        layout.panel(min, size, false);
        let middle = min.y + TOP_BAR_HEIGHT / 2.0;

        let pending = self.pending();
        let turn = self.shown_turn();
        let turn_text = format!("TURN {turn}");
        let turn_end = MARGIN + font::ui(TITLE).width(&turn_text);
        push_text_row(
            layout,
            Vec2::new(MARGIN, middle),
            TITLE,
            vec![(turn_text, TEXT)],
        );
        // The player's stockpile beside the turn number.
        let stockpile = self.stockpile_line();
        let stockpile_width: f32 = stockpile
            .iter()
            .map(|(text, _)| font::ui(BODY).width(text))
            .sum();
        push_text_row(
            layout,
            Vec2::new(turn_end + 2.0 * GAP, middle),
            BODY,
            stockpile,
        );
        let left_end = turn_end + 2.0 * GAP + stockpile_width;

        let label = if self.is_resolving() {
            self.resolving_label()
        } else {
            end_turn_label(pending)
        };
        // While waiting for the others, a click takes the turn back.
        let hint = if self.waiting_for_peers() {
            "TAKE BACK"
        } else {
            "SPACE"
        }
        .to_string();
        let width = single_line_button_width(&label, &hint);
        let button_min = Vec2::new(size.x - MARGIN - width, middle - END_TURN_HEIGHT / 2.0);
        let end_turn = Button {
            target: Target::EndTurn,
            label,
            hint,
            state: if self.is_playing_out() {
                ButtonState::Disabled
            } else if self.waiting_for_peers() {
                ButtonState::Ready
            } else {
                ButtonState::new(pending == (0, 0), false)
            },
            armed: false,
            faded: false,
            min: button_min.round(),
            max: (button_min + Vec2::new(width, END_TURN_HEIGHT)).round(),
        };

        let menu_width = single_line_button_width("MENU", "");
        let menu_min = Vec2::new(left_end + GAP * 2.0, middle - END_TURN_HEIGHT / 2.0);
        layout.buttons.push(Button {
            target: Target::OpenSettings,
            label: "MENU".into(),
            hint: String::new(),
            state: ButtonState::Ready,
            armed: false,
            faded: false,
            min: menu_min.round(),
            max: (menu_min + Vec2::new(menu_width, END_TURN_HEIGHT)).round(),
        });
        let left_end = menu_min.x + menu_width;

        // The notice sits centered in the space left between the two.
        let notice = self.shown_notice();
        if !notice.is_empty() {
            let width = font::ui(BODY).width(notice);
            let space = (left_end + 2.0 * GAP, end_turn.min.x - 2.0 * GAP);
            let left = ((space.0 + space.1 - width) / 2.0).max(space.0);
            if left + width <= space.1 {
                let line = vec![(notice.to_string(), NOTICE_TEXT)];
                push_text_row(layout, Vec2::new(left, middle), BODY, line);
            }
        }
        layout.buttons.push(end_turn);
    }

    /// A hovered city or Barracks: population and what the city delivers,
    /// or the Barracks' health, and what either is building.
    pub(super) fn structure_hover_panel(&self, i: usize, barracks: bool, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        if barracks {
            let queue = match (city.barracks_queue.first(), self.barracks_turns_left(i)) {
                (Some(build), Some(turns)) => {
                    format!("{} · {} LEFT", build.name(), turns_icon(turns))
                }
                _ => "EMPTY".into(),
            };
            panel.text(
                TITLE,
                vec![(format!("CITY {} BARRACKS", city.id + 1), city.team.color())],
            );
            panel.text(
                SMALL,
                vec![(
                    format!(
                        "HP {:.0}/{:.0}",
                        city.barracks_hp,
                        crate::game::city::BARRACKS_MAX_HP,
                    ),
                    GOLD_TEXT,
                )],
            );
            panel.text(SMALL, vec![(format!("QUEUE: {queue}"), DIM_TEXT)]);
            if let Some(build) = city.barracks_queue.first() {
                panel.bar((city.barracks_progress as f32 / build.work() as f32).clamp(0.0, 1.0));
            }
        } else {
            let queue = match (city.queue.first(), self.turns_left(i)) {
                (Some(build), Some(turns)) => {
                    format!("{} · {} LEFT", build.name(), turns_icon(turns))
                }
                _ => "EMPTY".into(),
            };
            panel.text(
                TITLE,
                vec![(format!("CITY {}", city.id + 1), city.team.color())],
            );
            panel.text(
                SMALL,
                vec![(
                    format!(
                        "POP {}/{MAX_CITY_POPULATION} · DELIVERS {}",
                        city.population,
                        price_hint(self.income(i))
                    ),
                    GOLD_TEXT,
                )],
            );
            panel.text(SMALL, vec![(format!("QUEUE: {queue}"), DIM_TEXT)]);
            if let Some(build) = city.queue.first() {
                panel.bar(
                    (city.progress as f32 / self.city_build_work(i, *build) as f32).clamp(0.0, 1.0),
                );
            }
        }
    }
}
