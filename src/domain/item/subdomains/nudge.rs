use crate::data::bit_cursor::BitCursor;
use crate::domain::forensic::legacy_layout::{self, LegacyItemLayoutContext};
use crate::domain::item::axiom_meta::ForensicAudit;
use crate::domain::stats::axiom::StatsAxiom;
use crate::item::ParsingResult;
use bitstream_io::BitRead;

/// Compatibility facade for legacy item nudge operations.
pub struct NudgeCombinator;

impl NudgeCombinator {
    pub fn apply_property_residue_nudge<R: BitRead>(
        &self,
        cursor: &mut BitCursor<R>,
        version: u8,
        rhythm_recovery: bool,
        is_compact: bool,
        is_runeword: bool,
        audit: &mut ForensicAudit,
    ) -> ParsingResult<()> {
        legacy_layout::apply_property_residue_nudge(
            cursor,
            version,
            rhythm_recovery,
            is_compact,
            is_runeword,
            audit,
        )
    }

    pub fn apply_alignment_padding<R: BitRead>(
        &self,
        cursor: &mut BitCursor<R>,
        start_bit: u64,
        code: &str,
        flags: u32,
        axiom: &StatsAxiom,
    ) -> ParsingResult<Vec<bool>> {
        legacy_layout::apply_alignment_padding(
            cursor,
            start_bit,
            code,
            flags,
            &LegacyItemLayoutContext::new(
                axiom.version,
                axiom.save_is_alpha,
                axiom.is_compact,
                axiom.idx,
            ),
        )
    }
}
