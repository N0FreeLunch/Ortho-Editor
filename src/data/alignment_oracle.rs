// d2r-core/src/data/alignment_oracle.rs

use std::collections::HashSet;

pub struct BitGenomicOracle {
    pub valid_stat_ids: HashSet<u16>, // d2r-data/ItemStatCost 기반 유효 ID 집합
}

impl BitGenomicOracle {
    pub fn new(valid_stat_ids: HashSet<u16>) -> Self {
        Self { valid_stat_ids }
    }

    /// 9비트 Stat ID 코돈(Codon) 스코어링 및 넛지 도출
    /// -3비트부터 +3비트까지 1비트 단위 슬라이딩 탐색
    pub fn codon_score_nudge(&self, stream: &[bool], start_offset: usize) -> isize {
        let mut best_nudge = 0;
        let mut max_score = 0;

        // Axiom 0337: -3..=3 nudge range contract
        for nudge in -3..=3 {
            let offset_res = (start_offset as isize) + nudge;
            if offset_res < 0 { continue; }
            let offset = offset_res as usize;
            
            if offset + 9 > stream.len() { continue; }
            
            let score = self.evaluate_codon_sequence(&stream[offset..offset + 9]);
            if score > max_score {
                max_score = score;
                best_nudge = nudge;
            }
        }
        best_nudge
    }

    /// 코돈 시퀀스의 정렬 점수를 평가합니다.
    /// Slice 1: Scaffold (Always returns 0 or minimal heuristic)
    fn evaluate_codon_sequence(&self, codon: &[bool]) -> usize {
        let mut val = 0u16;
        for (i, &b) in codon.iter().enumerate() {
            if b { val |= 1 << i; }
        }
        
        if self.valid_stat_ids.contains(&val) {
            100 // Exact match
        } else {
            0
        }
    }
}
