//! Shared by the random-session tests.

/// The seeds to run: `LIVEMARK_FUZZ_SEEDS=<n>` runs more sessions than
/// `default` (e.g. with `--release`), `LIVEMARK_FUZZ_ONLY=<seed>` replays
/// one.
pub fn seeds(default: u64) -> std::ops::Range<u64> {
    let number = |name| {
        std::env::var(name)
            .ok()
            .and_then(|n: String| n.parse::<u64>().ok())
    };
    match number("LIVEMARK_FUZZ_ONLY") {
        Some(seed) => seed..seed + 1,
        None => 0..number("LIVEMARK_FUZZ_SEEDS").unwrap_or(default),
    }
}

/// SplitMix64, as in roughdraft's random sessions.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }

    /// A char boundary in `text`, ends included.
    pub fn boundary(&mut self, text: &str) -> usize {
        let mut at = self.below(text.len() as u64 + 1) as usize;
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        at
    }
}
