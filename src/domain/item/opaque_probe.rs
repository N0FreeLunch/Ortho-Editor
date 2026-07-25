use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::domain::item::entity::{Item, ItemModule};
use crate::domain::item::scanner::{ItemMarker, MarkerStatus};
use crate::domain::item::serialization::{parse_item_at_with_limit, HuffmanTree};

pub const BOUNDARY_CANDIDATE_RADIUS_BITS: u64 = 128;

pub fn marker_status_label(status: MarkerStatus) -> &'static str {
    match status {
        MarkerStatus::Accepted => "accepted",
        MarkerStatus::Rejected => "rejected",
        MarkerStatus::Phantom => "phantom",
    }
}

pub fn collect_boundary_candidates(
    markers: &[ItemMarker],
    section_bit_offset: u64,
    retained_end: u64,
) -> Vec<BoundaryCandidate> {
    let window_start = retained_end.saturating_sub(BOUNDARY_CANDIDATE_RADIUS_BITS);
    let window_end = retained_end.saturating_add(BOUNDARY_CANDIDATE_RADIUS_BITS);
    let mut candidates = markers
        .iter()
        .filter_map(|marker| {
            let absolute_bit_position = section_bit_offset.checked_add(marker.offset)?;
            if absolute_bit_position < window_start || absolute_bit_position > window_end {
                return None;
            }
            Some(BoundaryCandidate {
                absolute_bit_position,
                code: marker.code.trim().to_string(),
                status: marker_status_label(marker.status).to_string(),
                distance_from_retained_end: absolute_bit_position as i64 - retained_end as i64,
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| {
        a.distance_from_retained_end
            .unsigned_abs()
            .cmp(&b.distance_from_retained_end.unsigned_abs())
            .then_with(|| a.absolute_bit_position.cmp(&b.absolute_bit_position))
            .then_with(|| a.code.cmp(&b.code))
            .then_with(|| a.status.cmp(&b.status))
    });
    candidates
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct BoundaryCandidate {
    pub absolute_bit_position: u64,
    pub code: String,
    pub status: String,
    pub distance_from_retained_end: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExtensionSweepRecord {
    pub extension_bits: u64,
    pub candidate_limit_bits: u64,
    pub ownership_crossing: bool,
    pub retained_interval_crossing: bool,
    pub outcome: String,
    pub module_kind: Option<String>,
    pub consumed_bits: Option<u64>,
    pub failure_error: Option<String>,
    pub failure_bit_offset_abs: Option<u64>,
    pub failure_bit_offset_rel: Option<u64>,
    pub failure_context_stack: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ExtensionSweepSummary {
    pub row_count: usize,
    pub base_outcome: Option<String>,
    pub positive_extension_count: usize,
    pub positive_ownership_crossing_count: usize,
    pub failure_offset_tracks_candidate_limit: bool,
    pub first_non_failure_extension_bits: Option<u64>,
    pub first_structured_extension_bits: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_structured_extension_crosses_retained_interval: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParserProbe {
    pub probe_source: String,
    pub candidate_start_bit: u64,
    pub candidate_limit_bits: u64,
    pub code_hint: Option<String>,
    pub forced_compact: Option<bool>,
    pub retained_outcome: String,
    pub retained_module_kind: String,
    pub outcome: String,
    pub module_kind: Option<String>,
    pub failure_error: Option<String>,
    pub failure_context_stack: Option<Vec<String>>,
    pub failure_bit_offset_abs: Option<u64>,
    pub failure_bit_offset_rel: Option<u64>,
    pub failure_context_relative_offset: Option<u64>,
    pub failure_hint: Option<String>,
}

#[derive(Deserialize, Debug)]
struct ParserFailureEnvelope {
    error: String,
    context_stack: Vec<String>,
    bit_offset: u64,
    context_relative_offset: u64,
    hint: Option<String>,
}

pub struct OpaqueProbeRequest<'a> {
    pub item: &'a Item,
    pub bytes: &'a [u8],
    pub huffman: &'a HuffmanTree,
    pub is_alpha: bool,
    pub item_index: usize,
    pub boundary_candidates: &'a [BoundaryCandidate],
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OpaqueProbeResult {
    pub parser_probe: ParserProbe,
    pub extension_sweep_summary: ExtensionSweepSummary,
    pub extension_sweep: Vec<ExtensionSweepRecord>,
}

pub fn probe_opaque_item(req: OpaqueProbeRequest<'_>) -> Result<OpaqueProbeResult> {
    let parser_probe = probe_parser(
        req.item,
        req.bytes,
        req.huffman,
        req.is_alpha,
        req.item_index,
    )?;
    let extension_sweep = build_extension_sweep(
        req.item,
        req.bytes,
        req.huffman,
        req.is_alpha,
        req.item_index,
        req.boundary_candidates,
    );
    let extension_sweep_summary = summarize_extension_sweep(&extension_sweep);

    Ok(OpaqueProbeResult {
        parser_probe,
        extension_sweep_summary,
        extension_sweep,
    })
}

pub fn build_extension_sweep(
    item: &Item,
    bytes: &[u8],
    huffman: &HuffmanTree,
    is_alpha: bool,
    item_index: usize,
    boundary_candidates: &[BoundaryCandidate],
) -> Vec<ExtensionSweepRecord> {
    let candidate_start_bit = item.range.start;
    let retained_limit_bits = item.total_bits as u64;
    let code_hint = match item.code.trim() {
        "" | "Opaque" => None,
        code => Some(code),
    };
    let accepted_at_retained_end = boundary_candidates.iter().any(|candidate| {
        candidate.status == "accepted" && candidate.distance_from_retained_end == 0
    });

    (0..=16)
        .map(|extension_bits| {
            let candidate_limit_bits = retained_limit_bits + extension_bits;
            let candidate_bits = bits_from_range(
                bytes,
                candidate_start_bit,
                candidate_start_bit.saturating_add(candidate_limit_bits),
            );
            let candidate_bytes = bits_to_bytes(&candidate_bits);
            let live_result = parse_item_at_with_limit(
                &candidate_bytes,
                0,
                candidate_start_bit,
                huffman,
                item_index,
                is_alpha,
                Some(candidate_limit_bits),
                None,
                code_hint,
            );
            match live_result {
                Ok((reparsed, consumed_bits)) => {
                    let module_kind = reparsed.modules.iter().find_map(|module| match module {
                        ItemModule::SemiOpaque { .. } => Some("SemiOpaque"),
                        ItemModule::Opaque { .. } => Some("Opaque"),
                        ItemModule::Residue { .. } => Some("Residue"),
                        _ => None,
                    });
                    let outcome = match module_kind {
                        Some("SemiOpaque") => "semi_opaque",
                        Some(_) => "opaque",
                        None => "parsed",
                    };
                    ExtensionSweepRecord {
                        extension_bits,
                        candidate_limit_bits,
                        ownership_crossing: extension_bits > 0 && accepted_at_retained_end,
                        retained_interval_crossing: extension_bits > 0,
                        outcome: outcome.to_string(),
                        module_kind: Some(module_kind.unwrap_or("Structured").to_string()),
                        consumed_bits: Some(consumed_bits),
                        failure_error: None,
                        failure_bit_offset_abs: None,
                        failure_bit_offset_rel: None,
                        failure_context_stack: None,
                    }
                }
                Err(failure) => ExtensionSweepRecord {
                    extension_bits,
                    candidate_limit_bits,
                    ownership_crossing: extension_bits > 0 && accepted_at_retained_end,
                    retained_interval_crossing: extension_bits > 0,
                    outcome: "parse_failure".to_string(),
                    module_kind: None,
                    consumed_bits: None,
                    failure_error: Some(failure.error.to_string()),
                    failure_bit_offset_abs: Some(failure.bit_offset),
                    failure_bit_offset_rel: failure.bit_offset.checked_sub(candidate_start_bit),
                    failure_context_stack: Some(failure.context_stack),
                },
            }
        })
        .collect()
}

pub fn summarize_extension_sweep(rows: &[ExtensionSweepRecord]) -> ExtensionSweepSummary {
    let failures = rows
        .iter()
        .filter(|record| record.outcome == "parse_failure")
        .collect::<Vec<_>>();
    ExtensionSweepSummary {
        row_count: rows.len(),
        base_outcome: rows
            .iter()
            .find(|record| record.extension_bits == 0)
            .map(|record| record.outcome.clone()),
        positive_extension_count: rows
            .iter()
            .filter(|record| record.extension_bits > 0)
            .count(),
        positive_ownership_crossing_count: rows
            .iter()
            .filter(|record| record.extension_bits > 0 && record.ownership_crossing)
            .count(),
        failure_offset_tracks_candidate_limit: !failures.is_empty()
            && failures
                .iter()
                .all(|record| record.failure_bit_offset_rel == Some(record.candidate_limit_bits)),
        first_non_failure_extension_bits: rows
            .iter()
            .find(|record| record.outcome != "parse_failure")
            .map(|record| record.extension_bits),
        first_structured_extension_bits: rows
            .iter()
            .find(|record| record.outcome == "parsed")
            .map(|record| record.extension_bits),
        first_structured_extension_crosses_retained_interval: rows
            .iter()
            .find(|record| record.outcome == "parsed")
            .map(|record| record.retained_interval_crossing),
    }
}

pub fn bits_from_range(bytes: &[u8], start_bit: u64, end_bit: u64) -> Vec<bool> {
    let mut out = Vec::with_capacity(end_bit.saturating_sub(start_bit) as usize);
    for bit_pos in start_bit..end_bit {
        let byte_idx = (bit_pos / 8) as usize;
        if byte_idx >= bytes.len() {
            break;
        }
        let bit_idx = (bit_pos % 8) as u8;
        out.push(((bytes[byte_idx] >> bit_idx) & 1) == 1);
    }
    out
}

pub fn bits_to_bytes(bits: &[bool]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((bits.len() + 7) / 8);
    for chunk in bits.chunks(8) {
        let mut byte = 0u8;
        for (i, &bit) in chunk.iter().enumerate() {
            if bit {
                byte |= 1 << i;
            }
        }
        bytes.push(byte);
    }
    bytes
}

fn parser_failure_envelope(item: &Item) -> Result<Option<ParserFailureEnvelope>> {
    const PREFIX: &str = "parser_failure_json:";
    let findings = item
        .forensic_audit
        .findings
        .iter()
        .filter_map(|finding| finding.rationale.strip_prefix(PREFIX))
        .collect::<Vec<_>>();

    if findings.len() > 1 {
        anyhow::bail!("multiple conflicting parser_failure_json findings");
    }

    findings
        .first()
        .map(|json| serde_json::from_str(json).context("malformed parser_failure_json finding"))
        .transpose()
}

fn apply_preserved_failure(item: &Item, probe: &mut ParserProbe) -> Result<()> {
    if probe.failure_error.is_some() {
        return Ok(());
    }

    if let Some(failure) = parser_failure_envelope(item)? {
        probe.failure_bit_offset_rel = failure.bit_offset.checked_sub(probe.candidate_start_bit);
        probe.failure_error = Some(failure.error);
        probe.failure_context_stack = Some(failure.context_stack);
        probe.failure_bit_offset_abs = Some(failure.bit_offset);
        probe.failure_context_relative_offset = Some(failure.context_relative_offset);
        probe.failure_hint = failure.hint;
    }

    Ok(())
}

pub fn probe_parser(
    item: &Item,
    bytes: &[u8],
    huffman: &HuffmanTree,
    is_alpha: bool,
    item_index: usize,
) -> Result<ParserProbe> {
    let candidate_start_bit = item.range.start;
    let candidate_limit_bits = item.total_bits as u64;
    let code_hint = match item.code.trim() {
        "" | "Opaque" => None,
        code => Some(code.to_string()),
    };

    let (retained_outcome, retained_module_kind) = item
        .modules
        .iter()
        .find_map(|module| match module {
            ItemModule::SemiOpaque { .. } => Some(("semi_opaque", "SemiOpaque")),
            ItemModule::Opaque { .. } => Some(("opaque", "Opaque")),
            ItemModule::Residue { .. } => Some(("opaque", "Residue")),
            _ => None,
        })
        .unwrap_or(("parsed", "Structured"));

    let candidate_bits = bits_from_range(
        bytes,
        candidate_start_bit,
        candidate_start_bit.saturating_add(candidate_limit_bits),
    );
    let candidate_bytes = bits_to_bytes(&candidate_bits);
    let live_result = parse_item_at_with_limit(
        &candidate_bytes,
        0,
        candidate_start_bit,
        huffman,
        item_index,
        is_alpha,
        Some(candidate_limit_bits),
        None,
        code_hint.as_deref(),
    );
    let (
        outcome,
        module_kind,
        failure_error,
        failure_context_stack,
        failure_bit_offset_abs,
        failure_bit_offset_rel,
        failure_context_relative_offset,
        failure_hint,
    ) = match live_result {
        Ok((reparsed, _consumed_bits)) => {
            let module_kind = reparsed.modules.iter().find_map(|module| match module {
                ItemModule::SemiOpaque { .. } => Some("SemiOpaque"),
                ItemModule::Opaque { .. } => Some("Opaque"),
                ItemModule::Residue { .. } => Some("Residue"),
                _ => None,
            });
            let outcome = match module_kind {
                Some("SemiOpaque") => "semi_opaque",
                Some(_) => "opaque",
                None => "parsed",
            };
            (
                outcome.to_string(),
                Some(module_kind.unwrap_or("Structured").to_string()),
                None,
                None,
                None,
                None,
                None,
                None,
            )
        }
        Err(failure) => {
            let relative = failure.bit_offset.checked_sub(candidate_start_bit);
            (
                "parse_failure".to_string(),
                None,
                Some(failure.error.to_string()),
                Some(failure.context_stack),
                Some(failure.bit_offset),
                relative,
                Some(failure.context_relative_offset),
                failure.hint,
            )
        }
    };

    let mut probe = ParserProbe {
        probe_source: "bounded_live_reparse".to_string(),
        candidate_start_bit,
        candidate_limit_bits,
        code_hint,
        forced_compact: None,
        retained_outcome: retained_outcome.to_string(),
        retained_module_kind: retained_module_kind.to_string(),
        outcome,
        module_kind,
        failure_error,
        failure_context_stack,
        failure_bit_offset_abs,
        failure_bit_offset_rel,
        failure_context_relative_offset,
        failure_hint,
    };
    apply_preserved_failure(item, &mut probe)?;
    Ok(probe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::item::{Confidence, ForensicMetadata, Intentionality};

    #[test]
    fn bounded_live_reparse_reports_concrete_failure() {
        let mut item = Item::default();
        item.range.start = 0;
        item.total_bits = 1;
        item.code = "Opaque".to_string();
        item.modules.push(ItemModule::Opaque(Vec::new()));

        let probe = probe_parser(&item, &[0], &HuffmanTree::new(), true, 0)
            .expect("bounded live reparse should return a probe");
        assert_eq!(probe.probe_source, "bounded_live_reparse");
        assert_eq!(probe.retained_outcome, "opaque");
        assert_eq!(probe.retained_module_kind, "Opaque");
        assert_eq!(probe.outcome, "parse_failure");
        assert!(probe.failure_error.is_some());
        assert!(probe.failure_bit_offset_abs.is_some());
    }

    #[test]
    fn retained_failure_envelope_survives_live_opaque_reparse() {
        let mut item = Item::default();
        item.range.start = 7661;
        item.total_bits = 168;
        item.code = "Opaque".to_string();
        item.modules.push(ItemModule::Opaque(Vec::new()));
        item.forensic_audit.record(ForensicMetadata::new(
            Confidence::VerifiedTruth,
            Intentionality::Artifactual,
            r#"parser_failure_json:{"error":"Io(\"boundary\")","context_stack":["Root","ExtendedStats"],"bit_offset":7829,"context_relative_offset":80,"hint":"limit"}"#,
        ));
        let mut probe = ParserProbe {
            probe_source: "bounded_live_reparse".to_string(),
            candidate_start_bit: 7661,
            candidate_limit_bits: 168,
            code_hint: None,
            forced_compact: None,
            retained_outcome: "opaque".to_string(),
            retained_module_kind: "Opaque".to_string(),
            outcome: "opaque".to_string(),
            module_kind: Some("Opaque".to_string()),
            failure_error: None,
            failure_context_stack: None,
            failure_bit_offset_abs: None,
            failure_bit_offset_rel: None,
            failure_context_relative_offset: None,
            failure_hint: None,
        };

        apply_preserved_failure(&item, &mut probe).expect("valid preserved failure envelope");

        assert_eq!(probe.outcome, "opaque");
        assert_eq!(probe.module_kind.as_deref(), Some("Opaque"));
        assert_eq!(probe.failure_error.as_deref(), Some("Io(\"boundary\")"));
        assert_eq!(probe.failure_bit_offset_abs, Some(7829));
        assert_eq!(probe.failure_bit_offset_rel, Some(168));
        assert_eq!(probe.failure_context_relative_offset, Some(80));
        assert_eq!(
            probe.failure_context_stack.as_deref(),
            Some(["Root".to_string(), "ExtendedStats".to_string()].as_slice())
        );
        assert_eq!(probe.failure_hint.as_deref(), Some("limit"));
    }

    #[test]
    fn extension_sweep_is_ordered_bounded_and_marks_boundary_crossing() {
        let mut item = Item::default();
        item.range.start = 0;
        item.total_bits = 1;
        item.code = "Opaque".to_string();
        item.modules.push(ItemModule::Opaque(Vec::new()));
        let boundary_candidates = vec![BoundaryCandidate {
            absolute_bit_position: 1,
            code: "next".to_string(),
            status: "accepted".to_string(),
            distance_from_retained_end: 0,
        }];

        let sweep = build_extension_sweep(
            &item,
            &[0, 0, 0],
            &HuffmanTree::new(),
            true,
            0,
            &boundary_candidates,
        );
        assert_eq!(sweep.len(), 17);
        for (expected_extension, record) in (0u64..=16).zip(&sweep) {
            assert_eq!(record.extension_bits, expected_extension);
            assert_eq!(record.candidate_limit_bits, 1 + expected_extension);
            assert_eq!(record.ownership_crossing, expected_extension > 0);
            assert_eq!(record.retained_interval_crossing, expected_extension > 0);
        }
        assert_eq!(sweep[0].outcome, "parse_failure");
        assert_eq!(sweep[0].failure_bit_offset_rel, Some(1));
    }

    #[test]
    fn extension_sweep_summary_classifies_limit_tracking_boundary_crossing() {
        let rows = (0u64..=16)
            .map(|extension_bits| ExtensionSweepRecord {
                extension_bits,
                candidate_limit_bits: 149 + extension_bits,
                ownership_crossing: extension_bits > 0,
                retained_interval_crossing: extension_bits > 0,
                outcome: "parse_failure".to_string(),
                module_kind: None,
                consumed_bits: None,
                failure_error: Some("limit".to_string()),
                failure_bit_offset_abs: Some(8901 + extension_bits),
                failure_bit_offset_rel: Some(149 + extension_bits),
                failure_context_stack: Some(vec!["Root".to_string(), "ExtendedStats".to_string()]),
            })
            .collect::<Vec<_>>();

        let summary = summarize_extension_sweep(&rows);

        assert_eq!(summary.row_count, 17);
        assert_eq!(summary.base_outcome.as_deref(), Some("parse_failure"));
        assert_eq!(summary.positive_extension_count, 16);
        assert_eq!(summary.positive_ownership_crossing_count, 16);
        assert!(summary.failure_offset_tracks_candidate_limit);
        assert_eq!(summary.first_non_failure_extension_bits, None);
        assert_eq!(summary.first_structured_extension_bits, None);
        assert_eq!(summary.first_structured_extension_crosses_retained_interval, None);
    }

    #[test]
    fn boundary_candidate_reports_absolute_position_status_and_distance() {
        let markers = vec![
            ItemMarker {
                offset: 1573,
                confidence: 700,
                code: "wyws".to_string(),
                score: 700,
                status: MarkerStatus::Accepted,
            },
            ItemMarker {
                offset: 1565,
                confidence: 100,
                code: "near".to_string(),
                score: 100,
                status: MarkerStatus::Rejected,
            },
        ];

        let candidates = collect_boundary_candidates(&markers, 7328, 8901);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].absolute_bit_position, 8901);
        assert_eq!(candidates[0].code, "wyws");
        assert_eq!(candidates[0].status, "accepted");
        assert_eq!(candidates[0].distance_from_retained_end, 0);
        assert_eq!(candidates[1].absolute_bit_position, 8893);
        assert_eq!(candidates[1].status, "rejected");
        assert_eq!(candidates[1].distance_from_retained_end, -8);
    }
}
