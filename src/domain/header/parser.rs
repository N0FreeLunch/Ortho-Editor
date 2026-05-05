use crate::data::bit_cursor::BitCursor;
use crate::domain::header::entity::{ItemHeader, ItemSegmentType};
use crate::domain::item::serialization::{HuffmanTree};
use crate::error::{ParsingError, ParsingResult};
use bitstream_io::{BitRead};
use crate::domain::header::HeaderAxiom;

pub fn parse_header<R: BitRead>(
    cursor: &mut BitCursor<R>,
    axiom: &HeaderAxiom,
    huffman: &HuffmanTree,
    ctx: Option<(&[u8], u64)>,
) -> ParsingResult<ItemHeader> {
    cursor.begin_segment(ItemSegmentType::Header);

    if axiom.alpha_mode {
        return alpha_sync(cursor, axiom, huffman, ctx);
    }

    let flags = cursor.read_bits::<u32>(32)?;
    let version = cursor.read_bits::<u8>(3)?;
    let mode = cursor.read_bits::<u8>(3)?;
    let location = cursor.read_bits::<u8>(3)?;
    let x = cursor.read_bits::<u8>(4)?;
    let y = cursor.read_bits::<u8>(4)?;
    let page = cursor.read_bits::<u8>(3)?;

    let mut socket_hint = 0;
    if (flags & (1 << 11)) != 0 {
        socket_hint = cursor.read_bits::<u8>(3)?;
    }

    let is_compact = (flags & (1 << 21)) != 0;

    if axiom.alpha_mode && !is_compact {
        let _padding = cursor.read_bits::<u8>(8)?; 
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
        level: None,
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
        alpha_unique_id_raw: None,
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

    let Some((peek_m, peek_l, peek_x, _code, flags, version, is_compact, header_bits, _nudge)) = 
        crate::item::peek_item_header_at(section_bytes, start_bit, huffman, axiom.alpha_mode)
    else {
        return Err(cursor.fail(ParsingError::Generic("Alpha heuristic probe failed".to_string())));
    };

    let current_total = cursor.pos();
    let target_header_bits = header_bits; 
    let skip_amount = (target_header_bits as i64) - (current_total as i64);
    
    if skip_amount > 0 {
        cursor.skip_and_record(skip_amount as u32)?;
    }

    let is_identified = (flags & (1 << 4)) != 0;
    let is_personalized = (flags & (1 << 28)) != 0; 
    let is_runeword = (flags & (1 << 26)) != 0;
    let is_socketed = (flags & (1 << 27)) != 0; 
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
        level: None,
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
        alpha_unique_id_raw: None,
    })
}
