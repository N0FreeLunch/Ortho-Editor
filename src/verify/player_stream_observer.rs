//! Read-only observation of the player stream already selected by a producer.
//! Receipts describe symmetry, not semantic option completeness or baseline approval.

use crate::domain::item::stream::{ItemStreamDialect, PlayerItemStream};
use crate::verify::symmetry::{
    DiffReport, ItemDiff, SymmetryOptions, calculate_symmetry_diff_from_player_stream_ref,
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationOutcome {
    Pass,
    ProductFailure,
    VerificationFailure,
    ExecutionFailure,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerStreamConsumer {
    JsonDump,
    Dashboard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SymmetryProofClass {
    CacheFreeSourceReconstruction,
    HistoricalCachePolicy,
    None,
}

/// A diagnostic receipt has no authority to authorize reuse or replace a baseline.
/// Bind persisted receipts to input/source/data/executable hashes in a run manifest.
#[derive(Debug, Serialize)]
pub struct PlayerStreamObservation {
    pub schema_version: u32,
    pub consumer: PlayerStreamConsumer,
    pub outcome: ObservationOutcome,
    pub dialect: ItemStreamDialect,
    pub proof_class: SymmetryProofClass,
    pub parsed_count: usize,
    pub compared_count: usize,
    pub diagnostics: Vec<String>,
    pub findings: Vec<ItemDiff>,
    pub reason: Option<String>,
    pub hint: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct ProducerFailure {
    pub outcome: ObservationOutcome,
    pub stage: &'static str,
    pub reason: String,
}

impl ProducerFailure {
    pub fn execution(stage: &'static str, reason: impl ToString) -> Self {
        Self {
            outcome: ObservationOutcome::ExecutionFailure,
            stage,
            reason: reason.to_string(),
        }
    }

    pub fn product(stage: &'static str, reason: impl ToString) -> Self {
        Self {
            outcome: ObservationOutcome::ProductFailure,
            stage,
            reason: reason.to_string(),
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self.outcome {
            ObservationOutcome::ProductFailure => 2,
            _ => 3,
        }
    }
}

/// Never calls a parser. An empty successfully produced stream is not a parse error.
pub fn observe_player_stream(
    bytes: &[u8],
    stream: &PlayerItemStream,
    consumer: PlayerStreamConsumer,
) -> PlayerStreamObservation {
    let mut receipt = PlayerStreamObservation {
        schema_version: 1,
        consumer,
        outcome: ObservationOutcome::NotApplicable,
        dialect: stream.dialect,
        proof_class: SymmetryProofClass::None,
        parsed_count: stream.items.len(),
        compared_count: 0,
        diagnostics: stream.diagnostics.clone(),
        findings: Vec::new(),
        reason: None,
        hint: None,
    };
    if stream.items.is_empty() {
        receipt.reason = Some("No player items to reconstruct".into());
        return receipt;
    }
    receipt.proof_class = if stream.dialect == ItemStreamDialect::D2rV3 {
        SymmetryProofClass::CacheFreeSourceReconstruction
    } else {
        SymmetryProofClass::HistoricalCachePolicy
    };
    match calculate_symmetry_diff_from_player_stream_ref(
        bytes,
        stream,
        SymmetryOptions::roundtrip(true),
        None,
    ) {
        Ok(report) => complete_observation(receipt, report),
        Err(error) => {
            receipt.outcome = ObservationOutcome::VerificationFailure;
            receipt.reason = Some(error.to_string());
            receipt.hint = Some(
                "Inspect the observer/symmetry contract; do not classify this as a parser failure",
            );
            receipt
        }
    }
}

fn complete_observation(
    mut receipt: PlayerStreamObservation,
    report: DiffReport,
) -> PlayerStreamObservation {
    receipt.compared_count = report.items.len();
    let all_match = report.items.iter().all(|item| item.is_match);
    // An incomplete or self-contradictory verifier result can never publish pass.
    let contract_valid = report.operation == "consumer_roundtrip"
        && report.item_count_a == receipt.parsed_count
        && report.item_count_b == receipt.parsed_count
        && report.items.len() == receipt.parsed_count
        && report.success == all_match;
    receipt.findings = report
        .items
        .into_iter()
        .filter(|item| !item.is_match)
        .map(|mut item| {
            remove_raw_dumps(&mut item);
            item
        })
        .collect();
    if !contract_valid {
        receipt.outcome = ObservationOutcome::VerificationFailure;
        receipt.reason = Some("Incomplete or inconsistent symmetry report".into());
        receipt.hint = Some("Inspect the observer/symmetry completion contract");
    } else if all_match {
        receipt.outcome = ObservationOutcome::Pass;
    } else {
        receipt.outcome = ObservationOutcome::ProductFailure;
        receipt.reason = Some("Selected player items did not satisfy symmetry".into());
        receipt.hint = Some(
            "Inspect typed rebuild errors and source mismatch offsets; preserve the original input",
        );
    }
    receipt
}

fn remove_raw_dumps(item: &mut ItemDiff) {
    item.orig_bits = None;
    item.target_bits = None;
    for child in &mut item.children {
        remove_raw_dumps(child);
    }
}

/// Add diagnostics after the product producer has completed its own payload.
pub fn attach_player_stream_observation(
    payload: &mut serde_json::Value,
    bytes: &[u8],
    stream: &PlayerItemStream,
    consumer: PlayerStreamConsumer,
) {
    let receipt = observe_player_stream(bytes, stream, consumer);
    payload["player_item_observation"] =
        serde_json::to_value(receipt).expect("Observation receipts contain JSON-compatible values");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::HuffmanTree;
    use d2r_macros::settled_contract_test;

    fn fresh() -> (Vec<u8>, PlayerItemStream) {
        let bytes = include_bytes!(
            "../../tests/fixtures/savegames/gameplay/normal/act1/TESTASSASSIN_Act1_FreshStart.d2s"
        )
        .to_vec();
        let stream = PlayerItemStream::read(&bytes, &HuffmanTree::new()).unwrap();
        (bytes, stream)
    }

    #[settled_contract_test]
    fn borrowed_observation_preserves_product_and_uses_source_reconstruction() {
        let (bytes, stream) = fresh();
        let before = format!("{:?}", stream.items);
        let receipt = observe_player_stream(&bytes, &stream, PlayerStreamConsumer::JsonDump);
        assert_eq!(receipt.outcome, ObservationOutcome::Pass);
        assert_eq!(
            receipt.proof_class,
            SymmetryProofClass::CacheFreeSourceReconstruction
        );
        assert_eq!(receipt.compared_count, stream.items.len());
        assert!(receipt.findings.is_empty());
        assert_eq!(format!("{:?}", stream.items), before);
    }

    #[settled_contract_test]
    fn typed_rebuild_and_source_failures_remain_product_evidence() {
        let (mut bytes, mut stream) = fresh();
        bytes[stream.items[0].range.start as usize / 8] ^= 1;
        stream.items[1].body.x ^= 1;
        let receipt = observe_player_stream(&bytes, &stream, PlayerStreamConsumer::Dashboard);
        assert_eq!(receipt.outcome, ObservationOutcome::ProductFailure);
        assert!(
            receipt
                .findings
                .iter()
                .any(|item| item.rebuild_error.is_some())
        );
        assert!(
            receipt
                .findings
                .iter()
                .any(|item| item.first_mismatch_offset.is_some())
        );
        assert!(
            receipt
                .findings
                .iter()
                .all(|item| item.orig_bits.is_none() && item.target_bits.is_none())
        );
    }

    #[settled_contract_test]
    fn empty_stream_is_not_applicable_and_legacy_policy_is_qualified() {
        let mut bytes = crate::save::D2S_MAGIC.to_le_bytes().to_vec();
        bytes.extend_from_slice(&105_u32.to_le_bytes());
        bytes.extend_from_slice(b"JM\0\0");
        let stream = PlayerItemStream::read(&bytes, &HuffmanTree::new()).unwrap();
        let receipt = observe_player_stream(&bytes, &stream, PlayerStreamConsumer::JsonDump);
        assert_eq!(receipt.outcome, ObservationOutcome::NotApplicable);
        assert_eq!(receipt.proof_class, SymmetryProofClass::None);
        assert_eq!(receipt.compared_count, 0);

        let bytes =
            include_bytes!("../../tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
        let stream = PlayerItemStream::read(bytes, &HuffmanTree::new()).unwrap();
        assert_ne!(stream.dialect, ItemStreamDialect::D2rV3);
        let receipt = observe_player_stream(bytes, &stream, PlayerStreamConsumer::JsonDump);
        assert_eq!(
            receipt.proof_class,
            SymmetryProofClass::HistoricalCachePolicy
        );
    }

    #[settled_contract_test]
    fn incomplete_report_is_verifier_failure_and_io_is_execution_failure() {
        let (bytes, stream) = fresh();
        let receipt = observe_player_stream(&bytes, &stream, PlayerStreamConsumer::JsonDump);
        let receipt = complete_observation(
            receipt,
            DiffReport {
                success: true,
                ..Default::default()
            },
        );
        assert_eq!(receipt.outcome, ObservationOutcome::VerificationFailure);
        let failure = ProducerFailure::execution("read", "missing input");
        assert_eq!(failure.outcome, ObservationOutcome::ExecutionFailure);
        assert_eq!(failure.exit_code(), 3);
        assert_eq!(
            ProducerFailure::product("parse", "invalid magic").exit_code(),
            2
        );
    }
}
