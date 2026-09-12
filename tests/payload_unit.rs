//! Unit tests for payload bits, ECC, crypto, capsule, and bit allocation.

use audiostego::cli::{
    ChannelMode, CommonEmbedParams, EccMode, LossyProfile, OutputFormat, Shaping, StrategyId,
};
use audiostego::payload::alloc::BitAllocation;
use audiostego::payload::bits;
use audiostego::payload::capsule::{self, CapsuleHeader, HEADER_BYTES};
use audiostego::payload::{crypto, ecc};
use audiostego::profile;

#[test]
fn bits_roundtrip_empty_and_full() {
    assert!(bits::bytes_to_bools(&[]).is_empty());
    assert!(bits::bools_to_bytes(&[]).is_empty());

    let data = b"audiostego\x00\xff";
    let bools = bits::bytes_to_bools(data);
    assert_eq!(bools.len(), data.len() * 8);
    assert_eq!(bits::bools_to_bytes(&bools), data);

    let bv = bits::bytes_to_bits(data);
    assert_eq!(bits::bits_to_bytes(&bv), data);
}

#[test]
fn bits_partial_byte_pads_high_bits_zero() {
    // 3 bits → one byte with high bits set only where provided.
    let out = bits::bools_to_bytes(&[true, false, true]);
    assert_eq!(out, vec![0b1010_0000]);
}

#[test]
fn ecc_none_crc_rs_roundtrip() {
    let msg = b"hello ecc payload";
    for mode in [
        EccMode::None,
        EccMode::Crc,
        EccMode::ReedSolomon { parity: 16 },
    ] {
        let protected = ecc::protect(msg, mode).unwrap();
        let got = ecc::recover(&protected, mode).unwrap();
        assert_eq!(got, msg, "mode={mode:?}");
    }
}

#[test]
fn ecc_crc_mismatch_and_short() {
    let protected = ecc::protect(b"abc", EccMode::Crc).unwrap();
    let mut bad = protected.clone();
    *bad.last_mut().unwrap() ^= 0xff;
    assert!(ecc::recover(&bad, EccMode::Crc).is_err());
    assert!(ecc::recover(&[1, 2, 3], EccMode::Crc).is_err());
}

#[test]
fn ecc_rs_recovers_flips_inside_codeword() {
    let msg = b"reed-solomon correction test payload!!";
    let mode = EccMode::ReedSolomon { parity: 16 };
    let mut protected = ecc::protect(msg, mode).unwrap();
    // Layout: [u8 chunk_len][encoded chunk+parity]...
    // Flip two bytes inside the first encoded block (skip the length prefix).
    assert!(protected.len() > 4);
    let chunk_len = protected[0] as usize;
    let block_start = 1;
    let flip_a = block_start + chunk_len / 2;
    let flip_b = block_start + chunk_len + 2; // inside parity region
    assert!(flip_b < protected.len());
    protected[flip_a] ^= 0x55;
    protected[flip_b] ^= 0xaa;
    let got = ecc::recover(&protected, mode).expect("RS should correct 2 symbol errors");
    assert_eq!(got, msg);
}

#[test]
fn crypto_roundtrip_wrong_key_and_short() {
    let key = b"secret-key";
    let pt = b"plaintext message";
    let blob = crypto::encrypt(key, pt).unwrap();
    assert_eq!(crypto::decrypt(key, &blob).unwrap(), pt);
    assert!(crypto::decrypt(b"other-key", &blob).is_err());
    assert!(crypto::decrypt(key, &blob[..20]).is_err());
}

#[test]
fn crypto_deterministic_nonce_depends_on_length() {
    let key = b"nonce-key";
    let a = crypto::encrypt(key, b"aaaa").unwrap();
    let b = crypto::encrypt(key, b"aaaa").unwrap();
    assert_eq!(a, b);
    // Same length, different content → same nonce prefix (intentional stego pattern).
    let c = crypto::encrypt(key, b"bbbb").unwrap();
    assert_eq!(&a[..12], &c[..12]);
    // Different length → different nonce.
    let d = crypto::encrypt(key, b"aaa").unwrap();
    assert_ne!(&a[..12], &d[..12]);
}

fn resolved_for_capsule(encrypt: bool, ecc: &str) -> audiostego::profile::ResolvedParams {
    let common = CommonEmbedParams {
        strategy: StrategyId::Qim,
        channel_mode: ChannelMode::Mid,
        lossy: LossyProfile::Off,
        output_format: OutputFormat::Wav,
        bitrate: 192,
        fft_size: Some(2048),
        hop_div: Some(1),
        band: Some("1000:6000".into()),
        strength: Some(0.12),
        shaping: Shaping::Masked,
        ecc: Some(ecc.into()),
        key: Some("capsule-key".into()),
        encrypt,
    };
    profile::resolve(&common, 44100, None, false).unwrap()
}

#[test]
fn capsule_header_bytes_roundtrip_and_tamper() {
    let params = resolved_for_capsule(false, "crc");
    let header = CapsuleHeader::from_params(&params, 32, 0.97);
    let bytes = header.to_bytes();
    assert_eq!(bytes.len(), HEADER_BYTES);
    let parsed = CapsuleHeader::from_bytes(&bytes).unwrap();
    assert_eq!(parsed.payload_len, 32);
    assert_eq!(parsed.fft_size, 2048);
    assert!(!parsed.encrypted());

    assert!(CapsuleHeader::from_bytes(&bytes[..10]).is_err());
    let mut bad_magic = bytes;
    bad_magic[0] = b'X';
    assert!(CapsuleHeader::from_bytes(&bad_magic).is_err());
    let mut bad_crc = header.to_bytes();
    bad_crc[44] ^= 0xff;
    assert!(CapsuleHeader::from_bytes(&bad_crc).is_err());
}

#[test]
fn capsule_body_plain_crc_and_encrypt_rs() {
    let msg = b"body payload";
    let plain = resolved_for_capsule(false, "crc");
    let body = capsule::build_body(&plain, msg).unwrap();
    let got = capsule::decode_body(false, &plain.key_bytes, EccMode::Crc, &body).unwrap();
    assert_eq!(got, msg);

    let enc = resolved_for_capsule(true, "rs:16");
    let body = capsule::build_body(&enc, msg).unwrap();
    let got = capsule::decode_body(
        true,
        &enc.key_bytes,
        EccMode::ReedSolomon { parity: 16 },
        &body,
    )
    .unwrap();
    assert_eq!(got, msg);
    assert!(
        capsule::decode_body(true, b"wrong", EccMode::ReedSolomon { parity: 16 }, &body).is_err()
    );
}

#[test]
fn bit_allocation_deterministic_and_capped() {
    let a = BitAllocation::plan(10, 8, 100, b"alloc-key");
    let b = BitAllocation::plan(10, 8, 100, b"alloc-key");
    assert_eq!(a.total_bits, 80); // capped by 10*8
    assert_eq!(a.frame_bits, b.frame_bits);
    let c = BitAllocation::plan(10, 8, 20, b"alloc-key");
    assert_eq!(c.total_bits, 20);
}
