//! `info` / `verify` orchestration and encrypt-related engine coverage.

mod common;

use audiostego::audio::encode;
use audiostego::audio::ffmpeg;
use audiostego::cli::{ChannelMode, InfoArgs, LossyProfile, StrategyId, VerifyArgs};
use audiostego::engine::{info, verify};
use common::{base_common, tone_stereo};

#[test]
fn info_reports_capacity_and_fit() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(3.0, 44100);
    let input = dir.path().join("c.wav");
    encode::encode_wav(&input, &carrier).unwrap();
    let report = info(&InfoArgs {
        input,
        common: base_common(StrategyId::Qim, ChannelMode::Mid),
        message_bytes: Some(16),
        report: None,
    })
    .expect("info");
    assert!(report.capacity_bits > 0);
    assert_eq!(report.fits, Some(true));
    assert_eq!(report.sample_rate, 44100);
}

#[test]
fn verify_lossless_qim_mid() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(3.0, 44100);
    let input = dir.path().join("c.wav");
    encode::encode_wav(&input, &carrier).unwrap();
    let report = verify(&VerifyArgs {
        input,
        message: None,
        message_text: Some("verify-ok".into()),
        work_dir: Some(dir.path().join("work")),
        common: base_common(StrategyId::Qim, ChannelMode::Mid),
        strict: false,
        report: None,
    })
    .expect("verify");
    assert!(report.extract_ok);
    assert_eq!(report.ber, 0.0);
    assert_eq!(report.expected_bytes, b"verify-ok".len());
}

#[test]
fn verify_lossy_mp3_smoke() {
    if !ffmpeg::ffmpeg_available() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(6.0, 44100);
    let input = dir.path().join("c.wav");
    encode::encode_wav(&input, &carrier).unwrap();
    let mut common = base_common(StrategyId::SpreadSpectrum, ChannelMode::Mid);
    common.lossy = LossyProfile::Mp3;
    common.output_format = audiostego::cli::OutputFormat::Mp3;
    common.strength = Some(0.4);
    common.fft_size = Some(4096);
    common.hop_div = Some(1);
    common.ecc = Some("rs:16".into());
    common.shaping = audiostego::cli::Shaping::Fixed;

    let report = verify(&VerifyArgs {
        input,
        message: None,
        message_text: Some("ok".into()),
        work_dir: Some(dir.path().join("work")),
        common,
        strict: false,
        report: None,
    })
    .expect("verify mp3 should return a report");

    if std::env::var("AUDIOSTEGO_LOSSY_STRICT").ok().as_deref() == Some("1") {
        assert!(
            report.extract_ok,
            "strict mp3 verify failed BER={}",
            report.ber
        );
    } else {
        eprintln!(
            "lossy verify BER={} extract_ok={} (set AUDIOSTEGO_LOSSY_STRICT=1 to assert)",
            report.ber, report.extract_ok
        );
    }
}

#[test]
fn sidecar_rs_used_when_capsule_header_missing() {
    // Lossy profiles encode the body with Reed-Solomon. After a missing
    // capsule (typical for MP3), extract must decode with sidecar ECC, not
    // the CRC default — otherwise a healthy payload still CRC-mismatches.
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(8.0, 44100);
    let input = dir.path().join("c.wav");
    let output = dir.path().join("o.wav");
    let sidecar = dir.path().join("side.json");
    encode::encode_wav(&input, &carrier).unwrap();
    let mut common = base_common(StrategyId::Qim, ChannelMode::Mid);
    common.lossy = LossyProfile::Off;
    common.fft_size = Some(2048);
    common.hop_div = Some(1);
    common.band = Some("1000:8000".into());
    common.strength = Some(0.12);
    common.ecc = Some("rs:16".into());

    audiostego::engine::embed(&audiostego::cli::EmbedArgs {
        input: input.clone(),
        message: None,
        message_text: Some("ok".into()),
        output: output.clone(),
        common: common.clone(),
        sidecar: Some(sidecar.clone()),
        report: None,
        dry_run: false,
        strict: false,
    })
    .expect("embed rs body");

    // Destroy the QIM header region (first STFT frame after the preamble)
    // so extract is forced onto sidecar parameters, including ecc=rs:16.
    let mut stego = audiostego::audio::decode::decode_file(&output).unwrap();
    let pre_len = 2048 * 4;
    let wipe = 2048;
    for ch in &mut stego.channels {
        let end = (pre_len + wipe).min(ch.len());
        if pre_len < ch.len() {
            for s in &mut ch[pre_len..end] {
                *s = 0.0;
            }
        }
    }
    encode::encode_wav(&output, &stego).unwrap();

    let (got, _) = audiostego::engine::extract(&audiostego::cli::ExtractArgs {
        input: output,
        original: None,
        output: dir.path().join("out.bin"),
        sidecar: Some(sidecar),
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
    .expect("sidecar RS extract after capsule wipe");
    assert_eq!(got, b"ok");
}

#[test]
fn verify_lossy_wav_profile_returns_report_on_extract_failure() {
    // Lossy-profile STFT into WAV (no ffmpeg). Spread-spectrum body CRC may
    // fail on a short tone; verify must still return a report.
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(8.0, 44100);
    let input = dir.path().join("c.wav");
    encode::encode_wav(&input, &carrier).unwrap();
    let mut common = base_common(StrategyId::Qim, ChannelMode::Mid);
    common.lossy = LossyProfile::Mp3;
    common.output_format = audiostego::cli::OutputFormat::Wav;
    common.fft_size = None;
    common.hop_div = None;
    common.band = None;
    common.strength = None;
    common.ecc = None;

    let report = verify(&VerifyArgs {
        input,
        message: None,
        message_text: Some("ok".into()),
        work_dir: Some(dir.path().join("work")),
        common,
        strict: false,
        report: None,
    })
    .expect("verify must return a report even if extract CRC fails");
    assert_eq!(report.expected_bytes, 2);
}
