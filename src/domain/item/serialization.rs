use bitstream_io::{BitRead, BitWrite, BitWriter, LittleEndian};
use std::io::{self, Cursor};
use crate::domain::item::Item;
use crate::domain::item::quality::ItemQuality;
use crate::domain::stats::{ItemProperty, StatsAxiom, ItemStats};
use crate::data::bit_cursor::BitCursor;
use crate::error::{ParsingResult, ParsingError, ParsingFailure};
use crate::domain::header::entity::{ItemSegmentType, ItemHeader};
use crate::domain::stats::{read_property_list, stat_save_bits};
use crate::domain::item::axiom_meta::{ForensicAudit, ForensicAxiom};
use crate::domain::forensic::v105::{V105NudgeAxiom, V105ShadowAxiom, V105HeaderGapAxiom};
use crate::domain::item::{ItemBitRange, RecordedBit, ItemBody, BitSegment, ItemModule};

pub fn find_next_item_match(bytes: &[u8], pos: u64, huffman: &HuffmanTree, alpha: bool) -> Option<u64> {
    let limit = (bytes.len() * 8) as u64;
    let mut probe = pos;
    while probe < limit {
        if let Some((mode, location, _x, code, flags, version, _is_compact, _header_bits, _nudge)) = peek_item_header_at(bytes, probe, huffman, alpha) {
            if is_plausible_item_header(mode, location, &code, flags, version, alpha) {
                return Some(probe);
            }
        }
        probe += 1;
    }
    None
}

pub fn is_plausible_item_header(
    mode: u8,
    location: u8,
    code: &str,
    _flags: u32,
    version: u8,
    alpha_mode: bool,
) -> bool {
    if code.len() < 3 { return false; }
    if !code.chars().all(|c| c.is_alphanumeric() || c == ' ') { return false; }
    if alpha_mode {
        if code.len() != 4 { return false; }
        if code.trim().is_empty() { return false; }
        if version > 7 { return false; }
        if mode > 15 || location > 15 { return false; }
    } else {
        if version > 2 { return false; }
        if mode > 6 || location > 15 { return false; }
    }
    true
}

pub fn peek_item_header_at(
    section_bytes: &[u8],
    start_bit: u64,
    huffman: &HuffmanTree,
    alpha_mode: bool,
) -> Option<(u8, u8, u8, String, u32, u8, bool, u64, i8)> {
    let mut reader = bitstream_io::BitReader::endian(Cursor::new(section_bytes), LittleEndian);
    if reader.skip(start_bit as u32).is_err() { return None; }

    let flags = reader.read::<32, u32>().ok()?;
    if !alpha_mode && (flags & 0xFFFF) != 0x4D4A {
        return None;
    }
    let version = reader.read::<3, u8>().ok()?;
    let mode = reader.read::<3, u8>().ok()?;
    let loc = reader.read::<3, u8>().ok()?;
    let x = reader.read::<4, u8>().ok()?;
    
    let axiom = StatsAxiom::new(version, ItemQuality::Normal, alpha_mode);
    let is_compact = axiom.is_compact(flags);
    let mut header_len = 32 + 3 + 3 + 3 + 4; 
    
    let geometry = axiom.header_geometry(flags, is_compact);

    if geometry.has_header_gap {
        if version == 5 || version == 0 {
            let axiom = StatsAxiom::new(version, ItemQuality::Normal, alpha_mode);
            let is_v105_shadow = axiom.is_v105_shadow(flags);
            let is_rw = axiom.is_runeword(flags);
            if is_rw || is_v105_shadow {
                header_len += 8; 
            } else {
                header_len += geometry.y_bits + geometry.page_bits + geometry.socket_hint_bits + 8;
            }
        } else {
            header_len += geometry.y_bits + geometry.page_bits + geometry.socket_hint_bits + 8;
        }
    } else if !geometry.skip_geometry {
        header_len += geometry.y_bits + geometry.page_bits + geometry.socket_hint_bits;
    }

    let mut code = String::new();
    let mut n_reader = bitstream_io::BitReader::endian(Cursor::new(section_bytes), LittleEndian);
    if n_reader.skip(start_bit as u32 + header_len as u32).is_err() { 
        return None; 
    }
    let mut n_cursor = BitCursor::new(n_reader);
    
    for _ in 0..4 {
        if let Ok(ch) = huffman.decode_recorded(&mut n_cursor) { code.push(ch); }
        else { return None; }
    }
    
    if is_plausible_item_header(mode, loc, &code, flags, version, alpha_mode) {
        return Some((mode, loc, x, code, flags, version, is_compact, header_len as u64, 0));
    }
    None
}

pub fn parse_item_at(
    bytes: &[u8],
    bit: u64,
    huffman: &HuffmanTree,
    idx: usize,
    alpha: bool,
) -> ParsingResult<(Item, u64)> {
    parse_item_at_with_limit(bytes, bit, huffman, idx, alpha, None)
}

pub fn parse_item_at_with_limit(
    bytes: &[u8],
    bit: u64,
    huffman: &HuffmanTree,
    _idx: usize,
    alpha: bool,
    limit: Option<u64>,
) -> ParsingResult<(Item, u64)> {
    let mut reader = bitstream_io::BitReader::endian(Cursor::new(bytes), LittleEndian);
    let _ = reader.skip(bit as u32);
    let mut cursor = BitCursor::new(reader);
    if let Some(l) = limit {
        cursor.set_limit(l);
    }
    let item = Item::from_reader_with_context(&mut cursor, huffman, Some((bytes, bit)), alpha)?;
    Ok((item, cursor.pos()))
}

pub fn read_player_items(bytes: &[u8], huffman: &HuffmanTree, alpha: bool) -> ParsingResult<Vec<Item>> {
    let mut all_items = Vec::new();
    let mut jm_positions = Vec::new();

    for i in 0..bytes.len().saturating_sub(3) {
        if bytes[i] == b'J' && bytes[i + 1] == b'M' {
            jm_positions.push(i);
        }
    }

    if jm_positions.is_empty() {
        return Err(ParsingFailure {
            error: ParsingError::MissingMarker { marker: "JM".to_string(), bit_offset: 0 },
            context_stack: vec!["read_player_items".to_string()],
            bit_offset: 0,
            context_relative_offset: 0,
            hint: Some("Could not find any JM markers.".to_string()),
        });
    }

    let mut real_jm_positions = Vec::new();
    for &pos in &jm_positions {
        if bytes.len() < pos + 4 { continue; }
        let count = u16::from_le_bytes([bytes[pos + 2], bytes[pos + 3]]);
        if count > 100 { continue; } 
        
        if count == 0 {
            real_jm_positions.push(pos);
        } else {
            let payload_start = pos + 4;
            if let Some((mode, loc, _, code, flags, v, _, _, _)) = peek_item_header_at(bytes, (payload_start * 8) as u64, huffman, alpha) {
                if is_plausible_item_header(mode, loc, &code, flags, v, alpha) {
                    real_jm_positions.push(pos);
                }
            }
        }
    }

    for (idx, &start_offset) in real_jm_positions.iter().enumerate() {
        let count = u16::from_le_bytes([bytes[start_offset + 2], bytes[start_offset + 3]]);
        let payload_start = start_offset + 4;
        
        let next_jm = real_jm_positions.get(idx + 1).cloned().unwrap_or(bytes.len());
        let section_bytes = &bytes[payload_start..next_jm];
        
        match Item::read_section(section_bytes, count, huffman, alpha) {
            Ok(mut items) => {
                all_items.append(&mut items);
            }
            Err(e) => {
                if !alpha { return Err(e); }
            }
        }
    }

    Ok(all_items)
}

pub fn from_bytes(bytes: &[u8], huffman: &HuffmanTree, alpha: bool) -> ParsingResult<Item> {
    let (item, _) = parse_item_at_with_limit(bytes, 0, huffman, 0, alpha, None)?;
    Ok(item)
}

impl Item {
    pub fn from_bytes(bytes: &[u8], huffman: &HuffmanTree, alpha: bool) -> ParsingResult<Item> {
        crate::domain::item::serialization::from_bytes(bytes, huffman, alpha)
    }

    pub fn read_player_items(bytes: &[u8], huffman: &HuffmanTree, alpha: bool) -> ParsingResult<Vec<Item>> {
        crate::domain::item::serialization::read_player_items(bytes, huffman, alpha)
    }

    pub fn read_section(section_bytes: &[u8], top_level_count: u16, huffman: &HuffmanTree, alpha_mode: bool) -> ParsingResult<Vec<Item>> {
        let mut items: Vec<Item> = Vec::new();
        let mut bit_pos = 0;
        let mut pending_gap_bits = Vec::new();
        let section_bits = (section_bytes.len() * 8) as u64;

        while items.len() < top_level_count as usize && bit_pos < section_bits {
            let start = if alpha_mode {
                find_next_item_match(section_bytes, bit_pos, huffman, alpha_mode).unwrap_or(bit_pos)
            } else {
                bit_pos
            };

            if start > bit_pos {
                let mut bit_reader = bitstream_io::BitReader::endian(Cursor::new(section_bytes), LittleEndian);
                if bit_reader.skip(bit_pos as u32).is_ok() {
                    for _ in 0..(start - bit_pos) {
                        if let Ok(b) = bit_reader.read_bit() {
                            pending_gap_bits.push(b);
                        }
                    }
                }
            }

            match parse_item_at_with_limit(section_bytes, start, huffman, items.len(), alpha_mode, Some(section_bits - start)) {
                Ok((item, consumed_bits)) => {
                    let axiom = StatsAxiom::new(item.header.version, item.header.quality.unwrap_or(ItemQuality::Normal), alpha_mode)
                        .with_personalization(item.header.is_personalized);
                    let final_consumed = axiom.calculate_alignment(consumed_bits, item.header.is_compact, &item.code, item.header.flags);

                    let end = start + final_consumed;
                    let mut final_item = item;
                    final_item.range.start = start;
                    final_item.range.end = end;
                    
                    let mut gap_recorded = Vec::new();
                    for &b in &pending_gap_bits {
                        gap_recorded.push(b);
                    }
                    final_item.gap_bits = gap_recorded;
                    pending_gap_bits = Vec::new();

                    let mut actual_bits = Vec::new();
                    let mut bit_reader = bitstream_io::BitReader::endian(Cursor::new(section_bytes), LittleEndian);
                    if bit_reader.skip(start as u32).is_ok() {
                        for i in 0..final_consumed {
                            if let Ok(b) = bit_reader.read_bit() {
                                let recorded = RecordedBit { bit: b, offset: start + i };
                                actual_bits.push(recorded);
                                if alpha_mode && i >= consumed_bits {
                                    final_item.body.alpha_alignment_padding.push(b);
                                }
                            }
                        }
                    }
                    final_item.bits = actual_bits;

                    let mut next_bit_pos = end;
                    if final_item.header.is_socketed && final_item.sockets.unwrap_or(0) > 0 {
                        let remaining = section_bits.saturating_sub(end);
                        if let Some((children, children_end)) = scan_socket_children(section_bytes, end, huffman, items.len(), alpha_mode, remaining) {
                            final_item.socketed_items = children;
                            next_bit_pos = children_end;
                        }
                    }
                    
                    bit_pos = next_bit_pos;
                    items.push(final_item);
                }
                Err(e) => {
                    if alpha_mode {
                        if let Some(next_real_start) = find_next_item_match(section_bytes, start + 1, huffman, alpha_mode) {
                            bit_pos = next_real_start;
                            continue;
                        }
                    }
                    if let ParsingError::Io(ref s) = e.error {
                        if s.contains("Bit limit exceeded") || s.contains("unexpected end of file") { break; }
                    }
                    if !alpha_mode { return Err(e); }
                    bit_pos = start + 8;
                }
            }
        }
        Ok(items)
    }

    pub fn from_reader<R: BitRead>(
        reader: &mut R,
        huffman: &HuffmanTree,
        alpha: bool,
    ) -> ParsingResult<Item> {
        let mut cursor = BitCursor::new(reader);
        Self::from_reader_with_context(&mut cursor, huffman, None, alpha)
    }

    pub fn from_reader_with_context<R: BitRead>(
        cursor: &mut BitCursor<R>,
        huff: &HuffmanTree,
        ctx: Option<(&[u8], u64)>,
        alpha_mode: bool,
    ) -> ParsingResult<Item> {
        cursor.set_trace(crate::item::item_trace_enabled());
        let start_bit = cursor.pos();
        cursor.begin_segment(ItemSegmentType::Root);
        cursor.begin_segment(ItemSegmentType::Header);

        let flags = cursor.read_bits::<u32>(32)?;
        if !alpha_mode && (flags & 0xFFFF) != 0x4D4A {
             return Err(cursor.fail(ParsingError::MissingMarker { marker: "JM".to_string(), bit_offset: start_bit }));
        }

        let version = cursor.read_bits::<u8>(3)? as u8;
        let mode = cursor.read_bits::<u8>(3)? as u8;
        let location = cursor.read_bits::<u8>(3)? as u8;
        let x = cursor.read_bits::<u8>(4)? as u8;
        
        let axiom = StatsAxiom::new(version, ItemQuality::Normal, alpha_mode);
        let is_compact = axiom.is_compact(flags);
        
        let mut y = 0;
        let mut page = 0;
        let mut header_socket_hint = 0;

        let geometry = axiom.header_geometry(flags, is_compact);

        let mut alpha_header_gap = None;
        if geometry.has_header_gap {
            if version == 5 || version == 0 {
                let is_v105_shadow = axiom.is_v105_shadow(flags);
                let is_rw = axiom.is_runeword(flags);

                if is_rw || is_v105_shadow {
                    let gap = cursor.read_bits::<u8>(8)? as u8;
                    alpha_header_gap = Some(gap);
                    if !is_compact {
                        y = (gap & 0x0F) as u8;
                        page = ((gap >> 4) & 0x07) as u8;
                        header_socket_hint = ((gap >> 7) & 0x01) as u8;
                    }
                } else {
                    if !is_compact {
                        y = cursor.read_bits::<u8>(geometry.y_bits)? as u8;
                        page = cursor.read_bits::<u8>(geometry.page_bits)? as u8;
                        header_socket_hint = cursor.read_bits::<u8>(geometry.socket_hint_bits)? as u8;
                    }
                    alpha_header_gap = Some(cursor.read_bits::<u8>(8)? as u8);
                }
            } else {
                y = cursor.read_bits::<u8>(geometry.y_bits)? as u8;
                page = cursor.read_bits::<u8>(geometry.page_bits)? as u8;
                header_socket_hint = cursor.read_bits::<u8>(geometry.socket_hint_bits)? as u8;
                alpha_header_gap = Some(cursor.read_bits::<u8>(8)? as u8);
            }
        } else if !geometry.skip_geometry {
            y = cursor.read_bits::<u8>(geometry.y_bits)? as u8;
            page = cursor.read_bits::<u8>(geometry.page_bits)? as u8;
            header_socket_hint = cursor.read_bits::<u8>(geometry.socket_hint_bits)? as u8;
        }
        cursor.end_segment();

        let is_ear = (flags & (1 << 24)) != 0;
        let (code, alpha_nudge, ear_class, ear_level, ear_player_name) = if is_ear {
            cursor.begin_segment(ItemSegmentType::Unknown);
            let class = Some(cursor.read_bits::<u8>(3)? as u8);
            let level = Some(cursor.read_bits::<u8>(7)? as u8);
            let name = Some(read_player_name(cursor, alpha_mode && version == 5)?);
            if alpha_mode && version == 5 { cursor.byte_align()?; }
            cursor.end_segment();
            (String::new(), None, class, level, name)
        } else {
            cursor.begin_segment(ItemSegmentType::Code);
            let mut code = String::new();
            for _ in 0..4 {
                code.push(huff.decode_recorded(cursor)?);
            }
            let mut nudge = None;
            if alpha_mode && (version == 5 || version == 0 || version == 1) {
                nudge = Some(cursor.read_bits::<u8>(2)?);
            }
            cursor.end_segment();
            (code, nudge, None, None, None)
        };

        let stats_data = if !is_compact {
            let socket_flag = axiom.is_socketed(flags, is_compact);
            let runeword_flag = axiom.is_runeword(flags);
            Self::read_extended_stats(cursor, &code, is_compact, socket_flag, runeword_flag, axiom.is_personalized(flags), version, alpha_mode, axiom.is_fragment(flags), &axiom)?
        } else {
            (None, None, None, false, None, false, None, None, None, None, None, None, [None; 6], None, None, None, None, None, false, None, None, None, None, None, 0, None, None, None, None)
        };

        let mut item = Item {
            body: ItemBody {
                code: code.clone(),
                x, y, page, location, mode,
                defense: stats_data.19,
                max_durability: stats_data.20,
                current_durability: stats_data.21,
                quantity: stats_data.22,
                v5_runeword_extra: stats_data.25,
                v105_7mgw_payload: None,
                alpha_nudge,
                alpha_header_gap,
                alpha_set_list_val: stats_data.28,
                alpha_shadow_skip_bits: None,
                alpha_alignment_padding: Vec::new(),
            },
            stats: ItemStats { properties: Vec::new(), set_attributes: Vec::new(), runeword_attributes: Vec::new() },
            bits: Vec::new(),
            code: code.clone(),
            defense: stats_data.19,
            max_durability: stats_data.20,
            current_durability: stats_data.21,
            quantity: stats_data.22,
            ear_class, ear_level, ear_player_name,
            personalized_player_name: stats_data.16,
            has_multiple_graphics: stats_data.3, multi_graphics_bits: stats_data.4,
            has_class_specific_data: stats_data.5, class_specific_bits: stats_data.6,
            low_high_graphic_bits: stats_data.7,
            magic_prefix: stats_data.8, magic_suffix: stats_data.9,
            rare_name_1: stats_data.10, rare_name_2: stats_data.11, rare_affixes: stats_data.12,
            unique_id: stats_data.13, runeword_id: stats_data.14, runeword_level: stats_data.15,
            properties: Vec::new(), set_attributes: Vec::new(), runeword_attributes: Vec::new(),
            num_socketed_items: 0, socketed_items: Vec::new(),
            timestamp_flag: stats_data.18,
            properties_complete: false,
            terminator_bit: false,
            header: ItemHeader {
                flags, version, mode, location, x, y, page, socket_hint: header_socket_hint,
                id: stats_data.0, level: stats_data.1, quality: stats_data.2,
                is_compact: axiom.is_compact(flags),
                is_identified: axiom.is_identified(flags),
                is_socketed: axiom.is_socketed(flags, is_compact),
                is_personalized: axiom.is_personalized(flags),
                is_runeword: axiom.is_runeword(flags),
                is_ethereal: axiom.is_ethereal(flags),
                is_ear,
                alpha_quality_raw: stats_data.26,
                alpha_v5_runeword_extra: stats_data.25,
                alpha_unique_id_raw: stats_data.27,
            },
            set_list_count: stats_data.24,
            tbk_ibk_teleport: stats_data.17,
            sockets: stats_data.23,
            modules: Vec::new(),
            range: ItemBitRange { start: start_bit, end: 0 },
            total_bits: 0,
            gap_bits: Vec::new(),
            segments: Vec::new(),
            expected_start_bit: 0,
            forensic_audit: ForensicAudit::new(),
        };

        if item.body.alpha_nudge.is_some() { item.forensic_audit.record(V105NudgeAxiom.metadata()); }
        if item.body.alpha_header_gap.is_some() { item.forensic_audit.record(V105HeaderGapAxiom.metadata()); }
        if item.body.alpha_shadow_skip_bits.is_some() { item.forensic_audit.record(V105ShadowAxiom.metadata()); }

        if !axiom.is_compact(item.header.flags) {
            let is_v105_shadow = axiom.is_v105_shadow(item.header.flags);
            let (props, complete, term, extra_bits, reserved_7mgw, shadow_bits) = read_item_stats(cursor, &item.code, item.header.version, ctx, huff, alpha_mode, item.header.quality, item.header.is_runeword, is_v105_shadow, item.header.is_personalized)?;
            item.properties = props.clone();
            item.stats.properties = props;
            item.properties_complete = complete;
            item.terminator_bit = term;
            item.body.alpha_shadow_skip_bits = shadow_bits;
            if let Some(extra) = extra_bits {
                item.header.alpha_v5_runeword_extra = Some(extra);
                item.body.v5_runeword_extra = Some(extra);
            }
            if let Some(res) = reserved_7mgw { item.body.v105_7mgw_payload = Some(res); }
        }

        let axiom = StatsAxiom::new(version, item.header.quality.unwrap_or(ItemQuality::Normal), alpha_mode)
            .with_personalization(item.header.is_personalized);
        let consumed_bits = cursor.pos() - start_bit;
        let final_consumed = axiom.calculate_alignment(consumed_bits, is_compact, &item.code, item.header.flags);
        if final_consumed > consumed_bits {
            let padding_count = (final_consumed - consumed_bits) as u32;
            let padding = cursor.with_context("AlphaAlignmentPadding", |c| {
                let mut bits = Vec::new();
                for _ in 0..padding_count { bits.push(c.read_bit()?); }
                Ok(bits)
            })?;
            item.body.alpha_alignment_padding = padding;
        }
        
        item.range.end = cursor.pos();
        item.total_bits = item.range.end - item.range.start;
        
        let start_idx = start_bit as usize;
        let end_idx = cursor.pos() as usize;
        if end_idx <= cursor.recorded_bits().len() {
             item.bits = cursor.recorded_bits()[start_idx..end_idx].to_vec();
        }
        
        item.segments = cursor.segments().iter()
            .filter(|s| s.start >= start_bit && s.end <= cursor.pos())
            .cloned()
            .collect();

        cursor.end_segment();
        Ok(item)
    }

    fn read_extended_stats<R: BitRead>(
        cursor: &mut BitCursor<R>,
        code: &str,
        is_compact: bool,
        is_socketed_flag: bool,
        is_runeword: bool,
        is_personalized: bool,
        version: u8,
        alpha_mode: bool,
        is_fragment: bool,
        axiom: &StatsAxiom,
    ) -> ParsingResult<(
        Option<u32>, Option<u8>, Option<ItemQuality>,
        bool, Option<u8>, bool, Option<u16>,
        Option<u8>, Option<u16>, Option<u16>,
        Option<u8>, Option<u8>, [Option<u16>; 6],
        Option<u16>, Option<u16>, Option<u8>,
        Option<String>, Option<u8>, bool,
        Option<u32>, Option<u32>, Option<u32>, Option<u32>,
        Option<u8>, u8, Option<u8>, Option<u8>, Option<u16>, Option<u8>
    )> {
        cursor.begin_segment(ItemSegmentType::ExtendedStats);
        let trimmed_code = code.trim();
        let is_alpha_early_exit = alpha_mode && (version == 1 || version == 4);

        let mut v5_runeword_extra = None;
        let mut alpha_quality_raw = None;
        let mut alpha_unique_id_raw = None;

        let (item_id, item_level, item_quality, has_multiple_graphics, has_class_specific_data, timestamp_flag) = if axiom.is_alpha() {
            if !is_compact {
                let quality_raw = cursor.read_bits::<u8>(3)?;
                let quality = ItemQuality::from(quality_raw);
                alpha_quality_raw = Some(quality_raw);
                if version == 5 && (is_runeword || is_fragment) {
                    v5_runeword_extra = Some(cursor.with_context("AlphaV5RunewordExtra", |c| c.read_bits::<u8>(2))?);
                    (Some(0u32), None, Some(quality), false, false, false)
                } else if version == 5 && is_v105_summary_code(trimmed_code) {
                    cursor.end_segment();
                    return Ok((
                        Some(0u32), None, Some(quality),
                        false, None, false, None,
                        None, None, None, None, None, [None; 6],
                        None, None, None, None, None, false,
                        None, None, None, None, None, 0, None, alpha_quality_raw, None, None
                    ));
                } else {
                    (Some(0u32), None, Some(quality), false, false, false)
                }
            } else { (Some(0u32), None, None, false, false, false) }
        } else {
            let id = cursor.read_bits::<u32>(32)?;
            let level = cursor.read_bits::<u8>(7)?;
            let quality_raw = cursor.read_bits::<u8>(4)?;
            let quality = ItemQuality::from(quality_raw);
            (Some(id), Some(level), Some(quality), false, false, false)
        };

        if is_alpha_early_exit {
            cursor.end_segment();
            return Ok((
                item_id, item_level, item_quality,
                false, None, false, None,
                None, None, None, None, None, [None; 6],
                None, None, None, None, None, timestamp_flag,
                None, None, None, None, None, 0, None, alpha_quality_raw, None, None
            ));
        }

        let mut multi_graphics_bits = None;
        let mut class_specific_bits = None;
        if has_multiple_graphics { multi_graphics_bits = Some(cursor.read_bits::<u8>(3)? as u8); }
        if has_class_specific_data { class_specific_bits = Some(cursor.read_bits::<u16>(11)? as u16); }
        
        let quality_val = item_quality.unwrap_or(ItemQuality::Normal);
        let (mut low_high_graphic_bits, mut magic_prefix, mut magic_suffix) = (None, None, None);
        let (mut rare_name_1, mut rare_name_2) = (None, None);
        let mut rare_affixes = [None; 6];
        let mut unique_id = None;

        match quality_val {
            ItemQuality::Low | ItemQuality::High => { low_high_graphic_bits = Some(cursor.read_bits::<u8>(3)? as u8); }
            ItemQuality::Magic => {
                magic_prefix = Some(cursor.read_bits::<u16>(11)? as u16);
                magic_suffix = Some(cursor.read_bits::<u16>(11)? as u16);
            }
            ItemQuality::Rare | ItemQuality::Crafted => {
                rare_name_1 = Some(cursor.read_bits::<u8>(8)? as u8);
                rare_name_2 = Some(cursor.read_bits::<u8>(8)? as u8);
                for i in 0..6 { if cursor.read_bit()? { rare_affixes[i] = Some(cursor.read_bits::<u16>(11)? as u16); } }
            }
            ItemQuality::Set | ItemQuality::Unique => { 
                let uid = cursor.read_bits::<u16>(12)? as u16;
                if alpha_mode { alpha_unique_id_raw = Some(uid); }
                unique_id = Some(uid); 
            }
            _ => {}
        }

        let (mut runeword_id, mut runeword_level) = (None, None);
        if is_runeword && !is_fragment && version != 5 {
            runeword_id = Some(cursor.read_bits::<u16>(12)? as u16);
            runeword_level = Some(cursor.read_bits::<u8>(4)? as u8);
        }

        let mut personalized_player_name = None;
        if is_personalized { 
            if alpha_mode && (version == 5 || version == 0 || version == 1) { cursor.byte_align()?; }
            personalized_player_name = Some(read_player_name(cursor, alpha_mode && (version == 5 || version == 0 || version == 1))?); 
        }

        let tbk_ibk_teleport = if trimmed_code == "tbk" || trimmed_code == "ibk" { Some(cursor.read_bits::<u8>(5)? as u8) } else { None };
        let timestamp_flag = cursor.read_bit()?;

        let template = item_template(trimmed_code);
        let (reads_defense, reads_durability, reads_quantity) = if let Some(template) = template {
            (template.is_armor, template.has_durability, template.is_stackable)
        } else {
            let is_scroll = trimmed_code == "tsc" || trimmed_code == "isc";
            let armor_like_unknown = has_class_specific_data || trimmed_code.contains(' ');
            (armor_like_unknown, armor_like_unknown, is_scroll)
        };

        let (mut defense, mut max_durability, mut current_durability, mut quantity, mut sockets) = (None, None, None, None, None);
        if reads_defense && axiom.reads_defense() { defense = Some(cursor.read_bits::<u32>(stat_save_bits(31).unwrap_or(11))?); }
        if reads_durability && axiom.reads_durability() {
            let max_bits = stat_save_bits(73).unwrap_or(8);
            let cur_bits = stat_save_bits(72).unwrap_or(9);
            let m_dur = cursor.read_bits::<u32>(max_bits)?;
            max_durability = Some(m_dur);
            if m_dur > 0 { current_durability = Some(cursor.read_bits::<u32>(cur_bits)?); let _extra = cursor.read_bit()?; }
        }
        if reads_quantity && axiom.reads_quantity() { quantity = Some(cursor.read_bits::<u32>(9)?); }
        if is_socketed_flag { sockets = Some(cursor.read_bits::<u8>(4)? as u8); }

        let mut set_list_val_raw = None;
        let mut set_list_count = 0;
        if quality_val == ItemQuality::Set {
            let val = cursor.read_bits::<u8>(5)?;
            set_list_val_raw = Some(val);
            set_list_count = match val { 1 => 1, 3 => 2, 7 => 3, 15 => 4, 31 => 5, _ => 0 };
        }

        cursor.end_segment();
        Ok((
            item_id, item_level, item_quality,
            has_multiple_graphics, multi_graphics_bits,
            has_class_specific_data, class_specific_bits,
            low_high_graphic_bits, magic_prefix, magic_suffix,
            rare_name_1, rare_name_2, rare_affixes,
            unique_id, runeword_id, runeword_level,
            personalized_player_name, tbk_ibk_teleport,
            timestamp_flag, defense, max_durability, current_durability, quantity, sockets, set_list_count, v5_runeword_extra, alpha_quality_raw, alpha_unique_id_raw, set_list_val_raw
        ))
    }

    pub fn to_bytes(&self, huffman: &HuffmanTree, alpha_mode: bool) -> io::Result<Vec<u8>> {
        let mut emitter = BitEmitter::new();
        emitter.write_bits(self.header.flags, 32)?;
        emitter.write_bits(self.header.version as u32, 3)?;
        emitter.write_bits(self.header.mode as u32, 3)?;
        emitter.write_bits(self.header.location as u32, 3)?;
        emitter.write_bits(self.header.x as u32, 4)?;
        
        let axiom = StatsAxiom::new(self.header.version, self.header.quality.unwrap_or(ItemQuality::Normal), alpha_mode)
            .with_personalization(self.header.is_personalized)
            .with_code(&self.code);
        let geometry = axiom.header_geometry(self.header.flags, self.header.is_compact);

        if geometry.has_header_gap {
            if self.header.version == 5 || self.header.version == 0 {
                let is_v105_shadow = axiom.is_v105_shadow(self.header.flags);
                let is_rw = axiom.is_runeword(self.header.flags);

                if is_v105_shadow {
                    let gap = self.body.alpha_header_gap.unwrap_or(0);
                    emitter.write_bits(gap as u32, 8)?;
                } else if is_rw {
                    let gap = self.body.alpha_header_gap.unwrap_or(0);
                    emitter.write_bits(gap as u32, 24)?;
                } else {
                    if !self.header.is_compact {
                        emitter.write_bits(self.header.y as u32, geometry.y_bits)?;
                        emitter.write_bits(self.header.page as u32, geometry.page_bits)?;
                        emitter.write_bits(self.header.socket_hint as u32, geometry.socket_hint_bits)?;
                    }
                    emitter.write_bits(self.body.alpha_header_gap.unwrap_or(0) as u32, 8)?;
                }
            } else {
                emitter.write_bits(self.header.y as u32, geometry.y_bits)?;
                emitter.write_bits(self.header.page as u32, geometry.page_bits)?;
                emitter.write_bits(self.header.socket_hint as u32, geometry.socket_hint_bits)?;
                emitter.write_bits(self.body.alpha_header_gap.unwrap_or(0) as u32, 8)?;
            }
        } else if !geometry.skip_geometry {
            emitter.write_bits(self.header.y as u32, geometry.y_bits)?;
            emitter.write_bits(self.header.page as u32, geometry.page_bits)?;
            emitter.write_bits(self.header.socket_hint as u32, geometry.socket_hint_bits)?;
        }

        if self.header.is_ear {
            emitter.write_bits(self.ear_class.unwrap_or(0) as u32, 3)?;
            emitter.write_bits(self.ear_level.unwrap_or(0) as u32, 7)?;
            write_player_name(&mut emitter, self.ear_player_name.as_deref().unwrap_or(""), alpha_mode && self.header.version == 5)?;
            if alpha_mode && self.header.version == 5 {
                emitter.byte_align()?;
            }
        } else {
            let encoded_code = huffman.encode(&self.code)?;
            emitter.extend_bits(encoded_code)?;
            if alpha_mode && (self.header.version == 5 || self.header.version == 0 || self.header.version == 1) {
                let trimmed = self.code.trim();
                let nudge = if let Some(n) = self.body.alpha_nudge {
                    n
                } else if self.header.version == 0 { 
                    match trimmed { "wwww" => 0, "u7cx" => 3, _ => 2 }
                } else if self.header.version == 5 && (trimmed == "gpb" || trimmed == "vps") { 
                    2 
                } else { 0 };
                emitter.write_bits(nudge as u32, 2)?;
            }
        }

        if !self.header.is_compact {
            let trimmed = self.code.trim();
            let is_v105_shadow = alpha_mode && self.header.version == 5 && (self.header.flags & (1 << 26)) != 0;
            let is_v105_summary = alpha_mode && self.header.version == 5 && !is_v105_shadow && is_v105_summary_code(trimmed);

            let quality_val = self.header.quality.unwrap_or(ItemQuality::Normal);
            let is_item_alpha = axiom.is_alpha();

            if is_item_alpha {
                let quality_to_write = self.header.alpha_quality_raw.unwrap_or(self.header.quality.map(|q| q as u8).unwrap_or(0));
                emitter.write_bits(quality_to_write as u32, 3)?;

                if is_v105_summary {
                    if trimmed == "7mgw" {
                        if let Some(payload) = &self.body.v105_7mgw_payload {
                            for &bit in payload { emitter.write_bit(bit)?; }
                        } else { emitter.write_bits(0, 28)?; }
                    }
                } else {
                    let is_runeword = axiom.is_runeword(self.header.flags);
                    let is_frag = axiom.is_fragment(self.header.flags);
                    if self.header.version == 5 && (is_runeword || is_frag) { 
                        emitter.write_bits(self.body.v5_runeword_extra.unwrap_or(0) as u32, 2)?;
                    }
                }
            }

            if !is_v105_summary {
                if !is_item_alpha {
                    emitter.write_bits(self.header.id.unwrap_or(0), 32)?;
                    emitter.write_bits(self.header.level.unwrap_or(0) as u32, 7)?;
                    emitter.write_bits(quality_val as u32, 4)?;
                }

                if is_item_alpha && (self.header.version == 1 || self.header.version == 4) {
                } else {
                    if self.has_multiple_graphics { emitter.write_bits(self.multi_graphics_bits.unwrap_or(0) as u32, 3)?; }
                    if self.has_class_specific_data { emitter.write_bits(self.class_specific_bits.unwrap_or(0) as u16 as u32, 11)?; }

                    match quality_val {
                        ItemQuality::Low | ItemQuality::High => { emitter.write_bits(self.low_high_graphic_bits.unwrap_or(0) as u32, 3)?; }
                        ItemQuality::Magic => {
                            emitter.write_bits(self.magic_prefix.unwrap_or(0) as u32, 11)?;
                            emitter.write_bits(self.magic_suffix.unwrap_or(0) as u32, 11)?;
                        }
                        ItemQuality::Rare | ItemQuality::Crafted => {
                            emitter.write_bits(self.rare_name_1.unwrap_or(0) as u32, 8)?;
                            emitter.write_bits(self.rare_name_2.unwrap_or(0) as u32, 8)?;
                            for i in 0..6 {
                                if let Some(affix) = self.rare_affixes[i] {
                                    emitter.write_bit(true)?; emitter.write_bits(affix as u32, 11)?;
                                } else { emitter.write_bit(false)?; }
                            }
                        }
                        ItemQuality::Set | ItemQuality::Unique => {
                            let uid_to_write = if axiom.is_alpha() { self.header.alpha_unique_id_raw.unwrap_or(self.unique_id.unwrap_or(0)) } else { self.unique_id.unwrap_or(0) };
                            emitter.write_bits(uid_to_write as u32, 12)?;
                        }
                        _ => {}
                    }

                    if axiom.is_runeword(self.header.flags) && !axiom.is_alpha() && self.header.version != 5 {
                        emitter.write_bits(self.runeword_id.unwrap_or(0) as u32, 12)?;
                        emitter.write_bits(self.runeword_level.unwrap_or(0) as u32, 12)?;
                        emitter.write_bits(0, 4)?; 
                    }

                    if axiom.is_personalized(self.header.flags) {
                        if alpha_mode && (self.header.version == 5 || self.header.version == 0 || self.header.version == 1) { emitter.byte_align()?; }
                        write_player_name(&mut emitter, self.personalized_player_name.as_deref().unwrap_or(""), alpha_mode && (self.header.version == 5 || self.header.version == 0 || self.header.version == 1))?;
                    }

                    if self.code.trim() == "tbk" || self.code.trim() == "ibk" { emitter.write_bits(self.tbk_ibk_teleport.unwrap_or(0) as u32, 5)?; }

                    emitter.write_bit(self.timestamp_flag)?;

                    let template = item_template(&self.code);
                    let (reads_defense, reads_durability, reads_quantity) = if let Some(t) = template {
                        (t.is_armor, t.has_durability, t.is_stackable)
                    } else { (false, false, false) };

                    if reads_defense && axiom.reads_defense() { emitter.write_bits(self.defense.unwrap_or(0), 11)?; }
                    if reads_durability && axiom.reads_durability() {
                        let m_dur = self.max_durability.unwrap_or(0);
                        emitter.write_bits(m_dur, 8)?;
                        if m_dur > 0 {
                            emitter.write_bits(self.current_durability.unwrap_or(0), 9)?;
                            emitter.write_bit(false)?; 
                        }
                    }
                    if reads_quantity && axiom.reads_quantity() { emitter.write_bits(self.quantity.unwrap_or(0), 9)?; }

                    if axiom.is_socketed(self.header.flags, self.header.is_compact) { emitter.write_bits(self.sockets.unwrap_or(0) as u32, 4)?; }

                    if quality_val == ItemQuality::Set {
                        let set_list_val = if let Some(val) = self.body.alpha_set_list_val { val as u32 } else { match self.set_list_count { 1 => 1, 2 => 3, 3 => 7, 4 => 15, 5 => 31, _ => 0 } };
                        emitter.write_bits(set_list_val, 5)?;
                    }
                }

                let is_v105_shadow = axiom.is_v105_shadow(self.header.flags);
                if is_v105_shadow {
                    if let Some(bits) = self.body.alpha_shadow_skip_bits { emitter.write_bits_u64(bits, 47)?; }
                    else { emitter.write_bits(0, 47)?; }
                }

                let has_props = !self.properties.is_empty();
                if self.header.version != 5 || is_v105_shadow || self.header.is_runeword || (alpha_mode && self.header.is_compact) || has_props {
                    write_property_list(&mut emitter, &self.code, &self.properties, self.header.version, self.header.is_runeword, self.terminator_bit, quality_val, is_v105_shadow, &axiom)?;
                    for set_props in &self.set_attributes {
                        write_property_list(&mut emitter, &self.code, set_props, self.header.version, false, false, quality_val, false, &axiom)?;
                    }
                }
            }
        }

        let current_bits = emitter.written_bits();
        let final_bits = axiom.calculate_alignment(current_bits as u64, self.header.is_compact, &self.code, self.header.flags);
        if final_bits > current_bits as u64 {
            let padding_needed = (final_bits - current_bits as u64) as u32;
            if !self.body.alpha_alignment_padding.is_empty() {
                for &bit in &self.body.alpha_alignment_padding { emitter.write_bit(bit)?; }
            } else { emitter.write_bits(0, padding_needed)?; }
        }

        Ok(emitter.into_bytes())
    }
}

pub fn read_item_stats<R: BitRead>(
    cursor: &mut BitCursor<R>,
    code: &str,
    version: u8,
    ctx: Option<(&[u8], u64)>,
    huffman: &HuffmanTree,
    alpha_mode: bool,
    quality: Option<ItemQuality>,
    is_runeword: bool,
    is_v105_shadow: bool,
    is_personalized: bool,
) -> ParsingResult<(Vec<ItemProperty>, bool, bool, Option<u8>, Option<Vec<bool>>, Option<u64>)> {
    let mut alpha_v5_runeword_extra = None;
    let mut alpha_shadow_skip_bits = None;
    cursor.begin_segment(ItemSegmentType::Stats);
    let trimmed_code = code.trim();
    let quality_val = quality.unwrap_or(ItemQuality::Normal);
    let axiom = StatsAxiom::new(version, quality_val, alpha_mode)
        .with_personalization(is_personalized)
        .with_code(trimmed_code);
    let is_alpha = axiom.is_alpha();

    let is_v105_shadow_final = alpha_mode && version == 5 && is_v105_shadow;
    let is_scroll = trimmed_code == "tsc" || trimmed_code == "isc";
    let is_potion = trimmed_code.starts_with('h') || trimmed_code.starts_with('m') || (version == 5 && trimmed_code.starts_with('7')) || (trimmed_code.starts_with('r') && trimmed_code.len() <= 3);
    
    if is_alpha && trimmed_code.is_empty() {
        return Ok((Vec::new(), true, false, None, None, None));
    }

    if is_alpha && version == 4 && !is_personalized {
        return Ok((Vec::new(), true, false, None, None, None));
    }

    if is_alpha && version == 5 && !is_v105_shadow_final && 
       (is_potion || is_scroll || quality_val < ItemQuality::Magic) {
          if trimmed_code == "7mgw" {
              let mut payload = Vec::new();
              for _ in 0..28 { payload.push(cursor.read_bit()?); }
              return Ok((Vec::new(), true, false, None, Some(payload), None));
          }
          return Ok((Vec::new(), true, false, None, None, None));
    }

    let section_recovery = if let Some((bytes, start)) = ctx {
        PropertyReaderContext { bytes, item_start_bit: start }
    } else {
        PropertyReaderContext { bytes: &[], item_start_bit: 0 }
    };
    if is_v105_shadow_final {
        let skip_bits_count = if version == 5 { 47 } else { 24 };
        let skip_bits = cursor.with_context("AlphaShadowSkip", |c| c.read_bits::<u64>(skip_bits_count))?;
        alpha_shadow_skip_bits = Some(skip_bits);
    }
    let (props, complete, term) = read_property_list(cursor, trimmed_code, version, section_recovery, huffman, is_runeword, is_v105_shadow_final, &axiom, |_, _, _, _, _| {
        Ok((Item::default(), 0))
    })?;
    
    if alpha_mode && version == 5 && is_runeword {
        cursor.begin_segment(ItemSegmentType::ExtendedStats);
        cursor.push_context("AlphaV5RunewordExtra");
        let extra = cursor.read_bits::<u8>(2)?;
        alpha_v5_runeword_extra = Some(extra);
        cursor.pop_context();
        cursor.end_segment();
    }
    
    Ok((props, complete, term, alpha_v5_runeword_extra, None, alpha_shadow_skip_bits))
}

pub fn is_v105_summary_code(code: &str) -> bool {
    matches!(code, "hp1"|"hp2"|"hp3"|"hp4"|"hp5"|"mp1"|"mp2"|"mp3"|"mp4"|"mp5"|"rvl"|"rvs"|"isc"|"tsc"|"w8cs"|"w88w"|"us g"|"xrs"|"6cs"|"7mgw")
}

pub fn read_player_name<R: BitRead>(cursor: &mut BitCursor<R>, alpha_v5: bool) -> ParsingResult<String> {
    let mut name = String::new();
    let width = if alpha_v5 { 8 } else { 7 };
    loop {
        let ch = cursor.read_bits::<u8>(width)?;
        if ch == 0 { break; }
        name.push(ch as char);
    }
    Ok(name)
}

pub fn item_template(code: &str) -> Option<&'static crate::data::item_codes::ItemTemplate> {
    crate::data::item_codes::ITEM_TEMPLATES.iter().find(|t| t.code == code.trim())
}

pub fn scan_socket_children(
    bytes: &[u8],
    bit_pos: u64,
    huffman: &HuffmanTree,
    _parent_idx: usize,
    alpha: bool,
    limit: u64,
) -> Option<(Vec<Item>, u64)> {
    let mut children = Vec::new();
    let mut current_pos = bit_pos;
    let max_pos = bit_pos + limit;
    let section_bits = (bytes.len() * 8) as u64;

    while current_pos < max_pos && current_pos < section_bits {
        if let Some((mode, location, _x, code, flags, version, _is_compact, _header_bits, _nudge)) = peek_item_header_at(bytes, current_pos, huffman, alpha) {
            if is_plausible_item_header(mode, location, &code, flags, version, alpha) {
                if mode == 6 || location == 6 {
                    let remaining = section_bits.saturating_sub(current_pos);
                    if let Ok((item, consumed)) = parse_item_at_with_limit(bytes, current_pos, huffman, 0, alpha, Some(remaining)) {
                        let mut item_end = current_pos + consumed;
                        if alpha {
                            if let Some(next_start) = find_next_item_match(bytes, current_pos + 64, huffman, alpha) {
                                if next_start < item_end && next_start < max_pos { item_end = next_start; }
                            }
                        }
                        let mut final_child = item;
                        final_child.range.start = current_pos;
                        final_child.range.end = item_end;
                        final_child.total_bits = item_end - current_pos;
                        children.push(final_child);
                        current_pos = item_end;
                        continue;
                    }
                } else { break; }
            }
        }
        current_pos += 1;
    }
    if children.is_empty() { None } else { Some((children, current_pos)) }
}

#[derive(Debug, Clone)]
pub struct PropertyReaderContext<'a> {
    pub bytes: &'a [u8],
    pub item_start_bit: u64,
}

pub struct BitEmitter {
    writer: BitWriter<Vec<u8>, LittleEndian>,
    written: u64,
}

impl BitEmitter {
    pub fn new() -> Self {
        BitEmitter {
            writer: BitWriter::endian(Vec::new(), LittleEndian),
            written: 0,
        }
    }

    pub fn write_bit(&mut self, bit: bool) -> io::Result<()> {
        if crate::item::item_trace_enabled() {
            println!("[TRACE] BitEmitter: bit {} at pos {}", bit as u8, self.written);
        }
        self.writer.write_bit(bit)?;
        self.written += 1;
        Ok(())
    }

    pub fn write_bits(&mut self, value: u32, count: u32) -> io::Result<()> {
        if count == 0 { return Ok(()); }
        for i in 0..count {
            let bit = if i < 32 {
                (value >> i) & 1 != 0
            } else {
                false
            };
            self.write_bit(bit)?;
        }
        Ok(())
    }

    pub fn write_bits_u64(&mut self, value: u64, count: u32) -> io::Result<()> {
        if count == 0 { return Ok(()); }
        for i in 0..count {
            let bit = if i < 64 {
                (value >> i) & 1 != 0
            } else {
                false
            };
            self.write_bit(bit)?;
        }
        Ok(())
    }

    pub fn extend_bits<I>(&mut self, bits: I) -> io::Result<()>
    where
        I: IntoIterator<Item = bool>,
    {
        for bit in bits {
            self.write_bit(bit)?;
        }
        Ok(())
    }

    pub fn byte_align(&mut self) -> io::Result<()> {
        let padding = (8 - (self.written % 8)) % 8;
        self.writer.byte_align()?;
        self.written += padding;
        Ok(())
    }

    pub fn written_bits(&self) -> u64 {
        self.written
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.writer.into_writer()
    }
}

pub struct HuffmanTree {
    root: Box<HuffmanNode>,
    encoding_table: std::collections::HashMap<char, Vec<bool>>,
}

struct HuffmanNode {
    symbol: Option<char>,
    left: Option<Box<HuffmanNode>>,
    right: Option<Box<HuffmanNode>>,
}

impl HuffmanNode {
    fn new() -> Self {
        HuffmanNode {
            symbol: None,
            left: None,
            right: None,
        }
    }
}

impl HuffmanTree {
    pub fn new() -> Self {
        let mut root = Box::new(HuffmanNode::new());
        let table = [
            ('0', "11111011"),
            (' ', "10"),
            ('1', "1111100"),
            ('2', "001100"),
            ('3', "1101101"),
            ('4', "11111010"),
            ('5', "00010110"),
            ('6', "1101111"),
            ('7', "01111"),
            ('8', "000100"),
            ('9', "01110"),
            ('a', "11110"),
            ('b', "0101"),
            ('c', "01000"),
            ('d', "110001"),
            ('e', "110000"),
            ('f', "010011"),
            ('g', "11010"),
            ('h', "00011"),
            ('i', "1111110"),
            ('j', "000101111"),
            ('k', "010010"),
            ('l', "11101"),
            ('m', "01101"),
            ('n', "001101"),
            ('o', "1111111"),
            ('p', "11001"),
            ('q', "11011001"),
            ('r', "11100"),
            ('s', "0010"),
            ('t', "01100"),
            ('u', "00001"),
            ('v', "1101110"),
            ('w', "00000"),
            ('x', "00111"),
            ('y', "0001010"),
            ('z', "11011000"),
        ];

        let mut encoding_table = std::collections::HashMap::new();
        for (symbol, pattern) in table {
            let mut bits = Vec::new();
            let mut current = &mut root;
            for bit in pattern.chars() {
                if bit == '1' {
                    bits.push(true);
                    if current.right.is_none() {
                        current.right = Some(Box::new(HuffmanNode::new()));
                    }
                    current = current.right.as_mut().unwrap();
                } else {
                    bits.push(false);
                    if current.left.is_none() {
                        current.left = Some(Box::new(HuffmanNode::new()));
                    }
                    current = current.left.as_mut().unwrap();
                }
            }
            current.symbol = Some(symbol);
            encoding_table.insert(symbol, bits);
        }
        HuffmanTree {
            root,
            encoding_table,
        }
    }

    pub fn encode(&self, code: &str) -> io::Result<Vec<bool>> {
        let mut bits = Vec::new();
        for c in code.chars() {
            let pattern = self.encoding_table.get(&c).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Char '{}' not in Huffman table", c),
                )
            })?;
            bits.extend(pattern);
        }
        Ok(bits)
    }

    fn decode_internal<F: FnMut() -> io::Result<bool>>(&self, mut read_bit: F) -> io::Result<char> {
        let mut current = &self.root;
        loop {
            if let Some(symbol) = current.symbol {
                return Ok(symbol);
            }
            let bit = read_bit()?;
            current = if bit {
                current.right.as_ref()
            } else {
                current.left.as_ref()
            }
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid Huffman bit"))?;
        }
    }

    pub fn decode_recorded<R: BitRead>(&self, cursor: &mut BitCursor<R>) -> ParsingResult<char> {
        self.decode_internal(|| cursor.read_bit().map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{:?}", e))))
            .map_err(|_| {
                cursor.fail(ParsingError::InvalidHuffmanBit { bit_offset: cursor.pos() })
                    .with_hint("Huffman bit pattern does not match Alpha v105 table. Possible bit drift or misalignment.")
            })
    }

    pub fn decode<R: BitRead>(&self, reader: &mut R) -> io::Result<char> {
        self.decode_internal(|| reader.read_bit())
    }
}

impl Item {
    pub fn serialize_section(items: &[Item], huffman: &HuffmanTree, alpha_mode: bool) -> io::Result<Vec<u8>> {
    let mut emitter = BitEmitter::new();
    for item in items {
        emitter.extend_bits(item.gap_bits.iter().cloned())?;
        let item_bytes = item.to_bytes(huffman, alpha_mode)?;
        for byte in item_bytes { emitter.write_bits(byte as u32, 8)?; }
        for child in &item.socketed_items {
            if alpha_mode { emitter.write_bits(2, 2)?; }
            let child_bytes = child.to_bytes(huffman, alpha_mode)?;
            for byte in child_bytes { emitter.write_bits(byte as u32, 8)?; }
        }
    }
    Ok(emitter.into_bytes())
}
}

fn write_player_name(emitter: &mut BitEmitter, name: &str, alpha_v5: bool) -> io::Result<()> {
    let width = if alpha_v5 { 8 } else { 7 };
    for ch in name.chars() { emitter.write_bits((ch as u8) as u32, width)?; }
    emitter.write_bits(0, width)?;
    Ok(())
}

fn write_property_list(emitter: &mut BitEmitter, code: &str, props: &[ItemProperty], version: u8, alpha_runeword: bool, terminator_bit: bool, _quality: ItemQuality, is_v105_shadow: bool, axiom: &StatsAxiom) -> io::Result<()> {
    let is_compact = code.trim().is_empty() || code.len() < 3;
    let rhythm = axiom.property_rhythm(alpha_runeword, is_v105_shadow, is_compact);
    let id_bits = rhythm.id_bits;
    let terminator = (1 << id_bits) - 1;
    for prop in props {
        let raw_id = prop.stat_id;
        emitter.write_bits(raw_id, id_bits)?;
        if let Some(width) = rhythm.value_bits {
            let effective_width = axiom.stat_bit_width(raw_id, width);
            emitter.write_bits(prop.raw_value as u32, effective_width)?;
        } else {
             let mapped_id = axiom.map_alpha_id(raw_id);
             if let Some(stat) = crate::data::stat_costs::STAT_COSTS.iter().find(|s| s.id == mapped_id) {
                 if stat.save_param_bits > 0 { emitter.write_bits(prop.param as u32, stat.save_param_bits as u32)?; }
                 emitter.write_bits(prop.raw_value as u32, stat.save_bits as u32)?;
             } else { emitter.write_bits(prop.raw_value as u32, 9)?; }
        }
    }
    emitter.write_bits(terminator, id_bits)?;
    let preserve_trailing_align = axiom.is_alpha() && version == 0 && code.trim().is_empty();
    if rhythm.has_terminal_bit {
        emitter.write_bit(terminator_bit)?;
        if rhythm.has_extra_terminal_bit { emitter.write_bit(terminator_bit)?; }
        if !preserve_trailing_align { emitter.byte_align()?; }
    }
    Ok(())
}
