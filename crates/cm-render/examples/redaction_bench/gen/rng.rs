//! A tiny seeded generator (splitmix64). The workspace has no `rand` crate and the set must be
//! identical for a given seed on every machine.

#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    /// An independent stream for `tag` (image index, retry number, ...).
    pub fn derive(seed: u64, tag: u64) -> Self {
        let mut r = Rng(seed ^ tag.wrapping_mul(0xD6E8_FEB8_6659_FD93));
        r.next_u64();
        r
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }

    /// Uniform in `lo..=hi`.
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next_u64() % ((hi - lo + 1) as u64)) as i64
    }

    pub fn chance(&mut self, p: f64) -> bool {
        p > 0.0 && ((self.next_u64() >> 11) as f64 / (1u64 << 53) as f64) < p
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    pub fn digit(&mut self) -> u8 {
        self.below(10) as u8
    }

    /// `n` random decimal digits as a string (leading zeros allowed).
    pub fn digits(&mut self, n: usize) -> String {
        (0..n).map(|_| (b'0' + self.digit()) as char).collect()
    }

    /// `n` characters drawn from `alphabet`.
    pub fn chars(&mut self, n: usize, alphabet: &str) -> String {
        let a: Vec<char> = alphabet.chars().collect();
        (0..n).map(|_| a[self.below(a.len())]).collect()
    }

    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}

pub const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
pub const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
pub const ALNUM: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
pub const UPNUM: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
pub const LOWNUM: &str = "abcdefghijklmnopqrstuvwxyz0123456789";
pub const HEX: &str = "0123456789abcdef";
pub const B64: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
pub const B64URL: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
