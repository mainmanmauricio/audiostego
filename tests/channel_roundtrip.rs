//! Lossless QIM round-trips across channel modes (float32 WAV).

mod common;

use audiostego::cli::ChannelMode;
use audiostego::cli::StrategyId;
use audiostego::engine::roundtrip_wav;
use common::{base_common, tone_stereo};

fn assert_qim_roundtrip(mode: ChannelMode) {
    let dir = tempfile::tempdir().unwrap();
    let carrier = tone_stereo(3.0, 44100);
    let msg = b"channel-matrix".to_vec();
    let got = roundtrip_wav(
        &carrier,
        &msg,
        &base_common(StrategyId::Qim, mode.clone()),
        dir.path(),
    )
    .unwrap_or_else(|e| panic!("roundtrip failed for {mode:?}: {e:#}"));
    assert_eq!(got, msg, "mismatch for {mode:?}");
}

#[test]
fn roundtrip_qim_mono() {
    assert_qim_roundtrip(ChannelMode::Mono);
}

#[test]
fn roundtrip_qim_left() {
    assert_qim_roundtrip(ChannelMode::Left);
}

#[test]
fn roundtrip_qim_right() {
    assert_qim_roundtrip(ChannelMode::Right);
}

#[test]
fn roundtrip_qim_channel0() {
    assert_qim_roundtrip(ChannelMode::Channel(0));
}

#[test]
fn roundtrip_qim_channel1() {
    assert_qim_roundtrip(ChannelMode::Channel(1));
}

#[test]
fn roundtrip_qim_mid() {
    assert_qim_roundtrip(ChannelMode::Mid);
}

#[test]
fn roundtrip_qim_side() {
    assert_qim_roundtrip(ChannelMode::Side);
}

#[test]
fn roundtrip_qim_both_mirror() {
    // Today extract only demodulates work[0]; BothMirror still embeds the full
    // body on ch0, so recovery behaves like Left.
    assert_qim_roundtrip(ChannelMode::BothMirror);
}

#[test]
fn roundtrip_qim_both_split() {
    assert_qim_roundtrip(ChannelMode::BothSplit);
}
