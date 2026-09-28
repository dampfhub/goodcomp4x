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
//! or hostile one. A host takes one guest. It runs up to
//! `MAX_HANDSHAKES` joins at once (`MAX_HANDSHAKES_PER_ADDRESS` from any one
//! address), each on its own thread with `JOIN_TIMEOUT` to finish, so a
//! silent connection holds up nobody; after `MAX_REFUSALS` joins with a
//! wrong code it stops listening, so the code can't be guessed at.

mod secure;

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};

use crate::game::{GameState, NetMessage, PROTOCOL_VERSION};
use secure::{Opener, Sealer, Side};

/// The port `--host` listens on and `--join` connects to when none is given.
pub const DEFAULT_PORT: u16 = 7777;
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

    /// The messages that have arrived, or why the connection is gone.
    fn poll(&mut self) -> Result<Vec<NetMessage>> {
        let mut messages = Vec::new();
        loop {
            match self.incoming.try_recv() {
                Ok(Ok(message)) => messages.push(message),
                Ok(Err(error)) => return Err(anyhow!(error)),
                Err(TryRecvError::Empty) => return Ok(messages),
                Err(TryRecvError::Disconnected) => bail!("the connection closed"),
            }
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
    /// The host's listener, until a guest joins.
    listener: Option<TcpListener>,
    /// Host: the join code guests must give (the game shows it).
    code: String,
    /// Host: joins running on their own threads report here.
    joins: (Sender<Join>, Receiver<Join>),
    /// Host: how many joins are running, by the address they come from.
    joining: Arc<Mutex<HashMap<IpAddr, usize>>>,
    /// Host: joins refused for a wrong code (`MAX_REFUSALS`).
    refusals: u32,
    link: Option<Link>,
    /// Whether the connection was lost (reported once).
    lost: bool,
}

impl Session {
    fn new(listener: Option<TcpListener>, code: String, link: Option<Link>) -> Self {
        Session {
            listener,
            code,
            joins: mpsc::channel(),
            joining: Arc::new(Mutex::new(HashMap::new())),
            refusals: 0,
            link,
            lost: false,
        }
    }

    /// `--host`: listens on `port` for a guest, and starts the game.
    pub fn host(port: u16) -> Result<(Session, GameState)> {
        let listener = TcpListener::bind(("0.0.0.0", port))
            .with_context(|| format!("can't listen on port {port}"))?;
        listener.set_nonblocking(true)?;
        let game = GameState::host_game();
        let code = game.join_code().unwrap_or_default().to_string();
        log::info!("hosting on port {port}; waiting for a player to join with code {code}");
        Ok((Session::new(Some(listener), code, None), game))
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
        Ok((Session::new(None, String::new(), Some(link)), game))
    }

    /// Once a frame: starts joins (host), takes in the first guest whose
    /// hello opens, hands arriving messages to the game, and sends the
    /// game's outgoing ones.
    pub fn pump(&mut self, game: &mut GameState) {
        self.accept();
        self.finish_joins(game);
        let Some(link) = self.link.as_mut() else {
            return;
        };
        let incoming = match link.poll() {
            Ok(messages) => messages,
            Err(error) => {
                self.disconnect(game, &format!("{error:#}"));
                return;
            }
        };
        for message in incoming {
            if let Err(why) = game.receive(message) {
                let why = format!("{} sent a bad message: {why}", link.peer);
                self.disconnect(game, &why);
                return;
            }
        }
        for message in game.take_outbox() {
            let Some(link) = self.link.as_mut() else {
                return;
            };
            if let Err(error) = link.send(&message) {
                self.disconnect(game, &format!("{error:#}"));
                return;
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

    /// Host: takes the first guest whose hello the game welcomes; turns the
    /// rest away.
    fn finish_joins(&mut self, game: &mut GameState) {
        while let Ok(join) = self.joins.1.try_recv() {
            match join {
                Join::Ready(mut link, hello) => {
                    if self.link.is_some() {
                        let _ = link.send(&NetMessage::Refused("THE GAME IS FULL".into()));
                        continue;
                    }
                    let reply = game.welcome(&hello);
                    let refused = matches!(reply, NetMessage::Refused(_));
                    if let Err(error) = link.send(&reply) {
                        log::warn!("welcoming {} failed: {error}", link.peer);
                        continue;
                    }
                    if refused {
                        log::warn!("refused {}: {reply:?}", link.peer);
                        continue;
                    }
                    log::info!("{} joined", link.peer);
                    self.link = Some(link);
                    self.listener = None;
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

    fn disconnect(&mut self, game: &mut GameState, why: &str) {
        self.link = None;
        if !self.lost {
            self.lost = true;
            log::warn!("the other player is gone: {why}");
            game.peer_lost();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Instant;

    impl Session {
        /// The port a host listens on (`--host` with port 0 picks one).
        fn port(&self) -> u16 {
            self.listener
                .as_ref()
                .and_then(|l| l.local_addr().ok())
                .map_or(0, |a| a.port())
        }
    }

    /// Joins `host` with `code` on a thread, pumping the host meanwhile.
    fn join(net: &mut Session, host: &mut GameState, code: &str) -> Result<(Session, GameState)> {
        let address = format!("127.0.0.1:{}", net.port());
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
        let (mut host_net, mut host) = Session::host(0).expect("listens");
        let code = host.join_code().unwrap().to_string();
        let (mut guest_net, mut guest) =
            join(&mut host_net, &mut host, &code.to_lowercase()).expect("joins, in any case");
        assert!(
            host_net.listener.is_none(),
            "one guest, then no more listening"
        );
        assert_eq!(host.checksum(), guest.checksum());

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
        let (mut host_net, mut host) = Session::host(0).expect("listens");
        let error = join(&mut host_net, &mut host, "WRONG1")
            .err()
            .expect("refused");
        assert!(
            format!("{error:#}").contains("WRONG JOIN CODE"),
            "{error:#}"
        );
        pump_host_until(&mut host_net, &mut host, |n| n.refusals == 1);
        assert!(host_net.listener.is_some(), "still listening");
        assert!(host_net.link.is_none());
        // The right code still gets in.
        let code = host.join_code().unwrap().to_string();
        join(&mut host_net, &mut host, &code).expect("joins");
    }

    #[test]
    fn too_many_wrong_codes_stop_the_host_listening() {
        let (mut host_net, mut host) = Session::host(0).expect("listens");
        for _ in 0..MAX_REFUSALS {
            assert!(join(&mut host_net, &mut host, "NOPE00").is_err());
        }
        pump_host_until(&mut host_net, &mut host, |n| n.listener.is_none());
        assert_eq!(host.notice(), "TOO MANY FAILED JOINS - NO LONGER LISTENING");
    }

    #[test]
    fn garbage_and_silence_hold_up_no_real_guest() {
        let (mut host_net, mut host) = Session::host(0).expect("listens");
        let port = host_net.port();
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
        let (mut host_net, mut host) = Session::host(0).expect("listens");
        let port = host_net.port();
        let silent: Vec<TcpStream> = (0..MAX_HANDSHAKES_PER_ADDRESS + 2)
            .map(|_| TcpStream::connect(("127.0.0.1", port)).unwrap())
            .collect();
        let local = IpAddr::from([127, 0, 0, 1]);
        pump_host_until(&mut host_net, &mut host, |n| {
            n.joining.lock().unwrap().get(&local) == Some(&MAX_HANDSHAKES_PER_ADDRESS)
        });
        for _ in 0..200 {
            host_net.pump(&mut host);
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            host_net.joining.lock().unwrap().get(&local),
            Some(&MAX_HANDSHAKES_PER_ADDRESS),
            "the rest were turned away"
        );
        drop(silent);
    }

    #[test]
    fn a_host_that_is_gone_says_so() {
        let (mut host_net, mut host) = Session::host(0).expect("listens");
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
        assert_eq!(guest.notice(), "BLUE LEFT - THE GAME CAN'T GO ON");
    }
}
