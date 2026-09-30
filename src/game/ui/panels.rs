//! The top bar, the debug panel and the city/Barracks hover panel.

use super::builder::{ButtonSpec, PanelBuilder, push_text_row, single_line_button_width};
use super::dock::Zone;
use super::paint::fade;
use super::queue::waiting_line;
use super::text::{end_turn_label, fit_text, price_hint, stock_spans};
use super::{
    BODY, DIM_TEXT, END_TURN_HEIGHT, GAP, GOLD_TEXT, LABEL_TEXT, Layout, Line, MARGIN, NOTICE_TEXT,
    REDUCED_TEXT, SMALL, TEXT, TITLE, TOP_BAR_HEIGHT, Target,
};
use crate::game::GameState;
use crate::game::city::{GATHER_YIELD, Lane, MAX_CITY_POPULATION, Stock, stock_icons, turns_icon};
use crate::game::font;
use crate::game::keys::Command;
use crate::game::scenario::Scenario;
use crate::game::strings::{hover_text, text};
use crate::game::unit::Team;
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
        panel.text(
            SMALL,
            vec![(text!("debug_title").into(), fade(LABEL_TEXT, true))],
        );
        let debug_button = |target, label: &str, hint: String| ButtonSpec::new(target, label, hint);
        // The current scenario's page is gold; pressing it restarts it.
        let scenario = |scenario: Scenario| {
            let hint = Command::Scenario(scenario).key();
            ButtonSpec::new(Target::Scenario(scenario), scenario.title(), hint)
                .hover_text(scenario.title_on_hover())
                .queued(scenario == self.scenario)
        };
        panel.compact_buttons(
            Scenario::ALL[..4]
                .iter()
                .map(|&page| scenario(page))
                .collect(),
        );
        if let Some(seed) = self.map_seed {
            panel.text(
                SMALL,
                vec![(text!("debug_map_seed", seed = seed), fade(DIM_TEXT, true))],
            );
        }
        panel.gap(GAP);
        let playing_out = || {
            self.is_resolving()
                .then(|| text!("debug_not_while_playing_out").to_string())
        };
        panel.compact_buttons(vec![
            scenario(Scenario::Siege),
            scenario(Scenario::Naval),
            debug_button(Target::SaveState, text!("debug_save"), Command::Save.key())
                .hover_text(hover_text!("debug_save"))
                .unavailable(playing_out()),
            debug_button(Target::LoadState, text!("debug_load"), Command::Load.key())
                .hover_text(hover_text!("debug_load"))
                .unavailable(
                    self.savestate
                        .is_none()
                        .then(|| text!("debug_nothing_saved").into()),
                ),
        ]);
        // What F9 would finish: the open Barracks' or city's current build.
        let complete = if self.is_resolving() {
            Some(text!("debug_not_while_playing_out"))
        } else if let Some(city) = self.selected_barracks {
            self.cities[city]
                .barracks_queue
                .is_empty()
                .then_some(text!("debug_barracks_queue_empty"))
        } else if let Some(city) = self.selected_city {
            self.cities[city]
                .queue
                .is_empty()
                .then_some(text!("debug_city_queue_empty"))
        } else {
            Some(text!("debug_open_a_city"))
        };
        // Beside it, how the Cavalry and Armored cap counts
        // (`city/barracks.rs`): those alive, or every one ever trained.
        let cap = if self.lifetime_special_cap {
            debug_button(
                Target::ToggleLifetimeCap,
                text!("debug_unit_cap_ever"),
                String::new(),
            )
            .hover_text(hover_text!("debug_unit_cap_ever"))
        } else {
            debug_button(
                Target::ToggleLifetimeCap,
                text!("debug_unit_cap_alive"),
                String::new(),
            )
            .hover_text(hover_text!("debug_unit_cap_alive"))
        };
        panel.compact_buttons(vec![
            debug_button(
                Target::CompleteProduction,
                text!("debug_finish_build"),
                Command::FinishBuild.key(),
            )
            .hover_text(hover_text!("debug_finish_build"))
            .unavailable(complete.map(String::from)),
            cap,
        ]);
        if let Some(saved) = self.saved_summary() {
            panel.text(
                SMALL,
                vec![(text!("debug_saved", saved = saved), fade(DIM_TEXT, true))],
            );
        }
        panel.gap(GAP);
        let playback = if self.settings.instant_playback {
            debug_button(
                Target::TogglePlayback,
                text!("debug_playback_at_once"),
                Command::Playback.key(),
            )
            .hover_text(hover_text!("debug_playback_at_once"))
        } else {
            debug_button(
                Target::TogglePlayback,
                text!("debug_playback_step_by_step"),
                Command::Playback.key(),
            )
            .hover_text(hover_text!("debug_playback_step_by_step"))
        };
        let fog = if self.fog_of_war {
            debug_button(Target::ToggleFog, text!("debug_fog_on"), Command::Fog.key())
                .hover_text(hover_text!("debug_fog_on"))
        } else {
            debug_button(
                Target::ToggleFog,
                text!("debug_fog_off"),
                Command::Fog.key(),
            )
            .hover_text(hover_text!("debug_fog_off"))
        };
        panel.compact_buttons(vec![playback]);
        // Beside fog, the economy experiment's variant
        // (`docs/rts-economy.md`): production speeds builds.
        let speedup = if self.production_speedup {
            debug_button(
                Target::ToggleProductionSpeedup,
                text!("debug_speedup_on"),
                String::new(),
            )
            .hover_text(hover_text!("debug_speedup_on"))
        } else {
            debug_button(
                Target::ToggleProductionSpeedup,
                text!("debug_speedup_off"),
                String::new(),
            )
            .hover_text(hover_text!("debug_speedup_off"))
        };
        panel.compact_buttons(vec![fog, speedup]);
        panel
    }

    /// The player's stockpile and its change a turn (every city's delivery,
    /// less the citizens' food), for the top bar in both presentations.
    pub(super) fn stockpile_line(&self) -> Line {
        let income = self.shown_side_income(self.local_team);
        let change = Stock {
            food: income.food - self.upkeep(self.local_team),
            ..income
        };
        stock_spans(self.stock(self.local_team), Some(change))
    }

    /// `team`'s supply (`city/supply.rs`): what it uses of what its cities
    /// give, red once it's all used.
    pub(super) fn supply_spans(&self, team: Team) -> Line {
        let (used, cap) = (self.supply_used(team), self.supply_cap(team));
        let color = if used >= cap { REDUCED_TEXT } else { TEXT };
        vec![
            (format!("{} ", text!("status_supply")), LABEL_TEXT),
            (format!("{used}/{cap}"), color),
        ]
    }

    /// The supply line of a city's or Barracks' tray: `supply_spans`, and
    /// once it's all used, that nothing new trains.
    pub(super) fn supply_tray_line(&self, team: Team) -> Line {
        let mut line = self.supply_spans(team);
        if self.supply_used(team) >= self.supply_cap(team) {
            line.push((
                " · FULL: NO NEW TROOPS, SHIPS OR SCOUTS".into(),
                REDUCED_TEXT,
            ));
        } else {
            line.push((" · TROOPS, SHIPS AND SCOUTS USE IT".into(), DIM_TEXT));
        }
        line
    }

    /// The player's supply for the top bar (`supply_spans`).
    pub(super) fn supply_line(&self) -> Line {
        let mut line = self.supply_spans(self.local_team);
        line[0].0.insert_str(0, "   ");
        line
    }

    /// What the top bar shows beside the turn number, in both
    /// presentations: the stockpile (`stockpile_line`), then the supply
    /// (`supply_line`).
    pub(super) fn status_line(&self) -> Line {
        let mut line = self.stockpile_line();
        line.extend(self.supply_line());
        line
    }

    /// `status_line` without the stockpile's change a turn, for an ImGui
    /// status bar too narrow for all of it.
    pub(super) fn brief_status_line(&self) -> Line {
        let mut line = stock_spans(self.stock(self.local_team), None);
        line.extend(self.supply_line());
        line
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

    /// The End Turn button, in both presentations (`top_bar`, and ImGui's
    /// status bar): it names whatever the turn still waits on (a click
    /// selects that), and is gold once nothing does. While a network game
    /// waits for the others' plans, a click takes this side's back.
    pub(super) fn end_turn_button(&self) -> ButtonSpec {
        let pending = self.pending();
        let label = if self.is_resolving() {
            self.resolving_label()
        } else {
            end_turn_label(pending, self.idle_queues().len())
        };
        let waiting = self.waiting_for_peers();
        let hint = if waiting {
            text!("end_turn_take_back").into()
        } else {
            Command::HoldOrEndTurn.key()
        };
        ButtonSpec::new(Target::EndTurn, label, hint)
            .hover_text(hover_text!("end_turn_button"))
            .queued(!waiting && pending == (0, 0))
            .unavailable(
                self.is_playing_out()
                    .then(|| text!("end_turn_playing_out").into()),
            )
    }

    /// The Menu button, which opens the settings menu, in both presentations.
    pub(super) fn menu_button(&self) -> ButtonSpec {
        ButtonSpec::new(Target::OpenSettings, text!("menu_button"), "")
            .hover_text(hover_text!("menu_button"))
    }

    /// Turn number, the stockpile and supply on the left, the latest notice in the
    /// middle, and on the right the End Turn button, which names whatever the turn is still
    /// waiting on (clicking it selects that).
    pub(super) fn top_bar(&self, size: Vec2, layout: &mut Layout) {
        let min = Vec2::new(0.0, size.y - TOP_BAR_HEIGHT);
        layout.panel(min, size, false);
        let middle = min.y + TOP_BAR_HEIGHT / 2.0;

        let turn = self.shown_turn();
        let turn_text = text!("status_turn", turn = turn);
        let turn_end = MARGIN + font::ui(TITLE).width(&turn_text);
        push_text_row(
            layout,
            Vec2::new(MARGIN, middle),
            TITLE,
            vec![(turn_text, self.turn_number_color(TEXT))],
        );
        // The player's stockpile and supply beside the turn number.
        let stockpile = self.status_line();
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

        let end_turn = self.end_turn_button();
        let width = single_line_button_width(&end_turn.label, &end_turn.hint);
        let button_min = Vec2::new(size.x - MARGIN - width, middle - END_TURN_HEIGHT / 2.0);
        let end_turn = end_turn.place(
            button_min.round(),
            (button_min + Vec2::new(width, END_TURN_HEIGHT)).round(),
            false,
        );

        let menu = self.menu_button();
        let menu_width = single_line_button_width(&menu.label, "");
        let menu_min = Vec2::new(left_end + GAP * 2.0, middle - END_TURN_HEIGHT / 2.0);
        layout.buttons.push(menu.place(
            menu_min.round(),
            (menu_min + Vec2::new(menu_width, END_TURN_HEIGHT)).round(),
            false,
        ));
        let left_end = menu_min.x + menu_width;

        // The notice sits centered in the space left between the two,
        // shortened to what fits.
        let face = font::ui(BODY);
        let space = (left_end + 2.0 * GAP, end_turn.min.x - 2.0 * GAP);
        let notice = fit_text(self.shown_notice(), space.1 - space.0, |t| face.width(t));
        if !notice.is_empty() {
            let width = face.width(&notice);
            let left = ((space.0 + space.1 - width) / 2.0).max(space.0);
            let line = vec![(notice, NOTICE_TEXT)];
            push_text_row(layout, Vec2::new(left, middle), BODY, line);
        }
        layout.buttons.push(end_turn);
    }

    /// A queue's line in the hover panel: the item worked (`worked_item`)
    /// and its turns left, or why there's none.
    fn hover_queue_text(&self, worked: Option<(&str, i32, f32)>, empty: bool) -> String {
        match worked {
            Some((name, turns, _)) => format!("{name} · {} LEFT", turns_icon(turns)),
            None if empty => "EMPTY".into(),
            None => "NOTHING IT CAN PAY FOR".into(),
        }
    }

    /// A hovered city or Barracks: population and what the city delivers,
    /// or the Barracks' health, and what either is building. Of another
    /// side's, only what's in sight: its population or health, not what it
    /// delivers or works, which hang on tiles, routes and plans the player
    /// can't see.
    pub(super) fn structure_hover_panel(&self, i: usize, barracks: bool, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        let own = city.team == self.local_team;
        if barracks {
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
            if !own {
                return;
            }
            let status = self.queue_status(i, Lane::Barracks);
            let worked = self.worked_item(i, Lane::Barracks, &status);
            let queue = self.hover_queue_text(worked, city.barracks_queue.is_empty());
            panel.text(SMALL, vec![(format!("QUEUE: {queue}"), DIM_TEXT)]);
            if let Some((_, _, done)) = worked {
                panel.bar(done);
            }
            if let Some(wait) = &status.head {
                let (size, line) = waiting_line(wait);
                panel.text(size, line);
            }
        } else {
            panel.text(
                TITLE,
                vec![(format!("CITY {}", city.id + 1), city.team.color())],
            );
            if !own {
                panel.text(
                    SMALL,
                    vec![(
                        format!("POP {}/{MAX_CITY_POPULATION}", city.population),
                        GOLD_TEXT,
                    )],
                );
                return;
            }
            let status = self.queue_status(i, Lane::City);
            let worked = self.worked_item(i, Lane::City, &status);
            let queue = match worked {
                // Everything queued waits: it gathers by itself.
                None if status.gathers && !city.queue.is_empty() => {
                    format!("GATHERING {}", stock_icons(GATHER_YIELD))
                }
                _ => self.hover_queue_text(worked, city.queue.is_empty()),
            };
            panel.text(
                SMALL,
                vec![(
                    format!(
                        "POP {}/{MAX_CITY_POPULATION} · DELIVERS {}",
                        city.population,
                        price_hint(self.net_delivery(i))
                    ),
                    GOLD_TEXT,
                )],
            );
            panel.text(SMALL, vec![(format!("QUEUE: {queue}"), DIM_TEXT)]);
            if let Some((_, _, done)) = worked {
                panel.bar(done);
            }
            if let Some(wait) = &status.head {
                let (size, line) = waiting_line(wait);
                panel.text(size, line);
            }
        }
    }
}
