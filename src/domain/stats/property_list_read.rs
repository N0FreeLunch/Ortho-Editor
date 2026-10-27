//! Select concrete list providers outside the canonical sequencing kernel.
use crate::data::bit_cursor::BitCursor;
use crate::data::generated::layout_policy::ALPHA_LEGACY_AUTHORITY_CODES;
use crate::domain::forensic::property_list::LegacyListReader;
use crate::domain::item::{HuffmanTree, Item};
use crate::domain::stats::StatsAxiom;
use crate::domain::stats::budget::PropertyListBudget;
use crate::domain::stats::parser::{
    MAX_ALPHA_V105_ITEM_BITS, PropertyReaderContext, property_list_depth_exhausted,
};
use crate::domain::stats::property_list::{
    ListReadConfig, ParsedPropertyList, read_property_stream,
};
use crate::domain::stats::retail_list::read_retail_property_list;
use crate::error::ParsingResult;
use bitstream_io::BitRead;
use std::collections::HashMap;

pub(crate) fn read_property_list_with_pair_policy<R: BitRead, F>(
    recorder: &mut BitCursor<R>,
    code: &str,
    version: u8,
    reader_ctx: PropertyReaderContext,
    huffman: &HuffmanTree,
    alpha_runeword: bool,
    is_v105_shadow: bool,
    axiom: &StatsAxiom,
    consume_d2r_v3_paired_stats: bool,
    child_marker_offsets: Option<&[u64]>,
    child_marker_codes: Option<&HashMap<u64, String>>,
    recovery_fn: F,
) -> ParsingResult<ParsedPropertyList<Item>>
where
    F: FnMut(&[u8], u64, &HuffmanTree, usize, bool) -> ParsingResult<(Item, u64)>,
{
    // Raw format selection must also retain legacy behavior for unadmitted versions.
    if !axiom.save_is_alpha {
        return read_retail_property_list(
            recorder,
            code,
            alpha_runeword,
            axiom,
            consume_d2r_v3_paired_stats,
        );
    }
    let start_pos = recorder.pos();
    let alpha = axiom.save_is_alpha && axiom.is_alpha();
    let is_authority_host = alpha && ALPHA_LEGACY_AUTHORITY_CODES.contains(&code.trim());
    let actual_runeword =
        (alpha_runeword || code.trim() == "Þ." || is_authority_host) && axiom.is_socketed;
    let budget = PropertyListBudget::new(recorder.limit(), alpha && actual_runeword);
    if let Some(limit) = budget.entry_extension() {
        recorder.set_limit(limit);
    }
    let is_compact = axiom.is_compact || code.trim().is_empty();
    let config = ListReadConfig {
        single_entry: is_compact && !actual_runeword,
        bit_budget: if alpha && !actual_runeword {
            Some(MAX_ALPHA_V105_ITEM_BITS)
        } else {
            None
        },
        mark_limit_as_terminal: alpha,
        child_stop_count: if alpha && actual_runeword {
            Some(3)
        } else {
            None
        },
    };

    let section_base_abs = if is_compact {
        recorder.base_pos.saturating_sub(reader_ctx.item_start_bit)
    } else {
        recorder.base_pos
    };
    if property_list_depth_exhausted() {
        return Ok((Vec::new(), false, false, Vec::new()));
    }
    let mut reader = LegacyListReader::new(
        code,
        version,
        reader_ctx,
        huffman,
        alpha_runeword,
        is_v105_shadow,
        is_compact,
        actual_runeword,
        is_authority_host,
        consume_d2r_v3_paired_stats,
        axiom,
        section_base_abs,
        child_marker_offsets,
        child_marker_codes,
        recovery_fn,
    );
    read_property_stream(recorder, start_pos, config, budget, &mut reader)
}
