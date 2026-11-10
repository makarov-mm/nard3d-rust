//! A small source of random numbers (SplitMix64): the dice, the computer's slips and the little differences between
//! one throw or one sound and the next.

use std::hash::{BuildHasher, Hasher};

pub struct Random(u64);

impl Random {
    pub fn new(seed: u64) -> Random {
        Random(seed)
    }

    /// Seeded differently at every run.
    pub fn from_entropy() -> Random {
        Random(std::collections::hash_map::RandomState::new().build_hasher().finish())
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// From 0 to n - 1.
    pub fn below(&mut self, n: usize) -> usize {
        ((u128::from(self.next_u64()) * n as u128) >> 64) as usize
    }

    /// From 0 up to, but not reaching, 1.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Between two numbers, evenly.
    pub fn between(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }

    /// A throw of a die.
    pub fn die(&mut self) -> i8 {
        1 + self.below(6) as i8
    }

    /// From the bell curve round zero with a deviation of one (Box and Muller's way).
    pub fn normal(&mut self) -> f32 {
        let (u, v) = (f64::from(self.unit()).max(1e-9), f64::from(self.unit()));
        ((-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()) as f32
    }
}
