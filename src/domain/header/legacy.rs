use crate::data::bit_cursor::BitCursor;
use crate::data::generated::layout_policy::LegacyFixedHeaderLayout as L;
use crate::domain::header::entity::{ItemHeader, ItemSegmentType};
use crate::error::ParsingResult;
use bitstream_io::BitRead;

/// Decodes the legacy non-Alpha fixed header without heuristic recovery.
pub fn parse_legacy_header<R: BitRead>(cursor: &mut BitCursor<R>) -> ParsingResult<ItemHeader> {
    cursor.begin_segment(ItemSegmentType::Header);

    let flags = cursor.read_bits::<u32>(L::FLAGS_BITS)?;
    let version = cursor.read_bits::<u8>(L::VERSION_BITS)?;
    let mode = cursor.read_bits::<u8>(L::MODE_BITS)?;
    let location = cursor.read_bits::<u8>(L::LOCATION_BITS)?;
    let x = cursor.read_bits::<u8>(L::X_BITS)?;
    let y = cursor.read_bits::<u8>(L::Y_BITS)?;
    let page = cursor.read_bits::<u8>(L::PAGE_BITS)?;

    let mut socket_hint = 0;
    if (flags & L::SOCKETED_MASK) != 0 {
        socket_hint = cursor.read_bits::<u8>(L::SOCKET_HINT_BITS)?;
    }

    let is_compact = (flags & L::COMPACT_MASK) != 0;

    let is_ear = (flags & L::EAR_MASK) != 0;
    let is_identified = (flags & L::IDENTIFIED_MASK) != 0;
    let is_personalized = (flags & L::PERSONALIZED_MASK) != 0;
    let is_runeword = (flags & L::RUNEWORD_MASK) != 0;
    let is_socketed = (flags & L::SOCKETED_MASK) != 0;
    let is_ethereal = (flags & L::ETHEREAL_MASK) != 0;

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
