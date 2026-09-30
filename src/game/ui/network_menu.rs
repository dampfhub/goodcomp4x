//! The settings menu's Multiplayer section: host a game or join one without
//! command-line flags (`docs/multiplayer.md`). Outside a network game it has
//! the players and port to host with, the address and join code to join
//! with, and a button for each; in one, who this machine plays and the join
//! code, and a button to leave; a host also sees its address on the local
//! network, and the code and address each have a COPY button.
//!
//! The typed fields are `Row::Field`s: ImGui draws each as a text box
//! (`render_field`, `imgui.rs`) whose edits come back as `set_net_field`;
//! classic draws it as a button that starts typing into it
//! (`Target::EditNetField`), after which `App` hands keys to
//! `type_net_text` until Enter, Tab or Escape, and Ctrl+V, Ctrl+C and
//! Ctrl+X to `paste_net_text` and `copy_net_field`. The game only asks:
//! Host, Join, Leave and COPY leave a `NetRequest` for `App` to carry out
//! with `src/net` or the clipboard, and `App` reports back through
//! `set_net_status` and `set_host_address`.

use std::net::{IpAddr, SocketAddr};

use super::builder::{ButtonSpec, PanelBuilder, Row};
use super::text::wrap;
use super::{BODY, GAP, GOLD_TEXT, LABEL_TEXT, SMALL, TEXT, TITLE, Target};
use crate::game::GameState;
use crate::game::strings::{hover_text, text};

/// The characters a line of the status takes before it wraps.
const STATUS_WRAP: usize = 44;
use crate::game::multiplayer::{DEFAULT_PORT, MAX_PLAYERS};

/// A typed field in the Multiplayer section.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NetField {
    /// The port to host on.
    Port,
    /// The host to join: `HOST` or `HOST:PORT`.
    Address,
    /// The join code the host shows.
    Code,
}

impl NetField {
    pub(super) fn name(self) -> &'static str {
        match self {
            NetField::Port => text!("net_field_port"),
            NetField::Address => text!("net_field_address"),
            NetField::Code => text!("net_field_code"),
        }
    }

    /// The longest text it takes: past any real port, address or code.
    fn max_len(self) -> usize {
        match self {
            NetField::Port => 5,
            NetField::Address => 64,
            NetField::Code => 12,
        }
    }

    /// `text` as the field keeps it: characters it can hold, up to its
    /// length. A join code is upper case; a port is digits. Whitespace goes,
    /// so a pasted code or address loses the spaces and line end around it.
    pub(super) fn clean(self, text: &str) -> String {
        text.chars()
            .filter(|c| match self {
                NetField::Port => c.is_ascii_digit(),
                NetField::Address => c.is_ascii_graphic(),
                NetField::Code => c.is_ascii_alphanumeric(),
            })
            .map(|c| match self {
                NetField::Code => c.to_ascii_uppercase(),
                _ => c,
            })
            .take(self.max_len())
            .collect()
    }
}

/// What the Multiplayer section asks `App` to do.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NetRequest {
    /// Host a new world for `players` people on `port`.
    Host { port: u16, players: usize },
    /// Join the game at `address` with `code`.
    Join { address: String, code: String },
    /// Leave the network game for a new game of one's own.
    Leave,
    /// Put this text on the clipboard (a COPY button).
    Copy(String),
}

/// The Multiplayer section's state: what's typed, and what's asked. It's
/// this machine's, not the game's, and survives new games like the menu.
#[derive(Clone, Debug)]
pub struct NetMenu {
    pub(super) port: String,
    pub(super) address: String,
    pub(super) code: String,
    pub(super) players: usize,
    /// The menu shows the Multiplayer page, not the settings.
    pub(super) open: bool,
    /// Classic: the field keys go into.
    pub(super) editing: Option<NetField>,
    /// The last thing to report: an error, or joining under way.
    pub(super) status: String,
    /// A join is under way: nothing more until it ends.
    pub(super) busy: bool,
    /// Hosting: the port this machine listens on, and its address on the
    /// local network if `App` found one.
    pub(super) host: Option<(u16, Option<IpAddr>)>,
    request: Option<NetRequest>,
}

impl Default for NetMenu {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT.to_string(),
            address: String::new(),
            code: String::new(),
            players: 2,
            open: false,
            editing: None,
            status: String::new(),
            busy: false,
            host: None,
            request: None,
        }
    }
}

impl NetMenu {
    /// What was typed last time (`App` keeps it between sessions): `port`,
    /// `address` and `players` lines.
    pub fn from_text(text: &str) -> Self {
        let mut menu = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once(' ') else {
                continue;
            };
            match key {
                "port" => menu.port = NetField::Port.clean(value),
                "address" => menu.address = NetField::Address.clean(value),
                "players" => {
                    if let Ok(players) = value.trim().parse::<usize>() {
                        menu.players = players.clamp(2, MAX_PLAYERS);
                    }
                }
                _ => {}
            }
        }
        menu
    }

    /// What's worth keeping for next time: not the code, which is one
    /// game's.
    pub fn to_text(&self) -> String {
        format!(
            "port {}\naddress {}\nplayers {}\n",
            self.port, self.address, self.players
        )
    }

    fn field(&self, field: NetField) -> &str {
        match field {
            NetField::Port => &self.port,
            NetField::Address => &self.address,
            NetField::Code => &self.code,
        }
    }

    fn field_mut(&mut self, field: NetField) -> &mut String {
        match field {
            NetField::Port => &mut self.port,
            NetField::Address => &mut self.address,
            NetField::Code => &mut self.code,
        }
    }

    /// Hosting: the address players on the local network join at
    /// (`HOST:PORT`), if one was found.
    fn host_address(&self) -> Option<String> {
        let (port, ip) = self.host?;
        Some(SocketAddr::new(ip?, port).to_string())
    }
}

impl GameState {
    /// The settings menu's Multiplayer page, while it's open: in place of
    /// the settings, so the menu stays the height of one of them.
    pub(super) fn network_panel_content(&self) -> PanelBuilder {
        let mut panel = PanelBuilder::default();
        panel.text(TITLE, vec![(text!("net_title").into(), TEXT)]);
        panel.gap(GAP);
        self.network_rows(&mut panel);
        panel.gap(GAP);
        panel.compact_buttons(vec![
            ButtonSpec::new(Target::CloseMultiplayer, text!("net_back_button"), "")
                .hover_text(hover_text!("net_back_button")),
            self.close_settings_button(),
        ]);
        panel
    }

    /// The Multiplayer page's rows: hosting and joining, or the game under
    /// way.
    fn network_rows(&self, panel: &mut PanelBuilder) {
        let menu = &self.net_menu;
        let button = |target, label: &str, hover: Option<&str>, unavailable: Option<String>| {
            ButtonSpec::new(target, label, "")
                .hover_text(hover)
                .unavailable(unavailable)
        };
        let busy = || menu.busy.then(|| text!("net_busy").to_string());
        let field = |panel: &mut PanelBuilder, field: NetField| {
            panel.rows.push(Row::Field(
                field,
                menu.field(field).to_string(),
                menu.editing == Some(field),
            ));
        };
        // What `App` reported, a line at a time: an error can be long.
        let status = |panel: &mut PanelBuilder| {
            for line in wrap(&menu.status, STATUS_WRAP) {
                panel.text(BODY, vec![(line, LABEL_TEXT)]);
            }
        };
        if self.is_networked() {
            let side = format!("{:?}", self.local_team).to_uppercase();
            match self.join_code() {
                Some(code) => {
                    let hosting = format!("{} ", text!("net_hosting_as"));
                    panel.text(BODY, vec![(hosting, LABEL_TEXT), (side, TEXT)]);
                    self.host_rows(panel, code);
                }
                None => {
                    let playing = format!("{} ", text!("net_playing_as"));
                    panel.text(BODY, vec![(playing, LABEL_TEXT), (side, TEXT)]);
                }
            }
            let open = self.open_seats();
            if !open.is_empty() {
                let waiting: Vec<String> = open
                    .iter()
                    .map(|t| format!("{t:?}").to_uppercase())
                    .collect();
                panel.text(
                    BODY,
                    vec![(
                        text!("net_seats_open", seats = waiting.join(", ")),
                        LABEL_TEXT,
                    )],
                );
            }
            status(panel);
            panel.compact_buttons(vec![button(
                Target::LeaveGame,
                text!("net_leave_button"),
                hover_text!("net_leave_button"),
                None,
            )]);
            return;
        }
        panel.text(
            BODY,
            vec![
                (format!("{}  ", text!("net_players")), LABEL_TEXT),
                (menu.players.to_string(), GOLD_TEXT),
            ],
        );
        panel.compact_buttons(vec![
            button(
                Target::NetPlayers(menu.players.saturating_sub(1)),
                "<",
                None,
                (menu.players <= 2).then(|| text!("net_players_too_few").into()),
            ),
            button(
                Target::NetPlayers(menu.players + 1),
                ">",
                None,
                (menu.players >= MAX_PLAYERS)
                    .then(|| text!("net_players_too_many", players = MAX_PLAYERS)),
            ),
        ]);
        field(panel, NetField::Port);
        panel.compact_buttons(vec![button(
            Target::HostGame,
            text!("net_host_button"),
            hover_text!("net_host_button"),
            busy(),
        )]);
        panel.gap(GAP);
        field(panel, NetField::Address);
        field(panel, NetField::Code);
        panel.compact_buttons(vec![button(
            Target::JoinGame,
            text!("net_join_button"),
            hover_text!("net_join_button"),
            busy(),
        )]);
        status(panel);
    }

    /// Hosting: what the players need to join, each with a COPY button:
    /// the join `code`, and this machine's address on the local network;
    /// then what players over the internet need instead.
    fn host_rows(&self, panel: &mut PanelBuilder, code: &str) {
        let copy = |target| {
            ButtonSpec::new(target, text!("net_copy_button"), "")
                .hover_text(hover_text!("net_copy_button"))
        };
        panel.title_with_button(
            vec![
                (format!("{}  ", text!("net_host_code")), LABEL_TEXT),
                (code.into(), GOLD_TEXT),
            ],
            copy(Target::CopyJoinCode),
        );
        let Some((port, _)) = self.net_menu.host else {
            return;
        };
        // Apart, so the two COPY buttons don't touch.
        panel.gap(GAP / 2.0);
        match self.net_menu.host_address() {
            Some(address) => panel.title_with_button(
                vec![
                    (format!("{}  ", text!("net_host_address")), LABEL_TEXT),
                    (address, GOLD_TEXT),
                ],
                copy(Target::CopyHostAddress),
            ),
            None => panel.text(
                SMALL,
                vec![(text!("net_host_no_address", port = port), LABEL_TEXT)],
            ),
        }
        let hint = text!("net_host_internet", port = port);
        for line in wrap(&hint, STATUS_WRAP) {
            panel.text(SMALL, vec![(line, LABEL_TEXT)]);
        }
    }

    /// The Multiplayer section's buttons.
    pub(super) fn activate_network_target(&mut self, target: Target) {
        let code = self.join_code().map(str::to_string);
        let menu = &mut self.net_menu;
        match target {
            Target::OpenMultiplayer => menu.open = true,
            Target::CloseMultiplayer => menu.open = false,
            Target::NetPlayers(players) => menu.players = players.clamp(2, MAX_PLAYERS),
            Target::EditNetField(field) => {
                menu.editing = (menu.editing != Some(field)).then_some(field);
            }
            Target::HostGame if !menu.busy => match menu.port.parse::<u16>() {
                Ok(port) if port > 0 => {
                    menu.status.clear();
                    menu.request = Some(NetRequest::Host {
                        port,
                        players: menu.players,
                    });
                }
                _ => menu.status = text!("net_bad_port").into(),
            },
            Target::JoinGame if !menu.busy => {
                if menu.address.is_empty() {
                    menu.status = text!("net_no_address").into();
                } else if menu.code.is_empty() {
                    menu.status = text!("net_no_code").into();
                } else {
                    menu.request = Some(NetRequest::Join {
                        address: menu.address.clone(),
                        code: menu.code.clone(),
                    });
                }
            }
            Target::LeaveGame => {
                menu.status.clear();
                menu.host = None;
                menu.request = Some(NetRequest::Leave);
            }
            Target::CopyJoinCode => menu.request = code.map(NetRequest::Copy),
            Target::CopyHostAddress => menu.request = menu.host_address().map(NetRequest::Copy),
            _ => {}
        }
    }

    /// ImGui: `field`'s text box now holds `text`.
    pub(super) fn set_net_field(&mut self, field: NetField, text: &str) {
        *self.net_menu.field_mut(field) = field.clean(text);
    }

    /// Classic: the field being typed into, if one is.
    pub fn net_field_editing(&self) -> Option<NetField> {
        self.net_menu.editing.filter(|_| self.settings_open)
    }

    /// Classic: typed `text` goes into the field being typed into.
    pub fn type_net_text(&mut self, text: &str) {
        if let Some(field) = self.net_field_editing() {
            let joined = format!("{}{text}", self.net_menu.field(field));
            *self.net_menu.field_mut(field) = field.clean(&joined);
        }
    }

    /// Classic, Ctrl+V: `text` from the clipboard goes into the field being
    /// typed into, after what's there, trimmed and cleaned as the field
    /// keeps it (`NetField::clean`, which also cuts it to the field's
    /// length).
    pub fn paste_net_text(&mut self, text: &str) {
        self.type_net_text(text.trim());
    }

    /// Classic, Ctrl+C: the text of the field being typed into, for `App`
    /// to put on the clipboard; Ctrl+X (`cut`) empties the field too.
    pub fn copy_net_field(&mut self, cut: bool) -> Option<String> {
        let field = self.net_field_editing()?;
        let text = self.net_menu.field_mut(field);
        Some(if cut {
            std::mem::take(text)
        } else {
            text.clone()
        })
    }

    /// Hosting on `port`: this machine's address on the local network, if
    /// `App` found one (`net::lan_address`), for the page to show.
    pub fn set_host_address(&mut self, port: u16, lan: Option<IpAddr>) {
        self.net_menu.host = Some((port, lan));
    }

    /// Classic: Backspace in the field being typed into.
    pub fn net_field_backspace(&mut self) {
        if let Some(field) = self.net_field_editing() {
            self.net_menu.field_mut(field).pop();
        }
    }

    /// The menu opens on the settings next time.
    pub(in crate::game) fn close_multiplayer_page(&mut self) {
        self.net_menu.open = false;
    }

    /// Classic: Enter, Tab or Escape: typing stops.
    pub fn stop_typing(&mut self) {
        self.net_menu.editing = None;
    }

    /// What the Multiplayer section asked for, for `App` to do.
    pub fn take_net_request(&mut self) -> Option<NetRequest> {
        self.net_menu.request.take()
    }

    /// What `App` has to report: an error, or how joining goes. `busy`
    /// while a join is under way.
    pub fn set_net_status(&mut self, status: String, busy: bool) {
        self.net_menu.status = status;
        self.net_menu.busy = busy;
    }

    /// The Multiplayer section, to keep between sessions and new games.
    pub fn net_menu(&self) -> &NetMenu {
        &self.net_menu
    }

    /// Starts this new game with `old`'s menus as they were: the settings,
    /// whether the menu is open, and what's typed in its Multiplayer
    /// section. Being another game, it's `old`'s next `generation`.
    pub fn keep_menus_of(&mut self, old: &GameState) {
        self.generation = old.generation.wrapping_add(1);
        self.settings = old.settings.clone();
        self.settings_open = old.settings_open;
        self.show_yields = old.show_yields;
        self.net_menu = old.net_menu.clone();
        self.net_menu.editing = None;
        self.net_menu.request = None;
    }

    /// Sets what the Multiplayer section shows as typed (`App`, from the
    /// last session).
    pub fn set_net_menu(&mut self, menu: NetMenu) {
        self.net_menu = menu;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_typed_is_kept_for_next_time_but_not_the_code() {
        let menu = NetMenu {
            port: "55741".into(),
            address: "203.0.113.9:55741".into(),
            code: "K7M2QX".into(),
            players: 4,
            ..NetMenu::default()
        };
        let kept = NetMenu::from_text(&menu.to_text());
        assert_eq!(kept.port, "55741");
        assert_eq!(kept.address, "203.0.113.9:55741");
        assert_eq!(kept.players, 4);
        assert_eq!(kept.code, "");
        // A file edited by hand can't get past what the fields take.
        let odd = NetMenu::from_text("players 99\nport 12a34567\naddress a b\n");
        assert_eq!(odd.players, MAX_PLAYERS);
        assert_eq!(odd.port, "12345");
        assert_eq!(odd.address, "ab");
        assert_eq!(NetMenu::from_text("").port, DEFAULT_PORT.to_string());
    }
}
