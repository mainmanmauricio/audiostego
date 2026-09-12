//! Window functions for WOLA STFT.

pub fn sqrt_hann(n: usize) -> Vec<f32> {
    if n == 0 {
        return Vec::new();
    }
    let nm1 = (n - 1) as f32;
    (0..n)
        .map(|i| {
            let hann = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / nm1).cos());
            hann.max(0.0).sqrt()
        })
        .collect()
}

pub fn rectangular(n: usize) -> Vec<f32> {
    vec![1.0; n]
}

/// Cola normalization weights for analysis*synthesis windows.
pub fn cola_weights(window: &[f32], hop: usize, length: usize) -> Vec<f32> {
    let n = window.len();
    let mut w = vec![0.0f32; length + n];
    let mut pos = 0usize;
    while pos < length {
        for i in 0..n {
            if pos + i < w.len() {
                w[pos + i] += window[i] * window[i];
            }
        }
        pos += hop;
    }
    for x in &mut w {
        if *x < 1e-8 {
            *x = 1.0;
        }
    }
    w
}
