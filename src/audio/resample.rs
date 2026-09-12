//! Sample-rate conversion via rubato.

use crate::audio::AudioBuffer;
use anyhow::{Context, Result};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

pub fn resample(audio: &AudioBuffer, target_rate: u32) -> Result<AudioBuffer> {
    if audio.sample_rate == target_rate || audio.channels.is_empty() {
        return Ok(audio.clone());
    }
    let ratio = target_rate as f64 / audio.sample_rate as f64;
    let params = SincInterpolationParameters {
        sinc_len: 64,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    let chunk = 1024usize;
    let mut resampler = SincFixedIn::<f32>::new(ratio, 2.0, params, chunk, audio.num_channels())
        .context("create resampler")?;

    let n = audio.num_frames();
    let mut pos = 0usize;
    let mut out_ch: Vec<Vec<f32>> = vec![Vec::new(); audio.num_channels()];

    while pos + chunk <= n {
        let waves: Vec<&[f32]> = audio
            .channels
            .iter()
            .map(|c| &c[pos..pos + chunk])
            .collect();
        let owned: Vec<Vec<f32>> = waves.iter().map(|s| s.to_vec()).collect();
        let result = resampler.process(&owned, None).context("resample chunk")?;
        for (ch, data) in result.into_iter().enumerate() {
            out_ch[ch].extend(data);
        }
        pos += chunk;
    }

    // Flush remaining with zero-pad.
    if pos < n {
        let rem = n - pos;
        let owned: Vec<Vec<f32>> = audio
            .channels
            .iter()
            .map(|c| {
                let mut v = c[pos..].to_vec();
                v.resize(chunk, 0.0);
                v
            })
            .collect();
        // Only first `rem` samples are real; after process, trim proportionally.
        let result = resampler.process(&owned, None).context("resample tail")?;
        let keep = ((rem as f64) * ratio).round() as usize;
        for (ch, data) in result.into_iter().enumerate() {
            let end = keep.min(data.len());
            out_ch[ch].extend_from_slice(&data[..end]);
        }
    }

    let mut buf = AudioBuffer::new(target_rate, out_ch);
    buf.ensure_equal_lengths();
    Ok(buf)
}
