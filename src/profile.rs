//! Profile-driven parameter resolution and strict validation.

use crate::cli::{
    ChannelMode, CommonEmbedParams, EccMode, LossyProfile, OutputFormat, Shaping, StrategyId,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedParams {
    pub strategy: StrategyId,
    pub channel_mode: ChannelMode,
    pub lossy: LossyProfile,
    pub output_format: OutputFormat,
    pub bitrate: u32,
    pub fft_size: usize,
    pub hop: usize,
    pub hop_div: u32,
    pub band_lo_hz: f32,
    pub band_hi_hz: f32,
    pub strength: f32,
    pub shaping: Shaping,
    pub ecc: EccMode,
    pub key_bytes: Vec<u8>,
    pub encrypt: bool,
    pub sample_rate: u32,
    pub upgraded: Vec<String>,
}

impl ResolvedParams {
    pub fn hop_div(&self) -> u32 {
        self.hop_div
    }

    pub fn bin_range(&self) -> (usize, usize) {
        let n = self.fft_size;
        let lo = hz_to_bin(self.band_lo_hz, self.sample_rate, n).max(1);
        let hi = hz_to_bin(self.band_hi_hz, self.sample_rate, n)
            .min(n / 2)
            .max(lo + 1);
        (lo, hi)
    }
}

pub fn hz_to_bin(hz: f32, sample_rate: u32, fft_size: usize) -> usize {
    let bin = (hz * fft_size as f32 / sample_rate as f32).round() as isize;
    bin.max(0) as usize
}

pub fn bin_to_hz(bin: usize, sample_rate: u32, fft_size: usize) -> f32 {
    bin as f32 * sample_rate as f32 / fft_size as f32
}

pub fn resolve_key(key: &Option<String>) -> Result<Vec<u8>> {
    match key {
        None => Ok(b"audiostego-default-key".to_vec()),
        Some(s) if s.starts_with('@') => {
            let path = Path::new(&s[1..]);
            std::fs::read(path).with_context(|| format!("reading key file {}", path.display()))
        }
        Some(s) => Ok(s.as_bytes().to_vec()),
    }
}

pub fn parse_band(s: &str) -> Result<(f32, f32)> {
    let parts: Vec<_> = s.split(':').collect();
    if parts.len() != 2 {
        bail!("band must be LO:HI Hz, got '{s}'");
    }
    let lo: f32 = parts[0].parse().context("band lo")?;
    let hi: f32 = parts[1].parse().context("band hi")?;
    if !(lo < hi) || lo < 0.0 {
        bail!("invalid band {lo}:{hi}");
    }
    Ok((lo, hi))
}

/// Resolve embedding parameters from CLI + carrier sample rate.
pub fn resolve(
    common: &CommonEmbedParams,
    sample_rate: u32,
    output_path: Option<&Path>,
    strict: bool,
) -> Result<ResolvedParams> {
    let mut upgraded = Vec::new();
    let mut strategy = common.strategy;
    let mut fft_size = common.fft_size.unwrap_or(if common.lossy.is_off() {
        2048
    } else {
        4096
    });
    let hop_div = common.hop_div.unwrap_or(if common.lossy.is_off() { 1 } else { 2 });
    if hop_div != 1 && hop_div != 2 && hop_div != 4 {
        bail!("hop_div must be 1, 2, or 4");
    }
    if !fft_size.is_power_of_two() || fft_size < 256 || fft_size > 65536 {
        bail!("fft_size must be a power of two in 256..=65536");
    }
    let hop = fft_size / hop_div as usize;

    let (default_lo, default_hi) = if common.lossy.is_off() {
        (500.0, 12_000.0_f32.min(sample_rate as f32 * 0.45))
    } else {
        (1000.0, 8000.0_f32.min(sample_rate as f32 * 0.45))
    };
    let (band_lo_hz, band_hi_hz) = match &common.band {
        Some(s) => parse_band(s)?,
        None => (default_lo, default_hi),
    };

    let mut strength = common.strength.unwrap_or(if common.lossy.is_off() {
        0.05
    } else {
        0.15
    });

    let ecc = match &common.ecc {
        Some(s) => EccMode::parse(s).map_err(|e| anyhow::anyhow!(e))?,
        None => {
            if common.lossy.is_off() {
                EccMode::Crc
            } else {
                EccMode::ReedSolomon { parity: 16 }
            }
        }
    };

    // Lossy profile retuning
    if !common.lossy.is_off() {
        if strategy != StrategyId::SpreadSpectrum {
            if strict {
                bail!(
                    "strict: strategy {} is fragile under --lossy {}; use spread-spectrum",
                    strategy,
                    common.lossy.as_str()
                );
            }
            upgraded.push(format!(
                "strategy {} -> spread-spectrum for lossy profile",
                strategy
            ));
            strategy = StrategyId::SpreadSpectrum;
        }
        if common.fft_size.is_none() && fft_size < 4096 {
            fft_size = 4096;
            upgraded.push("fft_size raised to 4096 for lossy profile".into());
        }
        if common.strength.is_none() && strength < 0.15 {
            strength = 0.15;
            upgraded.push("strength raised to 0.15 for lossy profile".into());
        }
    }

    let mut output_format = common.output_format;
    if output_format == OutputFormat::Auto {
        output_format = match common.lossy {
            LossyProfile::Off => {
                // Prefer extension of output path when present.
                if let Some(p) = output_path {
                    guess_format_from_path(p).unwrap_or(OutputFormat::Wav)
                } else {
                    OutputFormat::Wav
                }
            }
            LossyProfile::Mp3 => OutputFormat::Mp3,
            LossyProfile::Aac => OutputFormat::Aac,
            LossyProfile::Opus => OutputFormat::Opus,
            LossyProfile::Vorbis => OutputFormat::Vorbis,
        };
    }

    if strict && output_format.is_lossy() && strategy != StrategyId::SpreadSpectrum {
        bail!(
            "strict: refusing {} embedding into lossy {}",
            strategy,
            output_format.as_str()
        );
    }

    if common.encrypt && common.key.is_none() {
        bail!("--encrypt requires --key");
    }

    let key_bytes = resolve_key(&common.key)?;

    Ok(ResolvedParams {
        strategy,
        channel_mode: common.channel_mode.clone(),
        lossy: common.lossy,
        output_format,
        bitrate: common.bitrate,
        fft_size,
        hop,
        hop_div,
        band_lo_hz,
        band_hi_hz,
        strength,
        shaping: common.shaping,
        ecc,
        key_bytes,
        encrypt: common.encrypt,
        sample_rate,
        upgraded,
    })
}

pub fn guess_format_from_path(path: &Path) -> Option<OutputFormat> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("wav") => Some(OutputFormat::Wav),
        Some("flac") => Some(OutputFormat::Flac),
        Some("mp3") => Some(OutputFormat::Mp3),
        Some("opus") => Some(OutputFormat::Opus),
        Some("aac") | Some("m4a") => Some(OutputFormat::Aac),
        Some("ogg") | Some("oga") => Some(OutputFormat::Vorbis),
        _ => None,
    }
}

pub fn load_message(message: &Option<PathBuf>, message_text: &Option<String>) -> Result<Vec<u8>> {
    match (message, message_text) {
        (Some(path), None) => {
            std::fs::read(path).with_context(|| format!("reading message {}", path.display()))
        }
        (None, Some(text)) => Ok(text.as_bytes().to_vec()),
        (None, None) => bail!("provide --message or --message-text"),
        (Some(_), Some(_)) => bail!("provide only one of --message or --message-text"),
    }
}
