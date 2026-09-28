//! The network link of a multiplayer game (`docs/multiplayer.md`): the
//! lockstep messages of `game::multiplayer` over TCP, each a little-endian
//! `u32` length and then its bincode encoding. A reader thread per
//! connection turns incoming bytes into messages on a channel, so the frame
//! loop only polls (`Session::pump`) and never blocks on the network.
//!
//! Everything that arrives is untrusted: a frame over `MAX_MESSAGE` or one
//! that doesn't decode (bincode, with the same limit) drops the peer, and
//! the game checks every message before using it (`GameState::receive`),
//! dropping a peer that sends a malformed or hostile one. A host takes one
//! guest, who must give the join code it shows; a connection that doesn't
//! say hello within `HELLO_TIMEOUT` is dropped, and after
//! `MAX_REFUSALS` refused guests the host stops listening. The link isn't
//! encrypted: play on networks you trust.

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use bincode::Options;

use crate::game::{GameState, NetMessage};

/// The port `--host` listens on and `--join` connects to when none is given.
pub const DEFAULT_PORT: u16 = 7777;
/// The largest message accepted: a turn's plans are a few kilobytes.
const MAX_MESSAGE: usize = 1 << 20;
/// How long a new connection to the host has to say hello.
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
/// How many guests a host refuses (a wrong code, say) before it stops
/// listening, so a join code can't be guessed at.
const MAX_REFUSALS: u32 = 10;
/// How long `--join` waits to connect, and then for the host's welcome.
const JOIN_TIMEOUT: Duration = Duration::from_secs(10);

/// A connection to the other machine.
struct Link {
    writer: TcpStream,
    incoming: Receiver<io::Result<NetMessage>>,
    peer: SocketAddr,
}

impl Link {
    fn new(stream: TcpStream) -> Result<Self> {
        // Accepted from a non-blocking listener, a stream is non-blocking
        // too on some platforms (Windows): its reader thread blocks.
        stream.set_nonblocking(false)?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(None)?;
        let peer = stream.peer_addr()?;
        let mut reader = stream.try_clone()?;
        let (send, incoming) = mpsc::channel();
        thread::spawn(move || {
            loop {
                let message = read_message(&mut reader);
                let failed = message.is_err();
                if send.send(message).is_err() || failed {
                    return;
                }
            }
        });
        Ok(Self {
            writer: stream,
            incoming,
            peer,
        })
    }

    fn send(&mut self, message: &NetMessage) -> Result<()> {
        write_message(&mut self.writer, message)
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
    /// Closes the connection for both ends: the reader thread's copy of
    /// the stream would otherwise keep it open.
    fn drop(&mut self) {
        let _ = self.writer.shutdown(Shutdown::Both);
    }
}

/// The encoding both ends use: bincode, never decoding more than
/// `MAX_MESSAGE` bytes' worth, whatever lengths a message claims inside.
fn codec() -> impl Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(MAX_MESSAGE as u64)
}

fn write_message(writer: &mut impl Write, message: &NetMessage) -> Result<()> {
    let bytes = codec().serialize(message)?;
    if bytes.len() > MAX_MESSAGE {
        bail!("a {}-byte message is too big to send", bytes.len());
    }
    writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()?;
    Ok(())
}

fn read_message(reader: &mut impl Read) -> io::Result<NetMessage> {
    let mut length = [0; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_le_bytes(length) as usize;
    if length > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("a {length}-byte message is too big"),
        ));
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    codec()
        .deserialize(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// This machine's side of a multiplayer game.
pub struct Session {
    /// The host's listener, until a guest joins.
    listener: Option<TcpListener>,
    link: Option<Link>,
    /// Host: when the current connection opened, until it says hello.
    awaiting_hello: Option<Instant>,
    /// Host: guests refused so far (`MAX_REFUSALS`).
    refusals: u32,
    /// Whether the connection was lost (reported once).
    lost: bool,
}

impl Session {
    /// `--host`: listens on `port` for a guest, and starts the game.
    pub fn host(port: u16) -> Result<(Session, GameState)> {
        let listener = TcpListener::bind(("0.0.0.0", port))
            .with_context(|| format!("can't listen on port {port}"))?;
        listener.set_nonblocking(true)?;
        let game = GameState::host_game();
        log::info!(
            "hosting on port {port}; waiting for a player to join with code {}",
            game.join_code().unwrap_or_default()
        );
        let session = Session {
            listener: Some(listener),
            link: None,
            awaiting_hello: None,
            refusals: 0,
            lost: false,
        };
        Ok((session, game))
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
        write_message(
            &mut stream,
            &NetMessage::Hello {
                version: crate::game::PROTOCOL_VERSION,
                code: code.to_string(),
            },
        )?;
        stream.set_read_timeout(Some(JOIN_TIMEOUT))?;
        let welcome = read_message(&mut stream).context("the host didn't answer")?;
        let game = GameState::join_game(&welcome).map_err(|reason| anyhow!("{reason}"))?;
        log::info!("joined the game at {addr}");
        let session = Session {
            listener: None,
            link: Some(Link::new(stream)?),
            awaiting_hello: None,
            refusals: 0,
            lost: false,
        };
        Ok((session, game))
    }

    /// Once a frame: takes in a joining guest (host), hands arriving
    /// messages to the game, and sends the game's outgoing ones.
    pub fn pump(&mut self, game: &mut GameState) {
        if self.link.is_none()
            && let Some(listener) = &self.listener
        {
            match listener.accept() {
                Ok((stream, addr)) => match Link::new(stream) {
                    Ok(link) => {
                        log::info!("{addr} is connecting");
                        self.link = Some(link);
                        self.awaiting_hello = Some(Instant::now());
                    }
                    Err(error) => log::warn!("{addr} couldn't connect: {error:#}"),
                },
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => log::warn!("accepting a player failed: {error}"),
            }
        }
        // A connection that won't say hello doesn't get to hold the host.
        if self
            .awaiting_hello
            .is_some_and(|since| since.elapsed() > HELLO_TIMEOUT)
        {
            log::warn!("dropping a connection that didn't say hello");
            self.link = None;
            self.awaiting_hello = None;
            return;
        }
        let Some(link) = self.link.as_mut() else {
            return;
        };
        let incoming = match link.poll() {
            Ok(messages) => messages,
            Err(error) => {
                self.drop_peer(game, &format!("{error:#}"));
                return;
            }
        };
        for message in incoming {
            // Until it says hello, a connection may say nothing else.
            let hello = matches!(message, NetMessage::Hello { .. });
            if self.awaiting_hello.is_some() != hello {
                let why = format!("{} sent {message:?} out of turn", link.peer);
                self.drop_peer(game, &why);
                return;
            }
            if hello {
                // The host answers with the game, or a refusal; once a
                // guest is in, it stops listening.
                let reply = game.welcome(&message);
                let refused = matches!(reply, NetMessage::Refused(_));
                if let Err(error) = link.send(&reply) {
                    log::warn!("welcoming {} failed: {error:#}", link.peer);
                }
                self.awaiting_hello = None;
                if refused {
                    log::warn!("refused {}: {reply:?}", link.peer);
                    self.link = None;
                    self.refusals += 1;
                    if self.refusals >= MAX_REFUSALS {
                        log::warn!("refused {MAX_REFUSALS} players: no longer listening");
                        self.listener = None;
                        game.stop_listening();
                    }
                    return;
                }
                self.listener = None;
                continue;
            }
            if let Err(why) = game.receive(message) {
                let why = format!("{} sent a bad message: {why}", link.peer);
                self.drop_peer(game, &why);
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

    /// Drops a peer that broke the protocol: a guest still saying hello is
    /// just turned away (the host keeps listening); one in the game is
    /// gone for good.
    fn drop_peer(&mut self, game: &mut GameState, why: &str) {
        log::warn!("{why}");
        if self.awaiting_hello.take().is_some() {
            self.link = None;
            return;
        }
        self.disconnect(game, why);
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

    /// Pumps both ends and plays out their turns until `done`, or panics
    /// after a few seconds.
    fn pump_until(
        host: (&mut Session, &mut GameState),
        guest: (&mut Session, &mut GameState),
        done: impl Fn(&GameState, &GameState) -> bool,
    ) {
        let start = Instant::now();
        while !done(host.1, guest.1) {
            assert!(start.elapsed() < Duration::from_secs(10), "timed out");
            host.0.pump(host.1);
            guest.0.pump(guest.1);
            host.1.update(1.0);
            guest.1.update(1.0);
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn a_turn_plays_out_over_a_real_connection() {
        let (mut host_net, mut host) = Session::host(0).expect("listens");
        let port = host_net.port();
        let code = host.join_code().unwrap().to_string();
        let joining = thread::spawn(move || Session::join(&format!("127.0.0.1:{port}"), &code));
        // The host takes the guest in and welcomes it while it waits.
        let start = Instant::now();
        while !joining.is_finished() {
            assert!(start.elapsed() < Duration::from_secs(10), "join timed out");
            host_net.pump(&mut host);
            thread::sleep(Duration::from_millis(1));
        }
        let (mut guest_net, mut guest) = joining.join().unwrap().expect("joins");
        assert_eq!(host.checksum(), guest.checksum());

        // Each city has something to build, so End Turn sends the plan.
        for game in [&mut host, &mut guest] {
            game.select_city();
            game.queue_selected_city_gather();
            game.end_planning();
            assert!(game.is_resolving(), "{}", game.notice());
        }
        pump_until(
            (&mut host_net, &mut host),
            (&mut guest_net, &mut guest),
            |h, g| h.turn() == 1 && g.turn() == 1 && !h.is_resolving() && !g.is_resolving(),
        );
        // The guest's checksum reaches the host, and matches.
        for _ in 0..50 {
            host_net.pump(&mut host);
            guest_net.pump(&mut guest);
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(host.checksum(), guest.checksum());
        assert!(!host.notice().contains("DESYNC"), "{}", host.notice());
    }

    /// Pumps the host until `done`, or panics after a few seconds.
    fn pump_host_until(net: &mut Session, game: &mut GameState, done: impl Fn(&Session) -> bool) {
        let start = Instant::now();
        while !done(net) {
            assert!(start.elapsed() < Duration::from_secs(10), "timed out");
            net.pump(game);
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn a_wrong_code_is_turned_away_and_the_host_keeps_listening() {
        let (mut host_net, mut host) = Session::host(0).expect("listens");
        let port = host_net.port();
        let joining = thread::spawn(move || Session::join(&format!("127.0.0.1:{port}"), "WRONG1"));
        while !joining.is_finished() {
            host_net.pump(&mut host);
            thread::sleep(Duration::from_millis(1));
        }
        let error = joining.join().unwrap().err().expect("refused");
        assert!(
            format!("{error:#}").contains("WRONG JOIN CODE"),
            "{error:#}"
        );
        assert!(host_net.listener.is_some(), "still listening");
        assert_eq!(host_net.refusals, 1);
    }

    #[test]
    fn garbage_and_oversized_frames_are_dropped_without_harm() {
        let (mut host_net, mut host) = Session::host(0).expect("listens");
        let port = host_net.port();
        let before = host.checksum();
        // A frame claiming to be huge, then bytes that aren't a message.
        for bytes in [
            u32::MAX.to_le_bytes().to_vec(),
            [8u32.to_le_bytes().to_vec(), vec![0xff; 8]].concat(),
        ] {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(&bytes).unwrap();
            pump_host_until(&mut host_net, &mut host, |n| n.link.is_some());
            pump_host_until(&mut host_net, &mut host, |n| n.link.is_none());
        }
        assert_eq!(host.checksum(), before);
        assert!(host_net.listener.is_some(), "still listening");
        assert!(!host.notice().contains("LEFT"), "{}", host.notice());
    }

    #[test]
    fn a_message_before_hello_is_refused() {
        let (mut host_net, mut host) = Session::host(0).expect("listens");
        let port = host_net.port();
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_message(&mut stream, &NetMessage::Checksum { turn: 0, value: 0 }).unwrap();
        pump_host_until(&mut host_net, &mut host, |n| n.link.is_some());
        pump_host_until(&mut host_net, &mut host, |n| n.link.is_none());
        assert!(host_net.listener.is_some());
    }

    #[test]
    fn a_silent_connection_is_dropped_after_the_hello_timeout() {
        let (mut host_net, mut host) = Session::host(0).expect("listens");
        let port = host_net.port();
        let _silent = TcpStream::connect(("127.0.0.1", port)).unwrap();
        pump_host_until(&mut host_net, &mut host, |n| n.link.is_some());
        // As if the timeout had passed.
        host_net.awaiting_hello = Some(Instant::now() - HELLO_TIMEOUT * 2);
        host_net.pump(&mut host);
        assert!(host_net.link.is_none() && host_net.listener.is_some());
    }

    #[test]
    fn a_host_that_is_gone_says_so() {
        let (mut host_net, mut host) = Session::host(0).expect("listens");
        let port = host_net.port();
        let code = host.join_code().unwrap().to_string();
        let joining = thread::spawn(move || Session::join(&format!("127.0.0.1:{port}"), &code));
        while !joining.is_finished() {
            host_net.pump(&mut host);
            thread::sleep(Duration::from_millis(1));
        }
        let (mut guest_net, mut guest) = joining.join().unwrap().expect("joins");
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
