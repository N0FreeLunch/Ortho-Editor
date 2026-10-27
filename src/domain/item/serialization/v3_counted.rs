// This software is licensed under the PolyForm Noncommercial License 1.0.0.
// Required Notice: Copyright 2026 N0FreeLunch (https://github.com/N0FreeLunch/d2r-core)

//! Counted V3 item ownership, socket recursion and exact source-span retention.

use super::D2R_V3_SOCKET_CHILD_MODE;
use crate::data::bit_cursor::BitCursor;
use crate::domain::item::{HuffmanTree, Item};
use crate::error::{ParsingError, ParsingFailure, ParsingResult};
use crate::wire::read_legacy_source_bits as read_alignment_padding_bits;
use bitstream_io::{BitRead, LittleEndian};
use std::io::Cursor;

pub(super) fn parse_item_at_d2r_v3(
    bytes: &[u8],
    bit: u64,
    huffman: &HuffmanTree,
    idx: usize,
    save_version: u32,
) -> ParsingResult<(Item, u64)> {
    let mut reader = bitstream_io::BitReader::endian(Cursor::new(bytes), LittleEndian);
    let _ = reader.skip(bit as u32);
    let mut cursor = BitCursor::new(reader);
    cursor.set_pos(bit);
    cursor.base_pos = 0;

    let mut item = Item::from_reader_with_context_d2r_v3(
        &mut cursor,
        huffman,
        Some((bytes, bit)),
        idx,
        save_version,
    )?;
    let unaligned_end = cursor.pos();
    cursor.byte_align()?;
    if let Some(auxiliary) = item.body.d2r_v3_auxiliary.as_mut() {
        auxiliary.alignment_padding = Some(read_alignment_padding_bits(
            bytes,
            unaligned_end,
            cursor.pos() - unaligned_end,
        ));
    }
    let consumed = cursor.pos().saturating_sub(bit);
    if consumed == 0 {
        return Err(cursor.fail(ParsingError::UnexpectedValue {
            field: "D2R v3 item length".to_string(),
            value: "0".to_string(),
            reason: "The counted-item reader consumed no source bits".to_string(),
        }));
    }

    let raw_bits = read_alignment_padding_bits(bytes, bit, consumed);
    if raw_bits.len() as u64 != consumed {
        return Err(cursor.fail(ParsingError::UnexpectedValue {
            field: "D2R v3 raw item span".to_string(),
            value: raw_bits.len().to_string(),
            reason: format!("Could not retain all {consumed} consumed source bits"),
        }));
    }

    item.bits = raw_bits
        .into_iter()
        .enumerate()
        .map(|(offset, value)| crate::domain::item::RecordedBit {
            bit: value,
            offset: bit.saturating_add(offset as u64),
        })
        .collect();
    item.range.start = bit;
    item.range.end = bit + consumed;
    item.total_bits = consumed;
    item.logical_width = Some(consumed);
    item.expected_start_bit = bit;
    item.record_parser_consumed_bits(consumed);

    let mut total_consumed = consumed;
    let filled_sockets = item
        .body
        .d2r_v3_auxiliary
        .as_ref()
        .map_or(0, |auxiliary| auxiliary.filled_sockets);
    let socket_capacity = item.sockets.unwrap_or_default();
    if filled_sockets > socket_capacity || (filled_sockets > 0 && !item.header.is_socketed) {
        return Err(cursor.fail(ParsingError::UnexpectedValue {
            field: "D2R v3 filled socket count".to_string(),
            value: filled_sockets.to_string(),
            reason: format!(
                "Declared children exceed capacity {socket_capacity} or lack the socketed flag"
            ),
        }));
    }
    if item.header.is_socketed {
        // Capacity describes available slots, not the number of child records.
        for _ in 0..filled_sockets {
            let child_start = bit.saturating_add(total_consumed);
            let (child, child_consumed) =
                parse_item_at_d2r_v3(bytes, child_start, huffman, idx, save_version)?;
            if child.header.mode != D2R_V3_SOCKET_CHILD_MODE {
                return Err(ParsingFailure {
                    error: ParsingError::UnexpectedValue {
                        field: "D2R v3 socket child mode".to_string(),
                        value: child.header.mode.to_string(),
                        reason: format!(
                            "Expected mode {D2R_V3_SOCKET_CHILD_MODE}; location does not establish child ownership"
                        ),
                    },
                    context_stack: vec!["parse_item_at_d2r_v3".to_string()],
                    bit_offset: child_start,
                    context_relative_offset: child_start.saturating_sub(bit),
                    hint: Some(
                        "Retain the source stream and verify the declared child boundary."
                            .to_string(),
                    ),
                });
            }

            total_consumed = total_consumed.saturating_add(child_consumed);
            item.bits.extend(child.bits.iter().cloned());
            item.socketed_items.push(child);
        }
    }

    item.num_socketed_items = item.socketed_items.len() as u8;
    if item.header.save_is_alpha && item.header.is_runeword && !item.socketed_items.is_empty() {
        item.sockets = Some(item.socketed_items.len() as u8);
    }
    // Counted V3 children belong to filled_sockets, without Alpha 317/320 synthesis.
    // Retain the original flags and the captured parent/child source spans.
    item.range.end = bit.saturating_add(total_consumed);
    item.total_bits = total_consumed;
    item.logical_width = Some(total_consumed);

    Ok((item, total_consumed))
}

pub(super) fn d2r_v3_list_failure(field: &str, bit_offset: u64, reason: &str) -> ParsingFailure {
    ParsingFailure {
        error: ParsingError::UnexpectedValue {
            field: field.to_string(),
            value: "invalid or unsupported".to_string(),
            reason: reason.to_string(),
        },
        context_stack: vec!["D2R v3 player list ownership".to_string()],
        bit_offset,
        context_relative_offset: 0,
        hint: Some(
            "Preserve the input and verify the section boundary; do not scan a later JM list."
                .to_string(),
        ),
    }
}

/// Reads exactly the caller-owned counted list, without searching other markers.
/// `section_offset` is a byte offset in `bytes`; item ranges remain absolute.
pub fn read_counted_list_d2r_v3(
    bytes: &[u8],
    section_offset: usize,
    huffman: &HuffmanTree,
    save_version: u32,
) -> ParsingResult<Vec<Item>> {
    let section_bit = (section_offset as u64).saturating_mul(8);
    let header = section_offset
        .checked_add(4)
        .and_then(|end| bytes.get(section_offset..end))
        .ok_or_else(|| {
            d2r_v3_list_failure(
                "D2R v3 counted list header",
                section_bit,
                "The owned list header is truncated",
            )
        })?;
    if &header[..2] != b"JM" {
        return Err(d2r_v3_list_failure(
            "D2R v3 counted list marker",
            section_bit,
            "The owned section must begin with JM",
        ));
    }
    let count = u16::from_le_bytes([header[2], header[3]]);
    let mut items = Vec::with_capacity(count as usize);
    let mut bit = section_bit + 32;
    for idx in 0..count as usize {
        let (item, consumed) = parse_item_at_d2r_v3(bytes, bit, huffman, idx, save_version)?;
        bit += consumed;
        items.push(item);
    }
    Ok(items)
}
