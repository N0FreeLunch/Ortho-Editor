use crate::data::bit_cursor::BitCursor;
use crate::domain::header::entity::{ItemHeader, ItemSegmentType};
use crate::error::ParsingResult;
use bitstream_io::BitRead;

/// Decodes the legacy non-Alpha fixed header without heuristic recovery.
pub fn parse_legacy_header<R: BitRead>(cursor: &mut BitCursor<R>) -> ParsingResult<ItemHeader> {
    cursor.begin_segment(ItemSegmentType::Header);

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
        has_checksum: false,
        alpha_checksum: None,
        alpha_quality_raw: None,
        alpha_v5_runeword_extra: None,
        alpha_unique_id_raw: None,
        save_is_alpha: false,
    })
}
