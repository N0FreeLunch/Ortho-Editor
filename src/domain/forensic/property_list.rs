//! Legacy list probing and child recovery, injected into the common list engine.
mod socket_recovery;
use crate::data::bit_cursor::BitCursor;
use crate::data::generated::layout_policy::ALPHA_LEGACY_AUTHORITY_CODES;
use crate::domain::forensic::property_entry::{LegacyEntryPolicy, LegacyEntryRecovery};
use crate::domain::item::{HuffmanTree, Item};
use crate::domain::stats::StatsAxiom;
use crate::domain::stats::parser::PropertyReaderContext;
use crate::domain::stats::property_entry::read_property_entry;
use crate::domain::stats::property_list::{ParsedPropertyList, PropertyListReader};
use crate::error::ParsingResult;
use bitstream_io::BitRead;
use socket_recovery::SocketRecovery;
use std::collections::HashMap;

pub(crate) struct LegacyListReader<'a, F> {
    code: &'a str,
    version: u8,
    reader_ctx: PropertyReaderContext<'a>,
    huffman: &'a HuffmanTree,
    alpha_runeword: bool,
    is_v105_shadow: bool,
    is_compact: bool,
    actual_runeword: bool,
    authority_host: bool,
    consume_pairs: bool,
    axiom: &'a StatsAxiom,
    section_base_abs: u64,
    child_marker_offsets: Option<&'a [u64]>,
    sockets: SocketRecovery<'a>,
    recovery_fn: F,
}
impl<'a, F> LegacyListReader<'a, F> {
    pub(crate) fn new(
        code: &'a str,
        version: u8,
        reader_ctx: PropertyReaderContext<'a>,
        huffman: &'a HuffmanTree,
        alpha_runeword: bool,
        is_v105_shadow: bool,
        is_compact: bool,
        actual_runeword: bool,
        authority_host: bool,
        consume_pairs: bool,
        axiom: &'a StatsAxiom,
        section_base_abs: u64,
        child_marker_offsets: Option<&'a [u64]>,
        child_marker_codes: Option<&'a HashMap<u64, String>>,
        recovery_fn: F,
    ) -> Self {
        let sockets = SocketRecovery::new(
            reader_ctx.bytes,
            huffman,
            section_base_abs,
            child_marker_offsets,
            child_marker_codes,
        );
        Self {
            code,
            version,
            reader_ctx,
            huffman,
            alpha_runeword,
            is_v105_shadow,
            is_compact,
            actual_runeword,
            authority_host,
            consume_pairs,
            axiom,
            section_base_abs,
            child_marker_offsets,
            sockets,
            recovery_fn,
        }
    }
}
impl<R: BitRead, F> PropertyListReader<R, Item> for LegacyListReader<'_, F>
where
    F: FnMut(&[u8], u64, &HuffmanTree, usize, bool) -> ParsingResult<(Item, u64)>,
{
    fn stop_before_entry(&self, recorder: &BitCursor<R>) -> bool {
        if self.axiom.is_alpha() && self.actual_runeword {
            let current_rel_pos = recorder.pos().saturating_sub(self.section_base_abs);
            self.sockets
                .should_stop_before_child(current_rel_pos, self.authority_host)
        } else {
            false
        }
    }
    fn read_entry(
        &mut self,
        recorder: &mut BitCursor<R>,
    ) -> ParsingResult<Option<ParsedPropertyList<Item>>> {
        let policy = LegacyEntryPolicy::new(
            self.version,
            self.alpha_runeword,
            self.is_compact,
            self.is_v105_shadow,
            self.consume_pairs,
            self.axiom,
        );
        let mut recovery = LegacyEntryRecovery::new(
            self.alpha_runeword,
            self.axiom,
            self.reader_ctx.clone(),
            self.huffman,
            &mut self.recovery_fn,
        );
        read_property_entry(recorder, &policy, &mut recovery)
    }
    fn recover_children(
        &mut self,
        recorder: &mut BitCursor<R>,
        children: &mut Vec<Item>,
    ) -> ParsingResult<()> {
        let is_authority_host =
            self.axiom.is_alpha() && ALPHA_LEGACY_AUTHORITY_CODES.contains(&self.code.trim());
        let needs_socket_recovery = self.axiom.is_alpha()
            && (self.alpha_runeword
                || self.code.trim() == "ucb8"
                || self.code.trim() == "bwcw"
                || is_authority_host)
            && self.axiom.is_socketed
            && self
                .child_marker_offsets
                .map(|offsets| children.len() < offsets.len())
                .unwrap_or(false);
        if needs_socket_recovery {
            self.sockets.recover_children(
                recorder,
                self.code,
                self.axiom.is_alpha(),
                children,
                &mut self.recovery_fn,
            )?;
        }
        Ok(())
    }
}
