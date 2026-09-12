//! Bit packing helpers.

use bitvec::prelude::*;

pub fn bytes_to_bits(data: &[u8]) -> BitVec<u8, Msb0> {
    BitVec::from_iter(
        data.iter()
            .flat_map(|b| (0..8).rev().map(move |i| (b >> i) & 1 == 1)),
    )
}

pub fn bits_to_bytes(bits: &BitSlice<u8, Msb0>) -> Vec<u8> {
    let mut out = Vec::with_capacity((bits.len() + 7) / 8);
    for chunk in bits.chunks(8) {
        let mut byte = 0u8;
        for (i, bit) in chunk.iter().enumerate() {
            if *bit {
                byte |= 1 << (7 - i);
            }
        }
        out.push(byte);
    }
    out
}

pub fn bools_to_bytes(bits: &[bool]) -> Vec<u8> {
    let mut bv = BitVec::<u8, Msb0>::with_capacity(bits.len());
    for b in bits {
        bv.push(*b);
    }
    bits_to_bytes(&bv)
}

pub fn bytes_to_bools(data: &[u8]) -> Vec<bool> {
    bytes_to_bits(data).into_iter().collect()
}
