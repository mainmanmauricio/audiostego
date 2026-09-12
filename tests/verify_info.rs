//! `info` / `verify` orchestration and encrypt-related engine coverage.

mod common;

use audiostego::audio::encode;
use audiostego::audio::ffmpeg;
use audiostego::cli::{
    ChannelMode, InfoArgs, LossyProfile, StrategyId, VerifyArgs,
};
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
    common.strength = Some(0.4);
    common.fft_size = Some(4096);
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
