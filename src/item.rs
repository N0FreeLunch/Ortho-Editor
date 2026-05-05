use bitstream_io::{BitRead, BitReader as IoBitReader, LittleEndian};
use std::io::Cursor;
use crate::data::bit_cursor::BitCursor;

pub(crate) fn item_trace_enabled() -> bool {
    std::env::var_os("D2R_ITEM_TRACE").is_some()
}

#[macro_export]
macro_rules! item_trace {
    ($($arg:tt)*) => {
        if crate::item::item_trace_enabled() {
            println!($($arg)*);
        }
    };
}

pub use crate::domain::item::{Item, ItemQuality, ItemBitRange, RecordedBit, ItemModule, BitSegment, ItemBody};
pub use crate::domain::header::entity::{ItemSegmentType, ItemHeader};
pub use crate::domain::item::serialization::{HuffmanTree, find_next_item_match, peek_item_header_at, is_plausible_item_header};
pub use crate::error::{ParsingError, ParsingFailure, ParsingResult};
pub use crate::domain::stats::{ItemProperty, ItemStats};
use crate::domain::stats::{read_property_list, stat_save_bits, StatsAxiom};
use crate::domain::item::axiom_meta::{ForensicAudit, ForensicAxiom};
use crate::domain::forensic::v105::{V105NudgeAxiom, V105ShadowAxiom, V105HeaderGapAxiom};

#[derive(Debug, Clone)]
pub struct PropertyReaderContext<'a> {
    pub bytes: &'a [u8],
    pub item_start_bit: u64,
}

impl Item {
    pub fn read_player_items(bytes: &[u8], huffman: &HuffmanTree, alpha: bool) -> ParsingResult<Vec<Item>> {
        let mut all_items = Vec::new();
        let mut jm_positions = Vec::new();

        // Find all JM markers
        for i in 0..bytes.len().saturating_sub(3) {
            if bytes[i] == b'J' && bytes[i + 1] == b'M' {
                jm_positions.push(i);
            }
        }

        item_trace!("[DEBUG] Found {} JM markers at positions: {:?}", jm_positions.len(), jm_positions);

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
            if count > 100 { continue; } // Unlikely for a single section
            
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

        item_trace!("[DEBUG] Identified {} real JM sections at: {:?}", real_jm_positions.len(), real_jm_positions);

        for (idx, &start_offset) in real_jm_positions.iter().enumerate() {
            let count = u16::from_le_bytes([bytes[start_offset + 2], bytes[start_offset + 3]]);
            let payload_start = start_offset + 4;
            
            let next_jm = real_jm_positions.get(idx + 1).cloned().unwrap_or(bytes.len());
            let section_bytes = &bytes[payload_start..next_jm];
            
            item_trace!("[DEBUG] Parsing verified JM section at offset {} with count {}", start_offset, count);

            match Self::read_section(section_bytes, count, huffman, alpha) {
                Ok(mut items) => {
                    all_items.append(&mut items);
                }
                Err(e) => {
                    item_trace!("[DEBUG] Failed to parse section at offset {}: {:?}", start_offset, e);
                    if !alpha { return Err(e); }
                }
            }
        }

        Ok(all_items)
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
                let mut bit_reader = IoBitReader::endian(Cursor::new(section_bytes), LittleEndian);
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
                    let axiom = StatsAxiom::new(item.version, item.quality.unwrap_or(ItemQuality::Normal), alpha_mode)
                        .with_personalization(item.is_personalized);
                    let final_consumed = axiom.calculate_alignment(consumed_bits, item.is_compact, &item.code);

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
                    let mut bit_reader = IoBitReader::endian(Cursor::new(section_bytes), LittleEndian);
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
                    
                    bit_pos = end;
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

    pub fn from_bytes(bytes: &[u8], huffman: &HuffmanTree, alpha: bool) -> ParsingResult<Item> {
        let (item, _) = parse_item_at_with_limit(bytes, 0, huffman, 0, alpha, None)?;
        Ok(item)
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
        cursor.set_trace(item_trace_enabled());
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

        let stats = if !is_compact {
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
                defense: stats.19,
                max_durability: stats.20,
                current_durability: stats.21,
                quantity: stats.22,
                v5_runeword_extra: stats.25,
                v105_7mgw_payload: None,
                alpha_nudge,
                alpha_header_gap,
                alpha_set_list_val: stats.28,
                alpha_shadow_skip_bits: None,
                alpha_alignment_padding: Vec::new(),
            },
            stats: ItemStats { properties: Vec::new(), set_attributes: Vec::new(), runeword_attributes: Vec::new() },
            bits: Vec::new(),
            code: code.clone(),
            flags, version, is_ear, ear_class, ear_level, ear_player_name,
            personalized_player_name: stats.16,
            mode, x, y, page, location, header_socket_hint,
            has_multiple_graphics: stats.3, multi_graphics_bits: stats.4,
            has_class_specific_data: stats.5, class_specific_bits: stats.6,
            id: stats.0, level: stats.1, quality: stats.2, low_high_graphic_bits: stats.7,
            is_compact: axiom.is_compact(flags),
            is_socketed: axiom.is_socketed(flags, is_compact),
            is_identified: (flags & (1 << 4)) != 0,
            is_personalized: axiom.is_personalized(flags),
            is_runeword: axiom.is_runeword(flags),
            is_ethereal: axiom.is_ethereal(flags),
            magic_prefix: stats.8, magic_suffix: stats.9,
            rare_name_1: stats.10, rare_name_2: stats.11, rare_affixes: stats.12,
            unique_id: stats.13, alpha_unique_id_raw: stats.27, runeword_id: stats.14, runeword_level: stats.15,
            properties: Vec::new(), set_attributes: Vec::new(), runeword_attributes: Vec::new(),
            num_socketed_items: 0, socketed_items: Vec::new(),
            timestamp_flag: stats.18,
            properties_complete: false,
            terminator_bit: false,
            header: ItemHeader {
                flags, version, mode, location, x, y, page, socket_hint: header_socket_hint,
                id: stats.0, quality: stats.2,
                is_compact: axiom.is_compact(flags),
                is_identified: axiom.is_identified(flags),
                is_socketed: axiom.is_socketed(flags, is_compact),
                is_personalized: axiom.is_personalized(flags),
                is_runeword: axiom.is_runeword(flags),
                is_ethereal: axiom.is_ethereal(flags),
                is_ear,
                alpha_quality_raw: stats.26,
                alpha_v5_runeword_extra: stats.25,
                alpha_unique_id_raw: stats.27,
            },
            set_list_count: stats.24,
            tbk_ibk_teleport: stats.17,
            defense: stats.19,
            max_durability: stats.20,
            current_durability: stats.21,
            quantity: stats.22,
            sockets: stats.23,
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

        if !axiom.is_compact(item.flags) {
            let is_v105_shadow = axiom.is_v105_shadow(item.flags);
            let axiom = axiom.with_personalization(item.is_personalized);
            let (props, complete, term, extra_bits, reserved_7mgw, shadow_bits) = read_item_stats(cursor, &item.code, item.version, ctx, huff, alpha_mode, item.quality, item.is_runeword, is_v105_shadow, item.is_personalized)?;
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
            
            // Recalculate consumed bits if needed? No, cursor moved.
        }

        let axiom = StatsAxiom::new(version, item.quality.unwrap_or(ItemQuality::Normal), alpha_mode)
            .with_personalization(item.is_personalized);
        let consumed_bits = cursor.pos() - start_bit;
        let final_consumed = axiom.calculate_alignment(consumed_bits, is_compact, &item.code);
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
}

fn read_item_stats<R: BitRead>(
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
        .with_personalization(is_personalized);
    let is_alpha = axiom.is_alpha();

    crate::item_trace!("[DEBUG] read_item_stats for '{}', version={}, is_runeword={}, quality={:?}, is_alpha={}", trimmed_code, version, is_runeword, quality, is_alpha);

    let is_v105_shadow_final = alpha_mode && version == 5 && is_v105_shadow;
    let is_scroll = trimmed_code == "tsc" || trimmed_code == "isc";
    let is_potion = trimmed_code.starts_with('h') || trimmed_code.starts_with('m') || (version == 5 && trimmed_code.starts_with('7')) || (trimmed_code.starts_with('r') && trimmed_code.len() <= 3);
    
    if is_alpha && trimmed_code.is_empty() {
        crate::item_trace!("[DEBUG] Skipping properties for empty-code Alpha item");
        return Ok((Vec::new(), true, false, None, None, None));
    }

    if is_alpha && version == 4 && !is_personalized {
        crate::item_trace!("[DEBUG] Skipping properties for non-personalized Alpha v104 Item '{}'", trimmed_code);
        return Ok((Vec::new(), true, false, None, None, None));
    }

    if is_alpha && version == 5 && !is_v105_shadow_final && 
       (is_potion || is_scroll || quality_val < ItemQuality::Magic) {
          if trimmed_code == "7mgw" {
              let mut payload = Vec::new();
              for _ in 0..28 { payload.push(cursor.read_bit()?); }
              return Ok((Vec::new(), true, false, None, Some(payload), None));
          }
          crate::item_trace!("[DEBUG] Skipping properties for Alpha v105 Summary Item '{}'", trimmed_code);
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

fn is_v105_summary_code(code: &str) -> bool {
    matches!(code, "hp1"|"hp2"|"hp3"|"hp4"|"hp5"|"mp1"|"mp2"|"mp3"|"mp4"|"mp5"|"rvl"|"rvs"|"isc"|"tsc"|"w8cs"|"w88w"|"us g"|"xrs"|"6cs"|"7mgw")
}

fn read_player_name<R: BitRead>(cursor: &mut BitCursor<R>, alpha_v5: bool) -> ParsingResult<String> {
    let mut name = String::new();
    let width = if alpha_v5 { 8 } else { 7 };
    loop {
        let ch = cursor.read_bits::<u8>(width)?;
        if ch == 0 { break; }
        name.push(ch as char);
    }
    Ok(name)
}

fn item_template(code: &str) -> Option<&'static crate::data::item_codes::ItemTemplate> {
    crate::data::item_codes::ITEM_TEMPLATES.iter().find(|t| t.code == code.trim())
}

pub use crate::domain::item::serialization::{parse_item_at, parse_item_at_with_limit};

fn scan_socket_children(
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
