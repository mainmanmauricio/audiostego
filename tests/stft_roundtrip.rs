//! STFT round-trip reconstruction test.

use audiostego::dsp::stft::StftEngine;

#[test]
fn stft_istft_roundtrip_error_below_1e6() {
    let n = 8192;
    let mut signal = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f32 / 44100.0;
        signal[i] = 0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            + 0.25 * (2.0 * std::f32::consts::PI * 880.0 * t).sin();
    }
    let engine = StftEngine::new(1024, 512).unwrap();
    let frames = engine.analysis(&signal).unwrap();
    let reconstructed = engine.synthesis(&frames).unwrap();
    let m = signal.len().min(reconstructed.len());
    // Ignore edges where COLA windows don't fully overlap.
    let start = 1024;
    let end = m.saturating_sub(1024);
    let mut max_err = 0.0f32;
    let mut energy = 0.0f64;
    let mut err_e = 0.0f64;
    for i in start..end {
        let e = (signal[i] - reconstructed[i]).abs();
        max_err = max_err.max(e);
        energy += (signal[i] as f64).powi(2);
        err_e += (e as f64).powi(2);
    }
    let rel = (err_e / energy.max(1e-20)).sqrt();
    assert!(
        max_err < 1e-5 && rel < 1e-6,
        "max_err={max_err} rel={rel}"
    );
}
