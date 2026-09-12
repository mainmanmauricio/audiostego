//! Embed / extract / verify / info orchestration.

use crate::audio::{channels, decode, encode, resample, AudioBuffer};
use crate::cli::{
    ChannelMode, CommonEmbedParams, EmbedArgs, ExtractArgs, InfoArgs, StrategyId, VerifyArgs,
};
use crate::dsp::{metrics, stft, sync};
use crate::payload::{alloc::BitAllocation, bits, capsule, capsule::CapsuleHeader};
use crate::profile::{self, ResolvedParams};
use crate::strategy::{self, EmbedStrategy, FrameCtx};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Extra STFT frames between capsule header and payload to absorb WOLA bleed.
const GUARD_FRAMES: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedReport {
    pub strategy: String,
    pub channel_mode: String,
    pub fft_size: usize,
    pub hop: usize,
    pub band_lo_hz: f32,
    pub band_hi_hz: f32,
    pub strength: f32,
    pub ecc: String,
    pub output_format: String,
    pub message_bytes: usize,
    pub body_bytes: usize,
    pub capacity_bits: usize,
    pub used_bits: usize,
    pub scale_factor: f32,
    pub snr_db: f32,
    pub segmental_snr_db: f32,
    pub peak_abs_diff: f32,
    pub upgraded: Vec<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractReport {
    pub strategy: String,
    pub message_bytes: usize,
    pub sync_offset: usize,
    pub sync_score: f32,
    pub scale_factor: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoReport {
    pub sample_rate: u32,
    pub channels: usize,
    pub frames: usize,
    pub duration_secs: f32,
    pub strategy: String,
    pub capacity_bits: usize,
    pub capacity_bytes: usize,
    pub header_bits: usize,
    pub message_bytes: Option<usize>,
    pub fits: Option<bool>,
    pub params: ResolvedParamsPublic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedParamsPublic {
    pub strategy: String,
    pub channel_mode: String,
    pub lossy: String,
    pub output_format: String,
    pub bitrate: u32,
    pub fft_size: usize,
    pub hop: usize,
    pub band_lo_hz: f32,
    pub band_hi_hz: f32,
    pub strength: f32,
    pub shaping: String,
    pub ecc: String,
    pub encrypt: bool,
    pub upgraded: Vec<String>,
}

impl From<&ResolvedParams> for ResolvedParamsPublic {
    fn from(p: &ResolvedParams) -> Self {
        Self {
            strategy: p.strategy.to_string(),
            channel_mode: p.channel_mode.as_str(),
            lossy: p.lossy.as_str().into(),
            output_format: p.output_format.as_str().into(),
            bitrate: p.bitrate,
            fft_size: p.fft_size,
            hop: p.hop,
            band_lo_hz: p.band_lo_hz,
            band_hi_hz: p.band_hi_hz,
            strength: p.strength,
            shaping: format!("{:?}", p.shaping).to_ascii_lowercase(),
            ecc: p.ecc.as_str(),
            encrypt: p.encrypt,
            upgraded: p.upgraded.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyReport {
    pub embed: EmbedReport,
    pub extract_ok: bool,
    pub ber: f32,
    pub recovered_bytes: usize,
    pub expected_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sidecar {
    pub params: ResolvedParamsPublic,
    pub scale_factor: f32,
    pub message_bytes: usize,
    pub body_bytes: usize,
    pub sync_preamble_len: usize,
}

pub fn embed(args: &EmbedArgs) -> Result<EmbedReport> {
    let message = profile::load_message(&args.message, &args.message_text)?;
    let carrier = decode::decode_file(&args.input)?;
    let mut params = profile::resolve(
        &args.common,
        carrier.sample_rate,
        Some(&args.output),
        args.strict,
    )?;

    if params.channel_mode.warns_mono_fragile() {
        tracing::warn!("side channel mode is fragile under mono downmix");
    }

    let engine = stft::StftEngine::new(params.fft_size, params.hop)?;
    let plan = channels::prepare_for_embed(&carrier, &params.channel_mode)?;
    let (bin_lo, bin_hi) = params.bin_range();
    let strat = strategy::make_strategy(params.strategy);

    // Capacity based on first work channel frame count.
    let n_frames = engine.num_frames(plan.work[0].len());
    if n_frames == 0 {
        bail!("carrier too short for FFT size {}", params.fft_size);
    }
    let ctx0 = FrameCtx {
        index: 0,
        bins: bin_lo..bin_hi,
        sample_rate: params.sample_rate,
        strength: params.strength,
        shaping: params.shaping,
        key: params.key_bytes.clone(),
    };
    let bits_per_frame = strat.capacity_bits(&ctx0);
    if bits_per_frame == 0 {
        bail!("zero capacity with current band/strategy");
    }

    // Reserve header frames at the start (QIM capsule header).
    // Capsule header uses QIM (high capacity, reliable under lossless STFT).
    // Spread-spectrum remains available for the payload body under --lossy.
    let header_strat = strategy::make_strategy(StrategyId::Qim);
    let header_bpf = header_strat.capacity_bits(&ctx0).max(1);
    let header_bits = capsule::HEADER_BYTES * 8;
    let header_frames = (header_bits + header_bpf - 1) / header_bpf;
    let body_start = header_frames + GUARD_FRAMES;
    let body_frames = n_frames.saturating_sub(body_start);
    if body_frames == 0 {
        bail!("not enough frames for capsule header + guard");
    }

    let body = capsule::build_body(&params, &message)?;
    let body_bit_vec = bits::bytes_to_bools(&body);
    let capacity_bits = bits_per_frame * body_frames;
    // For both-split, roughly double.
    let capacity_bits = match params.channel_mode {
        ChannelMode::BothSplit if plan.work.len() >= 2 => capacity_bits * 2,
        _ => capacity_bits,
    };

    if body_bit_vec.len() > capacity_bits {
        bail!(
            "message+ECC needs {} bits but capacity is {} bits",
            body_bit_vec.len(),
            capacity_bits
        );
    }

    let report_base = |scale: f32, snr: metrics::AudioMetrics, dry: bool| EmbedReport {
        strategy: params.strategy.to_string(),
        channel_mode: params.channel_mode.as_str(),
        fft_size: params.fft_size,
        hop: params.hop,
        band_lo_hz: params.band_lo_hz,
        band_hi_hz: params.band_hi_hz,
        strength: params.strength,
        ecc: params.ecc.as_str(),
        output_format: params.output_format.as_str().into(),
        message_bytes: message.len(),
        body_bytes: body.len(),
        capacity_bits,
        used_bits: body_bit_vec.len(),
        scale_factor: scale,
        snr_db: snr.snr_db,
        segmental_snr_db: snr.segmental_snr_db,
        peak_abs_diff: snr.peak_abs_diff,
        upgraded: params.upgraded.clone(),
        dry_run: dry,
    };

    if args.dry_run {
        let snr = metrics::AudioMetrics {
            snr_db: 0.0,
            segmental_snr_db: 0.0,
            peak_abs_diff: 0.0,
        };
        let report = report_base(1.0, snr, true);
        write_optional_json(&args.report, &report)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(report);
    }

    // Placeholder scale; real scale applied after synthesis, then we must re-embed header
    // with the final scale. Two-pass: embed with scale=1, synthesize, normalize, then
    // patch header frames with correct scale.
    let mut scale_factor = 1.0f32;

    let mut embedded_work = Vec::new();
    for (ch_i, mono) in plan.work.iter().enumerate() {
        let frames = engine.analysis(mono)?;
        let mut frames = frames;

        // Split body bits across channels for BothSplit.
        let channel_body_bits: Vec<bool> = match params.channel_mode {
            ChannelMode::BothSplit if plan.work.len() >= 2 => body_bit_vec
                .iter()
                .enumerate()
                .filter(|(i, _)| i % 2 == ch_i)
                .map(|(_, b)| *b)
                .collect(),
            _ => body_bit_vec.clone(),
        };

        let bpf = bits_per_frame;
        let alloc =
            BitAllocation::plan(body_frames, bpf, channel_body_bits.len(), &params.key_bytes);

        // Embed header in first header_frames using spread-spectrum.
        // scale patched later — first pass uses 1.0
        let header = CapsuleHeader::from_params(&params, body.len() as u32, scale_factor);
        let mut hbits = capsule::header_bits(&header);
        // Pad header bits to fill header frames.
        hbits.resize(header_frames * header_bpf, false);

        for f in 0..header_frames {
            let ctx = FrameCtx {
                index: f,
                bins: bin_lo..bin_hi,
                sample_rate: params.sample_rate,
                strength: params.strength.max(0.2),
                shaping: params.shaping,
                key: params.key_bytes.clone(),
            };
            let start = f * header_bpf;
            let end = (start + header_bpf).min(hbits.len());
            header_strat.embed_frame(&mut frames[f], &hbits[start..end], &ctx);
        }

        // Embed body after guard frames.
        for f in 0..body_frames {
            let frame_idx = body_start + f;
            let ctx = FrameCtx {
                index: frame_idx,
                bins: bin_lo..bin_hi,
                sample_rate: params.sample_rate,
                strength: params.strength,
                shaping: params.shaping,
                key: params.key_bytes.clone(),
            };
            let mut frame_bits = vec![false; bpf];
            for (slot, &bit_idx) in alloc.frame_bits[f].iter().enumerate() {
                if slot < bpf && bit_idx < channel_body_bits.len() {
                    frame_bits[slot] = channel_body_bits[bit_idx];
                }
            }
            strat.embed_frame(&mut frames[frame_idx], &frame_bits, &ctx);
        }

        let mut synthesized = engine.synthesis(&frames)?;
        // Match original length (WOLA edges already covered by hop alignment).
        if synthesized.len() > mono.len() {
            synthesized.truncate(mono.len());
        } else if synthesized.len() < mono.len() {
            synthesized.extend_from_slice(&mono[synthesized.len()..]);
        }
        let peak = synthesized.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
        let ch_scale = if peak > 1.0 { 0.99 / peak } else { 1.0 };
        if ch_scale != 1.0 {
            scale_factor = ch_scale; // last channel wins; typically same
            for s in &mut synthesized {
                *s *= ch_scale;
            }
        }
        // Prepend keyed preamble (not mixed) for fast sync under leading delay.
        // Amplitude carries scale_factor so capsule-only extract can unscale
        // before reading the QIM header (chicken-and-egg otherwise).
        let pre_len = sync::default_preamble_len(params.fft_size.min(1024));
        let preamble = sync::preamble_samples(&params.key_bytes, pre_len);
        let mut with_pre = Vec::with_capacity(pre_len + synthesized.len());
        with_pre.extend(preamble.iter().map(|s| s * 0.25 * scale_factor));
        with_pre.extend_from_slice(&synthesized);
        embedded_work.push(with_pre);
    }

    // If we scaled, re-stamp header with the recorded scale (single channel re-embed is enough
    // for capsule correctness when all channels share params; full re-embed for safety).
    if (scale_factor - 1.0).abs() > 1e-6 {
        embedded_work.clear();
        for (ch_i, mono) in plan.work.iter().enumerate() {
            let mut frames = engine.analysis(mono)?;
            let channel_body_bits: Vec<bool> = match params.channel_mode {
                ChannelMode::BothSplit if plan.work.len() >= 2 => body_bit_vec
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| i % 2 == ch_i)
                    .map(|(_, b)| *b)
                    .collect(),
                _ => body_bit_vec.clone(),
            };
            let alloc = BitAllocation::plan(
                body_frames,
                bits_per_frame,
                channel_body_bits.len(),
                &params.key_bytes,
            );
            let header = CapsuleHeader::from_params(&params, body.len() as u32, scale_factor);
            let mut hbits = capsule::header_bits(&header);
            hbits.resize(header_frames * header_bpf, false);
            for f in 0..header_frames {
                let ctx = FrameCtx {
                    index: f,
                    bins: bin_lo..bin_hi,
                    sample_rate: params.sample_rate,
                    strength: params.strength.max(0.2),
                    shaping: params.shaping,
                    key: params.key_bytes.clone(),
                };
                let start = f * header_bpf;
                let end = (start + header_bpf).min(hbits.len());
                header_strat.embed_frame(&mut frames[f], &hbits[start..end], &ctx);
            }
            for f in 0..body_frames {
                let frame_idx = body_start + f;
                let ctx = FrameCtx {
                    index: frame_idx,
                    bins: bin_lo..bin_hi,
                    sample_rate: params.sample_rate,
                    strength: params.strength,
                    shaping: params.shaping,
                    key: params.key_bytes.clone(),
                };
                let mut frame_bits = vec![false; bits_per_frame];
                for (slot, &bit_idx) in alloc.frame_bits[f].iter().enumerate() {
                    if slot < bits_per_frame && bit_idx < channel_body_bits.len() {
                        frame_bits[slot] = channel_body_bits[bit_idx];
                    }
                }
                strat.embed_frame(&mut frames[frame_idx], &frame_bits, &ctx);
            }
            let mut synthesized = engine.synthesis(&frames)?;
            if synthesized.len() > mono.len() {
                synthesized.truncate(mono.len());
            } else if synthesized.len() < mono.len() {
                synthesized.extend_from_slice(&mono[synthesized.len()..]);
            }
            for s in &mut synthesized {
                *s *= scale_factor;
            }
            let pre_len = sync::default_preamble_len(params.fft_size.min(1024));
            let preamble = sync::preamble_samples(&params.key_bytes, pre_len);
            let mut with_pre = Vec::with_capacity(pre_len + synthesized.len());
            with_pre.extend(preamble.iter().map(|s| s * 0.25 * scale_factor));
            with_pre.extend_from_slice(&synthesized);
            embedded_work.push(with_pre);
        }
    }

    let stego = channels::reassemble(&plan, &embedded_work)?;
    let snr = {
        let pre_len = sync::default_preamble_len(params.fft_size.min(1024));
        let emb = &embedded_work[0];
        let payload = if emb.len() > pre_len {
            &emb[pre_len..]
        } else {
            emb.as_slice()
        };
        let n = plan.work[0].len().min(payload.len());
        metrics::compute_metrics(&plan.work[0][..n], &payload[..n])
    };

    encode::encode_file(&args.output, &stego, params.output_format, params.bitrate)?;

    let report = report_base(scale_factor, snr, false);
    write_optional_json(&args.report, &report)?;

    if let Some(side) = &args.sidecar {
        let sc = Sidecar {
            params: ResolvedParamsPublic::from(&params),
            scale_factor,
            message_bytes: message.len(),
            body_bytes: body.len(),
            sync_preamble_len: sync::default_preamble_len(params.fft_size.min(1024)),
        };
        write_optional_json(&Some(side.clone()), &sc)?;
    }

    params.sample_rate = carrier.sample_rate;
    Ok(report)
}

pub fn extract(args: &ExtractArgs) -> Result<(Vec<u8>, ExtractReport)> {
    let stego = decode::decode_file(&args.input)?;
    let key = profile::resolve_key(&args.key)?;

    // Load sidecar if present to restore params.
    let sidecar: Option<Sidecar> = match &args.sidecar {
        Some(p) => Some(serde_json::from_str(&std::fs::read_to_string(p)?)?),
        None => None,
    };

    let mut strategy = args
        .strategy
        .or_else(|| sidecar.as_ref().map(|s| parse_strategy(&s.params.strategy)))
        .unwrap_or(StrategyId::Qim);
    let mut channel_mode = args
        .channel_mode
        .clone()
        .or_else(|| {
            sidecar
                .as_ref()
                .and_then(|s| s.params.channel_mode.parse().ok())
        })
        .unwrap_or(ChannelMode::Mid);
    // Track whether STFT knobs came from CLI/sidecar so we may retry the
    // lossy resolve preset when the capsule is missing and nothing was set.
    let fft_from_user =
        args.fft_size.is_some() || sidecar.as_ref().map(|s| s.params.fft_size).is_some();
    let hop_from_user = args.hop_div.is_some()
        || sidecar
            .as_ref()
            .map(|s| s.params.fft_size / s.params.hop)
            .is_some();
    let band_from_user = args.band.is_some()
        || sidecar
            .as_ref()
            .map(|s| (s.params.band_lo_hz, s.params.band_hi_hz))
            .is_some();
    let stft_unset = !fft_from_user && !hop_from_user && !band_from_user;

    let mut fft_size = args
        .fft_size
        .or_else(|| sidecar.as_ref().map(|s| s.params.fft_size))
        .unwrap_or(2048);
    // Match lossless `profile::resolve` (hop_div=1). Previous default of 2
    // broke capsule-only extract for the documented lossless path.
    let mut hop_div = args
        .hop_div
        .or_else(|| {
            sidecar
                .as_ref()
                .map(|s| (s.params.fft_size / s.params.hop) as u32)
        })
        .unwrap_or(1);
    let mut strength = args
        .strength
        .or_else(|| sidecar.as_ref().map(|s| s.params.strength))
        .unwrap_or(0.05);
    let mut sample_rate = stego.sample_rate;
    let (mut band_lo, mut band_hi) = match &args.band {
        Some(s) => profile::parse_band(s)?,
        None => sidecar
            .as_ref()
            .map(|s| (s.params.band_lo_hz, s.params.band_hi_hz))
            .unwrap_or((500.0, 12_000.0_f32.min(sample_rate as f32 * 0.45))),
    };
    let mut scale_factor = sidecar.as_ref().map(|s| s.scale_factor).unwrap_or(1.0);
    let scale_from_sidecar = sidecar.as_ref().map(|s| s.scale_factor).is_some();
    // Capsule overwrites these when the QIM header survives. After lossy
    // codecs it often does not, so sidecar must restore ECC/encrypt/shaping.
    let mut ecc = sidecar
        .as_ref()
        .and_then(|s| crate::cli::EccMode::parse(&s.params.ecc).ok())
        .unwrap_or(crate::cli::EccMode::Crc);
    let mut encrypt = sidecar.as_ref().map(|s| s.params.encrypt).unwrap_or(false);
    let mut payload_len: Option<usize> = sidecar.as_ref().map(|s| s.body_bytes);
    let shaping = sidecar
        .as_ref()
        .map(|s| match s.params.shaping.as_str() {
            "fixed" => crate::cli::Shaping::Fixed,
            _ => crate::cli::Shaping::Masked,
        })
        .unwrap_or(crate::cli::Shaping::Masked);

    // Sync via keyed preamble correlation, then skip the preamble itself.
    let mut work = channels::extract_work_channels(&stego, &channel_mode)?;
    let mut mono = work[0].clone();

    // If no sidecar scale, estimate from preamble amplitude (±0.25 * scale).
    if !scale_from_sidecar {
        let pre_len_guess = sync::default_preamble_len(fft_size.min(1024));
        let preamble_guess = sync::preamble_samples(&key, pre_len_guess);
        let (off_guess, _) = sync::find_offset(&mono, &preamble_guess, args.max_offset)?;
        if off_guess + pre_len_guess <= mono.len() {
            let region = &mono[off_guess..off_guess + pre_len_guess];
            let mean_abs =
                region.iter().map(|s| s.abs()).sum::<f32>() / pre_len_guess.max(1) as f32;
            let guessed = mean_abs / 0.25;
            if guessed > 0.05 && guessed <= 1.05 {
                scale_factor = guessed;
            }
        }
    }

    if scale_factor > 0.0 && (scale_factor - 1.0).abs() > 1e-6 {
        for s in &mut mono {
            *s /= scale_factor;
        }
    }

    let pre_len = sidecar
        .as_ref()
        .map(|s| s.sync_preamble_len)
        .filter(|n| *n > 0)
        .unwrap_or_else(|| sync::default_preamble_len(fft_size.min(1024)));
    let preamble = sync::preamble_samples(&key, pre_len);
    let (pre_off, sync_score) = sync::find_offset(&mono, &preamble, args.max_offset)?;
    let mut payload_start = pre_off + pre_len;
    let offset = payload_start;
    if payload_start < mono.len() {
        mono = mono[payload_start..].to_vec();
    }

    let mono_after_sync = mono.clone();
    let mut engine = stft::StftEngine::new(fft_size, fft_size / hop_div as usize)?;
    let mut frames = engine.analysis(&mono)?;
    let (mut bin_lo, mut bin_hi) = {
        let lo = profile::hz_to_bin(band_lo, sample_rate, fft_size).max(1);
        let hi = profile::hz_to_bin(band_hi, sample_rate, fft_size)
            .min(fft_size / 2)
            .max(lo + 1);
        (lo, hi)
    };

    // Capsule header uses QIM (high capacity, reliable under lossless STFT).
    // Spread-spectrum remains available for the payload body under --lossy.
    let header_strat = strategy::make_strategy(StrategyId::Qim);
    let mut header_opt = try_demod_capsule(
        header_strat.as_ref(),
        &frames,
        bin_lo,
        bin_hi,
        sample_rate,
        strength,
        shaping,
        &key,
    );

    // First pass matches lossless resolve defaults. If the capsule is still
    // missing and the caller did not set hop/fft/band, retry the lossy preset.
    if header_opt.is_none() && stft_unset {
        fft_size = 4096;
        hop_div = 1;
        band_lo = 1000.0;
        band_hi = 8000.0_f32.min(sample_rate as f32 * 0.45);
        strength = 0.25;
        engine = stft::StftEngine::new(fft_size, fft_size / hop_div as usize)?;
        frames = engine.analysis(&mono_after_sync)?;
        bin_lo = profile::hz_to_bin(band_lo, sample_rate, fft_size).max(1);
        bin_hi = profile::hz_to_bin(band_hi, sample_rate, fft_size)
            .min(fft_size / 2)
            .max(bin_lo + 1);
        header_opt = try_demod_capsule(
            header_strat.as_ref(),
            &frames,
            bin_lo,
            bin_hi,
            sample_rate,
            strength,
            shaping,
            &key,
        );
    }

    if let Some(hdr) = header_opt {
        strategy = hdr.strategy;
        channel_mode = hdr.channel_mode.clone();
        fft_size = hdr.fft_size as usize;
        hop_div = hdr.hop_div as u32;
        band_lo = hdr.band_lo_hz;
        band_hi = hdr.band_hi_hz;
        strength = hdr.strength;
        scale_factor = hdr.scale_factor;
        sample_rate = hdr.sample_rate;
        ecc = hdr.ecc;
        encrypt = hdr.encrypted();
        payload_len = Some(hdr.payload_len as usize);

        let mut stego_use = stego.clone();
        if stego_use.sample_rate != sample_rate {
            stego_use = resample::resample(&stego_use, sample_rate)?;
        }
        work = channels::extract_work_channels(&stego_use, &channel_mode)?;
        mono = work[0].clone();
        if scale_factor > 0.0 && (scale_factor - 1.0).abs() > 1e-6 {
            for s in &mut mono {
                *s /= scale_factor;
            }
        }
        let pre_len = sync::default_preamble_len(fft_size.min(1024));
        let preamble = sync::preamble_samples(&key, pre_len);
        let (pre_off, _) = sync::find_offset(&mono, &preamble, args.max_offset)?;
        payload_start = pre_off + pre_len;
        if payload_start < mono.len() {
            mono = mono[payload_start..].to_vec();
        }
        engine = stft::StftEngine::new(fft_size, fft_size / hop_div as usize)?;
        frames = engine.analysis(&mono)?;
        bin_lo = profile::hz_to_bin(band_lo, sample_rate, fft_size).max(1);
        bin_hi = profile::hz_to_bin(band_hi, sample_rate, fft_size)
            .min(fft_size / 2)
            .max(bin_lo + 1);
    } else if sidecar.is_some() {
        tracing::debug!("capsule header not recovered; using sidecar parameters");
    } else {
        tracing::warn!("capsule header not recovered; using CLI/sidecar parameters");
    }

    let strat = strategy::make_strategy(strategy);
    if strat.requires_original() && args.original.is_none() {
        bail!("strategy {strategy} requires --original");
    }

    let (orig_frames, orig_frames_ch1) = if let Some(path) = &args.original {
        let mut orig = decode::decode_file(path)?;
        if orig.sample_rate != sample_rate {
            orig = resample::resample(&orig, sample_rate)?;
        }
        let owork = channels::extract_work_channels(&orig, &channel_mode)?;
        let mut omono = owork[0].clone();
        if omono.len() > mono.len() {
            omono.truncate(mono.len());
        } else if omono.len() < mono.len() {
            mono.truncate(omono.len());
            frames = engine.analysis(&mono)?;
        }
        let of0 = engine.analysis(&omono)?;
        let of1 = if owork.len() >= 2 {
            let mut o1 = owork[1].clone();
            o1.truncate(mono.len());
            Some(engine.analysis(&o1)?)
        } else {
            None
        };
        (Some(of0), of1)
    } else {
        (None, None)
    };

    let ctx0 = FrameCtx {
        index: 0,
        bins: bin_lo..bin_hi,
        sample_rate,
        strength,
        shaping,
        key: key.clone(),
    };
    let bpf = strat.capacity_bits(&ctx0).max(1);
    let header_bpf = header_strat.capacity_bits(&ctx0).max(1);
    let header_frames = (capsule::HEADER_BYTES * 8 + header_bpf - 1) / header_bpf;
    let body_start = header_frames + GUARD_FRAMES;
    let body_frames = frames.len().saturating_sub(body_start);

    let needed_bits = payload_len.unwrap_or(body_frames * bpf / 8) * 8;

    let recovered_bits = if matches!(channel_mode, ChannelMode::BothSplit) && work.len() >= 2 {
        // Embed puts even body bits on ch0 and odd on ch1; reassemble here.
        let ch0_bits = (needed_bits + 1) / 2;
        let ch1_bits = needed_bits / 2;

        let bits0 = extract_allocated_bits(
            &frames,
            orig_frames.as_deref(),
            body_start,
            body_frames,
            bpf,
            ch0_bits,
            &key,
            strat.as_ref(),
            bin_lo,
            bin_hi,
            sample_rate,
            strength,
            shaping,
        );

        let mut ch1 = work[1].clone();
        if scale_factor > 0.0 && (scale_factor - 1.0).abs() > 1e-6 {
            for s in &mut ch1 {
                *s /= scale_factor;
            }
        }
        if payload_start < ch1.len() {
            ch1 = ch1[payload_start..].to_vec();
        }
        ch1.truncate(mono.len());
        let frames1 = engine.analysis(&ch1)?;
        let body_frames1 = frames1.len().saturating_sub(body_start).min(body_frames);
        let bits1 = extract_allocated_bits(
            &frames1,
            orig_frames_ch1.as_deref(),
            body_start,
            body_frames1,
            bpf,
            ch1_bits,
            &key,
            strat.as_ref(),
            bin_lo,
            bin_hi,
            sample_rate,
            strength,
            shaping,
        );
        channels::interleave_split_bits(&bits0, &bits1)
    } else {
        extract_allocated_bits(
            &frames,
            orig_frames.as_deref(),
            body_start,
            body_frames,
            bpf,
            needed_bits,
            &key,
            strat.as_ref(),
            bin_lo,
            bin_hi,
            sample_rate,
            strength,
            shaping,
        )
    };

    let body_bytes = bits::bools_to_bytes(&recovered_bits);
    let body_bytes = if let Some(len) = payload_len {
        body_bytes[..len.min(body_bytes.len())].to_vec()
    } else {
        body_bytes
    };

    let message = capsule::decode_body(encrypt, &key, ecc, &body_bytes)?;
    std::fs::write(&args.output, &message)
        .with_context(|| format!("write {}", args.output.display()))?;

    let report = ExtractReport {
        strategy: strategy.to_string(),
        message_bytes: message.len(),
        sync_offset: offset,
        sync_score,
        scale_factor,
    };
    write_optional_json(&args.report, &report)?;
    Ok((message, report))
}

fn extract_allocated_bits(
    frames: &[Vec<num_complex::Complex<f32>>],
    orig_frames: Option<&[Vec<num_complex::Complex<f32>>]>,
    body_start: usize,
    body_frames: usize,
    bpf: usize,
    needed_bits: usize,
    key: &[u8],
    strat: &dyn EmbedStrategy,
    bin_lo: usize,
    bin_hi: usize,
    sample_rate: u32,
    strength: f32,
    shaping: crate::cli::Shaping,
) -> Vec<bool> {
    let alloc = BitAllocation::plan(body_frames, bpf, needed_bits, key);
    let mut recovered_bits = vec![false; alloc.total_bits];
    for f in 0..body_frames {
        let frame_idx = body_start + f;
        if frame_idx >= frames.len() {
            break;
        }
        let ctx = FrameCtx {
            index: frame_idx,
            bins: bin_lo..bin_hi,
            sample_rate,
            strength,
            shaping,
            key: key.to_vec(),
        };
        let mut extracted = Vec::new();
        let orig = orig_frames.and_then(|ofs| ofs.get(frame_idx).map(|v| v.as_slice()));
        strat.extract_frame(&frames[frame_idx], orig, &mut extracted, &ctx);
        for (slot, &bit_idx) in alloc.frame_bits[f].iter().enumerate() {
            if slot < extracted.len() && bit_idx < recovered_bits.len() {
                recovered_bits[bit_idx] = extracted[slot];
            }
        }
    }
    recovered_bits
}

/// Deprecated helper retained for API stability of older call sites.
#[allow(dead_code)]
fn find_sync_offset(
    _mono: &[f32],
    _key: &[u8],
    _fft_size: usize,
    _hop: usize,
    _sample_rate: u32,
    _band_lo: f32,
    _band_hi: f32,
    _strength: f32,
    _shaping: crate::cli::Shaping,
    _max_offset: usize,
) -> Result<(
    usize,
    f32,
    stft::StftEngine,
    Vec<Vec<num_complex::Complex<f32>>>,
    usize,
    usize,
    Option<CapsuleHeader>,
)> {
    bail!("find_sync_offset replaced by preamble sync")
}

pub fn info(args: &InfoArgs) -> Result<InfoReport> {
    let carrier = decode::decode_file(&args.input)?;
    let params = profile::resolve(&args.common, carrier.sample_rate, None, false)?;
    let engine = stft::StftEngine::new(params.fft_size, params.hop)?;
    let plan = channels::prepare_for_embed(&carrier, &params.channel_mode)?;
    let n_frames = engine.num_frames(plan.work[0].len());
    let (bin_lo, bin_hi) = params.bin_range();
    let strat = strategy::make_strategy(params.strategy);
    let ctx = FrameCtx {
        index: 0,
        bins: bin_lo..bin_hi,
        sample_rate: params.sample_rate,
        strength: params.strength,
        shaping: params.shaping,
        key: params.key_bytes.clone(),
    };
    // Capsule header uses QIM (high capacity, reliable under lossless STFT).
    // Spread-spectrum remains available for the payload body under --lossy.
    let header_strat = strategy::make_strategy(StrategyId::Qim);
    let header_bpf = header_strat.capacity_bits(&ctx).max(1);
    let header_bits = capsule::HEADER_BYTES * 8;
    let header_frames = (header_bits + header_bpf - 1) / header_bpf;
    let body_frames = n_frames.saturating_sub(header_frames + GUARD_FRAMES);
    let mut capacity_bits = strat.capacity_bits(&ctx) * body_frames;
    if matches!(params.channel_mode, ChannelMode::BothSplit) && plan.work.len() >= 2 {
        capacity_bits *= 2;
    }

    let message_bytes = args.message_bytes;
    let fits = message_bytes.map(|m| {
        let body = capsule::build_body(&params, &vec![0u8; m])
            .map(|b| b.len() * 8)
            .unwrap_or(usize::MAX);
        body <= capacity_bits
    });

    let report = InfoReport {
        sample_rate: carrier.sample_rate,
        channels: carrier.num_channels(),
        frames: carrier.num_frames(),
        duration_secs: carrier.num_frames() as f32 / carrier.sample_rate as f32,
        strategy: params.strategy.to_string(),
        capacity_bits,
        capacity_bytes: capacity_bits / 8,
        header_bits,
        message_bytes,
        fits,
        params: ResolvedParamsPublic::from(&params),
    };
    write_optional_json(&args.report, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(report)
}

pub fn verify(args: &VerifyArgs) -> Result<VerifyReport> {
    let message = profile::load_message(&args.message, &args.message_text)?;
    let work = match &args.work_dir {
        Some(p) => {
            std::fs::create_dir_all(p)?;
            p.clone()
        }
        None => std::env::temp_dir().join(format!(
            "audiostego_verify_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        )),
    };
    std::fs::create_dir_all(&work)?;
    let loaded = work.join("loaded.wav");
    let recovered = work.join("recovered.bin");
    let sidecar = work.join("sidecar.json");

    let embed_args = EmbedArgs {
        input: args.input.clone(),
        message: None,
        message_text: Some(String::from_utf8_lossy(&message).to_string()),
        // For binary messages, write temp message file instead.
        output: loaded.clone(),
        common: args.common.clone(),
        sidecar: Some(sidecar.clone()),
        report: None,
        dry_run: false,
        strict: args.strict,
    };
    // Prefer file path for arbitrary bytes.
    let msg_path = work.join("message.bin");
    std::fs::write(&msg_path, &message)?;
    let embed_args = EmbedArgs {
        message: Some(msg_path),
        message_text: None,
        ..embed_args
    };
    // Force wav for intermediate if lossy profile still wants codec output —
    // for verify we encode to the profile's format then re-decode.
    let mut embed_args = embed_args;
    if !args.common.lossy.is_off() || args.common.output_format.is_lossy() {
        // Keep requested format so verify exercises the real codec path.
        let ext = {
            let p = profile::resolve(&args.common, 44100, Some(&loaded), false)?;
            p.output_format.extension()
        };
        embed_args.output = work.join(format!("loaded.{ext}"));
    } else {
        embed_args.output = loaded.clone();
        embed_args.common.output_format = crate::cli::OutputFormat::Wav;
    }

    let emb = embed(&embed_args)?;

    let extract_args = ExtractArgs {
        input: embed_args.output.clone(),
        original: if emb.strategy == "differential" {
            Some(args.input.clone())
        } else {
            None
        },
        output: recovered.clone(),
        sidecar: Some(sidecar),
        max_offset: 16384,
        key: args.common.key.clone(),
        strategy: None,
        channel_mode: None,
        fft_size: None,
        hop_div: None,
        band: None,
        strength: None,
        report: None,
    };

    let (got, extract_ok) = match extract(&extract_args) {
        Ok((got, _)) => {
            let ok = got == message;
            (got, ok)
        }
        Err(e) => {
            tracing::warn!("verify extract failed: {e:#}");
            (Vec::new(), false)
        }
    };
    let ber = metrics::bit_error_rate(&message, &got);
    let report = VerifyReport {
        embed: emb,
        extract_ok,
        ber,
        recovered_bytes: got.len(),
        expected_bytes: message.len(),
    };
    write_optional_json(&args.report, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(report)
}

fn write_optional_json<T: Serialize>(path: &Option<PathBuf>, value: &T) -> Result<()> {
    if let Some(p) = path {
        let s = serde_json::to_string_pretty(value)?;
        std::fs::write(p, s).with_context(|| format!("write {}", p.display()))?;
    }
    Ok(())
}

fn parse_strategy(s: &str) -> StrategyId {
    match s {
        "differential" => StrategyId::Differential,
        "magnitude-lsb" => StrategyId::MagnitudeLsb,
        "spread-spectrum" => StrategyId::SpreadSpectrum,
        _ => StrategyId::Qim,
    }
}

/// Demodulate the QIM capsule header from the first STFT frames.
fn try_demod_capsule(
    header_strat: &dyn EmbedStrategy,
    frames: &[Vec<num_complex::Complex<f32>>],
    bin_lo: usize,
    bin_hi: usize,
    sample_rate: u32,
    strength: f32,
    shaping: crate::cli::Shaping,
    key: &[u8],
) -> Option<CapsuleHeader> {
    let ctx0 = FrameCtx {
        index: 0,
        bins: bin_lo..bin_hi,
        sample_rate,
        strength: strength.max(0.2),
        shaping,
        key: key.to_vec(),
    };
    let header_bpf = header_strat.capacity_bits(&ctx0).max(1);
    let header_frames = (capsule::HEADER_BYTES * 8 + header_bpf - 1) / header_bpf;
    let mut header_bits = Vec::new();
    for f in 0..header_frames.min(frames.len()) {
        let ctx = FrameCtx {
            index: f,
            bins: bin_lo..bin_hi,
            sample_rate,
            strength: strength.max(0.2),
            shaping,
            key: key.to_vec(),
        };
        header_strat.extract_frame(&frames[f], None, &mut header_bits, &ctx);
    }
    header_bits.truncate(capsule::HEADER_BYTES * 8);
    let header_bytes = bits::bools_to_bytes(&header_bits);
    CapsuleHeader::from_bytes(&header_bytes).ok()
}

/// Helper used by tests: embed+extract round trip in memory-ish via temp wav.
pub fn roundtrip_wav(
    carrier: &AudioBuffer,
    message: &[u8],
    common: &CommonEmbedParams,
    work: &Path,
) -> Result<Vec<u8>> {
    std::fs::create_dir_all(work)?;
    let input = work.join("carrier.wav");
    let output = work.join("loaded.wav");
    let msg = work.join("msg.bin");
    let out_msg = work.join("out.bin");
    let sidecar = work.join("side.json");
    encode::encode_wav(&input, carrier)?;
    std::fs::write(&msg, message)?;
    let embed_args = EmbedArgs {
        input: input.clone(),
        message: Some(msg),
        message_text: None,
        output: output.clone(),
        common: CommonEmbedParams {
            output_format: crate::cli::OutputFormat::Wav,
            ..common.clone()
        },
        sidecar: Some(sidecar.clone()),
        report: None,
        dry_run: false,
        strict: false,
    };
    embed(&embed_args)?;
    let extract_args = ExtractArgs {
        input: output,
        original: if common.strategy.requires_original() {
            Some(input)
        } else {
            None
        },
        output: out_msg.clone(),
        sidecar: Some(sidecar),
        max_offset: 8192,
        key: common.key.clone(),
        strategy: Some(common.strategy),
        channel_mode: Some(common.channel_mode.clone()),
        fft_size: common.fft_size,
        hop_div: common.hop_div,
        band: common.band.clone(),
        strength: common.strength,
        report: None,
    };
    let (got, _) = extract(&extract_args)?;
    Ok(got)
}
