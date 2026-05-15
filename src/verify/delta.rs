use serde::{Deserialize, Serialize};
use crate::verify::Report;

use crate::verify::save_integrity::D2SaveVerifyPayload;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]

#[serde(rename_all = "lowercase")]
pub enum DeltaStatus {
    Improved,
    Regressed,
    Unchanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FidelityDelta {
    pub prev_score: f32,
    pub curr_score: f32,
    pub delta: f32,
    pub status: DeltaStatus,
    pub new_issues: Vec<String>,
    pub fixed_issues: Vec<String>,
}

impl FidelityDelta {
    pub fn compare(curr: &Report<D2SaveVerifyPayload>, prev: &Report<D2SaveVerifyPayload>) -> Self {
        let curr_res = curr.scan_results.as_ref();
        let prev_res = prev.scan_results.as_ref();

        let curr_score = curr_res.map(|r| r.fidelity_score).unwrap_or(0.0);
        let prev_score = prev_res.map(|r| r.fidelity_score).unwrap_or(0.0);
        let delta = curr_score - prev_score;

        let mut status = DeltaStatus::Unchanged;
        if delta > 0.0001 {
            status = DeltaStatus::Improved;
        } else if delta < -0.0001 {
            status = DeltaStatus::Regressed;
        }

        // Issue DNA Matching: kind + message (we extract diff_len if possible)
        let curr_dnas: Vec<String> = curr.issues.iter().map(|i| extract_dna(i)).collect();
        let prev_dnas: Vec<String> = prev.issues.iter().map(|i| extract_dna(i)).collect();

        let mut new_issues = Vec::new();
        for dna in &curr_dnas {
            if !prev_dnas.contains(dna) {
                new_issues.push(dna.clone());
            }
        }

        let mut fixed_issues = Vec::new();
        for dna in &prev_dnas {
            if !curr_dnas.contains(dna) {
                fixed_issues.push(dna.clone());
            }
        }

        // If score is same but we have new issues, it might be a regression in quality 
        // even if the score (which might be coarse) didn't catch it.
        // But for Slice 3, we follow the score delta primarily as per spec.
        
        if status == DeltaStatus::Unchanged && !new_issues.is_empty() {
             // If we have new issues but score didn't drop, we still mark as regressed if they are "real" issues
             // But let's stay simple for now.
        }

        Self {
            prev_score,
            curr_score,
            delta,
            status,
            new_issues,
            fixed_issues,
        }
    }
}

fn extract_dna(issue: &crate::verify::ReportIssue) -> String {
    // issue-DNA matching (kind + domain + diff_len)
    // domain is often in the message or kind.
    // message: "Orig Len: 123, Emit Len: 456"
    let re_parity = regex::Regex::new(r"Orig Len: (\d+), Emit Len: (\d+)").unwrap();
    let mut diff_len = String::new();
    if let Some(caps) = re_parity.captures(&issue.message) {
        let orig: i64 = caps[1].parse().unwrap_or(0);
        let emit: i64 = caps[2].parse().unwrap_or(0);
        diff_len = format!(":diff={}", (orig - emit).abs());
    }
    
    format!("{}:{}{}", issue.kind, issue.message, diff_len)
}
