//! Simple BER / capacity sweep bench (run with `cargo bench` or `--release -- --bench`).

use audiostego::audio::AudioBuffer;
use audiostego::cli::{ChannelMode, CommonEmbedParams, LossyProfile, OutputFormat, Shaping, StrategyId};
use audiostego::dsp::metrics::bit_error_rate;
use audiostego::engine::roundtrip_wav;
use std::time::Instant;

fn tone(seconds: f32) -> AudioBuffer {
    let sr = 44100u32;
    let n = (seconds * sr as f32) as usize;
    let mut s = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f32 / sr as f32;
        s[i] = 0.4 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
    }
    AudioBuffer::new(sr, vec![s.clone(), s])
}

fn main() {
    let strengths = [0.05f32, 0.1, 0.2, 0.3];
    let strategies = [
        StrategyId::Qim,
        StrategyId::MagnitudeLsb,
        StrategyId::SpreadSpectrum,
        StrategyId::Differential,
    ];
    let msg = b"benchmark-message-0123456789";
    let carrier = tone(4.0);
    println!("strategy,strength,ber,elapsed_ms,ok");
    for strategy in strategies {
        for &strength in &strengths {
            let dir = tempfile::tempdir().unwrap();
            let common = CommonEmbedParams {
                strategy,
                channel_mode: ChannelMode::Mid,
                lossy: LossyProfile::Off,
                output_format: OutputFormat::Wav,
                bitrate: 192,
                fft_size: Some(if matches!(strategy, StrategyId::SpreadSpectrum) {
                    4096
                } else {
                    2048
                }),
                hop_div: Some(1),
                band: Some("1000:6000".into()),
                strength: Some(strength),
                shaping: Shaping::Masked,
                ecc: Some("crc".into()),
                key: Some("bench".into()),
                encrypt: false,
            };
            let t0 = Instant::now();
            let result = roundtrip_wav(&carrier, msg, &common, dir.path());
            let ms = t0.elapsed().as_millis();
            match result {
                Ok(got) => {
                    let ber = bit_error_rate(msg, &got);
                    println!(
                        "{},{},{:.6},{},{}",
                        strategy,
                        strength,
                        ber,
                        ms,
                        got == msg
                    );
                }
                Err(e) => {
                    println!("{},{},error,{},false  # {e:#}", strategy, strength, ms);
                }
            }
        }
    }
}
