use bitstream_io::BitRead;
use crate::data::bit_cursor::BitCursor;
use crate::domain::header::entity::{ItemHeader, HeaderAxiom, ItemSegmentType};
use crate::item::HuffmanTree;
use crate::error::{ParsingError, ParsingResult};

/// Parses an item header from the bit cursor.
pub fn parse_header<R: BitRead>(
    cursor: &mut BitCursor<R>,
    axiom: &HeaderAxiom,
    huffman: &HuffmanTree,
    ctx: Option<(&[u8], u64)>,
) -> ParsingResult<ItemHeader> {
    cursor.begin_segment(ItemSegmentType::Header);

    if axiom.is_alpha() {
        return alpha_sync(cursor, axiom, huffman, ctx);
    }

    let flags: u32 = cursor.read_bits::<u32>(32)?;
    let version: u8 = cursor.read_bits::<u8>(3)? as u8;
    
    let mode: u8 = cursor.read_bits::<u8>(3)? as u8;
    let location: u8 = cursor.read_bits::<u8>(3)? as u8;
    let x: u8 = (cursor.read_bits::<u8>(4)? as u32 & 0x0F) as u8;

    let is_compact = (flags & (1 << 21)) != 0;
    
    let (y, page, socket_hint) = if is_compact || axiom.is_alpha() {
        (0, 0, 0)
    } else {
        let y = (cursor.read_bits::<u8>(4)? as u32 & 0x0F) as u8;
        let page = (cursor.read_bits::<u8>(3)? as u32 & 0x07) as u8;
        let socket_hint_width = if axiom.is_alpha() { 3 } else { 3 }; // Restore 3 for now
        let socket_hint = (cursor.read_bits::<u8>(socket_hint_width)? as u32 & ((1 << socket_hint_width) - 1)) as u8;
        (y, page, socket_hint)
    };

    if axiom.is_alpha() {
        let _padding = cursor.read_bits::<u8>(8)?; // The mysterious 8-bit Alpha header extension
    }

    let is_ear = (flags & (1 << 16)) != 0;
    let is_identified = (flags & (1 << 4)) != 0;
    let is_personalized = (flags & (1 << 24)) != 0;
    let is_runeword = (flags & (1 << 26)) != 0;
    let is_socketed = (flags & (1 << 11)) != 0;
    let is_ethereal = (flags & (1 << 22)) != 0;

    cursor.end_segment();

    Ok(ItemHeader {
        flags,
        version,
        mode,
        location,
        x,
        y,
        page,
        socket_hint,
        id: None,
        quality: None,
        is_compact,
        is_identified,
        is_socketed,
        is_personalized,
        is_runeword,
        is_ethereal,
        is_ear,
        alpha_quality_raw: None,
        alpha_v5_runeword_extra: None,
    })
}

fn alpha_sync<R: BitRead>(
    cursor: &mut BitCursor<R>,
    axiom: &HeaderAxiom,
    huffman: &HuffmanTree,
    ctx: Option<(&[u8], u64)>,
) -> ParsingResult<ItemHeader> {
    let Some((section_bytes, start_bit)) = ctx else {
        return Err(cursor.fail(ParsingError::Generic("Alpha v105 requires context for heuristic sync".to_string())));
    };

    // Peek ahead to find the best-matched Alpha header.
    let Some((peek_m, peek_l, peek_x, _code, flags, version, is_compact, header_bits, _nudge)) = 
        crate::item::peek_item_header_at(section_bytes, start_bit, huffman, axiom.alpha_mode)
    else {
        return Err(cursor.fail(ParsingError::Generic("Alpha heuristic probe failed".to_string())));
    };

    // Re-sync cursor position.
    let current_total = cursor.pos();
    let target_header_bits = header_bits; 
    let skip_amount = (target_header_bits as i64) - (current_total as i64);
    
    if skip_amount > 0 {
        cursor.skip_and_record(skip_amount as u32)?;
    }

    let is_identified = (flags & (1 << 4)) != 0;
    let is_personalized = (flags & (1 << 28)) != 0; // Alpha v105 bit 28
    let is_runeword = (flags & (1 << 26)) != 0;
    let is_socketed = (flags & (1 << 27)) != 0; // Alpha v105 bit 27
    let is_ethereal = (flags & (1 << 22)) != 0;

    cursor.end_segment();

    Ok(ItemHeader {
        flags,
        version,
        mode: peek_m,
        location: peek_l,
        x: peek_x,
        y: 0,
        page: 0,
        socket_hint: 0,
        id: None,
        quality: None,
        is_compact,
        is_identified,
        is_socketed,
        is_personalized,
        is_runeword,
        is_ethereal,
        is_ear: false,
        alpha_quality_raw: None,
        alpha_v5_runeword_extra: None,
    })
}
