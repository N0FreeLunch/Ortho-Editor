use bitstream_io::BitRead;
use serde::Serialize;
use crate::item::{Item, HuffmanTree, peek_item_header_at, is_plausible_item_header};
use crate::error::ParsingResult;

#[derive(Debug, Clone, Serialize)]
pub struct DesyncReport {
    pub item_index: usize,
    pub oracle_start: u64,
    pub parser_start: u64,
    pub drift: i64,
    pub oracle_code: String,
    pub parser_code: String,
    pub bit_dump: Option<String>,
    pub is_match: bool,
}

/// Detects bit-level drift between empirical item starts (Oracle) and parser-calculated starts.
pub fn detect_desync(bytes: &[u8], huffman: &HuffmanTree, is_alpha: bool) -> ParsingResult<Vec<DesyncReport>> {
    // 1. Find the JM section
    let jm_pos = (0..bytes.len().saturating_sub(1))
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .ok_or_else(|| crate::error::ParsingFailure {
            error: crate::error::ParsingError::MissingMarker { marker: "JM".to_string(), bit_offset: 0 },
            context_stack: vec!["detect_desync".to_string()],
            bit_offset: 0,
            context_relative_offset: 0,
            hint: Some("No JM marker found".to_string()),
        })?;

    let count = u16::from_le_bytes([bytes[jm_pos + 2], bytes[jm_pos + 3]]);
    let section_payload = &bytes[jm_pos + 4..];
    let section_base_bit = ((jm_pos + 4) * 8) as u64;

    // 2. Standard Parsing
    let parsed_items = match Item::read_section(section_payload, 0, count, huffman, is_alpha) {
        Ok(items) => items,
        Err(_) => Vec::new() 
    };

    // 3. Oracle Search
    let mut oracle_starts = Vec::new();
    let mut bit_idx = section_base_bit;
    let total_bits = (bytes.len() * 8) as u64;
    
    let mut found_count = 0;
    while bit_idx < total_bits.saturating_sub(100) && found_count < count as usize {
        if let Some((mode, location, _x, code, flags, version, _is_compact, _header_bits, _nudge)) = 
            peek_item_header_at(bytes, bit_idx, huffman, is_alpha) 
        {
            if is_plausible_item_header(mode, location, &code, flags, version, is_alpha) {
                if !is_alpha || version == 5 {
                    oracle_starts.push((bit_idx, code.trim().to_string()));
                    found_count += 1;
                    bit_idx += 72; // Jump minimum item size
                    continue;
                }
            }
        }
        bit_idx += 8;
    }

    // 4. Comparison
    let mut reports = Vec::new();
    let compare_count = parsed_items.len().min(oracle_starts.len());

    for i in 0..compare_count {
        let oracle_start = oracle_starts[i].0;
        let parser_start = section_base_bit + parsed_items[i].expected_start_bit;
        let drift = parser_start as i64 - oracle_start as i64;
        
        let bit_dump = if drift != 0 {
            Some(dump_bits_at(bytes, oracle_start, 64))
        } else {
            None
        };

        reports.push(DesyncReport {
            item_index: i,
            oracle_start,
            parser_start,
            drift,
            oracle_code: oracle_starts[i].1.clone(),
            parser_code: parsed_items[i].code.trim().to_string(),
            bit_dump,
            is_match: drift == 0,
        });
    }

    Ok(reports)
}

pub fn dump_bits_at(bytes: &[u8], start_bit: u64, count: u32) -> String {
    let mut result = String::new();
    let mut reader = bitstream_io::BitReader::endian(std::io::Cursor::new(bytes), bitstream_io::LittleEndian);
    if reader.skip(start_bit as u32).is_err() {
        return "ERROR: Offset out of bounds".to_string();
    }

    for i in 0..count {
        if i > 0 && i % 8 == 0 { result.push(' '); }
        match reader.read_bit() {
            Ok(true) => result.push('1'),
            Ok(false) => result.push('0'),
            Err(_) => break,
        }
    }
    result
}
