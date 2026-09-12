//! In-memory strategy sanity checks (no WAV I/O).

use audiostego::cli::Shaping;
use audiostego::cli::StrategyId;
use audiostego::dsp::stft::StftEngine;
use audiostego::strategy::{self, FrameCtx};

fn tone(n: usize) -> Vec<f32> {
    let mut s = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f32 / 44100.0;
        s[i] = 0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            + 0.25 * (2.0 * std::f32::consts::PI * 1000.0 * t).sin()
            + 0.15 * (2.0 * std::f32::consts::PI * 2500.0 * t).sin();
    }
    s
}

fn roundtrip_strategy(id: StrategyId, strength: f32) {
    let fft = 2048usize;
    let hop = 2048usize; // non-overlapping
    let engine = StftEngine::new(fft, hop).unwrap();
    let signal = tone(fft * 20);
    let mut frames = engine.analysis(&signal).unwrap();
    let bins = 50..200;
    let key = b"unit-key".to_vec();
    let strat = strategy::make_strategy(id);
    let ctx0 = FrameCtx {
        index: 0,
        bins: bins.clone(),
        sample_rate: 44100,
        strength,
        shaping: Shaping::Fixed,
        key: key.clone(),
    };
    let bpf = strat.capacity_bits(&ctx0);
    assert!(bpf > 0);
    let mut expected: Vec<bool> = Vec::new();
    for (f, frame) in frames.iter_mut().enumerate() {
        let ctx = FrameCtx {
            index: f,
            bins: bins.clone(),
            sample_rate: 44100,
            strength,
            shaping: Shaping::Fixed,
            key: key.clone(),
        };
        let bits: Vec<bool> = (0..bpf).map(|i| (i + f) % 2 == 0).collect();
        expected.extend_from_slice(&bits);
        strat.embed_frame(frame, &bits, &ctx);
    }
    let synth = engine.synthesis(&frames).unwrap();
    let mut synth = synth;
    synth.truncate(signal.len());
    let frames2 = engine.analysis(&synth).unwrap();
    let mut got = Vec::new();
    for (f, frame) in frames2.iter().enumerate() {
        let ctx = FrameCtx {
            index: f,
            bins: bins.clone(),
            sample_rate: 44100,
            strength,
            shaping: Shaping::Fixed,
            key: key.clone(),
        };
        strat.extract_frame(frame, None, &mut got, &ctx);
    }
    got.truncate(expected.len());
    let errors = expected
        .iter()
        .zip(got.iter())
        .filter(|(a, b)| a != b)
        .count();
    let ber = errors as f32 / expected.len() as f32;
    assert!(
        ber < 0.05,
        "{id} in-memory BER={ber} errors={errors}/{}",
        expected.len()
    );
}

#[test]
fn qim_in_memory() {
    roundtrip_strategy(StrategyId::Qim, 0.15);
}

#[test]
fn lsb_in_memory() {
    roundtrip_strategy(StrategyId::MagnitudeLsb, 0.15);
}

#[test]
fn ss_in_memory() {
    roundtrip_strategy(StrategyId::SpreadSpectrum, 0.5);
}

#[test]
fn ss_in_memory_sparse_tone_low_strength() {
    // ber_sweep uses a 440 Hz tone with band 1000–6000 Hz: the mark sits on
    // leakage/empty bins, which used to lose CRC at every strength.
    let fft = 4096usize;
    let hop = 4096usize;
    let sr = 44100u32;
    let engine = StftEngine::new(fft, hop).unwrap();
    let mut signal = vec![0.0f32; fft * 20];
    for (i, s) in signal.iter_mut().enumerate() {
        let t = i as f32 / sr as f32;
        *s = 0.4 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
    }
    let mut frames = engine.analysis(&signal).unwrap();
    let lo = ((1000.0 * fft as f32 / sr as f32).round() as usize).max(1);
    let hi = ((6000.0 * fft as f32 / sr as f32).round() as usize).min(fft / 2);
    let bins = lo..hi;
    let key = b"bench".to_vec();
    let strat = strategy::make_strategy(StrategyId::SpreadSpectrum);
    let strength = 0.05f32;
    let ctx0 = FrameCtx {
        index: 0,
        bins: bins.clone(),
        sample_rate: sr,
        strength,
        shaping: Shaping::Masked,
        key: key.clone(),
    };
    let bpf = strat.capacity_bits(&ctx0);
    let mut expected: Vec<bool> = Vec::new();
    for (f, frame) in frames.iter_mut().enumerate() {
        let ctx = FrameCtx {
            index: f,
            bins: bins.clone(),
            sample_rate: sr,
            strength,
            shaping: Shaping::Masked,
            key: key.clone(),
        };
        let bits: Vec<bool> = (0..bpf).map(|i| (i + f) % 2 == 0).collect();
        expected.extend_from_slice(&bits);
        strat.embed_frame(frame, &bits, &ctx);
    }
    let mut synth = engine.synthesis(&frames).unwrap();
    synth.truncate(signal.len());
    let frames2 = engine.analysis(&synth).unwrap();
    let mut got = Vec::new();
    for (f, frame) in frames2.iter().enumerate() {
        let ctx = FrameCtx {
            index: f,
            bins: bins.clone(),
            sample_rate: sr,
            strength,
            shaping: Shaping::Masked,
            key: key.clone(),
        };
        strat.extract_frame(frame, None, &mut got, &ctx);
    }
    got.truncate(expected.len());
    let errors = expected
        .iter()
        .zip(got.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        errors,
        0,
        "sparse-tone SS errors={errors}/{}",
        expected.len()
    );
}

#[test]
fn differential_in_memory() {
    let fft = 2048usize;
    let hop = 2048usize;
    let engine = StftEngine::new(fft, hop).unwrap();
    let signal = tone(fft * 20);
    let orig_frames = engine.analysis(&signal).unwrap();
    let mut frames = orig_frames.clone();
    let bins = 50..200;
    let key = b"unit-key".to_vec();
    let strat = strategy::make_strategy(StrategyId::Differential);
    let ctx0 = FrameCtx {
        index: 0,
        bins: bins.clone(),
        sample_rate: 44100,
        strength: 0.2,
        shaping: Shaping::Fixed,
        key: key.clone(),
    };
    let bpf = strat.capacity_bits(&ctx0);
    let mut expected = Vec::new();
    for (f, frame) in frames.iter_mut().enumerate() {
        let ctx = FrameCtx {
            index: f,
            bins: bins.clone(),
            sample_rate: 44100,
            strength: 0.2,
            shaping: Shaping::Fixed,
            key: key.clone(),
        };
        let bits: Vec<bool> = (0..bpf).map(|i| (i + f) % 3 == 0).collect();
        expected.extend_from_slice(&bits);
        strat.embed_frame(frame, &bits, &ctx);
    }
    let synth = engine.synthesis(&frames).unwrap();
    let mut synth = synth;
    synth.truncate(signal.len());
    let frames2 = engine.analysis(&synth).unwrap();
    let orig2 = engine.analysis(&signal).unwrap();
    let mut got = Vec::new();
    for (f, frame) in frames2.iter().enumerate() {
        let ctx = FrameCtx {
            index: f,
            bins: bins.clone(),
            sample_rate: 44100,
            strength: 0.2,
            shaping: Shaping::Fixed,
            key: key.clone(),
        };
        strat.extract_frame(frame, Some(&orig2[f]), &mut got, &ctx);
    }
    got.truncate(expected.len());
    let errors = expected
        .iter()
        .zip(got.iter())
        .filter(|(a, b)| a != b)
        .count();
    let ber = errors as f32 / expected.len() as f32;
    assert!(ber < 0.01, "differential BER={ber}");
}
