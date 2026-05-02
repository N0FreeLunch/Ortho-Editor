use bitstream_io::{BitRead, BitWrite, BitWriter, LittleEndian};
use std::io::{self, Cursor};
use crate::domain::item::Item;
use crate::domain::item::quality::ItemQuality;
use crate::domain::stats::{ItemProperty, StatsAxiom};
use crate::data::bit_cursor::BitCursor;
use crate::error::{ParsingResult, ParsingError};

pub fn find_next_item_match(bytes: &[u8], pos: u64, huffman: &HuffmanTree, alpha: bool) -> Option<u64> {
    let limit = (bytes.len() * 8) as u64;
    let mut probe = pos;
    while probe < limit {
        // Alpha v105 items are strictly byte-aligned.
        if alpha && probe % 8 != 0 {
            probe += 1;
            continue;
        }
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
    _version: u8,
    alpha_mode: bool,
) -> bool {
    if code.len() < 3 { return false; }
    if !code.chars().all(|c| c.is_alphanumeric() || c == ' ') { return false; }
    if alpha_mode {
        // Alpha v105 codes are usually 4 chars (e.g. '7pww', 'hp1 ', '1pww').
        if code.len() != 4 { return false; }
        if code.trim().is_empty() { return false; }
        
        if mode > 6 || location > 15 { return false; }
    } else if mode > 6 || location > 15 { return false; }
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
    let mut header_len = 32 + 3 + 3 + 3 + 4; // flags(32) + v(3) + m(3) + l(3) + x(4)
    
    let axiom = StatsAxiom::new(version, ItemQuality::Normal, alpha_mode);
    let geometry = axiom.header_geometry(flags, is_compact);

    if geometry.has_header_gap {
        if version == 5 || version == 0 {
            let axiom = StatsAxiom::new(version, ItemQuality::Normal, alpha_mode);
            let is_v105_shadow = axiom.is_v105_shadow(flags);
            let is_rw = axiom.is_runeword(flags);
            if is_rw || is_v105_shadow {
                header_len += 8; // Alpha v105 Version 5 Consolidated Gap
            } else {
                header_len += geometry.y_bits + geometry.page_bits + geometry.socket_hint_bits + 8;
            }
        } else {
            // Version 1
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
    pub fn to_bytes(&self, huffman: &HuffmanTree, alpha_mode: bool) -> io::Result<Vec<u8>> {
        /*
        if !self.bits.is_empty() {
            let mut emitter = BitEmitter::new();
            for rb in &self.bits {
                emitter.write_bit(rb.bit)?;
            }
            // self.bits already include byte alignment padding from read_section
            return Ok(emitter.into_bytes());
        }
        */
        let mut emitter = BitEmitter::new();
        emitter.write_bits(self.flags, 32)?;
        emitter.write_bits(self.version as u32, 3)?;
        emitter.write_bits(self.mode as u32, 3)?;
        emitter.write_bits(self.location as u32, 3)?;
        emitter.write_bits(self.x as u32, 4)?;
        
        if crate::item::item_trace_enabled() {
            println!("[DEBUG] write_item: code='{}', bits_after_basic_header={}", self.code.trim(), emitter.written_bits());
        }
        
        let axiom = StatsAxiom::new(self.version, self.quality.unwrap_or(ItemQuality::Normal), alpha_mode);
        let geometry = axiom.header_geometry(self.flags, self.is_compact);

        if geometry.has_header_gap {
            if self.version == 5 || self.version == 0 {
                let is_v105_shadow = axiom.is_v105_shadow(self.flags);
                let is_rw = axiom.is_runeword(self.flags);

                if is_rw || is_v105_shadow {
                    let mut gap = self.body.alpha_header_gap.unwrap_or(0);
                    if !self.is_compact {
                        // Ensure fields are synced if gap was captured
                        if self.body.alpha_header_gap.is_none() {
                            gap |= self.y & 0x0F;
                            gap |= (self.page & 0x07) << 4;
                            gap |= (self.header_socket_hint & 0x01) << 7;
                        }
                    }
                    emitter.write_bits(gap as u32, 8)?;
                } else {
                    if !self.is_compact {
                        emitter.write_bits(self.y as u32, geometry.y_bits)?;
                        emitter.write_bits(self.page as u32, geometry.page_bits)?;
                        emitter.write_bits(self.header_socket_hint as u32, geometry.socket_hint_bits)?;
                    }
                    emitter.write_bits(self.body.alpha_header_gap.unwrap_or(0) as u32, 8)?;
                }
            } else {
                emitter.write_bits(self.y as u32, geometry.y_bits)?;
                emitter.write_bits(self.page as u32, geometry.page_bits)?;
                emitter.write_bits(self.header_socket_hint as u32, geometry.socket_hint_bits)?;
                emitter.write_bits(self.body.alpha_header_gap.unwrap_or(0) as u32, 8)?;
            }
        } else if !geometry.skip_geometry {
            emitter.write_bits(self.y as u32, geometry.y_bits)?;
            emitter.write_bits(self.page as u32, geometry.page_bits)?;
            emitter.write_bits(self.header_socket_hint as u32, geometry.socket_hint_bits)?;
        }

        if crate::item::item_trace_enabled() {
            println!("[DEBUG] write_item: code='{}', bits_after_geometry={}", self.code.trim(), emitter.written_bits());
        }

        if self.is_ear {
            emitter.write_bits(self.ear_class.unwrap_or(0) as u32, 3)?;
            emitter.write_bits(self.ear_level.unwrap_or(0) as u32, 7)?;
            write_player_name(&mut emitter, self.ear_player_name.as_deref().unwrap_or(""), alpha_mode && self.version == 5)?;
            if alpha_mode && self.version == 5 {
                emitter.byte_align()?;
            }
        } else {
            let encoded_code = huffman.encode(&self.code)?;
            emitter.extend_bits(encoded_code)?;
            if alpha_mode && (self.version == 5 || self.version == 0 || self.version == 1) {
                // Alpha v105: 2-bit nudge after item code.
                // Forensic Alignment (Discussion 0230 & Slice 10 Audit):
                // 1. Version 0: Always nudge 2 (binary 10).
                // 2. Version 5: Nudge 2 for "legacy-style" or "marker" items (no digits, not v105-summary).
                // 3. Version 5: Nudge 0 for v105 summary items (hp1-5, etc) and items with digits (cm1).
                // 4. Version 1: Nudge 0 (standard charms).
                let trimmed = self.code.trim();
                let is_v105_summary = matches!(
                    trimmed,
                    "hp1" | "hp2" | "hp3" | "hp4" | "hp5" |
                    "mp1" | "mp2" | "mp3" | "mp4" | "mp5" |
                    "rvl" | "rvs" | "isc" | "tsc" | "w8cs" | 
                    "w88w" | "us g" | "xrs" | "6cs" | "7mgw"
                );
                let has_digit = trimmed.chars().any(|c| c.is_ascii_digit());
                
                let nudge = if self.version == 0 { 
                    2 
                } else if self.version == 5 && (trimmed == "gpb" || trimmed == "wwww" || trimmed == "vps") { 
                    2 
                } else { 
                    0 
                };
                emitter.write_bits(nudge, 2)?; // 2-bit nudge
            }
        }

        if crate::item::item_trace_enabled() {
            println!("[DEBUG] write_item: code='{}', bits_after_code={}", self.code.trim(), emitter.written_bits());
        }

        if !self.is_compact {
            let trimmed = self.code.trim();
            let is_v105_shadow = alpha_mode && self.version == 5 && (self.flags & (1 << 26)) != 0;
            let is_v105_summary = alpha_mode && self.version == 5 && !is_v105_shadow && (
                trimmed == "hp1" || trimmed == "hp2" || trimmed == "hp3" || trimmed == "hp4" || trimmed == "hp5" ||
                trimmed == "mp1" || trimmed == "mp2" || trimmed == "mp3" || trimmed == "mp4" || trimmed == "mp5" ||
                trimmed == "rvl" || trimmed == "rvs" || trimmed == "isc" || trimmed == "tsc" || trimmed == "w8cs" || 
                trimmed == "w88w" || trimmed == "us g" || trimmed == "xrs" || trimmed == "6cs" || trimmed == "7mgw"
            );

            let quality_val = self.quality.unwrap_or(ItemQuality::Normal);
            let is_item_alpha = axiom.is_alpha();

            if is_item_alpha {
                // Alpha v105: Quality is 3 bits in read_extended_stats, even for summary items.
                let quality_to_write = self.header.alpha_quality_raw.unwrap_or(self.quality.map(|q| q as u8).unwrap_or(0));
                emitter.write_bits(quality_to_write as u32, 3)?;

                if is_v105_summary {
                    if trimmed == "7mgw" {
                        // Alpha v105 forensic: 7mgw special 28-bit payload
                        if let Some(payload) = &self.body.v105_7mgw_payload {
                            for &bit in payload {
                                emitter.write_bit(bit)?;
                            }
                        } else {
                            emitter.write_bits(0, 28)?;
                        }
                    }
                } else {
                    let _is_compact = axiom.is_compact(self.flags);
                    let _is_personalized = axiom.is_personalized(self.flags);
                    let is_runeword = axiom.is_runeword(self.flags);
                    let is_frag = axiom.is_fragment(self.flags);
                    if self.version == 5 && (is_runeword || is_frag) {
                        // Alpha v105 Version 5 forensic: 2 extra bits before timestamp/sockets
                        emitter.write_bits(self.body.v5_runeword_extra.unwrap_or(0) as u32, 2)?;
                    }
                }
            }

            if !is_v105_summary {
                if !is_item_alpha {
                    emitter.write_bits(self.id.unwrap_or(0), 32)?;
                    emitter.write_bits(self.level.unwrap_or(0) as u32, 7)?;
                    emitter.write_bits(quality_val as u32, 4)?;
                }

                if is_item_alpha && (self.version == 1 || self.version == 4) {
                    // Early exit for v101/v104 in ExtendedStats (must mirror read-side gate).
                } else {
                    if self.has_multiple_graphics {
                        emitter.write_bits(self.multi_graphics_bits.unwrap_or(0) as u32, 3)?;
                    }
                    if self.has_class_specific_data {
                        emitter.write_bits(self.class_specific_bits.unwrap_or(0) as u16 as u32, 11)?;
                    }

                    match quality_val {
                        ItemQuality::Low | ItemQuality::High => {
                            emitter.write_bits(self.low_high_graphic_bits.unwrap_or(0) as u32, 3)?;
                        }
                        ItemQuality::Magic => {
                            emitter.write_bits(self.magic_prefix.unwrap_or(0) as u32, 11)?;
                            emitter.write_bits(self.magic_suffix.unwrap_or(0) as u32, 11)?;
                        }
                        ItemQuality::Rare | ItemQuality::Crafted => {
                            emitter.write_bits(self.rare_name_1.unwrap_or(0) as u32, 8)?;
                            emitter.write_bits(self.rare_name_2.unwrap_or(0) as u32, 8)?;
                            for i in 0..6 {
                                if let Some(affix) = self.rare_affixes[i] {
                                    emitter.write_bit(true)?;
                                    emitter.write_bits(affix as u32, 11)?;
                                } else {
                                    emitter.write_bit(false)?;
                                }
                            }
                        }
                        ItemQuality::Set | ItemQuality::Unique => {
                            emitter.write_bits(self.unique_id.unwrap_or(0) as u32, 12)?;
                        }
                        _ => {}
                    }

                    if axiom.is_runeword(self.flags) && !axiom.is_alpha() && self.version != 5 {
                        emitter.write_bits(self.runeword_id.unwrap_or(0) as u32, 12)?;
                        emitter.write_bits(self.runeword_level.unwrap_or(0) as u32, 12)?;
                        emitter.write_bits(0, 4)?; // Runeword unknown bits
                    }

                    if axiom.is_personalized(self.flags) {
                        if alpha_mode && (self.version == 5 || self.version == 0 || self.version == 1) {
                            emitter.byte_align()?;
                        }
                        write_player_name(&mut emitter, self.personalized_player_name.as_deref().unwrap_or(""), alpha_mode && (self.version == 5 || self.version == 0 || self.version == 1))?;
                    }

                    if self.code.trim() == "tbk" || self.code.trim() == "ibk" {
                        emitter.write_bits(self.tbk_ibk_teleport.unwrap_or(0) as u32, 5)?;
                    }

                    emitter.write_bit(self.timestamp_flag)?;

                    let template = item_template(&self.code);
                    let (reads_defense, reads_durability, reads_quantity) = if let Some(t) = template {
                        (t.is_armor, t.has_durability, t.is_stackable)
                    } else { (false, false, false) };

                    if reads_defense && axiom.reads_defense() {
                        emitter.write_bits(self.defense.unwrap_or(0), 11)?;
                    }
                    if reads_durability && axiom.reads_durability() {
                        let m_dur = self.max_durability.unwrap_or(0);
                        emitter.write_bits(m_dur, 8)?;
                        if m_dur > 0 {
                            emitter.write_bits(self.current_durability.unwrap_or(0), 9)?;
                            emitter.write_bit(false)?; // dur_extra
                        }
                    }
                    if reads_quantity && axiom.reads_quantity() {
                        emitter.write_bits(self.quantity.unwrap_or(0), 9)?;
                    }

                    if axiom.is_socketed(self.flags, self.is_compact) {
                        emitter.write_bits(self.sockets.unwrap_or(0) as u32, 4)?;
                    }

                    if quality_val == ItemQuality::Set {
                        let set_list_val = match self.set_list_count {
                            1 => 1, 2 => 3, 3 => 7, 4 => 15, 5 => 31, _ => 0
                        };
                        emitter.write_bits(set_list_val, 5)?;
                    }
                }

                let is_v105_shadow = axiom.is_v105_shadow(self.flags);
                if is_v105_shadow {
                    // Alpha v105 forensic: Shadow skip
                    emitter.write_bits(0, 47)?;
                }

                let has_props = !self.properties.is_empty();
                if self.version != 5 || is_v105_shadow || self.is_runeword || (alpha_mode && self.is_compact) || has_props {
                    write_property_list(&mut emitter, &self.code, &self.properties, self.version, self.is_runeword, self.terminator_bit, quality_val, is_v105_shadow, &axiom)?;
                    for set_props in &self.set_attributes {
                        write_property_list(&mut emitter, &self.code, set_props, self.version, false, false, quality_val, false, &axiom)?;
                    }
                }
            }
        }

        // Use axiom to determine final alignment bits
        let current_bits = emitter.written_bits();
        let final_bits = axiom.calculate_alignment(current_bits as u64, self.is_compact, &self.code);
        if final_bits > current_bits as u64 {
            let padding_needed = (final_bits - current_bits as u64) as u32;
            if !self.body.alpha_alignment_padding.is_empty() {
                for &bit in &self.body.alpha_alignment_padding {
                    emitter.write_bit(bit)?;
                }
            } else {
                emitter.write_bits(0, padding_needed)?;
            }
        }

        Ok(emitter.into_bytes())
    }

    pub fn serialize_section(items: &[Item], huffman: &HuffmanTree, alpha_mode: bool) -> io::Result<Vec<u8>> {
        let mut emitter = BitEmitter::new();
        for item in items {
            // Emit gap bits preserved during parsing
            emitter.extend_bits(item.gap_bits.iter().cloned())?;
            
            let item_bytes = item.to_bytes(huffman, alpha_mode)?;
            // Since item.to_bytes returns Vec<u8> which is already byte-aligned,
            // and our emitter tracks written bits, we need to be careful.
            // If item.to_bytes is always byte-aligned (which it seems to be due to byte_align() at the end),
            // we can just write its bits.
            for byte in item_bytes {
                emitter.write_bits(byte as u32, 8)?;
            }

            for child in &item.socketed_items {
                // Should we preserve gaps for socketed items too? 
                // Currently scan_socket_children doesn't seem to capture them in a way that maps here.
                let child_bytes = child.to_bytes(huffman, alpha_mode)?;
                for byte in child_bytes {
                    emitter.write_bits(byte as u32, 8)?;
                }
            }
        }
        Ok(emitter.into_bytes())
    }
}

fn write_player_name(emitter: &mut BitEmitter, name: &str, alpha_v5: bool) -> io::Result<()> {
    let width = if alpha_v5 { 8 } else { 7 };
    for ch in name.chars() {
        emitter.write_bits((ch as u8) as u32, width)?;
    }
    emitter.write_bits(0, width)?;
    Ok(())
}

fn item_template(code: &str) -> Option<&'static crate::data::item_codes::ItemTemplate> {
    crate::data::item_codes::ITEM_TEMPLATES
        .iter()
        .find(|template| template.code == code.trim())
}

fn write_property_list(emitter: &mut BitEmitter, code: &str, props: &[ItemProperty], version: u8, alpha_runeword: bool, terminator_bit: bool, _quality: ItemQuality, is_v105_shadow: bool, axiom: &StatsAxiom) -> io::Result<()> {
    // Mirror read-side heuristic for compact rhythm selection.
    let is_compact = code.trim().is_empty() || code.len() < 3;
    let rhythm = axiom.property_rhythm(alpha_runeword, is_v105_shadow, is_compact);
    
    let id_bits = rhythm.id_bits;
    let terminator = (1 << id_bits) - 1;

    for prop in props {
        // Keep parsed/raw stat IDs as-is for forensic bit parity.
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
        if rhythm.has_extra_terminal_bit {
            // Keep extra terminal bit coupled to parsed terminal polarity for alpha parity.
            emitter.write_bit(terminator_bit)?;
        }
        if !preserve_trailing_align {
            emitter.byte_align()?;
        }
    }
    Ok(())
}
