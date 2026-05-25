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
        let mut stream = vec![false; 20];
        // Insert 31 at offset 5
        let codon_31 = vec![true, true, true, true, true, false, false, false, false];
        for (i, &b) in codon_31.iter().enumerate() {
            stream[5 + i] = b;
        }
        
        // Nudge check at offset 7 (should be -2)
        let nudge = oracle.codon_score_nudge(&stream, 7);
        assert_eq!(nudge, -2);
    }
}
