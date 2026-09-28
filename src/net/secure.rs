//! The encrypted channel under a network game (`docs/multiplayer.md`,
//! Security). Both ends first send, in the clear, `MAGIC` and their
//! protocol version, so mismatched builds stop before anything else. Then
//! they run SPAKE2 (a password-authenticated key exchange) on the host's
//! join code: it gives both the same strong key only if both used the same
//! code, and an eavesdropper learns nothing from it to guess the code with
//! (each guess takes a live attempt at the host, which allows only a few).
//! HKDF-SHA256 derives one key per direction from it, and every message is
//! then sealed with ChaCha20-Poly1305 under a nonce counting that
//! direction's messages: a tampered, replayed, reordered or dropped message
//! fails to open. A wrong code doesn't fail the exchange itself; it shows
//! when the first sealed message (the guest's hello) won't open.

use std::io::{self, Read, Write};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};

/// The first bytes either end sends: this game.
const MAGIC: &[u8; 4] = b"GC4X";
/// Who's who in the key exchange.
const GUEST_ID: &[u8] = b"goodcomp4x guest";
const HOST_ID: &[u8] = b"goodcomp4x host";
/// A SPAKE2 message on Ed25519: a group element and a side byte.
const SPAKE_MESSAGE: usize = 33;
/// HKDF's salt and the keys' labels.
const SALT: &[u8] = b"goodcomp4x lockstep";
const GUEST_TO_HOST: &[u8] = b"guest to host";
const HOST_TO_GUEST: &[u8] = b"host to guest";
/// Bound into every sealed message.
const ASSOCIATED_DATA: &[u8] = b"goodcomp4x message";
/// The largest message sealed or opened, before its 16-byte tag.
pub const MAX_PLAINTEXT: usize = 1 << 20;
const TAG: usize = 16;

/// Which end of the connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Host,
    Guest,
}

/// Seals this end's messages: a key, and how many it has sealed.
pub struct Sealer {
    cipher: ChaCha20Poly1305,
    sent: u64,
}

/// Opens the other end's messages: a key, and how many it has opened.
pub struct Opener {
    cipher: ChaCha20Poly1305,
    received: u64,
}

/// The nonce for a direction's `count`th message.
fn nonce(count: u64) -> Nonce {
    let mut bytes = [0; 12];
    bytes[4..].copy_from_slice(&count.to_be_bytes());
    *Nonce::from_slice(&bytes)
}

fn invalid(why: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.into())
}

impl Sealer {
    pub fn seal(&mut self, plaintext: &[u8]) -> io::Result<Vec<u8>> {
        if plaintext.len() > MAX_PLAINTEXT {
            return Err(invalid(format!(
                "a {}-byte message is too big",
                plaintext.len()
            )));
        }
        let next = self
            .sent
            .checked_add(1)
            .ok_or_else(|| invalid("out of nonces"))?;
        let sealed = self
            .cipher
            .encrypt(
                &nonce(self.sent),
                Payload {
                    msg: plaintext,
                    aad: ASSOCIATED_DATA,
                },
            )
            .map_err(|_| invalid("sealing failed"))?;
        self.sent = next;
        Ok(sealed)
    }
}

impl Opener {
    /// Opens the next message, or fails if it isn't the other end's next one,
    /// untouched: sealed under another key (a wrong join code), changed,
    /// replayed or out of order.
    pub fn open(&mut self, sealed: &[u8]) -> io::Result<Vec<u8>> {
        let next = self
            .received
            .checked_add(1)
            .ok_or_else(|| invalid("out of nonces"))?;
        let plaintext = self
            .cipher
            .decrypt(
                &nonce(self.received),
                Payload {
                    msg: sealed,
                    aad: ASSOCIATED_DATA,
                },
            )
            .map_err(|_| invalid("a message didn't open: a wrong join code, or tampering"))?;
        self.received = next;
        Ok(plaintext)
    }
}

/// Writes `bytes` as a frame: a little-endian `u32` length, then the bytes.
pub fn write_frame(writer: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    let length = u32::try_from(bytes.len()).map_err(|_| invalid("frame too big"))?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(bytes)?;
    writer.flush()
}

/// Reads a frame of at most a sealed `MAX_PLAINTEXT` message.
pub fn read_frame(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut length = [0; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_le_bytes(length) as usize;
    if length > MAX_PLAINTEXT + TAG {
        return Err(invalid(format!("a {length}-byte frame is too big")));
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// A join code as both ends feed it to the key exchange: case and spaces
/// don't matter.
fn normalize(code: &str) -> Vec<u8> {
    code.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_uppercase())
        .collect::<String>()
        .into_bytes()
}

/// Runs the handshake on `stream` as `side`, with the join `code` and this
/// build's protocol `version`: the version check, then the key exchange.
/// Returns the channel's two halves. A wrong code isn't caught here: the
/// halves just won't open the other end's messages.
pub fn handshake(
    stream: &mut (impl Read + Write),
    side: Side,
    code: &str,
    version: u32,
) -> io::Result<(Sealer, Opener)> {
    let mut preamble = [0; 8];
    preamble[..4].copy_from_slice(MAGIC);
    preamble[4..].copy_from_slice(&version.to_le_bytes());
    stream.write_all(&preamble)?;
    stream.flush()?;
    let mut theirs = [0; 8];
    stream.read_exact(&mut theirs)?;
    if &theirs[..4] != MAGIC {
        return Err(invalid("the other end isn't this game"));
    }
    let their_version = u32::from_le_bytes(theirs[4..].try_into().expect("four bytes"));
    if their_version != version {
        return Err(invalid(format!(
            "version mismatch: this build speaks {version}, the other {their_version}"
        )));
    }

    let password = Password::new(normalize(code));
    let (guest, host) = (Identity::new(GUEST_ID), Identity::new(HOST_ID));
    let (exchange, outbound) = match side {
        Side::Guest => Spake2::<Ed25519Group>::start_a(&password, &guest, &host),
        Side::Host => Spake2::<Ed25519Group>::start_b(&password, &guest, &host),
    };
    stream.write_all(&outbound)?;
    stream.flush()?;
    let mut inbound = [0; SPAKE_MESSAGE];
    stream.read_exact(&mut inbound)?;
    let shared = exchange
        .finish(&inbound)
        .map_err(|_| invalid("the key exchange failed"))?;

    let keys = Hkdf::<Sha256>::new(Some(SALT), &shared);
    let key = |label: &[u8]| {
        let mut key = [0; 32];
        keys.expand(label, &mut key)
            .expect("32 bytes is a valid length");
        ChaCha20Poly1305::new(Key::from_slice(&key))
    };
    let (seal_label, open_label) = match side {
        Side::Guest => (GUEST_TO_HOST, HOST_TO_GUEST),
        Side::Host => (HOST_TO_GUEST, GUEST_TO_HOST),
    };
    Ok((
        Sealer {
            cipher: key(seal_label),
            sent: 0,
        },
        Opener {
            cipher: key(open_label),
            received: 0,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    /// Both ends of a handshake over a real socket, with each end's code.
    fn pair(guest_code: &str, host_code: &str) -> ((Sealer, Opener), (Sealer, Opener)) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let host_code = host_code.to_string();
        let host = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            handshake(&mut stream, Side::Host, &host_code, 1).unwrap()
        });
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let guest = handshake(&mut stream, Side::Guest, guest_code, 1).unwrap();
        (guest, host.join().unwrap())
    }

    #[test]
    fn the_same_code_opens_both_ways_and_case_and_spaces_dont_matter() {
        let ((mut g_seal, mut g_open), (mut h_seal, mut h_open)) = pair("k7m2 qx", "K7M2QX");
        let sealed = g_seal.seal(b"hello").unwrap();
        assert_ne!(&sealed[..], b"hello", "not in the clear");
        assert_eq!(h_open.open(&sealed).unwrap(), b"hello");
        let reply = h_seal.seal(b"welcome").unwrap();
        assert_eq!(g_open.open(&reply).unwrap(), b"welcome");
    }

    #[test]
    fn a_wrong_code_opens_nothing() {
        let ((mut g_seal, _), (_, mut h_open)) = pair("AAAAAA", "BBBBBB");
        let sealed = g_seal.seal(b"hello").unwrap();
        assert!(h_open.open(&sealed).is_err());
    }

    #[test]
    fn a_tampered_replayed_or_reordered_message_fails_to_open() {
        let ((mut g_seal, _), (_, mut h_open)) = pair("CODE42", "CODE42");
        let first = g_seal.seal(b"one").unwrap();
        let second = g_seal.seal(b"two").unwrap();
        let mut tampered = first.clone();
        tampered[0] ^= 1;
        assert!(h_open.open(&tampered).is_err(), "tampered");
        assert!(h_open.open(&second).is_err(), "out of order");
        assert_eq!(h_open.open(&first).unwrap(), b"one");
        assert!(h_open.open(&first).is_err(), "replayed");
        assert_eq!(h_open.open(&second).unwrap(), b"two");
        // Each direction has its own key: a message can't be sent back.
        let ((mut g_seal, mut g_open), _) = pair("CODE42", "CODE42");
        let echoed = g_seal.seal(b"mine").unwrap();
        assert!(g_open.open(&echoed).is_err());
    }

    #[test]
    fn another_version_or_program_is_turned_away_before_the_exchange() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let host = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            handshake(&mut stream, Side::Host, "CODE", 2)
        });
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let guest = handshake(&mut stream, Side::Guest, "CODE", 3);
        assert!(
            guest
                .err()
                .unwrap()
                .to_string()
                .contains("version mismatch")
        );
        assert!(host.join().unwrap().is_err());

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let host = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            handshake(&mut stream, Side::Host, "CODE", 1)
        });
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(b"GET / HTTP/1.1\r\n\r\n").unwrap();
        assert!(host.join().unwrap().is_err());
    }

    #[test]
    fn oversized_frames_are_refused_before_reading_them() {
        let mut bytes = u32::MAX.to_le_bytes().to_vec();
        bytes.extend([0; 16]);
        assert!(read_frame(&mut &bytes[..]).is_err());
        let mut sealer = pair("X", "X").0.0;
        assert!(sealer.seal(&vec![0; MAX_PLAINTEXT + 1]).is_err());
    }
}
