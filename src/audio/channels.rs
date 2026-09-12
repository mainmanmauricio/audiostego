//! Channel mapping for embedding / extraction.

use crate::audio::AudioBuffer;
use crate::cli::ChannelMode;
use anyhow::{bail, Result};

/// Working set of mono streams that receive the payload.
#[derive(Debug, Clone)]
pub struct ChannelPlan {
    /// One or two mono buffers to embed into.
    pub work: Vec<Vec<f32>>,
    /// How to reassemble into the output AudioBuffer.
    pub mode: ChannelMode,
    /// Original multi-channel audio (needed for reassembly).
    pub original: AudioBuffer,
}

pub fn prepare_for_embed(audio: &AudioBuffer, mode: &ChannelMode) -> Result<ChannelPlan> {
    if mode.warns_mono_fragile() {
        tracing::warn!(
            "channel mode 'side' is destroyed by mono downmix; prefer 'mid' for robustness"
        );
    }
    let nch = audio.num_channels();
    if nch == 0 {
        bail!("carrier has no channels");
    }

    let work = match mode {
        ChannelMode::Mono => {
            vec![downmix(audio)]
        }
        ChannelMode::Left => {
            vec![audio.clone_channel(0)]
        }
        ChannelMode::Right => {
            if nch < 2 {
                bail!("right channel requested but audio is mono");
            }
            vec![audio.clone_channel(1)]
        }
        ChannelMode::Channel(n) => {
            let idx = *n as usize;
            if idx >= nch {
                bail!("channel:{n} out of range (have {nch} channels)");
            }
            vec![audio.clone_channel(idx)]
        }
        ChannelMode::Mid => {
            if nch == 1 {
                vec![audio.clone_channel(0)]
            } else {
                vec![mid_channel(audio)]
            }
        }
        ChannelMode::Side => {
            if nch < 2 {
                bail!("side mode requires stereo");
            }
            vec![side_channel(audio)]
        }
        ChannelMode::BothMirror | ChannelMode::BothSplit => {
            if nch < 2 {
                // Fall back to mono embed for single-channel sources.
                vec![audio.clone_channel(0)]
            } else {
                vec![audio.clone_channel(0), audio.clone_channel(1)]
            }
        }
    };

    Ok(ChannelPlan {
        work,
        mode: mode.clone(),
        original: audio.clone(),
    })
}

pub fn reassemble(plan: &ChannelPlan, embedded: &[Vec<f32>]) -> Result<AudioBuffer> {
    if embedded.is_empty() {
        bail!("no embedded channels");
    }
    let sr = plan.original.sample_rate;
    let nch = plan.original.num_channels();

    let channels = match &plan.mode {
        ChannelMode::Mono => {
            vec![embedded[0].clone()]
        }
        ChannelMode::Left => {
            let mut chs = plan.original.channels.clone();
            let target = embedded[0].len();
            let pad = target.saturating_sub(chs[0].len());
            if pad > 0 {
                for c in &mut chs {
                    let mut p = vec![0.0f32; pad];
                    p.append(c);
                    *c = p;
                }
            }
            chs[0] = embedded[0].clone();
            for c in &mut chs {
                c.truncate(target);
            }
            chs
        }
        ChannelMode::Right => {
            let mut chs = plan.original.channels.clone();
            let target = embedded[0].len();
            let pad = target.saturating_sub(chs[0].len());
            if pad > 0 {
                for c in &mut chs {
                    let mut p = vec![0.0f32; pad];
                    p.append(c);
                    *c = p;
                }
            }
            chs[1] = embedded[0].clone();
            for c in &mut chs {
                c.truncate(target);
            }
            chs
        }
        ChannelMode::Channel(idx) => {
            let mut chs = plan.original.channels.clone();
            let target = embedded[0].len();
            let pad = target.saturating_sub(chs[0].len());
            if pad > 0 {
                for c in &mut chs {
                    let mut p = vec![0.0f32; pad];
                    p.append(c);
                    *c = p;
                }
            }
            chs[*idx as usize] = embedded[0].clone();
            for c in &mut chs {
                c.truncate(target);
            }
            chs
        }
        ChannelMode::Mid => {
            if nch == 1 {
                vec![embedded[0].clone()]
            } else {
                let side = side_channel(&plan.original);
                let target = embedded[0].len();
                let mut side_p = vec![0.0f32; target.saturating_sub(side.len())];
                side_p.extend_from_slice(&side);
                side_p.truncate(target);
                let mut left = Vec::with_capacity(target);
                let mut right = Vec::with_capacity(target);
                for i in 0..target {
                    let m = embedded[0][i];
                    let s = side_p[i];
                    left.push(m + s);
                    right.push(m - s);
                }
                let mut chs = plan.original.channels.clone();
                // Pad original channels at front if embedded is longer (preamble).
                let pad = target.saturating_sub(chs[0].len());
                if pad > 0 {
                    for c in &mut chs {
                        let mut p = vec![0.0f32; pad];
                        p.append(c);
                        *c = p;
                    }
                }
                chs[0] = left;
                chs[1] = right;
                for c in &mut chs {
                    c.truncate(target);
                }
                chs
            }
        }
        ChannelMode::Side => {
            let mid = mid_channel(&plan.original);
            let target = embedded[0].len();
            let mut mid_p = vec![0.0f32; target.saturating_sub(mid.len())];
            mid_p.extend_from_slice(&mid);
            mid_p.truncate(target);
            let mut left = Vec::with_capacity(target);
            let mut right = Vec::with_capacity(target);
            for i in 0..target {
                let m = mid_p[i];
                let s = embedded[0][i];
                left.push(m + s);
                right.push(m - s);
            }
            let mut chs = plan.original.channels.clone();
            let pad = target.saturating_sub(chs[0].len());
            if pad > 0 {
                for c in &mut chs {
                    let mut p = vec![0.0f32; pad];
                    p.append(c);
                    *c = p;
                }
            }
            chs[0] = left;
            chs[1] = right;
            for c in &mut chs {
                c.truncate(target);
            }
            chs
        }
        ChannelMode::BothMirror | ChannelMode::BothSplit => {
            let mut chs = plan.original.channels.clone();
            if embedded.len() >= 2 && nch >= 2 {
                let n = embedded[0].len().min(embedded[1].len());
                chs[0] = embedded[0][..n].to_vec();
                chs[1] = embedded[1][..n].to_vec();
                for c in &mut chs {
                    c.truncate(n);
                }
            } else {
                chs[0] = embedded[0].clone();
                let n = embedded[0].len();
                for c in &mut chs {
                    c.truncate(n);
                }
            }
            chs
        }
    };

    Ok(AudioBuffer::new(sr, channels))
}

pub fn extract_work_channels(audio: &AudioBuffer, mode: &ChannelMode) -> Result<Vec<Vec<f32>>> {
    prepare_for_embed(audio, mode).map(|p| p.work)
}

fn downmix(audio: &AudioBuffer) -> Vec<f32> {
    let n = audio.num_frames();
    let nch = audio.num_channels() as f32;
    let mut out = vec![0.0f32; n];
    for ch in &audio.channels {
        for i in 0..n {
            out[i] += ch[i];
        }
    }
    for s in &mut out {
        *s /= nch;
    }
    out
}

fn mid_channel(audio: &AudioBuffer) -> Vec<f32> {
    let n = audio.num_frames();
    let mut out = vec![0.0f32; n];
    if audio.num_channels() == 1 {
        return audio.clone_channel(0);
    }
    for i in 0..n {
        out[i] = 0.5 * (audio.channels[0][i] + audio.channels[1][i]);
    }
    out
}

fn side_channel(audio: &AudioBuffer) -> Vec<f32> {
    let n = audio.num_frames();
    let mut out = vec![0.0f32; n];
    for i in 0..n {
        out[i] = 0.5 * (audio.channels[0][i] - audio.channels[1][i]);
    }
    out
}

/// Soft-combine two recovered bitstreams (both-mirror): majority / OR preference for 1 when tied via average.
pub fn combine_mirror_bits(a: &[bool], b: &[bool]) -> Vec<bool> {
    let n = a.len().min(b.len());
    (0..n).map(|i| a[i] || b[i]).collect() // soft-OR; better: use soft scores if available
}

/// Reassemble BothSplit body bits: even indices from channel 0, odd from channel 1.
pub fn interleave_split_bits(even: &[bool], odd: &[bool]) -> Vec<bool> {
    let mut out = Vec::with_capacity(even.len() + odd.len());
    let n = even.len().max(odd.len());
    for i in 0..n {
        if i < even.len() {
            out.push(even[i]);
        }
        if i < odd.len() {
            out.push(odd[i]);
        }
    }
    out
}
