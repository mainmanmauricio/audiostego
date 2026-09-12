//! Shared fixtures for integration tests.
//!
//! Each `tests/*.rs` binary includes this with `mod common;`.
//! Individual crates may not use every helper; allow dead_code across inclusions.

#![allow(dead_code)]

use audiostego::audio::{decode, encode, AudioBuffer};
use audiostego::cli::{
    ChannelMode, CommonEmbedParams, EmbedArgs, ExtractArgs, LossyProfile, OutputFormat, Shaping,
    StrategyId,
};
use audiostego::engine::{embed, extract};
use std::path::{Path, PathBuf};

pub fn tone_stereo(seconds: f32, sr: u32) -> AudioBuffer {
    let n = (seconds * sr as f32) as usize;
    let mut left = vec![0.0f32; n];
    let mut right = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f32 / sr as f32;
        left[i] = 0.4 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            + 0.2 * (2.0 * std::f32::consts::PI * 660.0 * t).sin();
        right[i] = 0.35 * (2.0 * std::f32::consts::PI * 550.0 * t).sin()
            + 0.15 * (2.0 * std::f32::consts::PI * 770.0 * t).sin();
    }
    AudioBuffer::new(sr, vec![left, right])
}

pub fn tone_mono(seconds: f32, sr: u32) -> AudioBuffer {
    let n = (seconds * sr as f32) as usize;
    let mut samples = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f32 / sr as f32;
        samples[i] = 0.45 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            + 0.2 * (2.0 * std::f32::consts::PI * 1000.0 * t).sin()
            + 0.1 * (2.0 * std::f32::consts::PI * 2500.0 * t).sin();
    }
    AudioBuffer::new(sr, vec![samples])
}

/// Music-like stereo: chords + light noise + transients + L/R detune.
pub fn music_like_stereo(seconds: f32, sr: u32) -> AudioBuffer {
    let n = (seconds * sr as f32) as usize;
    let mut left = vec![0.0f32; n];
    let mut right = vec![0.0f32; n];
    // A minor-ish chord tones (Hz).
    let freqs_l = [220.0_f32, 261.63, 329.63, 440.0, 523.25];
    let freqs_r = [221.5_f32, 262.5, 328.0, 442.0, 520.0];
    let amps = [0.22_f32, 0.18, 0.14, 0.10, 0.06];
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let mut l = 0.0f32;
        let mut r = 0.0f32;
        for ((&fl, &fr), &a) in freqs_l.iter().zip(freqs_r.iter()).zip(amps.iter()) {
            l += a * (2.0 * std::f32::consts::PI * fl * t).sin();
            r += a * (2.0 * std::f32::consts::PI * fr * t).sin();
        }
        // Soft noise
        let noise =
            ((i.wrapping_mul(1103515245).wrapping_add(12345) >> 16) as f32 / 32768.0 - 1.0) * 0.02;
        l += noise;
        r += noise * 0.9;
        // Periodic transient click every ~0.5 s
        if i % (sr as usize / 2) < 40 {
            let env = 1.0 - (i % (sr as usize / 2)) as f32 / 40.0;
            l += 0.15 * env;
            r += 0.12 * env;
        }
        left[i] = l.clamp(-0.95, 0.95);
        right[i] = r.clamp(-0.95, 0.95);
    }
    AudioBuffer::new(sr, vec![left, right])
}

/// Path to the optional vendored CC0 clip, if present on disk.
pub fn testdata_music_path() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/music/carrier-music.flac");
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

/// Prefer the vendored FLAC; fall back to a generated music-like buffer.
pub fn music_carrier() -> AudioBuffer {
    if let Some(path) = testdata_music_path() {
        match decode::decode_file(&path) {
            Ok(buf) => return buf,
            Err(e) => eprintln!(
                "testdata music decode failed ({}), using generated carrier",
                e
            ),
        }
    }
    music_like_stereo(8.0, 44100)
}

/// Existing suites: explicit band / strength / hop (not capsule-only safe).
pub fn base_common(strategy: StrategyId, channel: ChannelMode) -> CommonEmbedParams {
    CommonEmbedParams {
        strategy,
        channel_mode: channel,
        lossy: LossyProfile::Off,
        output_format: OutputFormat::Wav,
        bitrate: 192,
        fft_size: Some(2048),
        hop_div: Some(1),
        band: Some("1000:6000".into()),
        strength: Some(0.12),
        shaping: Shaping::Masked,
        ecc: Some("crc".into()),
        key: Some("test-key-42".into()),
        encrypt: false,
    }
}

/// True lossless `profile::resolve` defaults (band/fft/hop/strength unset).
/// Use for capsule-only extract tests.
pub fn default_common(strategy: StrategyId, channel: ChannelMode) -> CommonEmbedParams {
    CommonEmbedParams {
        strategy,
        channel_mode: channel,
        lossy: LossyProfile::Off,
        output_format: OutputFormat::Wav,
        bitrate: 192,
        fft_size: None,
        hop_div: None,
        band: None,
        strength: None,
        shaping: Shaping::Masked,
        ecc: Some("crc".into()),
        key: Some("test-key-42".into()),
        encrypt: false,
    }
}

/// Embed without sidecar; extract without sidecar or STFT overrides.
pub fn extract_capsule_only(
    carrier: &AudioBuffer,
    message: &[u8],
    common: &CommonEmbedParams,
    work: &Path,
) -> anyhow::Result<(Vec<u8>, audiostego::engine::ExtractReport)> {
    std::fs::create_dir_all(work)?;
    let input = work.join("carrier.wav");
    let output = work.join("loaded.wav");
    let msg = work.join("msg.bin");
    let out_msg = work.join("out.bin");
    encode::encode_wav(&input, carrier)?;
    std::fs::write(&msg, message)?;
    embed(&EmbedArgs {
        input: input.clone(),
        message: Some(msg),
        message_text: None,
        output: output.clone(),
        common: CommonEmbedParams {
            output_format: OutputFormat::Wav,
            ..common.clone()
        },
        sidecar: None,
        report: None,
        dry_run: false,
        strict: false,
    })?;
    extract(&ExtractArgs {
        input: output,
        original: if common.strategy.requires_original() {
            Some(input)
        } else {
            None
        },
        output: out_msg,
        sidecar: None,
        max_offset: 8192,
        key: common.key.clone(),
        strategy: None,
        channel_mode: None,
        fft_size: None,
        hop_div: None,
        band: None,
        strength: None,
        report: None,
    })
}
