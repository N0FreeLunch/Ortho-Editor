use bitstream_io::{BitRead, BitWrite, BitWriter, LittleEndian};
use std::io::{self, Cursor};
use crate::domain::item::Item;
use crate::domain::item::quality::ItemQuality;
use crate::domain::stats::{ItemProperty, StatsAxiom, ItemStats};
use crate::data::bit_cursor::BitCursor;
use crate::error::{ParsingResult, ParsingError, ParsingFailure};
use crate::domain::header::entity::{ItemSegmentType, HeaderAxiom};
use crate::domain::item::axiom_meta::{ForensicAudit, ForensicAxiom};
use crate::domain::forensic::v105::{V105NudgeAxiom, V105ShadowAxiom, V105HeaderGapAxiom};

pub fn find_next_item_match(bytes: &[u8], pos: u64, huffman: &HuffmanTree, alpha: bool) -> Option<u64> {
    let limit = (bytes.len() * 8) as u64;
    let mut probe = pos;
    let section_bits = limit;

    // Scan until section end to ensure we don't miss items due to small search windows
    while probe < section_bits {
        if let Some((mode, location, _x, code, flags, version, is_compact, header_len, _nudge)) = peek_item_header_at(bytes, probe, huffman, alpha) {
            if is_plausible_item_header(mode, location, &code, flags, version, alpha) {
                // Heuristic: Check if item body would fit in remaining section bits
                // A very minimal Alpha item is at least ~80 bits
                if probe + header_len + 80 <= section_bits {
                    return Some(probe);
                }
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
    let axiom = HeaderAxiom::new(version, alpha_mode);
    axiom.is_plausible(mode, location, code, _flags)
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
    
    let axiom = HeaderAxiom::new(version, alpha_mode);
    let s_axiom = StatsAxiom::new(version, ItemQuality::Normal, alpha_mode);
    let is_compact = s_axiom.is_compact(flags);
    let is_personalized = s_axiom.is_personalized(flags);
    let mut header_len = 32 + 3 + 3 + 3 + 4; 
    
    let geometry = axiom.header_geometry(flags, is_compact, is_personalized);

    if geometry.has_header_gap {
        if axiom.is_alpha() {
            let is_v105_shadow = s_axiom.is_v105_shadow(flags);
            let is_rw = s_axiom.is_runeword(flags);
            if is_rw || is_v105_shadow {
                let gap_bits = if (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0 { 8 } else { 24 }; 
                header_len += gap_bits;
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
    Ok((item, bit + cursor.pos()))
}

pub fn read_player_items(bytes: &[u8], huffman: &HuffmanTree, alpha: bool) -> ParsingResult<Vec<Item>> {
    let mut all_items = Vec::new();
    let jm_positions = crate::save::find_jm_markers(bytes);

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
        from_bytes(bytes, huffman, alpha)
    }

    pub fn read_player_items(bytes: &[u8], huffman: &HuffmanTree, alpha: bool) -> ParsingResult<Vec<Item>> {
        read_player_items(bytes, huffman, alpha)
    }

    pub fn read_section(section_bytes: &[u8], top_level_count: u16, huffman: &HuffmanTree, alpha_mode: bool) -> ParsingResult<Vec<Item>> {
        let mut items: Vec<Item> = Vec::new();
        let mut bit_pos = 0;
        let mut pending_gap_bits = Vec::new();
        // Strict boundary: JM sections are explicitly bounded by the next marker
        let section_bits = (section_bytes.len() * 8) as u64;

        while items.len() < top_level_count as usize && bit_pos < section_bits {
            let start = if alpha_mode {
                find_next_item_match(section_bytes, bit_pos, huffman, alpha_mode).unwrap_or(bit_pos)
            } else {
                bit_pos
            };

            // If we overshoot or find no valid start, stop parsing this section
            if start >= section_bits { break; }

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
                    let final_consumed = axiom.calculate_alignment(consumed_bits, item.header.is_compact, &item.body.code, item.header.flags);

                    let end = start + final_consumed;
                    // Do not allow item to exceed section boundary
                    if end > section_bits { break; }

                    let mut final_item = item;
                    final_item.range.start = start;
                    final_item.range.end = end;
                    // ... (rest of the logic preserved)
                    
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
                                let recorded = crate::domain::item::RecordedBit { bit: b, offset: start + i };
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

        let (header, alpha_header_gap) = crate::domain::item::entity::parse_item_header(cursor, alpha_mode)?;
        let (mut body, ear_class, ear_level, ear_player_name) = crate::domain::item::entity::parse_item_body(cursor, huff, &header, alpha_mode)?;
        body.alpha_header_gap = alpha_header_gap;

        let axiom = StatsAxiom::new(header.version, ItemQuality::Normal, alpha_mode);
        
        let ext_data = if !header.is_compact {
            crate::domain::item::entity::ExtendedStatsData::read_from_cursor(cursor, &body.code, &header, alpha_mode, &axiom)?
        } else {
            crate::domain::item::entity::ExtendedStatsData::default()
        };

        let mut final_header = header;
        final_header.id = ext_data.id;
        final_header.level = ext_data.level;
        final_header.quality = ext_data.quality;
        final_header.alpha_quality_raw = ext_data.alpha_quality_raw;
        final_header.alpha_v5_runeword_extra = ext_data.v5_runeword_extra;
        final_header.alpha_unique_id_raw = ext_data.alpha_unique_id_raw;

        body.defense = ext_data.defense;
        body.max_durability = ext_data.max_durability;
        body.current_durability = ext_data.current_durability;
        body.quantity = ext_data.quantity;
        body.v5_runeword_extra = ext_data.v5_runeword_extra;
        body.alpha_set_list_val = ext_data.alpha_set_list_val;

        let code = body.code.clone();

        let mut item = Item {
            body,
            stats: ItemStats { properties: Vec::new(), set_attributes: Vec::new(), runeword_attributes: Vec::new() },
            bits: Vec::new(),
            code: code.clone(),
            defense: ext_data.defense,
            max_durability: ext_data.max_durability,
            current_durability: ext_data.current_durability,
            quantity: ext_data.quantity,
            ear_class, ear_level, ear_player_name,
            personalized_player_name: ext_data.personalized_player_name,
            has_multiple_graphics: ext_data.has_multiple_graphics, multi_graphics_bits: ext_data.multi_graphics_bits,
            has_class_specific_data: ext_data.has_class_specific_data, class_specific_bits: ext_data.class_specific_bits,
            low_high_graphic_bits: ext_data.low_high_graphic_bits,
            magic_prefix: ext_data.magic_prefix, magic_suffix: ext_data.magic_suffix,
            rare_name_1: ext_data.rare_name_1, rare_name_2: ext_data.rare_name_2, rare_affixes: ext_data.rare_affixes,
            unique_id: ext_data.unique_id, runeword_id: ext_data.runeword_id, runeword_level: ext_data.runeword_level,
            properties: Vec::new(), set_attributes: Vec::new(), runeword_attributes: Vec::new(),
            num_socketed_items: 0, socketed_items: Vec::new(),
            timestamp_flag: ext_data.timestamp_flag,
            properties_complete: false,
            terminator_bit: false,
            header: final_header,
            set_list_count: ext_data.set_list_count,
            tbk_ibk_teleport: ext_data.tbk_ibk_teleport,
            sockets: ext_data.sockets,
            modules: Vec::new(),
            range: crate::domain::item::ItemBitRange { start: start_bit, end: 0 },
            total_bits: 0,
            gap_bits: Vec::new(),
            segments: Vec::new(),
            expected_start_bit: 0,
            forensic_audit: ForensicAudit::new(),
        };

        if item.body.alpha_nudge.is_some() { item.forensic_audit.record(V105NudgeAxiom.metadata()); }
        if item.body.alpha_header_gap.is_some() { item.forensic_audit.record(V105HeaderGapAxiom.metadata()); }
        if item.body.alpha_shadow_skip_bits.is_some() { item.forensic_audit.record(V105ShadowAxiom.metadata()); }

        if !item.header.is_compact {
            let is_v105_shadow = axiom.is_v105_shadow(item.header.flags);
            let (props, complete, term, _extra_bits, _payload, shadow_bits, nested_items) = crate::domain::stats::parser::read_item_stats(
                cursor, 
                &item.code, 
                item.header.version, 
                ctx, 
                huff, 
                alpha_mode, 
                item.header.quality, 
                item.header.is_runeword, 
                is_v105_shadow, 
                item.header.is_personalized
            )?;
            item.properties = props.clone();
            item.stats.properties = props;
            item.properties_complete = complete;
            item.terminator_bit = term;
            item.body.alpha_shadow_skip_bits = shadow_bits;
            item.socketed_items = nested_items;
        }

        let axiom = StatsAxiom::new(item.header.version, item.header.quality.unwrap_or(ItemQuality::Normal), alpha_mode)
            .with_personalization(item.header.is_personalized);
        let consumed_bits = cursor.pos() - start_bit;
        let final_consumed = axiom.calculate_alignment(consumed_bits, item.header.is_compact, &item.code, item.header.flags);
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
}


pub fn is_v105_summary_code(code: &str) -> bool {
    matches!(code, "hp1"|"hp2"|"hp3"|"hp4"|"hp5"|"mp1"|"mp2"|"mp3"|"mp4"|"mp5"|"rvl"|"rvs"|"isc"|"tsc"|"w8cs"|"w88w"|"us g"|"xrs"|"6cs"|"7mgw"|"fsh"|"7pus"|"ww7c")
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
        if let Some((mode, location, _x, code, flags, version, is_compact, _header_bits, _nudge)) = peek_item_header_at(bytes, current_pos, huffman, alpha) {
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
            } else { break; }
        } else { break; }
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

pub fn write_property_list(
    emitter: &mut BitEmitter,
    code: &str,
    props: &[ItemProperty],
    nested_items: &[Item],
    huffman: &HuffmanTree,
    version: u8,
    alpha_runeword: bool,
    terminator_bit: bool,
    _quality: ItemQuality,
    is_v105_shadow: bool,
    axiom: &StatsAxiom,
) -> io::Result<()> {
    let start_bits = emitter.written_bits();
    let is_compact = code.trim().is_empty() || code.len() < 3;
    let rhythm = axiom.property_rhythm(alpha_runeword, is_v105_shadow, is_compact, 0);
    let id_bits = rhythm.id_bits;
    let terminator = (1 << id_bits) - 1;
    let mut item_idx = 0;
    for prop in props {
        let raw_id = prop.stat_id;
        emitter.write_bits(raw_id, id_bits)?;
        
        let mut handled = false;
        if axiom.is_alpha() && (raw_id == 317 || axiom.map_alpha_id(raw_id) == 317) {
             if item_idx < nested_items.len() {
                 let child = &nested_items[item_idx];
                 if crate::item::item_trace_enabled() {
                     println!("[DEBUG] write_property_list Stat 317: Writing nested item {}, start_bits={}", item_idx, emitter.written_bits());
                 }
                 let child_bytes = child.to_bytes(huffman, axiom.save_is_alpha)?;
                 for byte in child_bytes { emitter.write_bits(byte as u32, 8)?; }
                 item_idx += 1;
                 handled = true;
             }
        }

        if !handled {
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
    }
    emitter.write_bits(terminator, id_bits)?;
    let preserve_trailing_align = axiom.is_alpha() && version == 0 && code.trim().is_empty();
    if rhythm.has_terminal_bit {
        emitter.write_bit(terminator_bit)?;
        if rhythm.has_extra_terminal_bit { emitter.write_bit(terminator_bit)?; }
        if !preserve_trailing_align { emitter.byte_align()?; }
    }
    
    if crate::item::item_trace_enabled() {
        println!("[DEBUG] write_property_list code='{}': props={}, start_bits={}, end_bits={}", code.trim(), props.len(), start_bits, emitter.written_bits());
    }
    Ok(())
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

pub fn write_player_name(emitter: &mut BitEmitter, name: &str, alpha_v5: bool) -> io::Result<()> {
    let width = if alpha_v5 { 8 } else { 7 };
    for ch in name.chars() { emitter.write_bits((ch as u8) as u32, width)?; }
    emitter.write_bits(0, width)?;
    Ok(())
}


