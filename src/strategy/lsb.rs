//! Magnitude LSB embedding (fragile, high capacity).

use crate::cli::StrategyId;
use crate::strategy::{band_bins, enforce_hermitian_edges, EmbedStrategy, FrameCtx};
use num_complex::Complex;

pub struct MagnitudeLsb;

fn step(strength: f32) -> f32 {
    // Large enough to survive float STFT round-trips on weak bins.
    (0.05 + strength * 0.5).max(0.05)
}

impl EmbedStrategy for MagnitudeLsb {
    fn id(&self) -> StrategyId {
        StrategyId::MagnitudeLsb
    }

    fn requires_original(&self) -> bool {
        false
    }

    fn capacity_bits(&self, ctx: &FrameCtx) -> usize {
        ctx.bins.end.saturating_sub(ctx.bins.start)
    }

    fn embed_frame(&self, spec: &mut [Complex<f32>], bits: &[bool], ctx: &FrameCtx) {
        let step = step(ctx.strength);
        let bins = band_bins(&ctx.bins);
        for (i, &bin) in bins.iter().enumerate() {
            if i >= bits.len() || bin >= spec.len() {
                break;
            }
            let phase = spec[bin].arg();
            // Boost near-silent bins so the LSB lattice is well-defined.
            let mag = spec[bin].norm().max(step * 4.0);
            let mut q = (mag / step).round() as i64;
            if bits[i] {
                if q % 2 == 0 {
                    q += 1;
                }
            } else if q % 2 != 0 {
                q -= 1;
            }
            let new_mag = (q as f32) * step;
            spec[bin] = Complex::from_polar(new_mag.max(0.0), phase);
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
        let step = step(ctx.strength);
        let bins = band_bins(&ctx.bins);
        for &bin in &bins {
            if bin >= spec.len() {
                break;
            }
            let mag = spec[bin].norm();
            let q = (mag / step).round() as i64;
            out.push(q % 2 != 0);
        }
    }
}
