//! The network link of a multiplayer game (`docs/multiplayer.md`): the
//! lockstep messages of `game::multiplayer`, encoded with postcard and sent
//! over TCP through an encrypted, authenticated channel (`secure.rs`) keyed
//! by the host's join code. A reader thread per connection opens incoming
//! messages onto a bounded channel, so the frame loop only polls
//! (`Session::pump`) and never blocks on the network.
//!
//! Everything that arrives is untrusted. Before the key exchange a
//! connection can only fail it; after, a message that won't open (tampered,
//! replayed, out of order, or sealed under a wrong code), is too big, or
//! doesn't decode drops the peer, and the game checks every message before
//! using it (`GameState::receive`), dropping a peer that sends a malformed
//! or hostile one. A host takes a guest for each open seat until the game
//! starts; a guest who leaves after that hands their side to the AI. It
//! runs up to
//! `MAX_HANDSHAKES` joins at once (`MAX_HANDSHAKES_PER_ADDRESS` from any one
//! address), each on its own thread with `JOIN_TIMEOUT` to finish, so a
//! silent connection holds up nobody; after `MAX_REFUSALS` joins with a
//! wrong code it stops listening, so the code can't be guessed at.

mod secure;

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};

use crate::game::{GameState, HOST_SEAT, NetMessage, PROTOCOL_VERSION, Settings, Team};
use secure::{Opener, Sealer, Side};

/// The port `--host` listens on and `--join` connects to when none is given.
pub use crate::game::DEFAULT_PORT;
/// How long a join has, on either end, to connect, run the handshake and
/// trade the hello and welcome.
const JOIN_TIMEOUT: Duration = Duration::from_secs(10);
/// How many joins a host runs at once, and from any one address; more are
/// turned away.
const MAX_HANDSHAKES: usize = 8;
const MAX_HANDSHAKES_PER_ADDRESS: usize = 3;
/// How many joins with a wrong code a host takes before it stops
/// listening.
const MAX_REFUSALS: u32 = 10;
/// Opened messages waiting for the frame loop; the reader waits past this.
const INBOX: usize = 64;

/// A message in postcard's encoding.
fn encode(message: &NetMessage) -> io::Result<Vec<u8>> {
    postcard::to_allocvec(message).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn decode(bytes: &[u8]) -> io::Result<NetMessage> {
    postcard::from_bytes(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Seals `message` and writes it as a frame.
fn send_sealed(
    stream: &mut TcpStream,
    sealer: &mut Sealer,
    message: &NetMessage,
) -> io::Result<()> {
    let sealed = sealer.seal(&encode(message)?)?;
    secure::write_frame(stream, &sealed)
}

/// Reads a frame and opens it as the other end's next message.
fn receive_sealed(stream: &mut TcpStream, opener: &mut Opener) -> io::Result<NetMessage> {
    let sealed = secure::read_frame(stream)?;
    decode(&opener.open(&sealed)?)
}

/// An established, encrypted connection to the other machine.
struct Link {
    writer: TcpStream,
    sealer: Sealer,
    incoming: Receiver<io::Result<NetMessage>>,
    peer: SocketAddr,
}

impl Link {
    /// Starts reading `stream`'s messages, opened with `opener`, on a thread.
    fn new(stream: TcpStream, sealer: Sealer, mut opener: Opener) -> io::Result<Self> {
        stream.set_read_timeout(None)?;
        stream.set_write_timeout(Some(JOIN_TIMEOUT))?;
        let peer = stream.peer_addr()?;
        let mut reader = stream.try_clone()?;
        let (send, incoming) = mpsc::sync_channel(INBOX);
        thread::spawn(move || {
            loop {
                let message = receive_sealed(&mut reader, &mut opener);
                let failed = message.is_err();
                if send.send(message).is_err() || failed {
                    return;
                }
            }
        });
        Ok(Self {
            writer: stream,
            sealer,
            incoming,
            peer,
        })
    }

    fn send(&mut self, message: &NetMessage) -> io::Result<()> {
        send_sealed(&mut self.writer, &mut self.sealer, message)
    }

    /// The next message that has arrived, if one has, or why the
    /// connection is gone.
    fn next(&mut self) -> Result<Option<NetMessage>> {
        match self.incoming.try_recv() {
            Ok(Ok(message)) => Ok(Some(message)),
            Ok(Err(error)) => Err(anyhow!(error)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => bail!("the connection closed"),
        }
    }
}

impl Drop for Link {
    /// Closes the connection for both ends: the reader thread's copy of the
    /// stream would otherwise keep it open.
    fn drop(&mut self) {
        let _ = self.writer.shutdown(Shutdown::Both);
    }
}

/// How a join the host ran on its own thread ended.
enum Join {
    /// The guest's hello opened: it knows the code.
    Ready(Link, NetMessage),
    /// Its hello didn't open under the code: a wrong code (or tampering).
    WrongCode(SocketAddr),
    /// Anything else: not this game, another version, a timeout, garbage.
    Failed(SocketAddr, String),
}

/// A host's side of a join, on its own thread: the handshake, then the
/// guest's hello, which only opens if it used the right code.
fn host_join(mut stream: TcpStream, addr: SocketAddr, code: &str) -> Join {
    let result = (|| -> io::Result<Join> {
        stream.set_nonblocking(false)?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(JOIN_TIMEOUT))?;
        stream.set_write_timeout(Some(JOIN_TIMEOUT))?;
        let (sealer, mut opener) =
            secure::handshake(&mut stream, Side::Host, code, PROTOCOL_VERSION)?;
        let sealed = secure::read_frame(&mut stream)?;
        let Ok(hello) = opener.open(&sealed) else {
            return Ok(Join::WrongCode(addr));
        };
        let hello = decode(&hello)?;
        Ok(Join::Ready(Link::new(stream, sealer, opener)?, hello))
    })();
    result.unwrap_or_else(|error| Join::Failed(addr, error.to_string()))
}

/// This machine's side of a multiplayer game.
pub struct Session {
    /// Whether this machine hosts.
    hosting: bool,
    /// The host's listener, until the game starts.
    listener: Option<TcpListener>,
    /// Host: the join code guests must give (the game shows it).
    code: String,
    /// Host: joins running on their own threads report here.
    joins: (Sender<Join>, Receiver<Join>),
    /// Host: how many joins are running, by the address they come from.
    joining: Arc<Mutex<HashMap<IpAddr, usize>>>,
    /// Host: joins refused for a wrong code (`MAX_REFUSALS`).
    refusals: u32,
    /// The connections: on the host, each guest's, by their seat; on a
    /// guest, the host's.
    links: Vec<(Team, Link)>,
}

/// Where a host listens: every interface, so players on other machines can
/// reach it. Tests listen on loopback only: each build of the tests is a new
/// executable, and Windows Firewall asks about every new one that listens on
/// the network, but never about loopback.
const LISTEN_ADDRESS: &str = if cfg!(test) { "127.0.0.1" } else { "0.0.0.0" };

impl Session {
    fn new(listener: Option<TcpListener>, code: String, links: Vec<(Team, Link)>) -> Self {
        Session {
            hosting: listener.is_some(),
            listener,
            code,
            joins: mpsc::channel(),
            joining: Arc::new(Mutex::new(HashMap::new())),
            refusals: 0,
            links,
        }
    }

    /// `--host`: listens on `port` and starts a new world for `players`
    /// people (the AI playing the rest, as `settings` ask).
    pub fn host(port: u16, players: usize, settings: &Settings) -> Result<(Session, GameState)> {
        Self::host_with(port, || GameState::host_game(players, settings))
    }

    /// `host`, with the game `host_game` makes once the port is open.
    fn host_with(port: u16, host_game: impl FnOnce() -> GameState) -> Result<(Session, GameState)> {
        let listener = TcpListener::bind((LISTEN_ADDRESS, port))
            .with_context(|| format!("can't listen on port {port}"))?;
        listener.set_nonblocking(true)?;
        let game = host_game();
        let code = game.join_code().unwrap_or_default().to_string();
        log::info!(
            "hosting on port {port} for {} more players; they join with code {code}",
            game.open_seats().len()
        );
        Ok((Session::new(Some(listener), code, Vec::new()), game))
    }

    /// The port a host listens on, until the game starts.
    pub fn port(&self) -> Option<u16> {
        let listener = self.listener.as_ref()?;
        listener.local_addr().ok().map(|address| address.port())
    }

    /// `--join`: connects to the host at `address` (`HOST:PORT`, or just
    /// `HOST` for `DEFAULT_PORT`) with the join code it shows, and builds
    /// the game it sends.
    pub fn join(address: &str, code: &str) -> Result<(Session, GameState)> {
        let target = if address.contains(':') {
            address.to_string()
        } else {
            format!("{address}:{DEFAULT_PORT}")
        };
        let addr = target
            .to_socket_addrs()
            .with_context(|| format!("can't find {target}"))?
            .next()
            .with_context(|| format!("{target} has no address"))?;
        let mut stream = TcpStream::connect_timeout(&addr, JOIN_TIMEOUT)
            .with_context(|| format!("can't reach a host at {addr}"))?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(JOIN_TIMEOUT))?;
        stream.set_write_timeout(Some(JOIN_TIMEOUT))?;
        let (mut sealer, mut opener) =
            secure::handshake(&mut stream, Side::Guest, code, PROTOCOL_VERSION)
                .context("couldn't set up a secure connection")?;
        send_sealed(
            &mut stream,
            &mut sealer,
            &NetMessage::Hello {
                version: PROTOCOL_VERSION,
            },
        )?;
        // The host closes on a wrong code; its welcome only opens under the
        // right one.
        let welcome = receive_sealed(&mut stream, &mut opener)
            .map_err(|_| anyhow!("WRONG JOIN CODE (or the host refused the connection)"))?;
        let game = GameState::join_game(&welcome).map_err(|reason| anyhow!("{reason}"))?;
        log::info!("joined the game at {addr}");
        let link = Link::new(stream, sealer, opener)?;
        Ok((
            Session::new(None, String::new(), vec![(HOST_SEAT, link)]),
            game,
        ))
    }

    /// Once a frame: starts joins (host), seats the guests whose hellos
    /// open, hands arriving messages to the game, and sends the game's
    /// outgoing ones (the host's to every guest).
    pub fn pump(&mut self, game: &mut GameState) {
        // Nobody joins a game under way: a guest couldn't rebuild it.
        if self.hosting && game.has_started() && self.listener.take().is_some() {
            log::info!("the game has started: no longer listening");
        }
        self.accept();
        self.finish_joins(game);
        let mut dropped = Vec::new();
        for (team, link) in &mut self.links {
            // A host takes nothing while a turn plays out: it waits in the
            // queue until the next turn's planning (`takes_messages`).
            while game.takes_messages() {
                let message = match link.next() {
                    Ok(Some(message)) => message,
                    Ok(None) => break,
                    Err(error) => {
                        dropped.push((*team, format!("{error:#}")));
                        break;
                    }
                };
                if let Err(why) = game.receive(*team, message) {
                    // Tell them why, so they see more than a closed
                    // connection.
                    if self.hosting {
                        let _ = link.send(&NetMessage::Refused(why.clone()));
                    }
                    dropped.push((*team, format!("{} sent a bad message: {why}", link.peer)));
                    break;
                }
            }
        }
        for (team, why) in dropped {
            self.drop_link(game, team, &why);
        }
        for message in game.take_outbox() {
            let mut failed = Vec::new();
            for (team, link) in &mut self.links {
                if let Err(error) = link.send(&message) {
                    failed.push((*team, format!("{error:#}")));
                }
            }
            for (team, why) in failed {
                self.drop_link(game, team, &why);
            }
        }
    }

    /// Host: starts a join on its own thread for each new connection, up
    /// to `MAX_HANDSHAKES` at once.
    fn accept(&mut self) {
        let Some(listener) = &self.listener else {
            return;
        };
        loop {
            match listener.accept() {
                Ok((stream, addr)) => {
                    {
                        let mut joining = self.joining.lock().expect("never poisoned");
                        let total: usize = joining.values().sum();
                        let here = joining.entry(addr.ip()).or_default();
                        if total >= MAX_HANDSHAKES || *here >= MAX_HANDSHAKES_PER_ADDRESS {
                            log::warn!("too many joins at once: turning {addr} away");
                            let _ = stream.shutdown(Shutdown::Both);
                            continue;
                        }
                        *here += 1;
                    }
                    let (report, joining) = (self.joins.0.clone(), self.joining.clone());
                    let code = self.code.clone();
                    thread::spawn(move || {
                        let join = host_join(stream, addr, &code);
                        let mut joining = joining.lock().expect("never poisoned");
                        if let Some(count) = joining.get_mut(&addr.ip()) {
                            *count -= 1;
                            if *count == 0 {
                                joining.remove(&addr.ip());
                            }
                        }
                        drop(joining);
                        let _ = report.send(join);
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return,
                Err(error) => {
                    log::warn!("accepting a player failed: {error}");
                    return;
                }
            }
        }
    }

    /// Host: seats each guest whose hello the game welcomes, in the next
    /// open seat; turns the rest away.
    fn finish_joins(&mut self, game: &mut GameState) {
        while let Ok(join) = self.joins.1.try_recv() {
            match join {
                Join::Ready(mut link, hello) => {
                    let (seat, reply) = game.welcome(&hello);
                    if let Err(error) = link.send(&reply) {
                        log::warn!("welcoming {} failed: {error}", link.peer);
                        if let Some(seat) = seat {
                            game.seat_left(seat);
                        }
                        continue;
                    }
                    match seat {
                        Some(seat) => {
                            log::info!("{} joined as {seat:?}", link.peer);
                            self.links.push((seat, link));
                        }
                        None => log::warn!("refused {}: {reply:?}", link.peer),
                    }
                }
                Join::WrongCode(addr) => {
                    self.refusals += 1;
                    log::warn!("{addr} gave a wrong join code ({})", self.refusals);
                    if self.refusals >= MAX_REFUSALS && self.listener.take().is_some() {
                        log::warn!("{MAX_REFUSALS} wrong join codes: no longer listening");
                        game.stop_listening();
                    }
                }
                Join::Failed(addr, why) => log::warn!("{addr} couldn't join: {why}"),
            }
        }
    }

    /// Drops the connection to `team`'s player: on the host, their side
    /// goes to the AI (or, before the game starts, their seat opens again);
    /// on a guest, the host is gone and the game with it.
    fn drop_link(&mut self, game: &mut GameState, team: Team, why: &str) {
        let before = self.links.len();
        self.links.retain(|(t, _)| *t != team);
        if self.links.len() == before {
            return;
        }
        log::warn!("{team:?}'s player is gone: {why}");
        if self.hosting {
            game.seat_left(team);
        } else {
            game.peer_lost();
        }
    }
}

/// This machine's address on its local network, for a host to show the
/// players it waits for: the address of the interface a packet to the
/// internet would leave by. Connecting a UDP socket only picks that route;
/// it sends nothing, so nobody outside hears of it. `None` offline.
pub fn lan_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    // TEST-NET-1 (RFC 5737), which nothing answers; nothing is sent to it.
    socket.connect(("192.0.2.1", 9)).ok()?;
    shown_lan_address(socket.local_addr().ok()?.ip())
}

/// `ip`, if it's an address another machine could reach this one at.
fn shown_lan_address(ip: IpAddr) -> Option<IpAddr> {
    (!ip.is_unspecified() && !ip.is_loopback() && !ip.is_multicast()).then_some(ip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::BuildUnit;
    use std::io::{Read, Write};
    use std::time::Instant;

    /// Joins `host` with `code` on a thread, pumping the host meanwhile.
    fn join(net: &mut Session, host: &mut GameState, code: &str) -> Result<(Session, GameState)> {
        let address = format!("127.0.0.1:{}", net.port().unwrap());
        let code = code.to_string();
        let joining = thread::spawn(move || Session::join(&address, &code));
        let start = Instant::now();
        while !joining.is_finished() {
            assert!(start.elapsed() < Duration::from_secs(20), "join timed out");
            net.pump(host);
            thread::sleep(Duration::from_millis(1));
        }
        joining.join().unwrap()
    }

    #[test]
    fn a_lan_address_is_one_another_machine_can_reach() {
        let lan: IpAddr = "192.168.1.20".parse().unwrap();
        assert_eq!(shown_lan_address(lan), Some(lan));
        for ip in ["0.0.0.0", "127.0.0.1", "::1", "224.0.0.1"] {
            assert_eq!(shown_lan_address(ip.parse().unwrap()), None, "{ip}");
        }
    }

    /// A small world's settings: one AI side, cities to start.
    fn small() -> Settings {
        Settings {
            world_ai: 1,
            ..Settings::default()
        }
    }

    /// A host listening on a free port, on the tests' fixed world
    /// (`host_test_game`).
    fn host(players: usize, settings: &Settings) -> (Session, GameState) {
        Session::host_with(0, || GameState::host_test_game(players, settings)).expect("listens")
    }

    /// Pumps every game's session and plays out its turn until `done`, or
    /// panics after a while.
    fn pump_all_until(
        games: &mut [(&mut Session, &mut GameState)],
        done: impl Fn(&[&GameState]) -> bool,
    ) {
        let start = Instant::now();
        loop {
            let states: Vec<&GameState> = games.iter().map(|(_, g)| &**g).collect();
            if done(&states) {
                return;
            }
            assert!(start.elapsed() < Duration::from_secs(30), "timed out");
            for (net, game) in games.iter_mut() {
                net.pump(game);
                game.update(1.0);
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    /// Pumps the host until `done`, or panics after a few seconds.
    fn pump_host_until(net: &mut Session, game: &mut GameState, done: impl Fn(&Session) -> bool) {
        let start = Instant::now();
        while !done(net) {
            assert!(start.elapsed() < Duration::from_secs(20), "timed out");
            net.pump(game);
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn a_turn_plays_out_over_an_encrypted_connection() {
        let (mut host_net, mut host) = host(2, &small());
        let code = host.join_code().unwrap().to_string();
        let (mut guest_net, mut guest) =
            join(&mut host_net, &mut host, &code.to_lowercase()).expect("joins, in any case");
        assert_eq!(host_net.links.len(), 1);
        assert_eq!(host.checksum(), guest.checksum());
        // A second guest finds the game full.
        let error = join(&mut host_net, &mut host, &code).err().expect("full");
        assert!(format!("{error:#}").contains("FULL"), "{error:#}");

        // Each city has something to build, so End Turn sends the plan.
        for game in [&mut host, &mut guest] {
            game.select_city();
            game.queue_selected_city_gather();
            game.end_planning();
            assert!(game.is_resolving(), "{}", game.notice());
        }
        let start = Instant::now();
        let resolved = |g: &GameState| g.turn() == 1 && !g.is_resolving();
        while !resolved(&host) || !resolved(&guest) {
            assert!(start.elapsed() < Duration::from_secs(20), "timed out");
            host_net.pump(&mut host);
            guest_net.pump(&mut guest);
            host.update(1.0);
            guest.update(1.0);
            thread::sleep(Duration::from_millis(1));
        }
        // The guest's checksum reaches the host, and matches.
        for _ in 0..50 {
            host_net.pump(&mut host);
            guest_net.pump(&mut guest);
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(host.checksum(), guest.checksum());
        assert!(!host.notice().contains("DESYNC"), "{}", host.notice());
    }

    #[test]
    fn a_wrong_code_is_turned_away_and_counted() {
        let (mut host_net, mut host) = host(2, &small());
        let error = join(&mut host_net, &mut host, "WRONG1")
            .err()
            .expect("refused");
        assert!(
            format!("{error:#}").contains("WRONG JOIN CODE"),
            "{error:#}"
        );
        pump_host_until(&mut host_net, &mut host, |n| n.refusals == 1);
        assert!(host_net.listener.is_some(), "still listening");
        assert!(host_net.links.is_empty());
        // The right code still gets in.
        let code = host.join_code().unwrap().to_string();
        join(&mut host_net, &mut host, &code).expect("joins");
    }

    #[test]
    fn too_many_wrong_codes_stop_the_host_listening() {
        let (mut host_net, mut host) = host(2, &small());
        for _ in 0..MAX_REFUSALS {
            assert!(join(&mut host_net, &mut host, "NOPE00").is_err());
        }
        pump_host_until(&mut host_net, &mut host, |n| n.listener.is_none());
        assert_eq!(host.notice(), "TOO MANY FAILED JOINS - NO LONGER LISTENING");
    }

    #[test]
    fn garbage_and_silence_hold_up_no_real_guest() {
        let (mut host_net, mut host) = host(2, &small());
        let port = host_net.port().unwrap();
        let before = host.checksum();
        // A silent connection and one that sends garbage (from the same
        // address as the guest, within its limit): neither blocks the real
        // guest, or harms the game.
        let _silent = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let mut garbage = TcpStream::connect(("127.0.0.1", port)).unwrap();
        garbage.write_all(b"GET / HTTP/1.1\r\n\r\n").unwrap();
        let code = host.join_code().unwrap().to_string();
        let (_guest_net, guest) = join(&mut host_net, &mut host, &code).expect("joins");
        assert_eq!(host.checksum(), before);
        assert_eq!(guest.checksum(), before);
        assert_eq!(host_net.refusals, 0, "garbage isn't a wrong code");
    }

    #[test]
    fn one_address_can_hold_only_a_few_joins_at_once() {
        let (mut host_net, mut host) = host(2, &small());
        let port = host_net.port().unwrap();
        let mut silent: Vec<TcpStream> = (0..MAX_HANDSHAKES_PER_ADDRESS + 2)
            .map(|_| TcpStream::connect(("127.0.0.1", port)).unwrap())
            .collect();
        for stream in &silent {
            stream.set_nonblocking(true).unwrap();
        }
        // How the host answered each connection: a join it took starts the
        // handshake (the host speaks first), one it turned away is closed.
        // Neither depends on how long this takes: a join the host took
        // stays counted only until `JOIN_TIMEOUT`, which a loaded machine
        // could outlast, but the handshake it started stays in the stream.
        let mut answers: Vec<Option<bool>> = vec![None; silent.len()];
        let start = Instant::now();
        while answers.contains(&None) {
            assert!(start.elapsed() < Duration::from_secs(20), "{answers:?}");
            host_net.pump(&mut host);
            for (stream, answer) in silent.iter_mut().zip(&mut answers) {
                if answer.is_none() {
                    *answer = match stream.read(&mut [0; 8]) {
                        Ok(read) => Some(read > 0),
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => None,
                        // Reset, or aborted: closed.
                        Err(_) => Some(false),
                    };
                }
            }
            thread::sleep(Duration::from_millis(1));
        }
        let taken = answers.iter().filter(|&&a| a == Some(true)).count();
        assert_eq!(
            taken, MAX_HANDSHAKES_PER_ADDRESS,
            "the rest were turned away: {answers:?}"
        );
        let local = IpAddr::from([127, 0, 0, 1]);
        let joining = host_net.joining.lock().unwrap().get(&local).copied();
        assert!(joining <= Some(MAX_HANDSHAKES_PER_ADDRESS), "{joining:?}");
    }

    #[test]
    fn three_players_play_over_the_network_and_one_leaving_hands_over_to_the_ai() {
        let settings = Settings {
            world_ai: 3,
            ..Settings::default()
        };
        let (mut host_net, mut host) = host(3, &settings);
        let code = host.join_code().unwrap().to_string();
        let (mut red_net, mut red) = join(&mut host_net, &mut host, &code).expect("Red joins");
        let (mut green_net, mut green) =
            join(&mut host_net, &mut host, &code).expect("Green joins");
        assert_eq!(host.open_seats(), []);
        let end_turn = |game: &mut GameState| {
            game.select_city();
            game.queue_selected_city_gather();
            game.end_planning();
        };
        for game in [&mut host, &mut red, &mut green] {
            end_turn(game);
        }
        {
            let mut games = [
                (&mut host_net, &mut host),
                (&mut red_net, &mut red),
                (&mut green_net, &mut green),
            ];
            pump_all_until(&mut games, |g| {
                g.iter().all(|g| g.turn() == 1 && !g.is_resolving())
            });
        }
        assert_eq!(host.checksum(), red.checksum());
        assert_eq!(host.checksum(), green.checksum());
        host_net.pump(&mut host);
        assert!(host_net.listener.is_none(), "no joining a game under way");

        // Green's player goes: the host hands Green to the AI, tells Red,
        // and the next turn goes on without them.
        drop(green_net);
        let mut games = [(&mut host_net, &mut host)];
        pump_all_until(&mut games, |g| g[0].notice().contains("GREEN LEFT"));
        for game in [&mut host, &mut red] {
            end_turn(game);
        }
        let mut games = [(&mut host_net, &mut host), (&mut red_net, &mut red)];
        pump_all_until(&mut games, |g| {
            g.iter().all(|g| g.turn() == 2 && !g.is_resolving())
        });
        for _ in 0..50 {
            host_net.pump(&mut host);
            red_net.pump(&mut red);
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(host.checksum(), red.checksum());
        assert!(!host.notice().contains("DESYNC"), "{}", host.notice());
    }

    #[test]
    fn a_host_that_is_gone_says_so() {
        let (mut host_net, mut host) = host(2, &small());
        let code = host.join_code().unwrap().to_string();
        let (mut guest_net, mut guest) = join(&mut host_net, &mut host, &code).expect("joins");
        drop(host_net);
        let start = Instant::now();
        while !guest.notice().contains("LEFT") {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "{}",
                guest.notice()
            );
            guest_net.pump(&mut guest);
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            guest.notice(),
            "BLUE (THE HOST) LEFT - THE GAME CAN'T GO ON"
        );
    }

    /// Pumps `game`'s session a few times, without playing anything out,
    /// for what's on its way to arrive.
    fn settle(games: &mut [(&mut Session, &mut GameState)]) {
        for _ in 0..50 {
            for (net, game) in games.iter_mut() {
                net.pump(game);
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    /// Ends `game`'s turn with `build` queued in a city (a Gather if none).
    fn end_turn_building(game: &mut GameState, build: Option<BuildUnit>) {
        game.select_city();
        match build {
            Some(unit) => game.queue_selected_city_unit(unit),
            None => game.queue_selected_city_gather(),
        }
        game.end_planning();
        assert!(game.is_resolving(), "{}", game.notice());
    }

    #[test]
    fn a_guest_takes_back_its_turn_over_the_network_and_its_new_orders_play_out() {
        let settings = Settings {
            world_ai: 3,
            ..Settings::default()
        };
        let (mut host_net, mut host) = host(3, &settings);
        let code = host.join_code().unwrap().to_string();
        let (mut red_net, mut red) = join(&mut host_net, &mut host, &code).expect("Red joins");
        let (mut green_net, mut green) =
            join(&mut host_net, &mut host, &code).expect("Green joins");
        // Red ends its turn, and the host has its plan.
        end_turn_building(&mut red, None);
        let mut games = [(&mut host_net, &mut host), (&mut red_net, &mut red)];
        pump_all_until(&mut games, |g| g[0].has_plan_from(Team::Red));
        // Red takes it back: the host lets go of the plan.
        red.take_back_turn();
        assert!(!red.is_resolving());
        let mut games = [(&mut host_net, &mut host), (&mut red_net, &mut red)];
        pump_all_until(&mut games, |g| !g[0].has_plan_from(Team::Red));
        assert_eq!(host.notice(), "RED IS CHANGING THEIR ORDERS");
        // Red queues a Melee and ends its turn again; Green and the host end
        // theirs, and the turn plays out with Red's new orders everywhere.
        end_turn_building(&mut red, Some(BuildUnit::Melee));
        end_turn_building(&mut green, None);
        end_turn_building(&mut host, None);
        let mut games = [
            (&mut host_net, &mut host),
            (&mut red_net, &mut red),
            (&mut green_net, &mut green),
        ];
        pump_all_until(&mut games, |g| {
            g.iter().all(|g| g.turn() == 1 && !g.is_resolving())
        });
        settle(&mut games);
        for game in [&host, &red, &green] {
            assert_eq!(game.checksum(), host.checksum());
            assert_eq!(game.units_queued(Team::Red, BuildUnit::Melee), 1);
        }
        assert!(!host.notice().contains("DESYNC"), "{}", host.notice());
        assert_eq!(host_net.links.len(), 2, "nobody dropped");
    }

    #[test]
    fn a_guest_that_plays_the_turn_out_first_may_send_its_next_plan() {
        let (mut host_net, mut host) = host(2, &small());
        let code = host.join_code().unwrap().to_string();
        let (mut guest_net, mut guest) = join(&mut host_net, &mut host, &code).expect("joins");
        end_turn_building(&mut guest, None);
        end_turn_building(&mut host, None);
        // The guest plays the turn out at once; the host is slower, and
        // still playing it out when the guest's next plan arrives.
        let start = Instant::now();
        while guest.turn() != 1 || guest.is_resolving() {
            assert!(start.elapsed() < Duration::from_secs(20), "timed out");
            host_net.pump(&mut host);
            guest_net.pump(&mut guest);
            guest.update(10.0);
            thread::sleep(Duration::from_millis(1));
        }
        assert!(host.is_playing_out());
        end_turn_building(&mut guest, None);
        settle(&mut [(&mut host_net, &mut host), (&mut guest_net, &mut guest)]);
        assert_eq!(host_net.links.len(), 1, "{}", host.notice());
        // Once the host has played it out too, the plan is in, and the next
        // turn plays out the same on both.
        let mut games = [(&mut host_net, &mut host), (&mut guest_net, &mut guest)];
        pump_all_until(&mut games, |g| g[0].has_plan_from(Team::Red));
        end_turn_building(&mut host, None);
        let mut games = [(&mut host_net, &mut host), (&mut guest_net, &mut guest)];
        pump_all_until(&mut games, |g| {
            g.iter().all(|g| g.turn() == 2 && !g.is_resolving())
        });
        settle(&mut games);
        assert_eq!(host.checksum(), guest.checksum());
        assert!(!host.notice().contains("DESYNC"), "{}", host.notice());
    }
}
