//! Sync via key-derived m-sequence preamble correlation.

use anyhow::Result;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rand::Rng;
use sha2::{Digest, Sha256};

/// Generate a bipolar (±1) m-sequence-like preamble from the key.
pub fn preamble_samples(key: &[u8], length: usize) -> Vec<f32> {
    let seed = key_seed(key, b"preamble");
    let mut rng = ChaCha8Rng::from_seed(seed);
    (0..length)
        .map(|_| if rng.gen::<bool>() { 1.0 } else { -1.0 })
        .collect()
}

pub fn key_seed(key: &[u8], domain: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(key);
    let dig = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&dig);
    out
}

/// Cross-correlate `signal` against `preamble` over offsets 0..=max_offset.
/// Returns (best_offset, peak_score).
pub fn find_offset(signal: &[f32], preamble: &[f32], max_offset: usize) -> Result<(usize, f32)> {
    if preamble.is_empty() || signal.len() < preamble.len() {
        return Ok((0, 0.0));
    }
    let max_off = max_offset.min(signal.len() - preamble.len());
    let mut best_off = 0usize;
    let mut best_score = f32::MIN;
    let pref_energy: f32 = preamble.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-8);

    for off in 0..=max_off {
        let mut dot = 0.0f32;
        let mut energy = 0.0f32;
        for i in 0..preamble.len() {
            let s = signal[off + i];
            dot += s * preamble[i];
            energy += s * s;
        }
        let score = dot / (energy.sqrt().max(1e-8) * pref_energy);
        if score > best_score {
            best_score = score;
            best_off = off;
        }
    }
    Ok((best_off, best_score))
}

/// Mix a low-level preamble into the start of the mono signal (in-place additive).
pub fn mix_preamble(signal: &mut [f32], preamble: &[f32], gain: f32) {
    let n = signal.len().min(preamble.len());
    for i in 0..n {
        signal[i] += preamble[i] * gain;
    }
}

pub fn default_preamble_len(fft_size: usize) -> usize {
    fft_size * 4
}
