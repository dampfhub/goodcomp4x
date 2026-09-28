//! Hash maps and sets for the game's own small keys (hexes, edges, ids), with
//! a fast fixed hasher in place of std's SipHash with random keys: the map,
//! the fog and the routes are looked up thousands of times a frame and a
//! turn. A fixed hasher also means the same inserts iterate in the same
//! order on every run. Keys come from the game itself (hexes on the map,
//! ids it hands out), so there's nothing for anyone to flood.
use std::hash::{BuildHasherDefault, Hasher};

pub type HashMap<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<FastHasher>>;
pub type HashSet<T> = std::collections::HashSet<T, BuildHasherDefault<FastHasher>>;

/// The multiply-and-add hash rustc uses (`rustc-hash`): a word at a time.
#[derive(Default, Clone, Copy)]
pub struct FastHasher(u64);

const SEED: u64 = 0xf135_7aea_2e62_a9c5;

impl FastHasher {
    fn add(&mut self, word: u64) {
        self.0 = self.0.wrapping_add(word).wrapping_mul(SEED);
    }
}

impl Hasher for FastHasher {
    fn write(&mut self, bytes: &[u8]) {
        let (chunks, rest) = bytes.as_chunks::<8>();
        for &chunk in chunks {
            self.add(u64::from_le_bytes(chunk));
        }
        if !rest.is_empty() {
            let mut word = [0; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, i: u8) {
        self.add(u64::from(i));
    }

    fn write_u16(&mut self, i: u16) {
        self.add(u64::from(i));
    }

    fn write_u32(&mut self, i: u32) {
        self.add(u64::from(i));
    }

    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }

    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }

    fn write_i32(&mut self, i: i32) {
        self.add(u64::from(i as u32));
    }

    fn finish(&self) -> u64 {
        // The multiply leaves the low bits weakest, and the table picks its
        // bucket from them: turn the strong high bits down.
        self.0.rotate_left(26)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::hex::Hex;

    #[test]
    fn nearby_hexes_spread_over_the_buckets() {
        // A hexagon of radius 20's hexes, bucketed by the hash's low bits
        // as a table of 2048 would: no bucket gets a crowd.
        let mut buckets = vec![0u32; 2048];
        for q in -20..=20 {
            for r in -20..=20 {
                let mut hasher = FastHasher::default();
                std::hash::Hash::hash(&Hex::new(q, r), &mut hasher);
                buckets[(hasher.finish() & 2047) as usize] += 1;
            }
        }
        assert!(
            buckets.iter().all(|&n| n <= 6),
            "{:?}",
            buckets.iter().max()
        );
    }
}
