//! Encode planar f32 audio to disk.

use crate::audio::ffmpeg;
use crate::audio::AudioBuffer;
use crate::cli::OutputFormat;
use anyhow::{bail, Context, Result};
use hound::{SampleFormat, WavSpec, WavWriter};
use std::path::Path;

pub fn encode_file(
    path: &Path,
    audio: &AudioBuffer,
    format: OutputFormat,
    bitrate_kbps: u32,
) -> Result<()> {
    match format {
        OutputFormat::Auto | OutputFormat::Wav => encode_wav(path, audio),
        OutputFormat::Flac
        | OutputFormat::Mp3
        | OutputFormat::Opus
        | OutputFormat::Aac
        | OutputFormat::Vorbis => {
            let tmp = path.with_extension("tmp.wav");
            encode_wav(&tmp, audio)?;
            let res = ffmpeg::transcode(&tmp, path, format, bitrate_kbps);
            let _ = std::fs::remove_file(&tmp);
            res
        }
    }
}

/// Write IEEE float32 WAV to avoid quantizing away spectral watermarks.
pub fn encode_wav(path: &Path, audio: &AudioBuffer) -> Result<()> {
    if audio.channels.is_empty() {
        bail!("no channels to encode");
    }
    let channels = audio.num_channels() as u16;
    let spec = WavSpec {
        channels,
        sample_rate: audio.sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer =
        WavWriter::create(path, spec).with_context(|| format!("create wav {}", path.display()))?;
    let n = audio.num_frames();
    for i in 0..n {
        for ch in 0..audio.num_channels() {
            let s = audio.channels[ch][i].clamp(-1.0, 1.0);
            writer.write_sample(s)?;
        }
    }
    writer.finalize().context("finalize wav")?;
    Ok(())
}
