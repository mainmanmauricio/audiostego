//! Decode audio to planar f32 via symphonia, with ffmpeg fallback.

use crate::audio::ffmpeg;
use crate::audio::AudioBuffer;
use anyhow::{bail, Context, Result};
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub fn decode_file(path: &Path) -> Result<AudioBuffer> {
    match decode_symphonia(path) {
        Ok(buf) => Ok(buf),
        Err(e) => {
            tracing::warn!(
                "symphonia decode failed for {}: {e:#}; trying ffmpeg",
                path.display()
            );
            ffmpeg::decode_via_ffmpeg(path)
        }
    }
}

fn decode_symphonia(path: &Path) -> Result<AudioBuffer> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .context("probe audio format")?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .context("no supported audio track")?
        .clone();
    let sample_rate = track
        .codec_params
        .sample_rate
        .context("missing sample rate")?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("create decoder")?;

    let mut channels: Vec<Vec<f32>> = Vec::new();
    let track_id = track.id;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(SymError::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => return Err(e).context("read packet"),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => append_decoded(&mut channels, &decoded)?,
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(e).context("decode packet"),
        }
    }

    if channels.is_empty() {
        bail!("no audio samples decoded from {}", path.display());
    }
    let mut buf = AudioBuffer::new(sample_rate, channels);
    buf.ensure_equal_lengths();
    Ok(buf)
}

fn append_decoded(channels: &mut Vec<Vec<f32>>, decoded: &AudioBufferRef<'_>) -> Result<()> {
    let spec_channels = decoded.spec().channels.count();
    if channels.is_empty() {
        *channels = vec![Vec::new(); spec_channels];
    }
    if channels.len() != spec_channels {
        bail!(
            "channel count changed mid-stream ({} -> {})",
            channels.len(),
            spec_channels
        );
    }

    match decoded {
        AudioBufferRef::F32(buf) => {
            for ch in 0..spec_channels {
                channels[ch].extend_from_slice(buf.chan(ch));
            }
        }
        AudioBufferRef::U8(buf) => convert_int(channels, buf, |s| (s as f32 - 128.0) / 128.0),
        AudioBufferRef::U16(buf) => {
            convert_int(channels, buf, |s| (s as f32 / 65535.0) * 2.0 - 1.0)
        }
        AudioBufferRef::U24(buf) => {
            convert_int(channels, buf, |s| {
                let v = s.inner() as f32;
                (v / 8_388_607.0) * 2.0 - 1.0
            })
        }
        AudioBufferRef::U32(buf) => {
            convert_int(channels, buf, |s| (s as f32 / u32::MAX as f32) * 2.0 - 1.0)
        }
        AudioBufferRef::S8(buf) => convert_int(channels, buf, |s| s as f32 / 128.0),
        AudioBufferRef::S16(buf) => convert_int(channels, buf, |s| s as f32 / 32768.0),
        AudioBufferRef::S24(buf) => {
            convert_int(channels, buf, |s| s.inner() as f32 / 8_388_608.0)
        }
        AudioBufferRef::S32(buf) => convert_int(channels, buf, |s| s as f32 / 2_147_483_648.0),
        AudioBufferRef::F64(buf) => {
            for ch in 0..spec_channels {
                channels[ch].extend(buf.chan(ch).iter().map(|s| *s as f32));
            }
        }
    }
    Ok(())
}

fn convert_int<S, F>(
    channels: &mut [Vec<f32>],
    buf: &symphonia::core::audio::AudioBuffer<S>,
    f: F,
) where
    S: Copy + symphonia::core::sample::Sample,
    F: Fn(S) -> f32,
{
    let n = buf.spec().channels.count();
    for ch in 0..n {
        channels[ch].extend(buf.chan(ch).iter().copied().map(&f));
    }
}
