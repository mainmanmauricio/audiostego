//! Embedding strategies.

pub mod differential;
pub mod lsb;
pub mod qim;
pub mod spread;

use crate::cli::{Shaping, StrategyId};
use crate::dsp::sync::key_seed;
use anyhow::Result;
use num_complex::Complex;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::ops::Range;

#[derive(Debug, Clone)]
pub struct FrameCtx {
    pub index: usize,
    pub bins: Range<usize>,
    pub sample_rate: u32,
    pub strength: f32,
    pub shaping: Shaping,
    pub key: Vec<u8>,
}

pub trait EmbedStrategy: Send + Sync {
    fn id(&self) -> StrategyId;
    fn requires_original(&self) -> bool;
    /// Bits that can be embedded in one frame.
    fn capacity_bits(&self, ctx: &FrameCtx) -> usize;
    fn embed_frame(&self, spec: &mut [Complex<f32>], bits: &[bool], ctx: &FrameCtx);
    fn extract_frame(
        &self,
        spec: &[Complex<f32>],
        orig: Option<&[Complex<f32>]>,
        out: &mut Vec<bool>,
        ctx: &FrameCtx,
    );
}

pub fn make_strategy(id: StrategyId) -> Box<dyn EmbedStrategy> {
    match id {
        StrategyId::Differential => Box::new(differential::Differential),
        StrategyId::MagnitudeLsb => Box::new(lsb::MagnitudeLsb),
        StrategyId::Qim => Box::new(qim::Qim),
        StrategyId::SpreadSpectrum => Box::new(spread::SpreadSpectrum),
    }
}

pub fn band_bins(bins: &Range<usize>) -> Vec<usize> {
    (bins.start..bins.end).collect()
}

pub fn mask_gain(mag: f32, strength: f32, shaping: Shaping) -> f32 {
    match shaping {
        Shaping::Fixed => strength,
        Shaping::Masked => strength * (mag + 1e-4).sqrt().min(1.0),
    }
}

pub fn frame_rng(key: &[u8], frame_index: usize, domain: &[u8]) -> ChaCha8Rng {
    let mut material = Vec::with_capacity(key.len() + 16);
    material.extend_from_slice(domain);
    material.extend_from_slice(key);
    material.extend_from_slice(&(frame_index as u64).to_le_bytes());
    let seed = key_seed(&material, b"frame-rng");
    ChaCha8Rng::from_seed(seed)
}

pub fn bipolar_bit(bit: bool) -> f32 {
    if bit {
        1.0
    } else {
        -1.0
    }
}

pub fn enforce_hermitian_edges(spec: &mut [Complex<f32>]) {
    if let Some(dc) = spec.first_mut() {
        dc.im = 0.0;
    }
    if let Some(ny) = spec.last_mut() {
        ny.im = 0.0;
    }
}

/// Shared helper used by tests / capacity reporting.
pub fn total_capacity(
    strategy: &dyn EmbedStrategy,
    num_frames: usize,
    bins: Range<usize>,
    sample_rate: u32,
    strength: f32,
    shaping: Shaping,
    key: &[u8],
) -> usize {
    let ctx = FrameCtx {
        index: 0,
        bins,
        sample_rate,
        strength,
        shaping,
        key: key.to_vec(),
    };
    strategy.capacity_bits(&ctx).saturating_mul(num_frames)
}

pub type StrategyResult<T> = Result<T>;
