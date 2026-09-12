//! Audio I/O: decode, encode, resample, channel mapping, ffmpeg helpers.

pub mod channels;
pub mod decode;
pub mod encode;
pub mod ffmpeg;
pub mod resample;

use serde::{Deserialize, Serialize};

/// Planar float PCM audio buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioBuffer {
    pub sample_rate: u32,
    /// channels[ch][sample]
    pub channels: Vec<Vec<f32>>,
}

impl AudioBuffer {
    pub fn new(sample_rate: u32, channels: Vec<Vec<f32>>) -> Self {
        Self {
            sample_rate,
            channels,
        }
    }

    pub fn num_channels(&self) -> usize {
        self.channels.len()
    }

    pub fn num_frames(&self) -> usize {
        self.channels.first().map(|c| c.len()).unwrap_or(0)
    }

    pub fn ensure_equal_lengths(&mut self) {
        if self.channels.is_empty() {
            return;
        }
        let n = self.channels.iter().map(|c| c.len()).min().unwrap_or(0);
        for c in &mut self.channels {
            c.truncate(n);
        }
    }

    pub fn peak(&self) -> f32 {
        self.channels
            .iter()
            .flat_map(|c| c.iter())
            .map(|s| s.abs())
            .fold(0.0_f32, f32::max)
    }

    pub fn scale(&mut self, factor: f32) {
        for ch in &mut self.channels {
            for s in ch {
                *s *= factor;
            }
        }
    }

    pub fn clone_channel(&self, idx: usize) -> Vec<f32> {
        self.channels
            .get(idx)
            .cloned()
            .unwrap_or_default()
    }
}
