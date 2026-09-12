//! Audio quality metrics.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioMetrics {
    pub snr_db: f32,
    pub segmental_snr_db: f32,
    pub peak_abs_diff: f32,
}

pub fn snr_db(original: &[f32], modified: &[f32]) -> f32 {
    let n = original.len().min(modified.len());
    if n == 0 {
        return 0.0;
    }
    let mut sig = 0.0f64;
    let mut noise = 0.0f64;
    for i in 0..n {
        let o = original[i] as f64;
        let d = (original[i] - modified[i]) as f64;
        sig += o * o;
        noise += d * d;
    }
    if noise < 1e-20 {
        return 120.0;
    }
    (10.0 * (sig / noise).log10()) as f32
}

pub fn segmental_snr_db(original: &[f32], modified: &[f32], seg: usize) -> f32 {
    let n = original.len().min(modified.len());
    if n == 0 || seg == 0 {
        return 0.0;
    }
    let mut acc = 0.0f64;
    let mut count = 0usize;
    let mut i = 0;
    while i + seg <= n {
        let s = snr_db(&original[i..i + seg], &modified[i..i + seg]) as f64;
        // Clamp insane values for silence segments.
        let s = s.clamp(-10.0, 80.0);
        acc += s;
        count += 1;
        i += seg;
    }
    if count == 0 {
        return snr_db(original, modified);
    }
    (acc / count as f64) as f32
}

pub fn peak_abs_diff(original: &[f32], modified: &[f32]) -> f32 {
    let n = original.len().min(modified.len());
    (0..n)
        .map(|i| (original[i] - modified[i]).abs())
        .fold(0.0_f32, f32::max)
}

pub fn compute_metrics(original: &[f32], modified: &[f32]) -> AudioMetrics {
    AudioMetrics {
        snr_db: snr_db(original, modified),
        segmental_snr_db: segmental_snr_db(original, modified, 2048),
        peak_abs_diff: peak_abs_diff(original, modified),
    }
}

pub fn bit_error_rate(expected: &[u8], got: &[u8]) -> f32 {
    let n = expected.len().max(got.len()) * 8;
    if n == 0 {
        return 0.0;
    }
    let mut errors = 0usize;
    let max_len = expected.len().max(got.len());
    for i in 0..max_len {
        let a = expected.get(i).copied().unwrap_or(0);
        let b = got.get(i).copied().unwrap_or(0);
        errors += (a ^ b).count_ones() as usize;
    }
    errors as f32 / n as f32
}
