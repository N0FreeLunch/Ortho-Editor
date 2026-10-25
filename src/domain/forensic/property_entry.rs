//! Legacy layout selection and inline child recovery for property entries.
use crate::data::bit_cursor::BitCursor;
use crate::data::generated::layout_policy::ALPHA_LEGACY_AUTHORITY_CODES;
use crate::domain::item::Item;
use crate::domain::payload::HuffmanTree;
use crate::domain::stats::parser::{
    PropertyReaderContext, nested_property_depth_failure, with_nested_property_depth,
};
use crate::domain::stats::property_entry::{
    PropertyEntryPolicy, PropertyEntryRecovery, PropertyRhythm, legacy_entry_value_width,
    trace_property_entry,
};
use crate::domain::stats::{StatMaturityTier, StatsAxiom};
use crate::error::ParsingResult;
use bitstream_io::BitRead;

pub(crate) struct LegacyEntryPolicy<'a> {
    version: u8,
    alpha_runeword: bool,
    is_compact: bool,
    is_v105_shadow: bool,
    consume_d2r_v3_paired_stats: bool,
    axiom: &'a StatsAxiom,
}

impl<'a> LegacyEntryPolicy<'a> {
    pub(crate) fn new(
        version: u8,
        alpha_runeword: bool,
        is_compact: bool,
        is_v105_shadow: bool,
        consume_d2r_v3_paired_stats: bool,
        axiom: &'a StatsAxiom,
    ) -> Self {
        Self {
            version,
            alpha_runeword,
            is_compact,
            is_v105_shadow,
            consume_d2r_v3_paired_stats,
            axiom,
        }
    }
}

impl PropertyEntryPolicy for LegacyEntryPolicy<'_> {
    fn trace_entry(&self, entry_start: u64, stat_id: u32) {
        trace_property_entry(entry_start, stat_id, &self.axiom.code);
    }

    fn rhythm(&self, stat_id: u32) -> PropertyRhythm {
        self.axiom.property_rhythm(
            self.alpha_runeword,
            self.is_v105_shadow,
            self.is_compact,
            stat_id,
        )
    }

    fn terminal_is_authority(&self) -> bool {
        self.axiom.save_is_alpha && ALPHA_LEGACY_AUTHORITY_CODES.contains(&self.axiom.code.trim())
    }

    fn tvs_padding_bits(&self) -> Option<u32> {
        self.axiom.tvs_padding_bits(self.alpha_runeword)
    }

    fn map_id(&self, stat_id: u32) -> u32 {
        self.axiom.map_alpha_id(stat_id)
    }

    fn omit_parameter(&self) -> bool {
        self.axiom
            .omits_property_parameter(&self.axiom.code, self.version)
    }

    fn effective_width(
        &self,
        stat_id: u32,
        stat_save_bits: Option<u32>,
        rhythm: PropertyRhythm,
    ) -> u32 {
        let default_width =
            self.axiom
                .default_property_value_width(self.version, stat_save_bits, rhythm);

        let effective_width = self.axiom.stat_bit_width(stat_id, default_width);
        legacy_entry_value_width(&self.axiom.code, stat_id, effective_width, || {
            self.axiom.map_alpha_id(stat_id)
        })
    }

    fn consume_paired_stats(&self) -> bool {
        self.consume_d2r_v3_paired_stats && !self.axiom.is_alpha()
    }

    fn maturity(&self, stat_id: u32, known: bool) -> StatMaturityTier {
        self.axiom.stat_maturity(stat_id, known)
    }
}

pub(crate) struct LegacyEntryRecovery<'a, 'f, F> {
    alpha_runeword: bool,
    axiom: &'a StatsAxiom,
    reader_ctx: PropertyReaderContext<'a>,
    huffman: &'a HuffmanTree,
    recovery_fn: &'f mut F,
}

impl<'a, 'f, F> LegacyEntryRecovery<'a, 'f, F> {
    pub(crate) fn new(
        alpha_runeword: bool,
        axiom: &'a StatsAxiom,
        reader_ctx: PropertyReaderContext<'a>,
        huffman: &'a HuffmanTree,
        recovery_fn: &'f mut F,
    ) -> Self {
        Self {
            alpha_runeword,
            axiom,
            reader_ctx,
            huffman,
            recovery_fn,
        }
    }
}

impl<R: BitRead, F> PropertyEntryRecovery<R, Item> for LegacyEntryRecovery<'_, '_, F>
where
    F: FnMut(&[u8], u64, &HuffmanTree, usize, bool) -> ParsingResult<(Item, u64)>,
{
    fn recover(
        &mut self,
        recorder: &mut BitCursor<R>,
        stat_id: u32,
        nested_items: &mut Vec<Item>,
    ) -> ParsingResult<bool> {
        let axiom = self.axiom;
        let alpha_runeword = self.alpha_runeword;
        let reader_ctx = &self.reader_ctx;
        let huffman = self.huffman;
        let recovery_fn = &mut *self.recovery_fn;
        let is_stat_317 = stat_id == 317 || axiom.map_alpha_id(stat_id) == 317;
        let is_stat_320 = stat_id == 320 || axiom.map_alpha_id(stat_id) == 320;
        let is_stat_387 = stat_id == 387 || axiom.map_alpha_id(stat_id) == 387;
        let is_already_nested = recorder.context_stack().iter().any(|s| s == "nested");
        let mut handled = false;
        if axiom.is_alpha()
            && alpha_runeword
            && axiom.is_socketed
            && (is_stat_317 || is_stat_320 || is_stat_387)
            && !is_already_nested
        {
            let entry_pos = recorder.pos();
            let absolute_entry_pos = reader_ctx.item_start_bit + entry_pos;
            let mut context = recorder.scoped_context("nested");
            let recorder = &mut *context;

            let mut found_pos = absolute_entry_pos;
            if is_stat_320 {
                for offset in 0..64 {
                    let probe_pos = absolute_entry_pos + offset;
                    if let Some(header_info) = crate::item::peek_item_header_at(
                        reader_ctx.bytes,
                        probe_pos,
                        huffman,
                        axiom.save_is_alpha,
                        0,
                    ) {
                        let (
                            mode,
                            loc,
                            _x,
                            code,
                            flags,
                            version,
                            _is_compact,
                            _header_bits,
                            _nudge,
                            _has_checksum,
                        ) = header_info;
                        if crate::item::is_plausible_item_header(
                            mode,
                            loc,
                            code.as_bytes(),
                            flags,
                            version,
                            axiom.save_is_alpha,
                        ) {
                            found_pos = probe_pos;
                            break;
                        }
                    }
                }
            }

            let result = with_nested_property_depth(nested_property_depth_failure, || {
                recovery_fn(
                    reader_ctx.bytes,
                    found_pos,
                    huffman,
                    nested_items.len(),
                    axiom.save_is_alpha,
                )
            });

            if let Ok((child, end_pos)) = result {
                nested_items.push(child);

                let absolute_end = found_pos + end_pos;
                if absolute_end > absolute_entry_pos {
                    let consumed = (absolute_end - absolute_entry_pos) as u32;
                    recorder.skip_and_record(consumed)?;
                }
                handled = true;
            }
        }
        Ok(handled)
    }
}
