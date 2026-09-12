//! Spread-spectrum embedding (lowest capacity, highest robustness).

use crate::cli::StrategyId;
use crate::strategy::{
    bipolar_bit, band_bins, enforce_hermitian_edges, frame_rng, EmbedStrategy, FrameCtx,
};
use num_complex::Complex;
use rand::Rng;

pub struct SpreadSpectrum;

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
        let alpha = ctx.strength.clamp(0.05, 0.5);
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
                let mag = spec[bin].norm().max(1e-8);
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
            let mut corr = 0.0f32;
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
                let mag = spec[bin].norm().max(1e-8);
                corr += pn * mag.ln();
            }
            out.push(corr >= 0.0);
        }
    }
}

/// Always used for the self-describing capsule header (independent of body strategy).
pub fn header_strategy() -> SpreadSpectrum {
    SpreadSpectrum
}
