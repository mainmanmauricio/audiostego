//! Spread-spectrum embedding (lowest capacity, highest robustness).

use crate::cli::StrategyId;
use crate::strategy::{
    band_bins, bipolar_bit, enforce_hermitian_edges, frame_rng, EmbedStrategy, FrameCtx,
};
use num_complex::Complex;
use rand::Rng;

pub struct SpreadSpectrum;

/// Multiplicative depth. Keep embed/extract independent of post-embed energy.
fn chip_gain(strength: f32) -> f32 {
    strength.clamp(0.05, 0.5)
}

/// Boost near-silent bins so a multiplicative chip survives float WAV + STFT.
/// A 1e-8 floor is lost in reconstruction; this matches LSB/differential scale.
fn mag_floor(strength: f32) -> f32 {
    (0.05 + chip_gain(strength) * 0.5).max(0.05)
}

/// PN covariance of log-magnitudes after removing a linear frequency trend.
/// Rectangular-window leakage of a tone is a smooth 1/Δk envelope that otherwise
/// dominates `sum(pn * ln(mag))` and flips payload bits (CRC failures).
fn detrended_corr(pn: &[f32], logs: &[f32]) -> f32 {
    let n = logs.len();
    if n == 0 {
        return 0.0;
    }
    let nf = n as f32;
    if n < 4 {
        let mean_y = logs.iter().sum::<f32>() / nf;
        let mean_pn = pn.iter().sum::<f32>() / nf;
        return pn
            .iter()
            .zip(logs.iter())
            .map(|(&p, &y)| (p - mean_pn) * (y - mean_y))
            .sum::<f32>();
    }

    let mut sum_x = 0.0f32;
    let mut sum_y = 0.0f32;
    let mut sum_xx = 0.0f32;
    let mut sum_xy = 0.0f32;
    for (i, &y) in logs.iter().enumerate() {
        let x = i as f32;
        sum_x += x;
        sum_y += y;
        sum_xx += x * x;
        sum_xy += x * y;
    }
    let denom = nf * sum_xx - sum_x * sum_x;
    let slope = if denom.abs() > 1e-6 {
        (nf * sum_xy - sum_x * sum_y) / denom
    } else {
        0.0
    };
    let intercept = (sum_y - slope * sum_x) / nf;
    let mean_pn = pn.iter().sum::<f32>() / nf;
    let mut corr = 0.0f32;
    for i in 0..n {
        let resid = logs[i] - (intercept + slope * i as f32);
        corr += (pn[i] - mean_pn) * resid;
    }
    corr
}

impl EmbedStrategy for SpreadSpectrum {
    fn id(&self) -> StrategyId {
        StrategyId::SpreadSpectrum
    }

    fn requires_original(&self) -> bool {
        false
    }

    fn capacity_bits(&self, ctx: &FrameCtx) -> usize {
        let band = ctx.bins.end.saturating_sub(ctx.bins.start);
        // Disjoint bin subsets; moderate packing for robust payload bits.
        (band / 32).max(1).min(8)
    }

    fn embed_frame(&self, spec: &mut [Complex<f32>], bits: &[bool], ctx: &FrameCtx) {
        let bins = band_bins(&ctx.bins);
        let n_bits = self.capacity_bits(ctx).min(bits.len());
        if n_bits == 0 || bins.is_empty() {
            return;
        }
        let alpha = chip_gain(ctx.strength);
        let floor = mag_floor(ctx.strength);
        // Partition bins across bits so multiplicative marks do not compound.
        let chunk = (bins.len() / n_bits).max(1);
        for bit_i in 0..n_bits {
            let mut rng = frame_rng(&ctx.key, ctx.index, &bit_i.to_le_bytes());
            let sign = bipolar_bit(bits[bit_i]);
            let start = bit_i * chunk;
            let end = if bit_i + 1 == n_bits {
                bins.len()
            } else {
                ((bit_i + 1) * chunk).min(bins.len())
            };
            for &bin in &bins[start..end] {
                if bin >= spec.len() {
                    break;
                }
                let pn: f32 = if rng.gen::<bool>() { 1.0 } else { -1.0 };
                let mag = spec[bin].norm().max(floor);
                let phase = spec[bin].arg();
                let new_mag = mag * (1.0 + alpha * sign * pn);
                spec[bin] = Complex::from_polar(new_mag.max(0.0), phase);
            }
        }
        enforce_hermitian_edges(spec);
    }

    fn extract_frame(
        &self,
        spec: &[Complex<f32>],
        _orig: Option<&[Complex<f32>]>,
        out: &mut Vec<bool>,
        ctx: &FrameCtx,
    ) {
        let bins = band_bins(&ctx.bins);
        let n_bits = self.capacity_bits(ctx);
        if n_bits == 0 || bins.is_empty() {
            return;
        }
        let chunk = (bins.len() / n_bits).max(1);
        for bit_i in 0..n_bits {
            let mut rng = frame_rng(&ctx.key, ctx.index, &bit_i.to_le_bytes());
            let start = bit_i * chunk;
            let end = if bit_i + 1 == n_bits {
                bins.len()
            } else {
                ((bit_i + 1) * chunk).min(bins.len())
            };
            let mut pn_vals = Vec::with_capacity(end.saturating_sub(start));
            let mut logs = Vec::with_capacity(end.saturating_sub(start));
            for &bin in &bins[start..end] {
                if bin >= spec.len() {
                    break;
                }
                let pn: f32 = if rng.gen::<bool>() { 1.0 } else { -1.0 };
                // Tiny epsilon only — do not re-clamp to the embed floor or
                // both bit polarities collapse when reconstruction undershoots.
                let mag = spec[bin].norm().max(1e-6);
                pn_vals.push(pn);
                logs.push(mag.ln());
            }
            out.push(detrended_corr(&pn_vals, &logs) >= 0.0);
        }
    }
}

/// Always used for the self-describing capsule header (independent of body strategy).
pub fn header_strategy() -> SpreadSpectrum {
    SpreadSpectrum
}
