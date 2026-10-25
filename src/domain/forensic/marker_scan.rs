use crate::domain::forensic::header_types::AlphaScannerGapProfile;
use crate::domain::item::entity::Item;
use crate::domain::payload::HuffmanTree;
use crate::error::ParsingFailure;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::atomic::Ordering;

mod engine;
use engine::{ScannerWorkAccumulator, scan_item_markers_internal};

impl engine::JumpPeekItem for Item {
    fn recorded_bit_len(&self) -> u64 {
        self.bits.len() as u64
    }
}

fn scan_context(gap_profile: AlphaScannerGapProfile) -> engine::ScanContext<Item, ParsingFailure> {
    engine::ScanContext {
        gap_profile,
        jump_peek: crate::domain::item::serialization::parse_item_at_with_limit,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkerStatus {
    Accepted,
    Rejected,
    Phantom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemMarker {
    pub offset: u64,
    pub confidence: u32,
    pub code: String,
    pub score: i32,
    pub status: MarkerStatus,
    pub selected_corrected_gap: Option<i8>,
    pub selected_trial_gap: Option<u64>,
    pub selected_trial_gap_is_rhythm: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerWorkCounters {
    pub probes: u64,
    pub base_header_peeks: u64,
    pub alternate_gap_peeks: u64,
    pub lookahead_checks: u64,
    pub base_header_peek_micros: u64,
    pub alternate_gap_peek_micros: u64,
    pub lookahead_check_micros: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerScanReceipt {
    pub markers: Vec<ItemMarker>,
    pub work: ScannerWorkCounters,
    pub timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkerPreselectionEnvelope {
    pub section_marker_byte_offset: u64,
    pub section_bit_offset: u64,
    pub declared_top_level_count: u16,
    pub accepted_marker_ordinal: usize,
    pub absolute_item_start_bit: u64,
    pub trimmed_code: String,
    pub marker_status: MarkerStatus,
    pub confidence: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkerPreselectionError {
    AcceptedMarkerOrdinalOutOfRange {
        requested: usize,
        accepted_count: usize,
    },
}

impl fmt::Display for MarkerPreselectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AcceptedMarkerOrdinalOutOfRange {
                requested,
                accepted_count,
            } => write!(
                f,
                "accepted marker ordinal {requested} is out of range; accepted marker count is {accepted_count}"
            ),
        }
    }
}

impl std::error::Error for MarkerPreselectionError {}

pub fn preselect_marker_local_envelope(
    section_bytes: &[u8],
    huffman: &HuffmanTree,
    alpha: bool,
    section_marker_byte_offset: u64,
    section_bit_offset: u64,
    declared_top_level_count: u16,
    accepted_marker_ordinal: usize,
) -> Result<MarkerPreselectionEnvelope, MarkerPreselectionError> {
    let accepted_markers = scan_item_markers(
        section_bytes,
        huffman,
        alpha,
        section_bit_offset,
        Some(declared_top_level_count),
        false,
    );

    let marker = accepted_markers.get(accepted_marker_ordinal).ok_or(
        MarkerPreselectionError::AcceptedMarkerOrdinalOutOfRange {
            requested: accepted_marker_ordinal,
            accepted_count: accepted_markers.len(),
        },
    )?;

    Ok(MarkerPreselectionEnvelope {
        section_marker_byte_offset,
        section_bit_offset,
        declared_top_level_count,
        accepted_marker_ordinal,
        absolute_item_start_bit: section_bit_offset + marker.offset,
        trimmed_code: marker.code.trim().to_string(),
        marker_status: marker.status,
        confidence: marker.confidence,
    })
}

pub fn scan_item_markers(
    bytes: &[u8],
    huffman: &HuffmanTree,
    alpha: bool,
    section_bit_offset: u64,
    expected_count: Option<u16>,
    verbose: bool,
) -> Vec<ItemMarker> {
    scan_item_markers_internal(
        bytes,
        huffman,
        alpha,
        section_bit_offset,
        expected_count,
        verbose,
        None,
        None,
        None,
        scan_context(AlphaScannerGapProfile::Extended),
    )
}

pub fn scan_item_markers_with_receipt(
    bytes: &[u8],
    huffman: &HuffmanTree,
    alpha: bool,
    section_bit_offset: u64,
    expected_count: Option<u16>,
    verbose: bool,
) -> ScannerScanReceipt {
    scan_item_markers_with_receipt_deadline(
        bytes,
        huffman,
        alpha,
        section_bit_offset,
        expected_count,
        verbose,
        None,
    )
}

pub fn scan_item_markers_with_receipt_deadline_profile(
    bytes: &[u8],
    huffman: &HuffmanTree,
    alpha: bool,
    section_bit_offset: u64,
    expected_count: Option<u16>,
    verbose: bool,
    deadline: Option<std::time::Instant>,
    gap_profile: AlphaScannerGapProfile,
) -> ScannerScanReceipt {
    let work = ScannerWorkAccumulator::default();
    let timed_out = std::sync::atomic::AtomicBool::new(false);
    let markers = scan_item_markers_internal(
        bytes,
        huffman,
        alpha,
        section_bit_offset,
        expected_count,
        verbose,
        Some(&work),
        deadline,
        Some(&timed_out),
        scan_context(gap_profile),
    );
    ScannerScanReceipt {
        markers,
        work: work.snapshot(),
        timed_out: timed_out.load(Ordering::Relaxed),
    }
}

pub fn scan_item_markers_with_receipt_deadline(
    bytes: &[u8],
    huffman: &HuffmanTree,
    alpha: bool,
    section_bit_offset: u64,
    expected_count: Option<u16>,
    verbose: bool,
    deadline: Option<std::time::Instant>,
) -> ScannerScanReceipt {
    scan_item_markers_with_receipt_deadline_profile(
        bytes,
        huffman,
        alpha,
        section_bit_offset,
        expected_count,
        verbose,
        deadline,
        AlphaScannerGapProfile::Extended,
    )
}

#[cfg(test)]
mod tests {
    use super::{preselect_marker_local_envelope, MarkerPreselectionError, MarkerStatus};
    use crate::domain::payload::HuffmanTree;
    use crate::save::find_jm_markers;

    #[test]
    fn preselect_marker_local_envelope_from_alpha_fixture() {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/savegames/original/amazon_v105_slice2_equipment.d2s"
        );
        let bytes = std::fs::read(fixture).expect("Alpha v105 fixture must be readable");
        let jm_positions = find_jm_markers(&bytes);
        let section_marker_byte_offset = jm_positions[0] as u64;
        let next_section_marker_byte_offset = jm_positions.get(1).copied().unwrap_or(bytes.len());
        let section_bytes = &bytes[jm_positions[0]..next_section_marker_byte_offset];
        let section_bit_offset = section_marker_byte_offset * 8;
        let declared_top_level_count =
            u16::from_le_bytes([bytes[jm_positions[0] + 2], bytes[jm_positions[0] + 3]]);
        let huffman = HuffmanTree::new();

        let envelope = preselect_marker_local_envelope(
            section_bytes,
            &huffman,
            true,
            section_marker_byte_offset,
            section_bit_offset,
            declared_top_level_count,
            0,
        )
        .expect("the first accepted marker must produce an envelope");

        assert_eq!(envelope.section_marker_byte_offset, 916);
        assert_eq!(envelope.section_bit_offset, 7328);
        assert_eq!(envelope.declared_top_level_count, 16);
        assert_eq!(envelope.accepted_marker_ordinal, 0);
        assert_eq!(envelope.marker_status, MarkerStatus::Accepted);
        assert!(!envelope.trimmed_code.is_empty());
        assert!(envelope.confidence > 0);
        assert!(envelope.absolute_item_start_bit >= envelope.section_bit_offset);

        let out_of_range = preselect_marker_local_envelope(
            section_bytes,
            &huffman,
            true,
            section_marker_byte_offset,
            section_bit_offset,
            declared_top_level_count,
            20,
        );
        assert_eq!(
            out_of_range,
            Err(MarkerPreselectionError::AcceptedMarkerOrdinalOutOfRange {
                requested: 20,
                accepted_count: 20,
            })
        );
    }
}
