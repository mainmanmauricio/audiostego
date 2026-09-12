//! Quantization Index Modulation on magnitude.

use crate::cli::StrategyId;
use crate::strategy::{band_bins, enforce_hermitian_edges, EmbedStrategy, FrameCtx};
use num_complex::Complex;

pub struct Qim;

fn delta(strength: f32) -> f32 {
    // Fixed step from strength so embed and extract agree (must not depend on
    // post-embed magnitude).
    (strength * 0.25).max(1e-4)
}

impl EmbedStrategy for Qim {
    fn id(&self) -> StrategyId {
        StrategyId::Qim
    }

    fn requires_original(&self) -> bool {
        false
    }

    fn capacity_bits(&self, ctx: &FrameCtx) -> usize {
        ctx.bins.end.saturating_sub(ctx.bins.start)
    }

    fn embed_frame(&self, spec: &mut [Complex<f32>], bits: &[bool], ctx: &FrameCtx) {
        let bins = band_bins(&ctx.bins);
        let delta = delta(ctx.strength);
        for (i, &bin) in bins.iter().enumerate() {
            if i >= bits.len() || bin >= spec.len() {
                break;
            }
            let mag = spec[bin].norm();
            let phase = spec[bin].arg();
            // Ditherless QIM: two interlaced lattices spaced by 2*delta.
            let c0 = (mag / (2.0 * delta)).round() * (2.0 * delta);
            let c1 = ((mag / (2.0 * delta)) - 0.5).round() * (2.0 * delta) + delta;
            let new_mag = if bits[i] { c1 } else { c0 }.max(0.0);
            spec[bin] = Complex::from_polar(new_mag, phase);
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
        let delta = delta(ctx.strength);
        for &bin in &bins {
            if bin >= spec.len() {
                break;
            }
            let mag = spec[bin].norm();
            let c0 = (mag / (2.0 * delta)).round() * (2.0 * delta);
            let c1 = ((mag / (2.0 * delta)) - 0.5).round() * (2.0 * delta) + delta;
            let d0 = (mag - c0).abs();
            let d1 = (mag - c1).abs();
            out.push(d1 < d0);
        }
    }
}
