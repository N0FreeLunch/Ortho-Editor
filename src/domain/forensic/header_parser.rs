use crate::data::bit_cursor::BitCursor;
use crate::domain::forensic::header_policy::HeaderAxiom;
use crate::domain::forensic::header_scan::peek_item_header_at;
use crate::domain::header::entity::{ItemHeader, ItemSegmentType};
use crate::domain::payload::HuffmanTree;
use crate::error::{ParsingError, ParsingResult};
use bitstream_io::BitRead;

/// Preserves the mixed legacy API while keeping recovery outside the codec.
pub fn parse_header<R: BitRead>(
    cursor: &mut BitCursor<R>,
    axiom: &HeaderAxiom,
    huffman: &HuffmanTree,
    ctx: Option<(&[u8], u64)>,
) -> ParsingResult<ItemHeader> {
    if axiom.alpha_mode {
        cursor.begin_segment(ItemSegmentType::Header);
        alpha_sync(cursor, axiom, huffman, ctx)
    } else {
        crate::domain::header::parse_legacy_header(cursor)
    }
}

fn alpha_sync<R: BitRead>(
    cursor: &mut BitCursor<R>,
    axiom: &HeaderAxiom,
    huffman: &HuffmanTree,
    ctx: Option<(&[u8], u64)>,
) -> ParsingResult<ItemHeader> {
    let Some((section_bytes, start_bit)) = ctx else {
        return Err(cursor.fail(ParsingError::Generic(
            "Alpha v105 requires context for heuristic sync".to_string(),
        )));
    };

    let header_info = peek_item_header_at(section_bytes, start_bit, huffman, axiom.alpha_mode, 0)
        .ok_or_else(|| {
        cursor.fail(ParsingError::Generic(
            "Alpha heuristic probe failed".to_string(),
        ))
    })?;
    let (
        peek_m,
        peek_l,
        peek_x,
        _code,
        flags,
        version,
        is_compact,
        header_bits,
        _nudge,
        has_checksum,
    ) = header_info;

    let current_total = cursor.pos();
    let target_header_bits = header_bits;
    let skip_amount = (target_header_bits as i64) - (current_total as i64);

    if skip_amount > 0 {
        cursor.skip_and_record(skip_amount as u32)?;
    }

    let is_identified = (flags & (1 << 4)) != 0;
    let is_personalized = axiom.is_personalized(flags, is_compact);
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
        has_checksum,
        alpha_checksum: None,
        alpha_quality_raw: None,
        alpha_v5_runeword_extra: None,
        alpha_unique_id_raw: None,
        save_is_alpha: axiom.alpha_mode,
    })
}
