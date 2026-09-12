//! Self-describing payload capsule header + body.

use crate::cli::{ChannelMode, EccMode, Shaping, StrategyId};
use crate::payload::{bits, crypto, ecc};
use crate::profile::ResolvedParams;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const MAGIC: &[u8; 4] = b"ASTG";
pub const VERSION: u8 = 1;
/// Fixed-size binary header embedded via QIM (independent of body strategy).
pub const HEADER_BYTES: usize = 48;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapsuleHeader {
    pub version: u8,
    pub strategy: StrategyId,
    pub channel_mode: ChannelMode,
    pub fft_size: u32,
    pub hop_div: u8,
    pub band_lo_hz: f32,
    pub band_hi_hz: f32,
    pub strength: f32,
    pub shaping: Shaping,
    pub ecc: EccMode,
    pub scale_factor: f32,
    pub sample_rate: u32,
    pub payload_len: u32,
    pub flags: u8, // bit0 = encrypted
    pub header_crc: u32,
}

impl CapsuleHeader {
    pub fn from_params(params: &ResolvedParams, payload_len: u32, scale_factor: f32) -> Self {
        let mut h = Self {
            version: VERSION,
            strategy: params.strategy,
            channel_mode: params.channel_mode.clone(),
            fft_size: params.fft_size as u32,
            hop_div: params.hop_div as u8,
            band_lo_hz: params.band_lo_hz,
            band_hi_hz: params.band_hi_hz,
            strength: params.strength,
            shaping: params.shaping,
            ecc: params.ecc,
            scale_factor,
            sample_rate: params.sample_rate,
            payload_len,
            flags: if params.encrypt { 1 } else { 0 },
            header_crc: 0,
        };
        let bytes = h.to_bytes_uncrcd();
        h.header_crc = crc32fast::hash(&bytes[..HEADER_BYTES - 4]);
        h
    }

    fn to_bytes_uncrcd(&self) -> [u8; HEADER_BYTES] {
        let mut b = [0u8; HEADER_BYTES];
        b[0..4].copy_from_slice(MAGIC);
        b[4] = self.version;
        b[5] = self.strategy.to_u8();
        b[6] = self.channel_mode.to_code();
        b[7] = self.hop_div;
        b[8..12].copy_from_slice(&self.fft_size.to_le_bytes());
        b[12..16].copy_from_slice(&self.band_lo_hz.to_le_bytes());
        b[16..20].copy_from_slice(&self.band_hi_hz.to_le_bytes());
        b[20..24].copy_from_slice(&self.strength.to_le_bytes());
        b[24] = match self.shaping {
            Shaping::Fixed => 0,
            Shaping::Masked => 1,
        };
        b[25] = self.ecc.to_code();
        b[26] = self.flags;
        b[27] = 0; // reserved
        b[28..32].copy_from_slice(&self.scale_factor.to_le_bytes());
        b[32..36].copy_from_slice(&self.sample_rate.to_le_bytes());
        b[36..40].copy_from_slice(&self.payload_len.to_le_bytes());
        // 40..44 reserved
        // 44..48 crc filled by caller
        b
    }

    pub fn to_bytes(&self) -> [u8; HEADER_BYTES] {
        let mut b = self.to_bytes_uncrcd();
        b[44..48].copy_from_slice(&self.header_crc.to_le_bytes());
        b
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self> {
        if b.len() < HEADER_BYTES {
            bail!("capsule header too short");
        }
        if &b[0..4] != MAGIC {
            bail!("bad capsule magic");
        }
        let crc_stored = u32::from_le_bytes(b[44..48].try_into().unwrap());
        let crc_got = crc32fast::hash(&b[..44]);
        if crc_stored != crc_got {
            bail!("capsule header CRC mismatch");
        }
        let version = b[4];
        if version != VERSION {
            bail!("unsupported capsule version {version}");
        }
        let strategy = StrategyId::from_u8(b[5]).context("bad strategy id")?;
        let channel_mode = ChannelMode::from_code(b[6]).context("bad channel mode")?;
        let hop_div = b[7];
        let fft_size = u32::from_le_bytes(b[8..12].try_into().unwrap());
        let band_lo_hz = f32::from_le_bytes(b[12..16].try_into().unwrap());
        let band_hi_hz = f32::from_le_bytes(b[16..20].try_into().unwrap());
        let strength = f32::from_le_bytes(b[20..24].try_into().unwrap());
        let shaping = match b[24] {
            0 => Shaping::Fixed,
            _ => Shaping::Masked,
        };
        let ecc = EccMode::from_code(b[25]).context("bad ecc")?;
        let flags = b[26];
        let scale_factor = f32::from_le_bytes(b[28..32].try_into().unwrap());
        let sample_rate = u32::from_le_bytes(b[32..36].try_into().unwrap());
        let payload_len = u32::from_le_bytes(b[36..40].try_into().unwrap());
        Ok(Self {
            version,
            strategy,
            channel_mode,
            fft_size,
            hop_div,
            band_lo_hz,
            band_hi_hz,
            strength,
            shaping,
            ecc,
            scale_factor,
            sample_rate,
            payload_len,
            flags,
            header_crc: crc_stored,
        })
    }

    pub fn encrypted(&self) -> bool {
        self.flags & 1 != 0
    }
}

/// Build the full bit payload: protected (+ optional encrypted) message bits.
pub fn build_body(params: &ResolvedParams, message: &[u8]) -> Result<Vec<u8>> {
    let plain = if params.encrypt {
        crypto::encrypt(&params.key_bytes, message)?
    } else {
        message.to_vec()
    };
    ecc::protect(&plain, params.ecc)
}

pub fn decode_body(
    params_encrypt: bool,
    key: &[u8],
    ecc_mode: EccMode,
    body: &[u8],
) -> Result<Vec<u8>> {
    let recovered = ecc::recover(body, ecc_mode)?;
    if params_encrypt {
        crypto::decrypt(key, &recovered)
    } else {
        Ok(recovered)
    }
}

pub fn header_bits(header: &CapsuleHeader) -> Vec<bool> {
    bits::bytes_to_bools(&header.to_bytes())
}

pub fn body_bits(body: &[u8]) -> Vec<bool> {
    bits::bytes_to_bools(body)
}
