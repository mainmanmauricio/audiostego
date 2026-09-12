//! Differential spectral embedding (requires original on extract).

use crate::cli::StrategyId;
use crate::strategy::{
    band_bins, bipolar_bit, enforce_hermitian_edges, frame_rng, EmbedStrategy, FrameCtx,
};
use num_complex::Complex;
use rand::Rng;

pub struct Differential;

impl EmbedStrategy for Differential {
    fn id(&self) -> StrategyId {
        StrategyId::Differential
    }

    fn requires_original(&self) -> bool {
        true
    }

    fn capacity_bits(&self, ctx: &FrameCtx) -> usize {
        ctx.bins.end.saturating_sub(ctx.bins.start)
    }

    fn embed_frame(&self, spec: &mut [Complex<f32>], bits: &[bool], ctx: &FrameCtx) {
        let bins = band_bins(&ctx.bins);
        let mut rng = frame_rng(&ctx.key, ctx.index, b"diff");
        let alpha = ctx.strength.clamp(0.01, 1.0);
        for (i, &bin) in bins.iter().enumerate() {
            if i >= bits.len() || bin >= spec.len() {
                break;
            }
            let sign = bipolar_bit(bits[i]);
            let mut dir = Complex::new(rng.gen::<f32>() * 2.0 - 1.0, rng.gen::<f32>() * 2.0 - 1.0);
            let n = dir.norm().max(1e-8);
            dir = dir.unscale(n);
            // Additive keyed perturbation large enough to survive float WAV.
            spec[bin] += dir.scale(sign * alpha * (spec[bin].norm() + 0.05));
        }
        enforce_hermitian_edges(spec);
    }

    fn extract_frame(
        &self,
        spec: &[Complex<f32>],
        orig: Option<&[Complex<f32>]>,
        out: &mut Vec<bool>,
        ctx: &FrameCtx,
    ) {
        let Some(orig) = orig else {
            return;
        };
        let bins = band_bins(&ctx.bins);
        let mut rng = frame_rng(&ctx.key, ctx.index, b"diff");
        for &bin in &bins {
            if bin >= spec.len() || bin >= orig.len() {
                break;
            }
            let mut dir = Complex::new(rng.gen::<f32>() * 2.0 - 1.0, rng.gen::<f32>() * 2.0 - 1.0);
            let n = dir.norm().max(1e-8);
            dir = dir.unscale(n);
            let delta = spec[bin] - orig[bin];
            let proj = delta.re * dir.re + delta.im * dir.im;
            out.push(proj >= 0.0);
        }
    }
}
