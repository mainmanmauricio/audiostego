//! Capsule-only extract and resolve-default coverage (tones).

mod common;

use audiostego::audio::encode;
use audiostego::cli::{
    ChannelMode, Cli, Commands, EmbedArgs, ExtractArgs, InfoArgs, LossyProfile, OutputFormat,
    StrategyId,
};
use audiostego::engine::{embed, extract, info, roundtrip_wav};
use audiostego::profile;
use clap::Parser;
use common::{default_common, extract_capsule_only, tone_stereo};

#[test]
fn capsule_only_lossless_qim_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(4.0, 44100);
    let msg = b"capsule-only-ok";
    let (got, _) = extract_capsule_only(
        &carrier,
        msg,
        &default_common(StrategyId::Qim, ChannelMode::Mid),
        dir.path(),
    )
    .expect("capsule-only extract");
    assert_eq!(got, msg);
}

#[test]
fn sidecar_control_recovers_same_payload() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(4.0, 44100);
    let msg = b"sidecar-control";
    let got = roundtrip_wav(
        &carrier,
        msg,
        &default_common(StrategyId::Qim, ChannelMode::Mid),
        dir.path(),
    )
    .expect("sidecar roundtrip");
    assert_eq!(got, msg);
}

#[test]
fn custom_band_sidecar_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(4.0, 44100);
    let mut common = default_common(StrategyId::Qim, ChannelMode::Mid);
    common.band = Some("1000:6000".into());
    let msg = b"custom-band";
    let got = roundtrip_wav(&carrier, msg, &common, dir.path()).expect("custom band + sidecar");
    assert_eq!(got, msg);
}

#[test]
fn lossy_preset_capsule_reports_spread_spectrum() {
    // No ffmpeg: embed WAV under lossy profile so resolve retunes STFT.
    // Capsule-only extract should recover the QIM header (strategy SS).
    // Body bit recovery is best-effort for SS on short tones — do not require it.
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(8.0, 44100);
    let input = dir.path().join("c.wav");
    let output = dir.path().join("o.wav");
    encode::encode_wav(&input, &carrier).unwrap();
    let mut common = default_common(StrategyId::Qim, ChannelMode::Mid);
    common.lossy = LossyProfile::Mp3;
    common.output_format = OutputFormat::Wav;
    common.strength = Some(0.4);
    let emb = embed(&EmbedArgs {
        input: input.clone(),
        message: None,
        message_text: Some("ok".into()),
        output: output.clone(),
        common: common.clone(),
        sidecar: None,
        report: None,
        dry_run: false,
        strict: false,
    })
    .expect("lossy-profile wav embed");
    assert_eq!(emb.strategy, "spread-spectrum");

    match extract(&ExtractArgs {
        input: output,
        original: None,
        output: dir.path().join("out.bin"),
        sidecar: None,
        max_offset: 16384,
        key: common.key.clone(),
        strategy: None,
        channel_mode: None,
        fft_size: None,
        hop_div: None,
        band: None,
        strength: None,
        report: None,
    }) {
        Ok((_got, report)) => {
            assert_eq!(report.strategy, "spread-spectrum");
        }
        Err(e) => {
            let msg = format!("{e:#}");
            // Header found → body ECC ran. Capsule miss would say "capsule header" / magic.
            assert!(
                msg.contains("CRC mismatch") && !msg.contains("capsule header"),
                "expected body CRC after capsule recover, got: {msg}"
            );
        }
    }
}

#[test]
fn key_at_file_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let key_path = dir.path().join("secret.key");
    // No trailing newline — @file reads raw bytes.
    std::fs::write(&key_path, b"file-key-bytes").unwrap();
    let carrier = tone_stereo(4.0, 44100);
    let mut common = default_common(StrategyId::Qim, ChannelMode::Mid);
    common.key = Some(format!("@{}", key_path.display()));
    let msg = b"key-file-msg";
    let (got, _) = extract_capsule_only(&carrier, msg, &common, dir.path()).expect("key @file");
    assert_eq!(got, msg);
}

#[test]
fn encrypt_wrong_key_errors() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(4.0, 44100);
    let input = dir.path().join("c.wav");
    let output = dir.path().join("o.wav");
    encode::encode_wav(&input, &carrier).unwrap();
    let mut common = default_common(StrategyId::Qim, ChannelMode::Mid);
    common.encrypt = true;
    common.key = Some("right-key".into());
    embed(&EmbedArgs {
        input,
        message: None,
        message_text: Some("secret".into()),
        output: output.clone(),
        common,
        sidecar: None,
        report: None,
        dry_run: false,
        strict: false,
    })
    .expect("encrypt embed");

    let err = extract(&ExtractArgs {
        input: output,
        original: None,
        output: dir.path().join("out.bin"),
        sidecar: None,
        max_offset: 8192,
        key: Some("wrong-key".into()),
        strategy: None,
        channel_mode: None,
        fft_size: None,
        hop_div: None,
        band: None,
        strength: None,
        report: None,
    });
    assert!(err.is_err(), "wrong encrypt key should fail AEAD");
}

#[test]
fn over_capacity_info_and_embed() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(1.0, 44100);
    let input = dir.path().join("c.wav");
    encode::encode_wav(&input, &carrier).unwrap();
    let common = default_common(StrategyId::Qim, ChannelMode::Mid);
    let huge = 1_000_000usize;
    let report = info(&InfoArgs {
        input: input.clone(),
        common: common.clone(),
        message_bytes: Some(huge),
        report: None,
    })
    .expect("info");
    assert_eq!(report.fits, Some(false));

    let err = embed(&EmbedArgs {
        input,
        message: None,
        message_text: Some("x".repeat(huge)),
        output: dir.path().join("o.wav"),
        common,
        sidecar: None,
        report: None,
        dry_run: false,
        strict: false,
    });
    let msg = format!("{:#}", err.unwrap_err());
    assert!(msg.contains("message+ECC needs"), "unexpected error: {msg}");
}

#[test]
fn encrypt_without_key_errors() {
    let mut common = default_common(StrategyId::Qim, ChannelMode::Mid);
    common.encrypt = true;
    common.key = None;
    let err = profile::resolve(&common, 44100, None, false);
    let msg = format!("{:#}", err.unwrap_err());
    assert!(msg.contains("--encrypt requires --key") || msg.contains("encrypt"));
}

#[test]
fn missing_message_load_errors() {
    let err = profile::load_message(&None, &None);
    assert!(err.is_err());
}

#[test]
fn differential_without_original_errors() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(4.0, 44100);
    let input = dir.path().join("c.wav");
    let output = dir.path().join("o.wav");
    encode::encode_wav(&input, &carrier).unwrap();
    let common = default_common(StrategyId::Differential, ChannelMode::Mid);
    embed(&EmbedArgs {
        input: input.clone(),
        message: None,
        message_text: Some("diff".into()),
        output: output.clone(),
        common: common.clone(),
        sidecar: None,
        report: None,
        dry_run: false,
        strict: false,
    })
    .expect("differential embed");

    let err = extract(&ExtractArgs {
        input: output,
        original: None,
        output: dir.path().join("out.bin"),
        sidecar: None,
        max_offset: 8192,
        key: common.key.clone(),
        strategy: None,
        channel_mode: None,
        fft_size: None,
        hop_div: None,
        band: None,
        strength: None,
        report: None,
    });
    let msg = format!("{:#}", err.unwrap_err());
    assert!(
        msg.contains("requires --original"),
        "unexpected error: {msg}"
    );
}

#[test]
fn roundtrip_16khz_and_48khz() {
    for sr in [16_000u32, 48_000] {
        let dir = tempfile::tempdir().unwrap();
        let carrier = tone_stereo(3.0, sr);
        let msg = b"rate-check";
        let got = roundtrip_wav(
            &carrier,
            msg,
            &default_common(StrategyId::Qim, ChannelMode::Mid),
            dir.path(),
        )
        .unwrap_or_else(|e| panic!("sr={sr}: {e:#}"));
        assert_eq!(got, msg, "sr={sr}");
    }
}

#[test]
fn hop_div_2_sidecar_embed_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(4.0, 44100);
    let mut common = default_common(StrategyId::Qim, ChannelMode::Mid);
    common.hop_div = Some(2);
    // Embed must succeed; exact recover is not required (WOLA / overlap).
    let input = dir.path().join("c.wav");
    let output = dir.path().join("o.wav");
    encode::encode_wav(&input, &carrier).unwrap();
    let report = embed(&EmbedArgs {
        input,
        message: None,
        message_text: Some("hop2".into()),
        output,
        common,
        sidecar: Some(dir.path().join("side.json")),
        report: None,
        dry_run: false,
        strict: false,
    })
    .expect("hop_div 2 embed");
    assert!(report.used_bits > 0);
    eprintln!(
        "hop_div=2 embed ok used_bits={} snr={:.1}",
        report.used_bits, report.snr_db
    );
}

#[test]
fn both_split_capacity_roughly_double_mid() {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(4.0, 44100);
    let input = dir.path().join("c.wav");
    encode::encode_wav(&input, &carrier).unwrap();
    let mid = info(&InfoArgs {
        input: input.clone(),
        common: default_common(StrategyId::Qim, ChannelMode::Mid),
        message_bytes: None,
        report: None,
    })
    .unwrap();
    let split = info(&InfoArgs {
        input,
        common: default_common(StrategyId::Qim, ChannelMode::BothSplit),
        message_bytes: None,
        report: None,
    })
    .unwrap();
    assert!(
        split.capacity_bits >= mid.capacity_bits * 2 - mid.capacity_bits / 10,
        "mid={} split={}",
        mid.capacity_bits,
        split.capacity_bits
    );
    assert!(
        split.capacity_bits <= mid.capacity_bits * 2 + mid.capacity_bits / 10,
        "mid={} split={}",
        mid.capacity_bits,
        split.capacity_bits
    );
}

#[test]
fn cli_channel_parse_and_message_conflict() {
    let ok = Cli::try_parse_from([
        "audiostego",
        "embed",
        "-i",
        "in.wav",
        "-o",
        "out.wav",
        "--message-text",
        "hi",
        "-c",
        "channel:1",
    ]);
    assert!(ok.is_ok());
    if let Ok(Cli {
        command: Commands::Embed(args),
        ..
    }) = ok
    {
        assert_eq!(args.common.channel_mode, ChannelMode::Channel(1));
    }

    let conflict = Cli::try_parse_from([
        "audiostego",
        "embed",
        "-i",
        "in.wav",
        "-o",
        "out.wav",
        "-m",
        "a.bin",
        "--message-text",
        "hi",
    ]);
    assert!(conflict.is_err());

    // Embed without message parses; load_message fails at runtime.
    let parsed = Cli::try_parse_from(["audiostego", "embed", "-i", "in.wav", "-o", "out.wav"]);
    assert!(parsed.is_ok());
}
