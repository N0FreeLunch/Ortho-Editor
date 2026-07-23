// This software is licensed under the PolyForm Noncommercial License 1.0.0.
// Required Notice: Copyright 2026 N0FreeLunch (https://github.com/N0FreeLunch/d2r-core)

use bitstream_io::{BitRead, BitReader, BitWrite, BitWriter, LittleEndian};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use thiserror::Error;

/// Oracle entry parameters required for validating and performing transport section injection.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct TransportOracleEntry {
    pub target_file: String,
    pub simulation_status: String,
    pub base_payload_bits: usize,
    pub target_payload_bits: usize,
    pub projected_delta_bits: isize,
    pub base_next_jm_bit_offset: usize,
    pub projected_next_jm_bit_offset: usize,
}

/// Result of a successful transport section 1 payload injection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportInjectResult {
    pub bytes: Vec<u8>,
    pub injected_bits: usize,
    pub target_item_count: u16,
    pub projected_next_jm_bit_offset: usize,
}

/// Typed error conditions encountered during transport injection assembly.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TransportInjectError {
    #[error("Target simulation status is '{status}', expected 'dry_run_success'")]
    IneligibleStatus { status: String },

    #[error("Target projected delta bits is {delta} <= 0 (zero payload or invalid); injection rejected")]
    IneligibleDelta { delta: isize },

    #[error("Save section mapping failed: {0}")]
    MappingFailed(String),

    #[error("Base save file has fewer than 2 JM markers (found {count})")]
    InvalidBaseJmCount { count: usize },

    #[error("Target save file has fewer than 2 JM markers (found {count})")]
    InvalidTargetJmCount { count: usize },

    #[error("Invalid base payload bit boundaries: start {start} > end {end}")]
    InvalidBasePayloadBoundaries { start: usize, end: usize },

    #[error("Base payload bit count mismatch: extracted {extracted} bits, oracle expected {expected} bits")]
    BasePayloadMismatch { extracted: usize, expected: usize },

    #[error("Target payload bit count mismatch: extracted {extracted} bits, oracle expected {expected} bits")]
    TargetPayloadMismatch { extracted: usize, expected: usize },

    #[error("Bitstream read error: {0}")]
    BitReadFailed(String),

    #[error("Bitstream write error: {0}")]
    BitWriteFailed(String),

    #[error("Projected next JM bit offset mismatch: assembled {assembled} bits, oracle expected {expected} bits")]
    ProjectedOffsetMismatch { assembled: usize, expected: usize },
}

/// Performs pure Section 1 payload injection in memory.
///
/// Takes raw bytes of base save and target save, plus verified oracle parameters.
/// Rebuilds bitstream payload, updates header file size, repairs checksum, and returns
/// the resulting save bytes.
pub fn inject_section1(
    base_bytes: &[u8],
    target_bytes: &[u8],
    oracle_entry: &TransportOracleEntry,
) -> Result<TransportInjectResult, TransportInjectError> {
    if oracle_entry.simulation_status != "dry_run_success" {
        return Err(TransportInjectError::IneligibleStatus {
            status: oracle_entry.simulation_status.clone(),
        });
    }

    if oracle_entry.projected_delta_bits <= 0 {
        return Err(TransportInjectError::IneligibleDelta {
            delta: oracle_entry.projected_delta_bits,
        });
    }

    // 1. Map base save sections
    let base_map = crate::save::map_core_sections(base_bytes)
        .map_err(|e| TransportInjectError::MappingFailed(e.to_string()))?;
    if base_map.jm_positions.len() < 2 {
        return Err(TransportInjectError::InvalidBaseJmCount {
            count: base_map.jm_positions.len(),
        });
    }

    let base_jm1_byte = base_map.jm_positions[0];
    let base_jm1_bit = base_jm1_byte * 8;
    let base_payload_start_bit = base_jm1_bit + 32;
    let base_payload_end_bit = oracle_entry.base_next_jm_bit_offset;

    if base_payload_end_bit < base_payload_start_bit {
        return Err(TransportInjectError::InvalidBasePayloadBoundaries {
            start: base_payload_start_bit,
            end: base_payload_end_bit,
        });
    }

    let actual_base_payload_bits = base_payload_end_bit - base_payload_start_bit;
    if actual_base_payload_bits != oracle_entry.base_payload_bits {
        return Err(TransportInjectError::BasePayloadMismatch {
            extracted: actual_base_payload_bits,
            expected: oracle_entry.base_payload_bits,
        });
    }

    // 2. Map target save sections
    let target_map = crate::save::map_core_sections(target_bytes)
        .map_err(|e| TransportInjectError::MappingFailed(e.to_string()))?;
    if target_map.jm_positions.len() < 2 {
        return Err(TransportInjectError::InvalidTargetJmCount {
            count: target_map.jm_positions.len(),
        });
    }

    let target_jm1_byte = target_map.jm_positions[0];
    let target_item_count = u16::from_le_bytes([
        target_bytes[target_jm1_byte + 2],
        target_bytes[target_jm1_byte + 3],
    ]);

    let target_payload_start_bit = target_jm1_byte * 8 + 32;
    let target_payload_end_bit = target_map.jm_positions[1] * 8;
    let actual_target_payload_bits =
        target_payload_end_bit.saturating_sub(target_payload_start_bit);

    if actual_target_payload_bits != oracle_entry.target_payload_bits {
        return Err(TransportInjectError::TargetPayloadMismatch {
            extracted: actual_target_payload_bits,
            expected: oracle_entry.target_payload_bits,
        });
    }

    let mut target_reader = BitReader::endian(Cursor::new(target_bytes), LittleEndian);
    target_reader
        .skip(target_payload_start_bit as u32)
        .map_err(|e| TransportInjectError::BitReadFailed(e.to_string()))?;

    let mut target_payload_bits = Vec::with_capacity(oracle_entry.target_payload_bits);
    for _ in 0..oracle_entry.target_payload_bits {
        let bit = target_reader
            .read_bit()
            .map_err(|e| TransportInjectError::BitReadFailed(e.to_string()))?;
        target_payload_bits.push(bit);
    }

    // 3. Assemble bitstream
    let mut writer = BitWriter::endian(Vec::new(), LittleEndian);

    for &b in &base_bytes[..base_jm1_byte] {
        writer
            .write::<8, u8>(b)
            .map_err(|e| TransportInjectError::BitWriteFailed(e.to_string()))?;
    }

    writer
        .write::<8, u8>(b'J')
        .map_err(|e| TransportInjectError::BitWriteFailed(e.to_string()))?;
    writer
        .write::<8, u8>(b'M')
        .map_err(|e| TransportInjectError::BitWriteFailed(e.to_string()))?;
    writer
        .write::<16, u16>(target_item_count)
        .map_err(|e| TransportInjectError::BitWriteFailed(e.to_string()))?;

    for &bit in &target_payload_bits {
        writer
            .write_bit(bit)
            .map_err(|e| TransportInjectError::BitWriteFailed(e.to_string()))?;
    }

    writer
        .byte_align()
        .map_err(|e| TransportInjectError::BitWriteFailed(e.to_string()))?;

    let current_bit_offset = writer
        .writer()
        .ok_or_else(|| TransportInjectError::BitWriteFailed("BitWriter buffer lost".to_string()))?
        .len()
        * 8;

    if current_bit_offset != oracle_entry.projected_next_jm_bit_offset {
        return Err(TransportInjectError::ProjectedOffsetMismatch {
            assembled: current_bit_offset,
            expected: oracle_entry.projected_next_jm_bit_offset,
        });
    }

    let base_jm2_byte = base_map.jm_positions[1];
    for &b in &base_bytes[base_jm2_byte..] {
        writer
            .write::<8, u8>(b)
            .map_err(|e| TransportInjectError::BitWriteFailed(e.to_string()))?;
    }

    let mut result_bytes = writer.into_writer();

    // Fix file size in header (offset 8..12)
    let file_size = result_bytes.len() as u32;
    result_bytes[8..12].copy_from_slice(&file_size.to_le_bytes());

    // Fix checksum
    crate::engine::checksum::Checksum::fix(&mut result_bytes);

    Ok(TransportInjectResult {
        bytes: result_bytes,
        injected_bits: oracle_entry.target_payload_bits,
        target_item_count,
        projected_next_jm_bit_offset: current_bit_offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE_AMAZON_EMPTY: &[u8] =
        include_bytes!("../../tests/fixtures/savegames/original/amazon_empty.d2s");
    const TARGET_AMAZON_10_SCROLLS: &[u8] =
        include_bytes!("../../tests/fixtures/savegames/original/amazon_10_scrolls.d2s");
    const TARGET_AMAZON_ACT2_START: &[u8] =
        include_bytes!("../../tests/fixtures/savegames/original/amazon_v105_act2_start.d2s");
    const TARGET_TESTAMAZON: &[u8] =
        include_bytes!("../../tests/fixtures/savegames/original/TESTAMAZON.d2s");

    #[test]
    fn transport_injection_positive_fixture_pair() {
        let entry = TransportOracleEntry {
            target_file: ".\\tests\\fixtures\\savegames\\original\\amazon_10_scrolls.d2s".into(),
            simulation_status: "dry_run_success".into(),
            base_payload_bits: 320,
            target_payload_bits: 1368,
            projected_delta_bits: 1048,
            base_next_jm_bit_offset: 7576,
            projected_next_jm_bit_offset: 8624,
        };

        let res = inject_section1(BASE_AMAZON_EMPTY, TARGET_AMAZON_10_SCROLLS, &entry)
            .expect("Injection should succeed");

        assert_eq!(res.injected_bits, 1368);
        assert_eq!(res.projected_next_jm_bit_offset, 8624);

        // Verify header file size match
        let file_size = u32::from_le_bytes(res.bytes[8..12].try_into().unwrap()) as usize;
        assert_eq!(res.bytes.len(), file_size);

        // Verify checksum is valid
        let stored_checksum = u32::from_le_bytes(res.bytes[12..16].try_into().unwrap());
        let calc_checksum = crate::engine::checksum::recalculate_checksum(&res.bytes).unwrap();
        assert_eq!(stored_checksum, calc_checksum);
    }

    #[test]
    fn transport_injection_zero_delta_rejection() {
        let entry = TransportOracleEntry {
            target_file: ".\\tests\\fixtures\\savegames\\original\\amazon_v105_act2_start.d2s".into(),
            simulation_status: "dry_run_success".into(),
            base_payload_bits: 320,
            target_payload_bits: 320,
            projected_delta_bits: 0,
            base_next_jm_bit_offset: 7576,
            projected_next_jm_bit_offset: 7576,
        };

        let err = inject_section1(BASE_AMAZON_EMPTY, TARGET_AMAZON_ACT2_START, &entry)
            .expect_err("Zero-delta injection should be rejected");

        assert_eq!(err, TransportInjectError::IneligibleDelta { delta: 0 });
    }

    #[test]
    fn transport_injection_negative_delta_rejection() {
        let entry = TransportOracleEntry {
            target_file: ".\\tests\\fixtures\\savegames\\original\\TESTAMAZON.d2s".into(),
            simulation_status: "dry_run_success".into(),
            base_payload_bits: 320,
            target_payload_bits: 0,
            projected_delta_bits: -320,
            base_next_jm_bit_offset: 7576,
            projected_next_jm_bit_offset: 7256,
        };

        let err = inject_section1(BASE_AMAZON_EMPTY, TARGET_TESTAMAZON, &entry)
            .expect_err("Negative-delta injection should be rejected");

        assert_eq!(err, TransportInjectError::IneligibleDelta { delta: -320 });
    }
}
