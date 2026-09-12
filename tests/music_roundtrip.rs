//! Music-like / vendored-carrier round-trips.

mod common;

use audiostego::audio::encode;
use audiostego::cli::{ChannelMode, InfoArgs, StrategyId, VerifyArgs};
use audiostego::engine::{info, verify};
use common::{default_common, extract_capsule_only, music_carrier, testdata_music_path};

#[test]
fn music_capsule_only_qim_mid() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = music_carrier();
    let msg = b"music-capsule";
    let (got, _) = extract_capsule_only(
        &carrier,
        msg,
        &default_common(StrategyId::Qim, ChannelMode::Mid),
        dir.path(),
    )
    .expect("music capsule-only");
    assert_eq!(got, msg);
}

#[test]
fn music_verify_lossless() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = music_carrier();
    let input = dir.path().join("c.wav");
    encode::encode_wav(&input, &carrier).unwrap();
    let report = verify(&VerifyArgs {
        input,
        message: None,
        message_text: Some("verify-music".into()),
        work_dir: Some(dir.path().join("work")),
        common: default_common(StrategyId::Qim, ChannelMode::Mid),
        strict: false,
        report: None,
    })
    .expect("verify music");
    assert!(report.extract_ok);
    assert_eq!(report.ber, 0.0);
}

#[test]
fn music_snr_floor() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = music_carrier();
    let input = dir.path().join("c.wav");
    let output = dir.path().join("o.wav");
    encode::encode_wav(&input, &carrier).unwrap();
    let report = audiostego::engine::embed(&audiostego::cli::EmbedArgs {
        input,
        message: None,
        message_text: Some("snr".into()),
        output,
        common: default_common(StrategyId::Qim, ChannelMode::Mid),
        sidecar: None,
        report: None,
        dry_run: false,
        strict: false,
    })
    .expect("embed for snr");
    assert!(report.snr_db.is_finite(), "snr={}", report.snr_db);
    assert!(
        report.snr_db > 8.0,
        "SNR too low on music-like: {}",
        report.snr_db
    );
}

#[test]
fn vendored_flac_info_when_present() {
    let Some(path) = testdata_music_path() else {
        eprintln!("skipping: testdata/music/carrier-music.flac not present");
        return;
    };
    let report = info(&InfoArgs {
        input: path,
        common: default_common(StrategyId::Qim, ChannelMode::Mid),
        message_bytes: None,
        report: None,
    })
    .expect("info on vendored flac");
    assert!(report.sample_rate >= 8_000);
    assert!(report.channels >= 1);
    assert!(
        report.duration_secs >= 10.0,
        "duration={}",
        report.duration_secs
    );
}
