use bitstream_io::{BitRead, BitReader, LittleEndian};
use rayon::prelude::*;
use serde::Serialize;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Instant;

use crate::domain::item::axiom_meta::{FidelityScore, ForensicAudit};
use crate::domain::item::stream::{ItemStreamDialect, PlayerItemStream};
use crate::item::{HuffmanTree, Item, RecordedBit, peek_item_header_at};
use crate::verify::dedup_pool::{ItemMemoizationPool, ItemSignature};

#[derive(Debug, Clone, Serialize, serde::Deserialize, Default)]
pub struct DiffReport {
    pub success: bool,
    pub operation: String,
    pub item_count_a: usize,
    pub item_count_b: usize,
    pub items: Vec<ItemDiff>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct RebuildFailure {
    pub kind: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, Default)]
pub struct ItemDiff {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rebuild_error: Option<RebuildFailure>,
    pub label: String,
    pub code: String,
    pub is_match: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mismatch_type: Option<String>,
    pub original_len: usize,
    pub target_len: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_mismatch_offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded_bit_offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded_bit_value: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded_bit_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded_bit_source_label_unavailable: Option<bool>,
    pub fidelity_score: f32,
    pub forensic_audit: ForensicAudit,
    pub version: u8,
    pub flags: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha_header_gap: Option<u32>,
    pub alpha_alignment_padding_len: usize,
    pub alpha_body_gap_len: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovered_alpha_header_gap: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed_alpha_header_gap: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orig_bits: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_bits: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_source_contract: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_bits_preserved: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison_bit_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reserialized_bit_source: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ItemDiff>,
}

#[derive(Debug, Clone, Default)]
pub struct SymmetryOptions {
    pub roundtrip: bool,
    pub target_index: Option<usize>,
    pub fail_fast: bool,
    pub pool: Option<Arc<ItemMemoizationPool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymmetryStageEvent {
    Started {
        stage: &'static str,
    },
    Completed {
        stage: &'static str,
        elapsed_ms: u64,
        item_count: Option<usize>,
        outcome: &'static str,
    },
}

impl SymmetryOptions {
    pub fn roundtrip(roundtrip: bool) -> Self {
        Self {
            roundtrip,
            ..Default::default()
        }
    }

    pub fn with_pool(mut self, pool: Option<Arc<ItemMemoizationPool>>) -> Self {
        self.pool = pool;
        self
    }
}

pub fn calculate_symmetry_diff(
    bytes_a: &[u8],
    bytes_b: Option<&[u8]>,
    options: SymmetryOptions,
) -> anyhow::Result<DiffReport> {
    calculate_symmetry_diff_inner(bytes_a, bytes_b, options, None, None)
}

pub fn calculate_symmetry_diff_with_stage_observer(
    bytes_a: &[u8],
    bytes_b: Option<&[u8]>,
    options: SymmetryOptions,
    observer: &mut dyn FnMut(SymmetryStageEvent),
) -> anyhow::Result<DiffReport> {
    calculate_symmetry_diff_inner(bytes_a, bytes_b, options, None, Some(observer))
}

pub fn calculate_symmetry_diff_from_parsed_items(
    bytes_a: &[u8],
    items: Vec<Item>,
    options: SymmetryOptions,
) -> anyhow::Result<DiffReport> {
    calculate_symmetry_diff_inner(bytes_a, None, options, Some(items), None)
}

pub fn calculate_symmetry_diff_from_parsed_items_with_stage_observer(
    bytes_a: &[u8],
    items: Vec<Item>,
    options: SymmetryOptions,
    observer: &mut dyn FnMut(SymmetryStageEvent),
) -> anyhow::Result<DiffReport> {
    calculate_symmetry_diff_inner(bytes_a, None, options, Some(items), Some(observer))
}

/// Audit an already selected consumer stream without inferring its dialect again.
/// V3 scores describe exact source reconstruction, not semantic option fidelity.
/// Legacy recovery retains its historical confidence score and cache policy.
pub fn calculate_symmetry_diff_from_player_stream(
    bytes: &[u8],
    stream: PlayerItemStream,
    options: SymmetryOptions,
    observer: Option<&mut dyn FnMut(SymmetryStageEvent)>,
) -> anyhow::Result<DiffReport> {
    calculate_symmetry_diff_from_player_stream_ref(bytes, &stream, options, observer)
}

/// Borrow the exact stream used by a consumer without cloning or parsing it again.
pub fn calculate_symmetry_diff_from_player_stream_ref(
    bytes: &[u8],
    stream: &PlayerItemStream,
    options: SymmetryOptions,
    mut observer: Option<&mut dyn FnMut(SymmetryStageEvent)>,
) -> anyhow::Result<DiffReport> {
    anyhow::ensure!(
        options.roundtrip,
        "Consumer stream audits require roundtrip mode"
    );
    anyhow::ensure!(!stream.items.is_empty(), "No player items to audit");
    if let Some(index) = options.target_index {
        anyhow::ensure!(
            index < stream.items.len(),
            "Target item index {index} is out of range"
        );
    }
    let started = Instant::now();
    if let Some(callback) = observer.as_mut() {
        callback(SymmetryStageEvent::Started {
            stage: "strict_rebuild_compare",
        });
    }
    let huffman = HuffmanTree::new();
    let mut report = DiffReport {
        operation: "consumer_roundtrip".to_string(),
        item_count_a: stream.items.len(),
        item_count_b: stream.items.len(),
        ..Default::default()
    };
    for (index, item) in stream.items.iter().enumerate() {
        if options.target_index.is_some_and(|target| target != index) {
            continue;
        }
        let diff = match stream.dialect {
            ItemStreamDialect::D2rV3 => {
                compare_v3_source_item(item, &huffman, bytes, format!("Item {index}"))
            }
            dialect => compare_item_with_reserialized(
                index,
                item,
                &huffman,
                dialect == ItemStreamDialect::Alpha,
                format!("Item {index}"),
                bytes,
                None,
            ),
        };
        let stop = options.fail_fast && !diff.is_match;
        report.items.push(diff);
        if stop {
            break;
        }
    }
    report.success = !report.items.is_empty() && report.items.iter().all(|item| item.is_match);
    if let Some(callback) = observer.as_mut() {
        callback(SymmetryStageEvent::Completed {
            stage: "strict_rebuild_compare",
            elapsed_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            item_count: Some(report.items.len()),
            outcome: if report.success { "ok" } else { "mismatch" },
        });
    }
    Ok(report)
}

fn clear_v3_replay_fields(item: &mut Item) {
    item.bits.clear();
    item.segments.clear();
    item.gap_bits.clear();
    item.body.alpha_code_bits.clear();
    item.range = Default::default();
    item.total_bits = 0;
    item.logical_width = None;
    for child in &mut item.socketed_items {
        clear_v3_replay_fields(child);
    }
}

fn compare_v3_source_item(
    item: &Item,
    huffman: &HuffmanTree,
    bytes: &[u8],
    label: String,
) -> ItemDiff {
    use crate::data::repository::legacy_item_template as item_template;
    let mut diff = ItemDiff {
        label,
        code: item.code.trim().to_string(),
        version: item.header.version,
        flags: item.header.flags,
        forensic_audit: item.forensic_audit.clone(),
        bit_source_contract: Some("v3_typed_rebuild_against_original_save_range".to_string()),
        cached_bits_preserved: Some(false),
        comparison_bit_source: Some("original_save_range".to_string()),
        reserialized_bit_source: Some("explicit_v3_codec".to_string()),
        ..Default::default()
    };
    let span_valid = item.range.start < item.range.end
        && item.range.end <= bytes.len() as u64 * 8
        && item.socketed_items.iter().all(|child| {
            child.range.start >= item.range.start && child.range.end <= item.range.end
        });
    let original: Vec<bool> = if span_valid {
        (item.range.start..item.range.end)
            .map(|offset| bytes[(offset / 8) as usize] & (1 << (offset % 8)) != 0)
            .collect()
    } else {
        Vec::new()
    };
    diff.original_len = original.len();
    diff.orig_bits = span_valid.then(|| {
        original
            .iter()
            .map(|&bit| if bit { '1' } else { '0' })
            .collect()
    });
    let rebuilt = if !span_valid {
        Err(RebuildFailure { kind: "invalid_source_span".to_string(), reason: "Item source range is empty, outside the save, or does not contain its socket children".to_string() })
    } else if item.is_opaque() || item.is_semi_opaque() || !item.modules.is_empty() {
        Err(RebuildFailure {
            kind: "unsupported_layout".to_string(),
            reason: "Opaque, residue or unexplained modules cannot establish a typed rebuild"
                .to_string(),
        })
    } else {
        let mut typed = item.clone();
        clear_v3_replay_fields(&mut typed);
        // Source-owned V3 alignment is a typed field, not the item replay cache.
        let template = item_template(typed.code.trim());
        let deferred_stored_host = typed.header.is_runeword
            || !typed.socketed_items.is_empty()
            || typed
                .body
                .d2r_v3_auxiliary
                .as_ref()
                .is_some_and(|auxiliary| auxiliary.filled_sockets != 0);
        let emitted = if typed.is_compact {
            typed.to_bits_d2r_v3_compact(huffman)
        } else if typed.header.is_socketed
            && template.is_some_and(|template| template.is_weapon || template.is_armor)
            && deferred_stored_host
        {
            typed.to_bits_d2r_v3_socket_host(huffman)
        } else if template.is_some_and(|template| template.is_weapon) {
            if deferred_stored_host {
                typed.to_bits_d2r_v3_weapon(huffman)
            } else {
                typed.to_bits_d2r_v3_stored_weapon(huffman)
            }
        } else if template.is_some_and(|template| template.is_armor) {
            if typed.header.is_runeword
                || !typed.socketed_items.is_empty()
                || typed
                    .body
                    .d2r_v3_auxiliary
                    .as_ref()
                    .is_some_and(|auxiliary| auxiliary.filled_sockets != 0)
            {
                if typed.header.is_socketed {
                    typed.to_bits_d2r_v3_empty_socket_armor(huffman)
                } else {
                    typed.to_bits_d2r_v3_armor(huffman)
                }
            } else {
                typed.to_bits_d2r_v3_stored_armor(huffman)
            }
        } else if matches!(typed.code.trim(), "cm1" | "cm2" | "cm3" | "jew") {
            typed.to_bits_d2r_v3_accessory(huffman)
        } else if matches!(
            typed.code.trim(),
            "key" | "tbk" | "ibk" | "box" | "vip" | "ice"
        ) {
            typed.to_bits_d2r_v3_stored_misc(huffman)
        } else if matches!(typed.code.trim(), "rin" | "amu") {
            if deferred_stored_host {
                typed.to_bits_d2r_v3_jewelry(huffman)
            } else {
                typed.to_bits_d2r_v3_stored_jewelry(huffman)
            }
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "No explicit V3 inverse for this item class",
            ))
        };
        emitted.map_err(|error| RebuildFailure {
            kind: if error.kind() == std::io::ErrorKind::Unsupported {
                "unsupported_layout"
            } else {
                "unsupported_or_inconsistent_state"
            }
            .to_string(),
            reason: error.to_string(),
        })
    };
    match rebuilt {
        Err(error) => {
            diff.mismatch_type = Some("Rebuild".to_string());
            diff.segment = Some("V3Layout".to_string());
            diff.rebuild_error = Some(error);
        }
        Ok(target) => {
            diff.target_len = target.len();
            diff.target_bits = Some(
                target
                    .iter()
                    .map(|&bit| if bit { '1' } else { '0' })
                    .collect(),
            );
            let mismatch = original.iter().zip(&target).position(|(a, b)| a != b);
            diff.is_match = mismatch.is_none() && original.len() == target.len();
            if !diff.is_match {
                diff.mismatch_type = Some(
                    if original.len() == target.len() {
                        "Content"
                    } else {
                        "Length"
                    }
                    .to_string(),
                );
                diff.segment = Some("V3Source".to_string());
                if let Some(offset) = mismatch {
                    diff.first_mismatch_offset = Some(offset as u64);
                    diff.recorded_bit_offset = Some(item.range.start + offset as u64);
                    diff.recorded_bit_value = Some(original[offset]);
                    diff.recorded_bit_source = Some("original_save_range".to_string());
                }
            }
        }
    }
    for (index, child) in item.socketed_items.iter().enumerate() {
        diff.children.push(compare_v3_source_item(
            child,
            huffman,
            bytes,
            format!("Child {index}"),
        ));
    }
    if diff.children.iter().any(|child| !child.is_match) {
        diff.is_match = false;
        if diff.mismatch_type.is_none() {
            diff.mismatch_type = Some("Child".to_string());
        }
    }
    diff.fidelity_score = if diff.is_match { 1.0 } else { 0.0 };
    diff
}

fn calculate_symmetry_diff_inner(
    bytes_a: &[u8],
    bytes_b: Option<&[u8]>,
    options: SymmetryOptions,
    preparsed_items: Option<Vec<Item>>,
    mut observer: Option<&mut dyn FnMut(SymmetryStageEvent)>,
) -> anyhow::Result<DiffReport> {
    crate::init_rayon_thread_pool();
    let huffman = HuffmanTree::new();
    let is_alpha_a = is_alpha(bytes_a);
    let mut report = DiffReport {
        operation: if options.roundtrip {
            "roundtrip"
        } else {
            "compare"
        }
        .to_string(),
        ..Default::default()
    };

    if options.roundtrip {
        let items = if let Some(items) = preparsed_items {
            items
        } else {
            let parse_started = Instant::now();
            if let Some(callback) = observer.as_mut() {
                callback(SymmetryStageEvent::Started {
                    stage: "strict_roundtrip_parse",
                });
            }
            let parsed_items = Item::read_player_items(bytes_a, &huffman, is_alpha_a);
            if let Some(callback) = observer.as_mut() {
                let (item_count, outcome) = match &parsed_items {
                    Ok(items) => (Some(items.len()), "ok"),
                    Err(_) => (None, "error"),
                };
                callback(SymmetryStageEvent::Completed {
                    stage: "strict_roundtrip_parse",
                    elapsed_ms: parse_started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                    item_count,
                    outcome,
                });
            }
            parsed_items.map_err(|e| anyhow::anyhow!("{}", e))?
        };
        report.item_count_a = items.len();
        report.item_count_b = items.len();

        let compare_started = Instant::now();
        if let Some(callback) = observer.as_mut() {
            callback(SymmetryStageEvent::Started {
                stage: "strict_rebuild_compare",
            });
        }
        let filtered_items: Vec<(usize, &Item)> = items
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                if let Some(target) = options.target_index {
                    *i == target
                } else {
                    true
                }
            })
            .collect();

        let pool_ref = options.pool.as_deref();
        let mut diffs: Vec<ItemDiff> = if options.fail_fast {
            let mut results = Vec::new();
            for (i, item) in filtered_items {
                let diff = compare_item_with_reserialized(
                    i,
                    item,
                    &huffman,
                    is_alpha_a,
                    format!("Item {}", i),
                    bytes_a,
                    pool_ref,
                );
                let is_match = diff.is_match;
                results.push(diff);
                if !is_match {
                    break;
                }
            }
            results
        } else {
            let pool_arc = options.pool.clone();
            filtered_items
                .into_par_iter()
                .map(|(i, item)| {
                    compare_item_with_reserialized(
                        i,
                        item,
                        &huffman,
                        is_alpha_a,
                        format!("Item {}", i),
                        bytes_a,
                        pool_arc.as_deref(),
                    )
                })
                .collect()
        };

        report.items.append(&mut diffs);
        if let Some(callback) = observer.as_mut() {
            callback(SymmetryStageEvent::Completed {
                stage: "strict_rebuild_compare",
                elapsed_ms: compare_started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                item_count: Some(report.items.len()),
                outcome: "ok",
            });
        }
    } else {
        let bytes_b =
            bytes_b.ok_or_else(|| anyhow::anyhow!("file_b is required when roundtrip is false"))?;
        let is_alpha_b = is_alpha(bytes_b);
        let items_a = Item::read_player_items(bytes_a, &huffman, is_alpha_a)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let items_b = Item::read_player_items(bytes_b, &huffman, is_alpha_b)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        report.item_count_a = items_a.len();
        report.item_count_b = items_b.len();

        let len = items_a.len().min(items_b.len());
        let filtered_indices: Vec<usize> = (0..len)
            .filter(|&i| {
                if let Some(target) = options.target_index {
                    i == target
                } else {
                    true
                }
            })
            .collect();

        let mut diffs: Vec<ItemDiff> = if options.fail_fast {
            let mut results = Vec::new();
            for i in filtered_indices {
                let diff = compare_two_items(
                    &items_a[i],
                    &items_b[i],
                    format!("Item {}", i),
                    bytes_a,
                    bytes_b,
                );
                let is_match = diff.is_match;
                results.push(diff);
                if !is_match {
                    break;
                }
            }
            results
        } else {
            filtered_indices
                .into_par_iter()
                .map(|i| {
                    compare_two_items(
                        &items_a[i],
                        &items_b[i],
                        format!("Item {}", i),
                        bytes_a,
                        bytes_b,
                    )
                })
                .collect()
        };

        report.items.append(&mut diffs);
    }

    report.success =
        report.item_count_a == report.item_count_b && report.items.iter().all(|i| i.is_match);
    Ok(report)
}

fn is_alpha(bytes: &[u8]) -> bool {
    if bytes.len() < 8 {
        return false;
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    version == 105 || version == 6
}

fn compare_item_with_reserialized(
    idx: usize,
    item: &Item,
    huffman: &HuffmanTree,
    alpha_mode: bool,
    label: String,
    original_bytes: &[u8],
    pool: Option<&ItemMemoizationPool>,
) -> ItemDiff {
    let sig = pool.map(|_| ItemSignature::from_item(item, alpha_mode));
    if let (Some(pool), Some(sig)) = (pool, sig.as_ref()) {
        if let Some(receipt) = pool.get(sig) {
            return receipt.to_item_diff(label);
        }
    }

    let is_alpha_socketed_host = alpha_mode && !item.socketed_items.is_empty();
    let preserve_raw_bits =
        is_alpha_socketed_host || should_preserve_alpha_compare_bits(item, alpha_mode);
    let mut strict_item = item.clone();
    if !preserve_raw_bits {
        strict_item.bits.clear();
    }
    let reserialized_bits: Vec<bool> = if alpha_mode {
        strict_item
            .to_bits(idx, huffman, alpha_mode)
            .unwrap_or_default()
    } else {
        let reserialized_bytes = strict_item
            .to_bytes(idx, huffman, alpha_mode)
            .unwrap_or_default();

        let mut rebuilt_bits = Vec::new();
        let mut reader = BitReader::endian(Cursor::new(&reserialized_bytes), LittleEndian);
        while let Ok(bit) = reader.read_bit() {
            rebuilt_bits.push(bit);
        }
        rebuilt_bits
    };

    let original_bits: &[RecordedBit] = item.bits.as_slice();
    let (original_bits, reserialized_bits, original_len, target_len) = if is_alpha_socketed_host {
        // Alpha socketed hosts are validated through the emitted parent window.
        // Their socketed children are compared separately, so the host comparison
        // must stop before nested child emission can perturb the prefix.
        let compare_len = item
            .total_bits
            .min(original_bits.len() as u64)
            .min(reserialized_bits.len() as u64) as usize;
        (
            &original_bits[..compare_len],
            &reserialized_bits[..compare_len],
            compare_len,
            compare_len,
        )
    } else {
        (
            original_bits,
            reserialized_bits.as_slice(),
            original_bits.len(),
            reserialized_bits.len(),
        )
    };

    let mut mismatch_idx = None;
    for i in 0..original_bits.len().min(reserialized_bits.len()) {
        if original_bits[i].bit != reserialized_bits[i] {
            mismatch_idx = Some(i);
            break;
        }
    }

    let mut item_diff = ItemDiff {
        label,
        code: item.code.trim().to_string(),
        original_len,
        target_len,
        fidelity_score: FidelityScore::from_audit(&item.forensic_audit).value,
        forensic_audit: item.forensic_audit.clone(),
        version: item.header.version,
        flags: item.header.flags,
        alpha_header_gap: item.body.alpha_header_gap,
        alpha_alignment_padding_len: item.body.alpha_alignment_padding.len(),
        alpha_body_gap_len: item.body.alpha_body_gap_bits.len(),
        discovered_alpha_header_gap: if alpha_mode {
            peek_item_header_at(original_bytes, item.range.start, huffman, alpha_mode, 0)
                .map(|p| p.8 as u32)
        } else {
            None
        },
        parsed_alpha_header_gap: if alpha_mode {
            Some(item.body.alpha_header_gap_bits.len() as u32)
        } else {
            None
        },
        orig_bits: Some(
            original_bits
                .iter()
                .map(|b| if b.bit { '1' } else { '0' })
                .collect(),
        ),
        target_bits: Some(
            reserialized_bits
                .iter()
                .map(|&b| if b { '1' } else { '0' })
                .collect(),
        ),
        bit_source_contract: Some(
            if preserve_raw_bits {
                "raw_capture_preserving_rebuild"
            } else {
                "strict_rebuild_after_cached_bits_clear"
            }
            .to_string(),
        ),
        cached_bits_preserved: Some(preserve_raw_bits),
        comparison_bit_source: Some("original_item_bits".to_string()),
        reserialized_bit_source: Some("item_to_bits".to_string()),
        ..Default::default()
    };

    if mismatch_idx.is_some() || original_len != target_len {
        item_diff.is_match = false;
        let mut m_type = if original_len != target_len {
            "Length".to_string()
        } else {
            "Content".to_string()
        };

        let len_diff = (original_len as i32 - target_len as i32).abs();
        if len_diff == 2 {
            m_type.push_str(" [Nudge (2-bit)]");
        } else if len_diff > 0 && len_diff % 16 == 0 {
            m_type.push_str(&format!(" [RW-Gap ({}-bit)]", len_diff));
        }

        item_diff.mismatch_type = Some(m_type);
        if let Some(idx) = mismatch_idx {
            item_diff.first_mismatch_offset = Some(idx as u64);
            if let Some(recorded_bit) = original_bits.get(idx) {
                item_diff.recorded_bit_offset = Some(recorded_bit.offset);
                item_diff.recorded_bit_value = Some(recorded_bit.bit);
                item_diff.recorded_bit_source = Some("original_item_bits".to_string());
                item_diff.recorded_bit_source_label_unavailable = Some(true);
            }
            item_diff.segment = Some(
                item.query_bit(idx as u64)
                    .map(|s| s.label)
                    .unwrap_or_else(|| "Unknown".to_string()),
            );
        }
    } else {
        item_diff.is_match = true;
    }

    for (i, child) in item.socketed_items.iter().enumerate() {
        item_diff.children.push(compare_item_with_reserialized(
            0,
            child,
            huffman,
            alpha_mode,
            format!("Child {}", i),
            original_bytes,
            pool,
        ));
    }
    if !is_alpha_socketed_host && !item_diff.children.iter().all(|c| c.is_match) {
        item_diff.is_match = false;
    }

    item_diff
}

/// Standalone pure single-item verification helper without cache lookup or insertion.
pub fn compare_single_item_diff(
    idx: usize,
    item: &Item,
    huffman: &HuffmanTree,
    alpha_mode: bool,
    label: String,
    original_bytes: &[u8],
) -> ItemDiff {
    compare_item_with_reserialized(idx, item, huffman, alpha_mode, label, original_bytes, None)
}

fn compare_two_items(
    item_a: &Item,
    item_b: &Item,
    label: String,
    bytes_a: &[u8],
    bytes_b: &[u8],
) -> ItemDiff {
    let _is_alpha = is_alpha(bytes_a);

    let mut item_diff = ItemDiff {
        label,
        code: item_a.code.trim().to_string(),
        original_len: item_a.bits.len(),
        target_len: item_b.bits.len(),
        fidelity_score: FidelityScore::from_audit(&item_a.forensic_audit).value,
        forensic_audit: item_a.forensic_audit.clone(),
        version: item_a.header.version,
        flags: item_a.header.flags,
        alpha_header_gap: item_a.body.alpha_header_gap,
        alpha_alignment_padding_len: item_a.body.alpha_alignment_padding.len(),
        alpha_body_gap_len: item_a.body.alpha_body_gap_bits.len(),
        discovered_alpha_header_gap: if item_a.header.version >= 5 {
            // In this context, we re-parse using symmetry's huffman
            let huffman = HuffmanTree::new();
            peek_item_header_at(bytes_a, item_a.range.start, &huffman, true, 0).map(|p| p.8 as u32)
        } else {
            None
        },
        parsed_alpha_header_gap: if item_a.header.version >= 5 {
            Some(item_a.body.alpha_header_gap_bits.len() as u32)
        } else {
            None
        },
        orig_bits: Some(
            item_a
                .bits
                .iter()
                .map(|b| if b.bit { '1' } else { '0' })
                .collect(),
        ),
        target_bits: Some(
            item_b
                .bits
                .iter()
                .map(|b| if b.bit { '1' } else { '0' })
                .collect(),
        ),
        ..Default::default()
    };

    if item_a.bits.len() != item_b.bits.len() {
        item_diff.is_match = false;
        let mut m_type = "Length".to_string();
        let len_diff = (item_a.bits.len() as i32 - item_b.bits.len() as i32).abs();
        if len_diff == 2 {
            m_type.push_str(" [Nudge (2-bit)]");
        } else if len_diff > 0 && len_diff % 16 == 0 {
            m_type.push_str(&format!(" [RW-Gap ({}-bit)]", len_diff));
        }
        item_diff.mismatch_type = Some(m_type);
    } else {
        let mut mismatch_idx = None;
        for i in 0..item_a.bits.len() {
            if item_a.bits[i].bit != item_b.bits[i].bit {
                mismatch_idx = Some(i);
                break;
            }
        }
        if let Some(idx) = mismatch_idx {
            item_diff.is_match = false;
            item_diff.mismatch_type = Some("Content".to_string());
            item_diff.first_mismatch_offset = Some(idx as u64);
            item_diff.segment = Some(
                item_a
                    .query_bit(idx as u64)
                    .map(|s| s.label)
                    .unwrap_or_else(|| "Unknown".to_string()),
            );
        } else {
            item_diff.is_match = true;
        }
    }

    for i in 0..item_a.socketed_items.len().max(item_b.socketed_items.len()) {
        if i < item_a.socketed_items.len() && i < item_b.socketed_items.len() {
            item_diff.children.push(compare_two_items(
                &item_a.socketed_items[i],
                &item_b.socketed_items[i],
                format!("Child {}", i),
                bytes_a,
                bytes_b,
            ));
        } else {
            item_diff.is_match = false;
            item_diff.children.push(ItemDiff {
                label: format!("Child {}", i),
                code: if i < item_a.socketed_items.len() {
                    item_a.socketed_items[i].code.trim().to_string()
                } else {
                    item_b.socketed_items[i].code.trim().to_string()
                },
                is_match: false,
                mismatch_type: Some("ChildCount".to_string()),
                original_len: if i < item_a.socketed_items.len() {
                    item_a.socketed_items[i].bits.len()
                } else {
                    0
                },
                target_len: if i < item_b.socketed_items.len() {
                    item_b.socketed_items[i].bits.len()
                } else {
                    0
                },
                ..Default::default()
            });
        }
    }
    if !item_diff.children.iter().all(|c| c.is_match) {
        item_diff.is_match = false;
    }
    item_diff
}

fn should_preserve_alpha_compare_bits(item: &Item, alpha_mode: bool) -> bool {
    if !alpha_mode {
        return false;
    }

    let trimmed = item.code.trim();
    // Preserve compare bits for ambiguous alpha families, opaque placeholders, and raw non-ASCII codes
    // that the strict rebuild cannot canonically round-trip without preserved modules.
    !trimmed.is_ascii()
        || trimmed == "Opaque"
        || trimmed.is_empty()
        || matches!(trimmed, "vhgu" | "d pl" | "bst")
        || item.modules.iter().any(|m| {
            matches!(
                m,
                crate::domain::item::ItemModule::Opaque(_)
                    | crate::domain::item::ItemModule::Residue(_)
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::item::entity::{Item, RecordedBit};
    use d2r_macros::hypothesis_test;

    #[test]
    fn test_mismatch_labeling() {
        let mut item_a = Item::default();
        item_a.code = "test".to_string();
        for i in 0..100 {
            item_a.bits.push(RecordedBit {
                bit: false,
                offset: i as u64,
            });
        }

        let mut item_b = item_a.clone();

        // 2-bit diff
        item_b.bits.truncate(98);
        let diff = compare_two_items(&item_a, &item_b, "Test".to_string(), &[], &[]);
        assert_eq!(diff.mismatch_type.unwrap(), "Length [Nudge (2-bit)]");

        // 16-bit diff
        item_b.bits.truncate(84);
        let diff = compare_two_items(&item_a, &item_b, "Test".to_string(), &[], &[]);
        assert_eq!(diff.mismatch_type.unwrap(), "Length [RW-Gap (16-bit)]");

        // 32-bit diff
        item_b.bits.truncate(68);
        let diff = compare_two_items(&item_a, &item_b, "Test".to_string(), &[], &[]);
        assert_eq!(diff.mismatch_type.unwrap(), "Length [RW-Gap (32-bit)]");
    }

    #[hypothesis_test]
    fn reused_roundtrip_matches_existing_parse_path() {
        const FIXTURE: &[u8] =
            include_bytes!("../../tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
        let huffman = HuffmanTree::new();
        let parsed_items =
            Item::read_player_items(FIXTURE, &huffman, is_alpha(FIXTURE)).expect("fixture parses");
        assert!(
            !parsed_items.is_empty(),
            "fixture should exercise item comparison"
        );

        let pool = Arc::new(ItemMemoizationPool::new());
        let cases = [
            (None, false, false),
            (Some(0), true, false),
            (None, true, true),
            (Some(0), false, true),
        ];

        for (target_index, fail_fast, use_pool) in cases {
            let mut options = SymmetryOptions::roundtrip(true);
            options.target_index = target_index;
            options.fail_fast = fail_fast;
            if use_pool {
                options.pool = Some(pool.clone());
            }

            let parsed_path = calculate_symmetry_diff(FIXTURE, None, options.clone())
                .expect("parsed route works");
            let reused_path =
                calculate_symmetry_diff_from_parsed_items(FIXTURE, parsed_items.clone(), options)
                    .expect("reused route works");

            assert_eq!(
                serde_json::to_value(parsed_path).expect("parsed report serializes"),
                serde_json::to_value(reused_path).expect("reused report serializes"),
                "report parity for target_index={target_index:?}, fail_fast={fail_fast}, use_pool={use_pool}"
            );
        }
    }

    #[hypothesis_test]
    fn reused_roundtrip_stage_trace_omits_duplicate_parse() {
        const FIXTURE: &[u8] =
            include_bytes!("../../tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
        let huffman = HuffmanTree::new();
        let parsed_items =
            Item::read_player_items(FIXTURE, &huffman, is_alpha(FIXTURE)).expect("fixture parses");
        let mut events = Vec::new();

        calculate_symmetry_diff_from_parsed_items_with_stage_observer(
            FIXTURE,
            parsed_items,
            SymmetryOptions::roundtrip(true),
            &mut |event| events.push(event),
        )
        .expect("reused route works");

        assert_eq!(
            events.len(),
            2,
            "only the rebuild/compare phase is measured"
        );
        assert!(matches!(
            events[0],
            SymmetryStageEvent::Started {
                stage: "strict_rebuild_compare"
            }
        ));
        assert!(matches!(
            events[1],
            SymmetryStageEvent::Completed {
                stage: "strict_rebuild_compare",
                outcome: "ok",
                ..
            }
        ));
    }
}

#[cfg(test)]
mod consumer_audit_tests {
    use super::*;
    use d2r_macros::{invariant_test, settled_contract_test};
    const FRESH: &str =
        "tests/fixtures/savegames/gameplay/normal/act1/TESTASSASSIN_Act1_FreshStart.d2s";
    fn input(relative: &str) -> (Vec<u8>, PlayerItemStream) {
        let bytes =
            std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)).unwrap();
        let stream = PlayerItemStream::read(&bytes, &HuffmanTree::new()).unwrap();
        (bytes, stream)
    }
    #[invariant_test]
    fn fresh_source_rebuild_survives_poisoned_raw_cache() {
        let (bytes, mut stream) = input(FRESH);
        assert_eq!(stream.dialect, ItemStreamDialect::D2rV3);
        for item in &mut stream.items {
            item.bits = vec![RecordedBit {
                bit: true,
                offset: u64::MAX,
            }];
            item.total_bits = 1;
            item.logical_width = Some(1);
        }
        let report = calculate_symmetry_diff_from_player_stream(
            &bytes,
            stream,
            SymmetryOptions::roundtrip(true),
            None,
        )
        .unwrap();
        assert_eq!(report.items.len(), 8);
        assert!(report.success, "{report:?}");
        assert!(report.items.iter().all(|row| row.fidelity_score == 1.0
            && row.cached_bits_preserved == Some(false)
            && row.comparison_bit_source.as_deref() == Some("original_save_range")));
    }
    #[settled_contract_test]
    fn source_change_and_semantic_change_cannot_use_memoized_pass() {
        use crate::verify::dedup_pool::CachedVerificationReceipt;
        use std::collections::HashMap;
        let (mut bytes, mut stream) = input(FRESH);
        let range = stream.items[0].range.clone();
        let signature = ItemSignature::from_item(&stream.items[0], false);
        let mut map = HashMap::new();
        map.insert(
            signature,
            CachedVerificationReceipt::from_diff(&ItemDiff {
                is_match: true,
                fidelity_score: 1.0,
                ..Default::default()
            }),
        );
        let pool = Arc::new(ItemMemoizationPool::from_map(map));
        bytes[range.start as usize / 8] ^= 1;
        stream.items[1].body.x ^= 1;
        let report = calculate_symmetry_diff_from_player_stream(
            &bytes,
            stream,
            SymmetryOptions::roundtrip(true).with_pool(Some(pool.clone())),
            None,
        )
        .unwrap();
        assert!(!report.success);
        assert_eq!(report.items[0].mismatch_type.as_deref(), Some("Content"));
        assert_eq!(report.items[0].recorded_bit_offset, Some(range.start));
        assert!(report.items[1].rebuild_error.is_some());
        assert_eq!(pool.stats().total_queries(), 0);
    }
    #[settled_contract_test]
    fn unsupported_socket_parent_is_not_a_cached_pass() {
        use crate::verify::dedup_pool::CachedVerificationReceipt;
        use std::collections::HashMap;

        fn poison(item: &mut Item) {
            item.bits = vec![RecordedBit {
                bit: true,
                offset: u64::MAX,
            }];
            item.body.alpha_code_bits = vec![true; 17];
            item.total_bits = 1;
            item.logical_width = Some(1);
            for child in &mut item.socketed_items {
                poison(child);
            }
        }
        let (bytes, mut stream) =
            input("tests/fixtures/savegames/gameplay/items/NEC_ITEM_Runeword_Hustle_72h.d2s");
        assert_eq!(stream.dialect, ItemStreamDialect::D2rV3);
        assert_eq!(stream.items.len(), 1);
        assert!(stream.items[0].header.is_runeword);
        poison(&mut stream.items[0]);
        let mut invalid = PlayerItemStream {
            dialect: stream.dialect,
            items: stream.items.clone(),
            diagnostics: stream.diagnostics.clone(),
        };
        let report = calculate_symmetry_diff_from_player_stream(
            &bytes,
            stream,
            SymmetryOptions::roundtrip(true),
            None,
        )
        .unwrap();
        assert!(report.success, "{report:?}");
        let root = &report.items[0];
        assert!(root.is_match);
        assert_eq!(root.fidelity_score, 1.0);
        assert_eq!(root.cached_bits_preserved, Some(false));
        assert_eq!(
            root.comparison_bit_source.as_deref(),
            Some("original_save_range")
        );
        assert_eq!(root.children.len(), 3);
        assert!(
            root.children
                .iter()
                .all(|child| child.is_match && child.cached_bits_preserved == Some(false))
        );

        // This extra typed field has no owner in the admitted V3 host layout.
        invalid.items[0].runeword_level = Some(1);
        let mut map = HashMap::new();
        map.insert(
            ItemSignature::from_item(&invalid.items[0], false),
            CachedVerificationReceipt::from_diff(&ItemDiff {
                is_match: true,
                fidelity_score: 1.0,
                ..Default::default()
            }),
        );
        let pool = Arc::new(ItemMemoizationPool::from_map(map));
        let report = calculate_symmetry_diff_from_player_stream(
            &bytes,
            invalid,
            SymmetryOptions::roundtrip(true).with_pool(Some(pool.clone())),
            None,
        )
        .unwrap();
        assert!(!report.success);
        let root = &report.items[0];
        assert_eq!(root.mismatch_type.as_deref(), Some("Rebuild"));
        assert!(
            root.rebuild_error
                .as_ref()
                .is_some_and(|error| !error.reason.is_empty())
        );
        assert_eq!(root.fidelity_score, 0.0);
        assert_eq!(root.cached_bits_preserved, Some(false));
        assert_eq!(root.children.len(), 3);
        assert!(root.children.iter().all(|child| child.is_match));
        assert_eq!(pool.stats().total_queries(), 0);
    }

    #[settled_contract_test]
    fn invalid_span_index_empty_and_nonroundtrip_are_rejected() {
        let (bytes, mut stream) = input(FRESH);
        stream.items[0].range.end = u64::MAX;
        let report = calculate_symmetry_diff_from_player_stream(
            &bytes,
            stream,
            SymmetryOptions::roundtrip(true),
            None,
        )
        .unwrap();
        assert_eq!(
            report.items[0].rebuild_error.as_ref().unwrap().kind,
            "invalid_source_span"
        );
        let (_, stream) = input(FRESH);
        assert!(
            calculate_symmetry_diff_from_player_stream(
                &bytes,
                stream,
                SymmetryOptions {
                    roundtrip: true,
                    target_index: Some(8),
                    ..Default::default()
                },
                None
            )
            .is_err()
        );
        let (_, mut stream) = input(FRESH);
        stream.items.clear();
        assert!(
            calculate_symmetry_diff_from_player_stream(
                &bytes,
                stream,
                SymmetryOptions::roundtrip(true),
                None
            )
            .is_err()
        );
        let (_, stream) = input(FRESH);
        assert!(
            calculate_symmetry_diff_from_player_stream(
                &bytes,
                stream,
                SymmetryOptions::default(),
                None
            )
            .is_err()
        );
    }
}
