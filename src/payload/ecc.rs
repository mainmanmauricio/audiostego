//! Reed-Solomon and CRC helpers.

use crate::cli::EccMode;
use anyhow::{bail, Result};
use reed_solomon::{Decoder, Encoder};

pub fn protect(data: &[u8], mode: EccMode) -> Result<Vec<u8>> {
    match mode {
        EccMode::None => Ok(data.to_vec()),
        EccMode::Crc => {
            let mut out = data.to_vec();
            let crc = crc32fast::hash(data);
            out.extend_from_slice(&crc.to_le_bytes());
            Ok(out)
        }
        EccMode::ReedSolomon { parity } => {
            let mut body = data.to_vec();
            let crc = crc32fast::hash(data);
            body.extend_from_slice(&crc.to_le_bytes());
            let enc = Encoder::new(parity as usize);
            let max_data = 255usize.saturating_sub(parity as usize);
            if max_data < 1 {
                bail!("RS parity too large");
            }
            let mut out = Vec::new();
            // Length-prefix the whole body once, then chunk.
            let mut framed = Vec::with_capacity(2 + body.len());
            let len = body.len() as u16;
            framed.extend_from_slice(&len.to_le_bytes());
            framed.extend_from_slice(&body);
            for chunk in framed.chunks(max_data) {
                let encoded = enc.encode(chunk);
                // Store chunk data length (u8) then encoded bytes (chunk + parity).
                out.push(chunk.len() as u8);
                out.extend_from_slice(encoded.as_ref());
            }
            Ok(out)
        }
    }
}

pub fn recover(data: &[u8], mode: EccMode) -> Result<Vec<u8>> {
    match mode {
        EccMode::None => Ok(data.to_vec()),
        EccMode::Crc => {
            if data.len() < 4 {
                bail!("crc payload too short");
            }
            let (body, crc_bytes) = data.split_at(data.len() - 4);
            let mut arr = [0u8; 4];
            arr.copy_from_slice(crc_bytes);
            let expected = u32::from_le_bytes(arr);
            let got = crc32fast::hash(body);
            if expected != got {
                bail!("CRC mismatch (expected {expected:#x}, got {got:#x})");
            }
            Ok(body.to_vec())
        }
        EccMode::ReedSolomon { parity } => {
            let dec = Decoder::new(parity as usize);
            let mut framed = Vec::new();
            let mut i = 0usize;
            while i < data.len() {
                let chunk_len = data[i] as usize;
                i += 1;
                let block_len = chunk_len + parity as usize;
                if i + block_len > data.len() {
                    bail!("truncated RS block");
                }
                let block = &data[i..i + block_len];
                i += block_len;
                let corrected = dec
                    .correct(block, None)
                    .map_err(|e| anyhow::anyhow!("RS decode failed: {e:?}"))?;
                framed.extend_from_slice(&corrected.as_ref()[..chunk_len]);
            }
            if framed.len() < 2 {
                bail!("RS framed payload too short");
            }
            let msg_len = u16::from_le_bytes([framed[0], framed[1]]) as usize;
            if framed.len() < 2 + msg_len {
                bail!("RS length prefix exceeds data");
            }
            let body = &framed[2..2 + msg_len];
            if body.len() < 4 {
                bail!("RS recovered body too short");
            }
            let (msg, crc_bytes) = body.split_at(body.len() - 4);
            let mut arr = [0u8; 4];
            arr.copy_from_slice(crc_bytes);
            let expected = u32::from_le_bytes(arr);
            let got = crc32fast::hash(msg);
            if expected != got {
                bail!("CRC after RS mismatch");
            }
            Ok(msg.to_vec())
        }
    }
}
