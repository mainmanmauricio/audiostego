//! Key-seeded per-frame bit allocation / interleaving.

use crate::dsp::sync::key_seed;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Assign each payload bit index to a (frame, slot) pair via keyed shuffle.
#[derive(Debug, Clone)]
pub struct BitAllocation {
    /// For each frame: ordered list of global bit indices assigned to that frame.
    pub frame_bits: Vec<Vec<usize>>,
    pub total_bits: usize,
}

impl BitAllocation {
    pub fn plan(num_frames: usize, bits_per_frame: usize, total_bits: usize, key: &[u8]) -> Self {
        let capacity = num_frames.saturating_mul(bits_per_frame);
        let n = total_bits.min(capacity);
        let mut indices: Vec<usize> = (0..n).collect();
        let seed = key_seed(key, b"bit-alloc");
        let mut rng = ChaCha8Rng::from_seed(seed);
        indices.shuffle(&mut rng);

        let mut frame_bits = vec![Vec::new(); num_frames];
        for (i, bit_idx) in indices.into_iter().enumerate() {
            let frame = i / bits_per_frame.max(1);
            if frame < num_frames {
                frame_bits[frame].push(bit_idx);
            }
        }
        Self {
            frame_bits,
            total_bits: n,
        }
    }
}
