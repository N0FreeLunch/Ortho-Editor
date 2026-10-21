use crate::data::bit_cursor::BitCursor;
use crate::data::generated::layout_policy::{
    D2rV3ItemHeaderFlags as F, D2rV3ItemHeaderLayout as W,
};
use crate::domain::header::entity::{ItemHeader, ItemSegmentType};
use crate::error::ParsingResult;
use bitstream_io::BitRead;

pub(crate) fn read_d2r_v3_header_with_storage<R: BitRead>(
    cursor: &mut BitCursor<R>,
) -> ParsingResult<(ItemHeader, Option<u8>)> {
    cursor.begin_segment(ItemSegmentType::Header);

    let flags = cursor.read_bits::<u32>(W::FLAGS)?;
    let has_extended_version = cursor.read_bits::<u8>(W::VERSION_EXTENSION)? != 0;
    let version_low = cursor.read_bits::<u8>(W::VERSION_LOW)?;
    let version = if has_extended_version {
        version_low.saturating_add(W::EXTENDED_VERSION_OFFSET)
    } else {
        version_low
    };
    let mode = cursor.read_bits::<u8>(W::MODE)?;

    let mut raw_stored_in = None;
    let (location, x, y, page) = if W::GROUND_MODES.contains(&mode) {
        let column = cursor.read_bits::<u16>(W::GROUND_COORDINATE)?;
        let row = cursor.read_bits::<u16>(W::GROUND_COORDINATE)?;
        (
            0,
            column.min(u8::MAX as u16) as u8,
            row.min(u8::MAX as u16) as u8,
            0,
        )
    } else {
        let position = cursor.read_bits::<u8>(W::POSITION)?;
        let column = cursor.read_bits::<u8>(W::POSITION)?;
        let row = cursor.read_bits::<u8>(W::POSITION)?;
        let stored_in = cursor.read_bits::<u8>(W::STORED_IN)?;
        raw_stored_in = Some(stored_in);
        let stored_in = stored_in.saturating_sub(1);
        (position, column, row, stored_in)
    };

    cursor.end_segment();

    Ok((
        ItemHeader {
            flags,
            version,
            mode,
            location,
            x,
            y,
            page,
            socket_hint: 0,
            id: None,
            level: None,
            quality: None,
            is_compact: flags & F::COMPACT != 0,
            is_identified: flags & F::IDENTIFIED != 0,
            is_socketed: flags & F::SOCKETED != 0,
            is_personalized: flags & F::PERSONALIZED != 0,
            is_runeword: flags & F::RUNEWORD != 0,
            is_ethereal: flags & F::ETHEREAL != 0,
            is_ear: flags & F::EAR != 0,
            has_checksum: false,
            alpha_checksum: None,
            alpha_quality_raw: None,
            alpha_v5_runeword_extra: None,
            alpha_unique_id_raw: None,
            save_is_alpha: false,
        },
        raw_stored_in,
    ))
}
