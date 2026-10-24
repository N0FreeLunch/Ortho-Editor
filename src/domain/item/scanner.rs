//! Compatibility exports for forensic item marker scanning.

pub use crate::domain::forensic::marker_scan::{
    ItemMarker, MarkerPreselectionEnvelope, MarkerPreselectionError, MarkerStatus,
    ScannerScanReceipt, ScannerWorkCounters, preselect_marker_local_envelope, scan_item_markers,
    scan_item_markers_with_receipt, scan_item_markers_with_receipt_deadline,
    scan_item_markers_with_receipt_deadline_profile,
};
