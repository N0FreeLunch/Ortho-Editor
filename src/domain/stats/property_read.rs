//! Compose canonical entry reads with an explicit save-format selector.
use crate::data::bit_cursor::BitCursor;
use crate::domain::forensic::property_entry::{LegacyEntryPolicy, LegacyEntryRecovery};
use crate::domain::item::HuffmanTree;
use crate::domain::stats::parser::PropertyReaderContext;
use crate::domain::stats::property_entry::read_property_entry;
use crate::domain::stats::retail_entry::{NoChildRecovery, RetailEntryPolicy};
use crate::domain::stats::{ItemProperty, StatsAxiom};
use crate::error::ParsingResult;
use bitstream_io::BitRead;

pub(crate) fn parse_single_property_internal<R: BitRead, F>(
    recorder: &mut BitCursor<R>,
    _version: u8,
    huffman: &HuffmanTree,
    alpha_runeword: bool,
    is_compact: bool,
    is_v105_shadow: bool,
    consume_d2r_v3_paired_stats: bool,
    axiom: &StatsAxiom,
    reader_ctx: PropertyReaderContext,
    recovery_fn: &mut F,
) -> ParsingResult<
    Option<(
        Vec<ItemProperty>,
        bool,
        bool,
        Vec<crate::domain::item::Item>,
    )>,
>
where
    F: FnMut(
        &[u8],
        u64,
        &HuffmanTree,
        usize,
        bool,
    ) -> ParsingResult<(crate::domain::item::Item, u64)>,
{
    if !axiom.save_is_alpha {
        let policy = RetailEntryPolicy::new(&axiom.code, consume_d2r_v3_paired_stats);
        return read_property_entry(recorder, &policy, &mut NoChildRecovery);
    }
    let policy = LegacyEntryPolicy::new(
        _version,
        alpha_runeword,
        is_compact,
        is_v105_shadow,
        consume_d2r_v3_paired_stats,
        axiom,
    );
    let mut recovery =
        LegacyEntryRecovery::new(alpha_runeword, axiom, reader_ctx, huffman, recovery_fn);
    read_property_entry(recorder, &policy, &mut recovery)
}
