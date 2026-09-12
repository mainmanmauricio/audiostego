//! Lossy round-trip tests (gated on ffmpeg availability) + always-on profile resolve.

mod common;

use audiostego::audio::ffmpeg;
use audiostego::audio::{encode, AudioBuffer};
use audiostego::cli::{
    ChannelMode, CommonEmbedParams, EccMode, EmbedArgs, ExtractArgs, LossyProfile, OutputFormat,
    Shaping, StrategyId,
};
use audiostego::dsp::metrics::bit_error_rate;
use audiostego::engine::{embed, extract};
use audiostego::profile;
use common::base_common;

fn tone(seconds: f32, sr: u32) -> AudioBuffer {
    let n = (seconds * sr as f32) as usize;
    let mut samples = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f32 / sr as f32;
        samples[i] = 0.45 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            + 0.2 * (2.0 * std::f32::consts::PI * 1000.0 * t).sin()
            + 0.1 * (2.0 * std::f32::consts::PI * 2500.0 * t).sin();
    }
    AudioBuffer::new(sr, vec![samples.clone(), samples])
}

fn lossy_common(lossy: LossyProfile) -> CommonEmbedParams {
    CommonEmbedParams {
        strategy: StrategyId::Qim, // should upgrade to spread-spectrum
        channel_mode: ChannelMode::Mid,
        lossy,
        output_format: OutputFormat::Auto,
        bitrate: 192,
        fft_size: None,
        hop_div: None,
        band: None,
        strength: None,
        shaping: Shaping::Masked,
        ecc: None,
        key: Some("lossy-key".into()),
        encrypt: false,
    }
}

#[test]
fn lossy_profiles_retune_like_mp3() {
    let cases = [
        (LossyProfile::Mp3, OutputFormat::Mp3),
        (LossyProfile::Aac, OutputFormat::Aac),
        (LossyProfile::Opus, OutputFormat::Opus),
        (LossyProfile::Vorbis, OutputFormat::Vorbis),
    ];
    for (lossy, expected_fmt) in cases {
        let common = lossy_common(lossy);
        let resolved = profile::resolve(&common, 44100, None, false).unwrap();
        assert_eq!(resolved.strategy, StrategyId::SpreadSpectrum, "{lossy:?}");
        assert_eq!(resolved.output_format, expected_fmt, "{lossy:?}");
        assert!(resolved.fft_size >= 4096, "{lossy:?}");
        assert!(
            matches!(resolved.ecc, EccMode::ReedSolomon { parity: 16 }),
            "{lossy:?} ecc={:?}",
            resolved.ecc
        );
        assert!(!resolved.upgraded.is_empty(), "{lossy:?}");
    }
}

#[test]
fn strict_rejects_fragile_aac() {
    let mut common = base_common(StrategyId::Qim, ChannelMode::Mid);
    common.lossy = LossyProfile::Aac;
    let err = profile::resolve(&common, 44100, None, true).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("strict") || msg.contains("fragile") || msg.contains("spread-spectrum"),
        "unexpected error: {msg}"
    );
}

#[test]
fn lossy_profile_retunes_and_embeds_mp3() {
    if !ffmpeg::ffmpeg_available() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone(4.0, 44100);
    let input = dir.path().join("c.wav");
    encode::encode_wav(&input, &carrier).unwrap();

    let common = lossy_common(LossyProfile::Mp3);
    let report = embed(&EmbedArgs {
        input,
        message: None,
        message_text: Some("ok".into()),
        output: dir.path().join("loaded.mp3"),
        common,
        sidecar: Some(dir.path().join("side.json")),
        report: None,
        dry_run: false,
        strict: false,
    })
    .expect("embed under lossy profile");
    assert!(dir.path().join("loaded.mp3").exists());
    assert_eq!(report.strategy, "spread-spectrum");
    eprintln!("lossy embed SNR={:.1} dB", report.snr_db);
}

fn lossy_embed_smoke(lossy: LossyProfile, ext: &str) {
    if !ffmpeg::ffmpeg_available() {
        eprintln!("skipping: ffmpeg not available ({ext})");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone(4.0, 44100);
    let input = dir.path().join("c.wav");
    encode::encode_wav(&input, &carrier).unwrap();
    let output = dir.path().join(format!("loaded.{ext}"));
    let common = lossy_common(lossy);
    let report = embed(&EmbedArgs {
        input,
        message: None,
        message_text: Some("ok".into()),
        output: output.clone(),
        common,
        sidecar: Some(dir.path().join("side.json")),
        report: None,
        dry_run: false,
        strict: false,
    })
    .unwrap_or_else(|e| panic!("embed {ext}: {e:#}"));
    assert!(output.exists(), "{ext} missing");
    assert_eq!(report.strategy, "spread-spectrum");
}

#[test]
fn lossy_embed_smoke_aac() {
    lossy_embed_smoke(LossyProfile::Aac, "aac");
}

#[test]
fn lossy_embed_smoke_opus() {
    lossy_embed_smoke(LossyProfile::Opus, "opus");
}

#[test]
fn lossy_embed_smoke_vorbis() {
    lossy_embed_smoke(LossyProfile::Vorbis, "ogg");
}

/// FLAC encode smoke only — integer PCM decode is not bit-exact for magnitude QIM.
#[test]
fn flac_encode_smoke() {
    if !ffmpeg::ffmpeg_available() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone(3.0, 44100);
    let input = dir.path().join("c.wav");
    encode::encode_wav(&input, &carrier).unwrap();
    let output = dir.path().join("loaded.flac");
    let mut common = base_common(StrategyId::Qim, ChannelMode::Mid);
    common.output_format = OutputFormat::Flac;
    embed(&EmbedArgs {
        input,
        message: None,
        message_text: Some("flac-smoke".into()),
        output: output.clone(),
        common,
        sidecar: Some(dir.path().join("side.json")),
        report: None,
        dry_run: false,
        strict: false,
    })
    .expect("flac embed");
    assert!(output.exists());
}

/// Exact recovery through MP3 is encoder-dependent. Enabled when
/// AUDIOSTEGO_LOSSY_STRICT=1; otherwise documents best-effort status.
#[test]
fn mp3_spread_spectrum_roundtrip_strict() {
    if !ffmpeg::ffmpeg_available() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    if std::env::var("AUDIOSTEGO_LOSSY_STRICT").ok().as_deref() != Some("1") {
        eprintln!(
            "skipping strict mp3 BER assert (set AUDIOSTEGO_LOSSY_STRICT=1 to enable); \
             use `audiostego verify --lossy mp3` for empirical evidence"
        );
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone(12.0, 44100);
    let input = dir.path().join("c.wav");
    encode::encode_wav(&input, &carrier).unwrap();
    let msg = b"ok";
    std::fs::write(dir.path().join("m.bin"), msg).unwrap();

    let common = CommonEmbedParams {
        strategy: StrategyId::SpreadSpectrum,
        channel_mode: ChannelMode::Mid,
        lossy: LossyProfile::Mp3,
        output_format: OutputFormat::Mp3,
        bitrate: 192,
        fft_size: Some(4096),
        hop_div: Some(1),
        band: Some("1000:8000".into()),
        strength: Some(0.4),
        shaping: Shaping::Fixed,
        ecc: Some("rs:16".into()),
        key: Some("lossy-key".into()),
        encrypt: false,
    };
    let loaded = dir.path().join("loaded.mp3");
    let sidecar = dir.path().join("side.json");
    embed(&EmbedArgs {
        input: input.clone(),
        message: Some(dir.path().join("m.bin")),
        message_text: None,
        output: loaded.clone(),
        common,
        sidecar: Some(sidecar.clone()),
        report: None,
        dry_run: false,
        strict: false,
    })
    .expect("embed mp3");

    let (got, _) = extract(&ExtractArgs {
        input: loaded,
        original: None,
        output: dir.path().join("out.bin"),
        sidecar: Some(sidecar),
        max_offset: 16384,
        key: Some("lossy-key".into()),
        strategy: Some(StrategyId::SpreadSpectrum),
        channel_mode: Some(ChannelMode::Mid),
        fft_size: Some(4096),
        hop_div: Some(1),
        band: Some("1000:8000".into()),
        strength: Some(0.4),
        report: None,
    })
    .expect("extract mp3");
    let ber = bit_error_rate(msg, &got);
    eprintln!("strict mp3 BER={ber}");
    assert_eq!(got, msg, "BER={ber}");
}
