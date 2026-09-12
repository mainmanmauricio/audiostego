//! Lossless embed/extract round-trips across strategies and channel modes.

mod common;

use audiostego::audio::encode;
use audiostego::cli::{ChannelMode, EmbedArgs, LossyProfile, StrategyId};
use audiostego::dsp::metrics::bit_error_rate;
use audiostego::engine::{embed, roundtrip_wav};
use common::{base_common, tone_stereo};

#[test]
fn roundtrip_qim_mid() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(3.0, 44100);
    let msg = b"hello audiostego qim".to_vec();
    let got = roundtrip_wav(
        &carrier,
        &msg,
        &base_common(StrategyId::Qim, ChannelMode::Mid),
        dir.path(),
    )
    .expect("roundtrip");
    assert_eq!(got, msg);
}

#[test]
fn roundtrip_lsb_left() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(3.0, 44100);
    let msg = b"lsb payload".to_vec();
    let mut common = base_common(StrategyId::MagnitudeLsb, ChannelMode::Left);
    common.strength = Some(0.25);
    let got = roundtrip_wav(&carrier, &msg, &common, dir.path()).expect("roundtrip");
    assert_eq!(got, msg);
}

#[test]
fn roundtrip_spread_smoke() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(6.0, 44100);
    let msg = b"ok".to_vec();
    let mut common = base_common(StrategyId::SpreadSpectrum, ChannelMode::Mid);
    common.strength = Some(0.4);
    common.shaping = audiostego::cli::Shaping::Fixed;
    let got = roundtrip_wav(&carrier, &msg, &common, dir.path()).expect("ss roundtrip");
    assert_eq!(got, msg);
}

#[test]
fn roundtrip_spread_ber_sweep_tone() {
    // Mirrors `ber_sweep`: 440 Hz stereo tone, band 1000:6000, CRC, mid.
    let dir = tempfile::tempdir().unwrap();
    let sr = 44100u32;
    let n = (4.0 * sr as f32) as usize;
    let mut s = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f32 / sr as f32;
        s[i] = 0.4 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
    }
    let carrier = audiostego::audio::AudioBuffer::new(sr, vec![s.clone(), s]);
    let msg = b"benchmark-message-0123456789";
    let mut common = base_common(StrategyId::SpreadSpectrum, ChannelMode::Mid);
    common.fft_size = Some(4096);
    common.hop_div = Some(1);
    common.band = Some("1000:6000".into());
    common.strength = Some(0.05);
    common.shaping = audiostego::cli::Shaping::Masked;
    common.ecc = Some("crc".into());
    common.key = Some("bench".into());
    let got = roundtrip_wav(&carrier, msg, &common, dir.path()).expect("ss ber-sweep tone");
    assert_eq!(got, msg);
}

#[test]
fn roundtrip_differential_requires_original() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(3.0, 44100);
    let msg = b"diff-msg".to_vec();
    let got = roundtrip_wav(
        &carrier,
        &msg,
        &base_common(StrategyId::Differential, ChannelMode::Mid),
        dir.path(),
    )
    .expect("roundtrip");
    assert_eq!(got, msg);
}

#[test]
fn sync_survives_leading_offset() {
    use audiostego::audio::decode;
    use audiostego::cli::ExtractArgs;
    use audiostego::engine::extract;

    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(3.0, 44100);
    let msg = b"offset-test".to_vec();
    let common = base_common(StrategyId::Qim, ChannelMode::Mid);
    // Embed normally.
    let _ = roundtrip_wav(&carrier, &msg, &common, dir.path()).expect("embed/extract");

    // Manually pad the loaded file and re-extract.
    let loaded_path = dir.path().join("loaded.wav");
    let mut loaded = decode::decode_file(&loaded_path).unwrap();
    let pad = 1500usize;
    for ch in &mut loaded.channels {
        let mut padded = vec![0.0f32; pad];
        padded.extend_from_slice(ch);
        *ch = padded;
    }
    let padded_path = dir.path().join("padded.wav");
    encode::encode_wav(&padded_path, &loaded).unwrap();

    let out = dir.path().join("from_padded.bin");
    let args = ExtractArgs {
        input: padded_path,
        original: None,
        output: out.clone(),
        sidecar: Some(dir.path().join("side.json")),
        max_offset: 8192,
        key: Some("test-key-42".into()),
        strategy: Some(StrategyId::Qim),
        channel_mode: Some(ChannelMode::Mid),
        fft_size: Some(2048),
        hop_div: Some(1),
        band: Some("1000:6000".into()),
        strength: Some(0.12),
        report: None,
    };
    let (got, report) = extract(&args).expect("extract padded");
    assert!(report.sync_offset > 0, "expected nonzero sync offset");
    assert_eq!(got, msg);
}

#[test]
fn snr_floor_default_settings() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(4.0, 44100);
    let input = dir.path().join("c.wav");
    let output = dir.path().join("o.wav");
    let msg = dir.path().join("m.bin");
    encode::encode_wav(&input, &carrier).unwrap();
    std::fs::write(&msg, b"snr-check").unwrap();
    let args = EmbedArgs {
        input,
        message: Some(msg),
        message_text: None,
        output: output.clone(),
        common: base_common(StrategyId::Qim, ChannelMode::Mid),
        sidecar: None,
        report: None,
        dry_run: false,
        strict: false,
    };
    let report = embed(&args).unwrap();
    assert!(report.snr_db > 15.0, "SNR too low: {}", report.snr_db);
}

#[test]
fn strict_rejects_fragile_lossy_combo() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(2.0, 44100);
    let input = dir.path().join("c.wav");
    encode::encode_wav(&input, &carrier).unwrap();
    let mut common = base_common(StrategyId::Qim, ChannelMode::Mid);
    common.lossy = LossyProfile::Mp3;
    let args = EmbedArgs {
        input,
        message: None,
        message_text: Some("x".into()),
        output: dir.path().join("o.mp3"),
        common,
        sidecar: None,
        report: None,
        dry_run: true,
        strict: true,
    };
    let err = embed(&args).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("strict") || msg.contains("fragile") || msg.contains("spread-spectrum"),
        "unexpected error: {msg}"
    );
}

#[test]
fn ber_helper_zero_on_equal() {
    assert_eq!(bit_error_rate(b"abc", b"abc"), 0.0);
}

#[test]
fn roundtrip_qim_encrypted() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(3.0, 44100);
    let msg = b"encrypted-payload".to_vec();
    let mut common = base_common(StrategyId::Qim, ChannelMode::Mid);
    common.encrypt = true;
    let got = roundtrip_wav(&carrier, &msg, &common, dir.path()).expect("encrypt roundtrip");
    assert_eq!(got, msg);
}
