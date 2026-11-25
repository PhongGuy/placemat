//! Fresh element ids and jitter seeds.

use super::{Board, ElementId, Seed};

/// Length of a generated element id.
const ID_LEN: usize = 8;

/// The source of new element ids and seeds: from OS entropy in the editor, or from a fixed
/// seed where a test or tool needs the same sequence every run.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Rng(fastrand::Rng);

impl Rng {
    /// A generator seeded from OS entropy.
    pub fn from_entropy() -> Self {
        Self(fastrand::Rng::new())
    }

    /// A generator that repeats the same sequence for the same `seed`.
    pub fn seeded(seed: u64) -> Self {
        Self(fastrand::Rng::with_seed(seed))
    }

    /// A new id of 8 base62 characters that `board` does not already use.
    pub fn element_id(&mut self, board: &Board) -> ElementId {
        // 62^8 ids make a redraw rare; a draw is only kept once it is free on the board.
        loop {
            let text: String = (0..ID_LEN).map(|_| self.0.alphanumeric()).collect();
            if let Some(id) = ElementId::new(&text)
                && !board.contains_id(&text)
            {
                return id;
            }
        }
    }

    /// A seed drawn uniformly from `0..=Seed::MAX`.
    pub fn seed(&mut self) -> Seed {
        Seed::clamped(self.0.u32(0..=Seed::MAX))
    }
}
