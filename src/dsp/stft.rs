//! Short-time Fourier transform with sqrt-Hann WOLA.

use crate::dsp::window::{cola_weights, rectangular, sqrt_hann};
use anyhow::{bail, Result};
use num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

pub struct StftConfig {
    pub fft_size: usize,
    pub hop: usize,
}

pub struct StftEngine {
    pub config: StftConfig,
    window: Vec<f32>,
    r2c: Arc<dyn RealToComplex<f32>>,
    c2r: Arc<dyn ComplexToReal<f32>>,
}

impl StftEngine {
    pub fn new(fft_size: usize, hop: usize) -> Result<Self> {
        if !fft_size.is_power_of_two() {
            bail!("fft_size must be power of two");
        }
        if hop == 0 || fft_size % hop != 0 {
            bail!("hop must divide fft_size");
        }
        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(fft_size);
        let c2r = planner.plan_fft_inverse(fft_size);
        // Non-overlapping frames use a rectangular window so each block is
        // independent — required for reliable per-frame spectral watermarks.
        // Overlapping hops use sqrt-Hann WOLA.
        let window = if hop == fft_size {
            rectangular(fft_size)
        } else {
            sqrt_hann(fft_size)
        };
        Ok(Self {
            config: StftConfig { fft_size, hop },
            window,
            r2c,
            c2r,
        })
    }

    pub fn spectrum_len(&self) -> usize {
        self.config.fft_size / 2 + 1
    }

    pub fn num_frames(&self, signal_len: usize) -> usize {
        if signal_len < self.config.fft_size {
            return 0;
        }
        1 + (signal_len - self.config.fft_size) / self.config.hop
    }

    /// Analysis: returns frames of complex spectra (length fft/2+1 each).
    pub fn analysis(&self, signal: &[f32]) -> Result<Vec<Vec<Complex<f32>>>> {
        let n = self.config.fft_size;
        let hop = self.config.hop;
        let n_frames = self.num_frames(signal.len());
        let mut frames = Vec::with_capacity(n_frames);
        let mut input = self.r2c.make_input_vec();
        let mut spectrum = self.r2c.make_output_vec();

        for f in 0..n_frames {
            let start = f * hop;
            for i in 0..n {
                input[i] = signal[start + i] * self.window[i];
            }
            self.r2c
                .process(&mut input, &mut spectrum)
                .map_err(|e| anyhow::anyhow!("fft forward: {e}"))?;
            // Zero imag of DC / Nyquist for safety.
            spectrum[0].im = 0.0;
            if let Some(last) = spectrum.last_mut() {
                last.im = 0.0;
            }
            frames.push(spectrum.clone());
        }
        Ok(frames)
    }

    /// Synthesis with WOLA overlap-add and cola normalization.
    pub fn synthesis(&self, frames: &[Vec<Complex<f32>>]) -> Result<Vec<f32>> {
        let n = self.config.fft_size;
        let hop = self.config.hop;
        if frames.is_empty() {
            return Ok(Vec::new());
        }
        let out_len = (frames.len() - 1) * hop + n;
        let mut acc = vec![0.0f32; out_len];
        let weights = cola_weights(&self.window, hop, out_len);
        let mut spectrum = self.c2r.make_input_vec();
        let mut time = self.c2r.make_output_vec();
        let scale = 1.0 / n as f32;

        for (f, frame) in frames.iter().enumerate() {
            if frame.len() != spectrum.len() {
                bail!(
                    "frame {} spectrum length {} != expected {}",
                    f,
                    frame.len(),
                    spectrum.len()
                );
            }
            spectrum.copy_from_slice(frame);
            spectrum[0].im = 0.0;
            if let Some(last) = spectrum.last_mut() {
                last.im = 0.0;
            }
            self.c2r
                .process(&mut spectrum, &mut time)
                .map_err(|e| anyhow::anyhow!("fft inverse: {e}"))?;
            let start = f * hop;
            for i in 0..n {
                acc[start + i] += time[i] * scale * self.window[i];
            }
        }
        for i in 0..out_len {
            acc[i] /= weights[i];
        }
        Ok(acc)
    }
}

/// Peak-normalize so max abs <= 1.0; returns scale factor applied (1.0 if none).
pub fn peak_normalize(signal: &mut [f32], ceiling: f32) -> f32 {
    let peak = signal.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
    if peak <= ceiling || peak < 1e-12 {
        return 1.0;
    }
    let scale = ceiling / peak;
    for s in signal.iter_mut() {
        *s *= scale;
    }
    scale
}
