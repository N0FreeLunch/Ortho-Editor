// d2r-core/src/data/alignment_oracle.rs

use std::collections::HashSet;

pub struct BitGenomicOracle {
    pub valid_stat_ids: HashSet<u16>, // Valid ID set based on d2r-data/ItemStatCost
}

impl BitGenomicOracle {
    pub fn new(valid_stat_ids: HashSet<u16>) -> Self {
        Self { valid_stat_ids }
    }

    /// 9-bit Stat ID Codon scoring and nudge derivation
    /// 1-bit unit sliding search from -3 bits to +3 bits
    pub fn codon_score_nudge(&self, stream: &[bool], start_offset: usize) -> isize {
        let mut best_nudge = 0;
        let mut max_score = 0;

        // Axiom 0337: -3..=3 nudge range contract
        for nudge in -3..=3 {
            let offset_res = (start_offset as isize) + nudge;
            if offset_res < 0 { continue; }
            let offset = offset_res as usize;
            
            if offset + 9 > stream.len() { continue; }
            
            let score = self.evaluate_codon_context(stream, offset);
            if score > max_score {
                max_score = score;
                best_nudge = nudge;
            }
        }
        best_nudge
    }

    /// Multi-Codon context consistency evaluation (Axiom 0340)
    /// Scores the rhythm of the Codon + Value + Next Codon structure.
    fn evaluate_codon_context(&self, stream: &[bool], offset: usize) -> usize {
        if offset + 9 > stream.len() { return 0; }
        let current_id = self.bits_to_u16(&stream[offset..offset+9]);
        
        if !self.valid_stat_ids.contains(&current_id) {
            // Terminator (0x1FF) pre-recognition (Axiom 0340)
            if current_id == 0x1FF { return 10; }
            return 0;
        }
        
        let mut score = 50; // Base valid ID match
        
        // Lookahead value bit offset (Axiom 0105 mappings)
        let val_width = self.get_stat_value_width(current_id);
        let next_offset = offset + 9 + val_width;
        
        if next_offset + 9 <= stream.len() {
            let next_id = self.bits_to_u16(&stream[next_offset..next_offset+9]);
            // Next codon / terminator alignment bonus
            if self.valid_stat_ids.contains(&next_id) || next_id == 0x1FF {
                score += 50; 
            }
        } else if next_offset == stream.len() {
            // Exactly at the end of stream after value bits
            score += 20;
        }

        score
    }

    /// Evaluates the alignment score of the codon sequence. (Legacy Scaffold)
    fn evaluate_codon_sequence(&self, codon: &[bool]) -> usize {
        let val = self.bits_to_u16(codon);
        if self.valid_stat_ids.contains(&val) {
            100 // Exact match
        } else {
            0
        }
    }

    fn bits_to_u16(&self, bits: &[bool]) -> u16 {
        let mut val = 0u16;
        for (i, &b) in bits.iter().enumerate() {
            if b { val |= 1 << i; }
        }
        val
    }

    /// Alpha v105 Stat Value bit width (Axiom 0105/0340)
    fn get_stat_value_width(&self, _stat_id: u16) -> usize {
        // FIXME: Needs linkage with variable widths per version/stat.
        // Slice 2: Prioritizing the 9-bit fixed rhythm of Alpha v105 (v1/v2/v4/v6).
        9
    }
}
