use bitstream_io::BitRead;
use crate::data::bit_cursor::BitCursor;
use crate::domain::stats::axiom::StatsAxiom;
use crate::item::ParsingResult;
use crate::domain::forensic::v105::axioms::V105PropertyNudgeAxiom;
use crate::domain::item::axiom_meta::{ForensicAudit, ForensicAxiom};

pub struct NudgeCombinator;

impl NudgeCombinator {
    pub fn apply_property_residue_nudge<R: BitRead>(
        &self,
        cursor: &mut BitCursor<R>,
        version: u8,
        rhythm_recovery: bool,
        is_runeword: bool,
        audit: &mut ForensicAudit,
    ) -> ParsingResult<()> {
        let p_nudge = V105PropertyNudgeAxiom::default().get_nudge(version) as usize;
        if p_nudge > 0 && !rhythm_recovery && !is_runeword {
            cursor.push_context("AlphaPropertyResidueNudge");
            let _ = cursor.read_bits::<u32>(p_nudge as u32)?;
            audit.record(V105PropertyNudgeAxiom::default().metadata());
            cursor.pop_context();
        }
        Ok(())
    }

    pub fn apply_alignment_padding<R: BitRead>(
        &self,
        cursor: &mut BitCursor<R>,
        start_bit: u64,
        code: &str,
        flags: u32,
        axiom: &StatsAxiom,
    ) -> ParsingResult<Vec<bool>> {
        let consumed_bits = cursor.pos() - start_bit;
        let final_consumed = axiom.calculate_alignment(consumed_bits, code, flags);
        
        if final_consumed > consumed_bits {
            let padding_count = (final_consumed - consumed_bits) as u32;
            let padding = cursor.with_context("AlphaAlignmentPadding", |c| {
                let mut bits = Vec::new();
                for _ in 0..padding_count { 
                    match c.read_bit() {
                        Ok(bit) => bits.push(bit),
                        Err(_) => break, 
                    }
                }
                Ok(bits)
            })?;
            return Ok(padding);
        }
        Ok(Vec::new())
    }
}
