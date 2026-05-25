#[cfg(test)]
mod tests {
    use crate::data::alignment_oracle::BitGenomicOracle;
    use std::collections::HashSet;

    #[test]
    fn test_codon_nudge_scaffold() {
        let mut valid_ids = HashSet::new();
        valid_ids.insert(31); // item_defense_percent
        
        let oracle = BitGenomicOracle::new(valid_ids);
        
        // 9-bit codon for 31 (000011111) -> [true, true, true, true, true, false, false, false, false]
        let mut stream = vec![false; 30];
        // Insert 31 at offset 5
        let codon_31 = vec![true, true, true, true, true, false, false, false, false];
        for (i, &b) in codon_31.iter().enumerate() {
            stream[5 + i] = b;
        }
        
        // Nudge check at offset 7 (should be -2)
        let nudge = oracle.codon_score_nudge(&stream, 7);
        assert_eq!(nudge, -2);
    }

    #[test]
    fn test_codon_context_scoring() {
        let mut valid_ids = HashSet::new();
        valid_ids.insert(31); // item_defense_percent
        valid_ids.insert(32); // another stat
        
        let oracle = BitGenomicOracle::new(valid_ids);
        
        // Structure: [31] (9 bits) + [Value] (9 bits) + [32] (9 bits)
        // Offset 0: 31
        // Offset 9: Value (all zeros)
        // Offset 18: 32
        let mut stream = vec![false; 40];
        
        let codon_31 = vec![true, true, true, true, true, false, false, false, false];
        let codon_32 = vec![false, false, false, false, false, true, false, false, false];
        
        for (i, &b) in codon_31.iter().enumerate() { stream[0 + i] = b; }
        for (i, &b) in codon_32.iter().enumerate() { stream[18 + i] = b; }
        
        // Test context score at offset 0
        let nudge = oracle.codon_score_nudge(&stream, 1);
        assert_eq!(nudge, -1);
        
        // Shift bits to create drift
        let mut drifted = vec![false; 40];
        for i in 0..39 { drifted[i+1] = stream[i]; } // 1-bit right drift
        
        let nudge_drifted = oracle.codon_score_nudge(&drifted, 0);
        assert_eq!(nudge_drifted, 1);
    }
}
