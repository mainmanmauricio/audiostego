//! ffmpeg subprocess helpers for decode/encode of non-WAV formats.

use crate::audio::decode;
use crate::audio::AudioBuffer;
use crate::cli::OutputFormat;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn require_ffmpeg() -> Result<()> {
    if ffmpeg_available() {
        Ok(())
    } else {
        bail!("ffmpeg not found on PATH (required for non-WAV formats)")
    }
}

/// Decode any format by converting to a temp WAV via ffmpeg, then reading with hound/symphonia.
pub fn decode_via_ffmpeg(path: &Path) -> Result<AudioBuffer> {
    require_ffmpeg()?;
    let tmp = temp_path("astg_dec", "wav");
    let status = Command::new("ffmpeg")
        .args(["-y", "-i"])
        .arg(path)
        .args(["-acodec", "pcm_s16le", "-f", "wav"])
        .arg(&tmp)
        .output()
        .context("spawn ffmpeg decode")?;
    if !status.status.success() {
        let err = String::from_utf8_lossy(&status.stderr);
        let _ = std::fs::remove_file(&tmp);
        bail!("ffmpeg decode failed: {err}");
    }
    let buf = decode::decode_file(&tmp);
    let _ = std::fs::remove_file(&tmp);
    buf
}

pub fn transcode(
    input_wav: &Path,
    output: &Path,
    format: OutputFormat,
    bitrate_kbps: u32,
) -> Result<()> {
    require_ffmpeg()?;
    let br = format!("{bitrate_kbps}k");
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-i"]).arg(input_wav);
    match format {
        OutputFormat::Flac => {
            cmd.args(["-c:a", "flac"]);
        }
        OutputFormat::Mp3 => {
            cmd.args(["-c:a", "libmp3lame", "-b:a", &br]);
        }
        OutputFormat::Opus => {
            // Opus is typically 48 kHz; ffmpeg will resample.
            cmd.args(["-c:a", "libopus", "-b:a", &br, "-ar", "48000"]);
        }
        OutputFormat::Aac => {
            cmd.args(["-c:a", "aac", "-b:a", &br]);
        }
        OutputFormat::Vorbis => {
            cmd.args(["-c:a", "libvorbis", "-b:a", &br]);
        }
        OutputFormat::Wav | OutputFormat::Auto => {
            bail!("transcode called for wav/auto");
        }
    }
    cmd.arg(output);
    let status = cmd.output().context("spawn ffmpeg encode")?;
    if !status.status.success() {
        let err = String::from_utf8_lossy(&status.stderr);
        bail!("ffmpeg encode to {} failed: {err}", format.as_str());
    }
    Ok(())
}

fn temp_path(prefix: &str, ext: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}_{nanos}.{ext}"))
}
