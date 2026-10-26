// This software is licensed under the PolyForm Noncommercial License 1.0.0.
// Required Notice: Copyright 2026 N0FreeLunch (https://github.com/N0FreeLunch/d2r-core)

use crate::data::bit_cursor::BitCursor;
use crate::data::generated::layout_policy::{
    LEGACY_CHARACTER_ATTRIBUTE_WIRE_LAYOUT as CHARACTER_ATTRIBUTE_WIRE,
    LEGACY_V105_SAVE_HEADER_LEN_BYTES,
};
use crate::error::{ParsingError, ParsingFailure, ParsingResult};

/// Consumes the admitted character prefix and returns its absolute list boundary.
/// The caller retains item decoding and its existing ownership-error policy.
pub(crate) fn locate_v3_player_list<F>(bytes: &[u8], failure: F) -> ParsingResult<(usize, u32)>
where
    F: Fn(&str, u64, &str) -> ParsingFailure,
{
    let save_version = bytes
        .get(4..8)
        .and_then(|raw| <[u8; 4]>::try_from(raw).ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| ParsingFailure {
            error: ParsingError::UnexpectedValue {
                field: "D2R v3 save version".to_string(),
                value: "missing".to_string(),
                reason: "The save header does not contain a complete version field".to_string(),
            },
            context_stack: vec!["read_player_items_d2r_v3".to_string()],
            bit_offset: 32,
            context_relative_offset: 0,
            hint: None,
        })?;
    if save_version != 0x69 {
        return Err(failure(
            "D2R v3 prefix version",
            32,
            "Only the fixture-verified version 0x69 prefix is supported",
        ));
    }
    if bytes.get(..4)
        != Some(
            crate::domain::header::axiom::D2S_MAGIC
                .to_le_bytes()
                .as_slice(),
        )
    {
        return Err(failure(
            "D2R v3 save magic",
            0,
            "A full save header is required; use the explicit-offset API for list fragments",
        ));
    }
    // Locate the player list by consuming its prefix, not by marker recovery.
    let gf_pos = LEGACY_V105_SAVE_HEADER_LEN_BYTES;
    if bytes.get(gf_pos..gf_pos + CHARACTER_ATTRIBUTE_WIRE.marker.len())
        != Some(CHARACTER_ATTRIBUTE_WIRE.marker.as_slice())
    {
        return Err(failure(
            "D2R v3 attribute marker",
            gf_pos as u64 * 8,
            "The verified fixed header must be followed by gf",
        ));
    }
    let mut cursor = BitCursor::from_raw_at(
        bytes,
        (gf_pos as u64 + CHARACTER_ATTRIBUTE_WIRE.marker.len() as u64) * 8,
    );
    cursor.push_context("D2R v3 character attributes");
    loop {
        let id_start = cursor.pos();
        let stat_id = cursor.read_bits::<u32>(CHARACTER_ATTRIBUTE_WIRE.stat_id_bits)?;
        if stat_id == CHARACTER_ATTRIBUTE_WIRE.terminator_id {
            break;
        }
        // Character-stat widths are distinct from item-property widths.
        // Unverified character IDs must not turn into a guessed section boundary.
        if stat_id > 15 {
            return Err(failure(
                "D2R v3 character attribute ID",
                id_start,
                "The full-save locator currently supports only verified character IDs 0..15",
            ));
        }
        cursor.skip(crate::domain::stats::attribute::char_stat_save_bits(stat_id, false) as u64)?;
    }
    cursor.byte_align()?;
    let if_pos = (cursor.pos() / 8) as usize;
    if bytes.get(if_pos..if_pos + 2) != Some(b"if".as_slice()) {
        return Err(failure(
            "D2R v3 skill marker",
            cursor.pos(),
            "The character-stat terminator and alignment must end immediately before if",
        ));
    }
    let player_pos = if_pos + 2 + super::skills::SKILL_SECTION_LEN;
    Ok((player_pos, save_version))
}

#[cfg(test)]
mod tests {
    use super::locate_v3_player_list;
    use crate::error::ParsingError;
    use crate::item::{HuffmanTree, Item};
    use d2r_macros::settled_contract_test;

    #[settled_contract_test]
    fn prefix_boundary_does_not_consume_the_item_list() {
        let beta = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/savegames/beta/amazon_v105_slice2_equipment.d2s"
        ))
        .expect("The registered beta authority must exist");
        assert_eq!(&beta[833..835], b"gf");
        assert_eq!(&beta[884..886], b"if");
        assert_eq!(&beta[916..920], b"JM\x10\x00");

        let prefix_only = &beta[..916];
        let boundary = locate_v3_player_list(prefix_only, |field, offset, reason| {
            panic!("Valid prefix rejected at {offset}: {field}: {reason}")
        })
        .expect("A complete character prefix needs no item-list bytes");
        assert_eq!(boundary, (916, 0x69));

        let failure = Item::read_player_items_d2r_v3(prefix_only, &HuffmanTree::new())
            .expect_err("The item caller still requires its owned counted-list header");
        match failure.error {
            ParsingError::UnexpectedValue {
                field,
                value,
                reason,
            } => {
                assert_eq!(field, "D2R v3 counted list header");
                assert_eq!(value, "invalid or unsupported");
                assert_eq!(reason, "The owned list header is truncated");
            }
            other => panic!("Unexpected item boundary failure: {other:?}"),
        }
        assert_eq!(failure.bit_offset, 916 * 8);
        assert_eq!(failure.context_relative_offset, 0);
        assert_eq!(failure.context_stack, vec!["D2R v3 player list ownership"]);
        assert_eq!(
            failure.hint.as_deref(),
            Some(
                "Preserve the input and verify the section boundary; do not scan a later JM list."
            )
        );
    }
}
