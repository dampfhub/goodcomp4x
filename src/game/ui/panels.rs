//! The top bar, the debug panel and the city/Barracks hover panel.

use super::builder::{ButtonSpec, PanelBuilder, push_text_row, single_line_button_width};
use super::dock::Zone;
use super::paint::fade;
use super::text::{end_turn_label, signed_quantity, turns_at_rate};
use super::{
    BODY, Button, ButtonState, DIM_TEXT, END_TURN_HEIGHT, GAP, GOLD_TEXT, LABEL_TEXT, Layout,
    MARGIN, NOTICE_TEXT, SMALL, TEXT, TITLE, TOP_BAR_HEIGHT, Target,
};
use crate::game::GameState;
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
            !self.cities[city].queue.is_empty() && self.cities[city].pending_building.is_none()
        } else {
            false
        };
        panel.compact_buttons(vec![debug_button(
            Target::CompleteProduction,
            "COMPLETE PRODUCTION",
            "F9",
            ButtonState::new(false, self.is_resolving() || !can_complete),
        )]);
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
        panel.compact_buttons(vec![debug_button(
            Target::ToggleFog,
            fog,
            "F10",
            ButtonState::Ready,
        )]);
        panel
    }

    /// Turn number on the left, the latest notice in the middle, and on the
    /// right the End Turn button, which names whatever the turn is still
    /// waiting on (clicking it selects that).
    pub(super) fn top_bar(&self, size: Vec2, layout: &mut Layout) {
        let min = Vec2::new(0.0, size.y - TOP_BAR_HEIGHT);
        layout.panel(min, size, false);
        let middle = min.y + TOP_BAR_HEIGHT / 2.0;

        let pending = self.pending();
        let turn = if self.is_resolving() {
            self.turn
        } else {
            self.turn + 1
        };
        let turn_text = format!("TURN {turn}");
        let left_end = MARGIN + font::ui(TITLE).width(&turn_text);
        push_text_row(
            layout,
            Vec2::new(MARGIN, middle),
            TITLE,
            vec![(turn_text, TEXT)],
        );

        let label = if self.is_resolving() {
            "RESOLVING".to_string()
        } else {
            end_turn_label(pending)
        };
        let hint = "SPACE".to_string();
        let width = single_line_button_width(&label, &hint);
        let button_min = Vec2::new(size.x - MARGIN - width, middle - END_TURN_HEIGHT / 2.0);
        let end_turn = Button {
            target: Target::EndTurn,
            label,
            hint,
            state: if self.is_resolving() {
                ButtonState::Disabled
            } else {
                ButtonState::new(pending == (0, 0), false)
            },
            armed: false,
            faded: false,
            min: button_min.round(),
            max: (button_min + Vec2::new(width, END_TURN_HEIGHT)).round(),
        };

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

    /// The open city: population, stores and income, growth, what it's
    /// building, and a card for each unit it can build.
    pub(super) fn structure_hover_panel(&self, i: usize, barracks: bool, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        if barracks {
            let tile = city.barracks.unwrap();
            let active = city.worked.first() == Some(&tile);
            let production = if active { self.barracks_income(i) } else { 0 };
            let queue = city.barracks_queue.first().map_or_else(
                || "EMPTY".into(),
                |build| {
                    format!(
                        "{} · {} LEFT",
                        build.name(),
                        turns_at_rate(build.cost() - city.barracks_production, production)
                    )
                },
            );
            panel.text(
                TITLE,
                vec![(format!("CITY {} BARRACKS", city.id + 1), city.team.color())],
            );
            panel.text(
                SMALL,
                vec![(
                    format!(
                        "HP {:.0}/{:.0} · {} PROD/T",
                        city.barracks_hp,
                        crate::game::city::BARRACKS_MAX_HP,
                        signed_quantity(production)
                    ),
                    GOLD_TEXT,
                )],
            );
            panel.text(SMALL, vec![(format!("QUEUE: {queue}"), DIM_TEXT)]);
            if let Some(build) = city.barracks_queue.first() {
                panel.bar((city.barracks_production as f32 / build.cost() as f32).clamp(0.0, 1.0));
            }
        } else {
            let (growth, _, _) = self.growth_status(i);
            let (_, production) = self.income(i);
            let queue = city.queue.first().map_or_else(
                || "EMPTY".into(),
                |build| {
                    format!(
                        "{} · {} LEFT",
                        build.name(),
                        turns_at_rate(
                            self.city_build_cost(i, *build) - city.production,
                            production
                        )
                    )
                },
            );
            panel.text(
                TITLE,
                vec![(format!("CITY {}", city.id + 1), city.team.color())],
            );
            panel.text(
                SMALL,
                vec![(
                    format!("POP {growth}% · {} PROD/T", signed_quantity(production)),
                    GOLD_TEXT,
                )],
            );
            panel.bar(growth as f32 / 100.0);
            panel.text(SMALL, vec![(format!("QUEUE: {queue}"), DIM_TEXT)]);
            if let Some(build) = city.queue.first() {
                panel.bar(
                    (city.production as f32 / self.city_build_cost(i, *build) as f32)
                        .clamp(0.0, 1.0),
                );
            }
        }
    }
}
