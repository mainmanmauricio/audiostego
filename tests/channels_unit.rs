//! Channel mapping unit tests (no STFT / WAV I/O).

mod common;

use audiostego::audio::channels::{self, ChannelPlan};
use audiostego::cli::ChannelMode;
use common::tone_stereo;

fn max_abs_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0f32, f32::max)
}

#[test]
fn prepare_work_channel_counts() {
    let stereo = tone_stereo(0.5, 44100);
    let cases: &[(ChannelMode, usize)] = &[
        (ChannelMode::Mono, 1),
        (ChannelMode::Left, 1),
        (ChannelMode::Right, 1),
        (ChannelMode::Channel(0), 1),
        (ChannelMode::Channel(1), 1),
        (ChannelMode::Mid, 1),
        (ChannelMode::Side, 1),
        (ChannelMode::BothMirror, 2),
        (ChannelMode::BothSplit, 2),
    ];
    for (mode, expected) in cases {
        let plan = channels::prepare_for_embed(&stereo, mode).unwrap();
        assert_eq!(plan.work.len(), *expected, "mode={mode:?}");
    }
}

#[test]
fn prepare_reassemble_mid_side_restores_lr() {
    let original = tone_stereo(0.25, 44100);
    for mode in [ChannelMode::Mid, ChannelMode::Side] {
        let plan = channels::prepare_for_embed(&original, &mode).unwrap();
        let out = channels::reassemble(&plan, &plan.work).unwrap();
        assert_eq!(out.num_channels(), 2);
        assert!(
            max_abs_diff(&out.channels[0], &original.channels[0]) < 1e-5,
            "L mismatch for {mode:?}"
        );
        assert!(
            max_abs_diff(&out.channels[1], &original.channels[1]) < 1e-5,
            "R mismatch for {mode:?}"
        );
    }
}

#[test]
fn prepare_reassemble_left_replaces_only_left() {
    let original = tone_stereo(0.25, 44100);
    let plan = channels::prepare_for_embed(&original, &ChannelMode::Left).unwrap();
    let mut embedded = plan.work.clone();
    for s in &mut embedded[0] {
        *s = 0.123;
    }
    let out = channels::reassemble(&plan, &embedded).unwrap();
    assert!(out.channels[0].iter().all(|&s| (s - 0.123).abs() < 1e-6));
    assert!(max_abs_diff(&out.channels[1], &original.channels[1]) < 1e-5);
}

#[test]
fn prepare_errors_on_mono_and_oob() {
    let mono = common::tone_mono(0.2, 44100);
    assert!(channels::prepare_for_embed(&mono, &ChannelMode::Right).is_err());
    assert!(channels::prepare_for_embed(&mono, &ChannelMode::Side).is_err());

    let stereo = tone_stereo(0.2, 44100);
    assert!(channels::prepare_for_embed(&stereo, &ChannelMode::Channel(9)).is_err());
}

#[test]
fn combine_mirror_bits_or_and_truncate() {
    let a = vec![true, false, true, false];
    let b = vec![false, false, true];
    let got = channels::combine_mirror_bits(&a, &b);
    assert_eq!(got, vec![true, false, true]);
}

#[test]
fn interleave_split_bits_even_odd() {
    let even = vec![true, true, false];
    let odd = vec![false, true];
    let got = channels::interleave_split_bits(&even, &odd);
    assert_eq!(got, vec![true, false, true, true, false]);
}

#[test]
fn channel_plan_debug_smoke() {
    let stereo = tone_stereo(0.1, 44100);
    let plan: ChannelPlan = channels::prepare_for_embed(&stereo, &ChannelMode::Mid).unwrap();
    assert_eq!(plan.work.len(), 1);
}
