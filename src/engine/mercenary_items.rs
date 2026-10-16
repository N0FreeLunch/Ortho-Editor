//! Shared source-bound mercenary list collection and slot evidence.
//! JSON consumers retain their own payload format and report exported counts explicitly.

use crate::domain::forensic::v105::{MercenaryEquipmentItem, MercenaryFooter};
use crate::domain::item::stream::{ItemStreamDialect, PlayerItemStream};
use crate::item::{HuffmanTree, Item};

#[derive(Debug, serde::Serialize)]
pub struct MercenaryItemStreamEvidence {
    pub dialect: ItemStreamDialect,
    pub status: &'static str,
    pub declared_count: Option<u16>,
    pub parsed_count: usize,
    pub exported_count: usize,
    pub source_range: Option<std::ops::Range<u64>>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug)]
pub struct MercenaryItemStream {
    pub items: Vec<Item>,
    pub evidence: MercenaryItemStreamEvidence,
}

impl MercenaryItemStreamEvidence {
    pub fn record_exported_count(&mut self, count: usize) {
        if self.status == "error" {
            return;
        }
        let item_count = self.declared_count.map(usize::from).unwrap_or(count);
        self.exported_count = count;
        if self.exported_count != item_count {
            self.status = "partial";
            self.diagnostics
                .push("Mercenary declaration and exported count differ".to_string());
        }
    }
}

fn mercenary_stream_failure(
    mut evidence: MercenaryItemStreamEvidence,
    reason: impl Into<String>,
) -> MercenaryItemStream {
    evidence.status = "error";
    evidence.diagnostics.push(reason.into());
    MercenaryItemStream {
        items: Vec::new(),
        evidence,
    }
}

pub fn collect_mercenary_items(
    bytes: &[u8],
    map: &crate::save::SaveSectionMap,
    huffman: &HuffmanTree,
    player_stream: &PlayerItemStream,
) -> MercenaryItemStream {
    let dialect = player_stream.dialect;
    let is_alpha = dialect == ItemStreamDialect::Alpha;
    let mut evidence = MercenaryItemStreamEvidence {
        dialect,
        status: "absent",
        declared_count: None,
        parsed_count: 0,
        exported_count: 0,
        source_range: None,
        diagnostics: Vec::new(),
    };
    let Some(jf) = map.jf_pos.or_else(|| find_marker(bytes, b"jf")) else {
        return MercenaryItemStream {
            items: Vec::new(),
            evidence,
        };
    };

    let jm_positions = crate::save::find_jm_markers(bytes);
    let jm_pos = if dialect == ItemStreamDialect::D2rV3 {
        let primary_end = player_stream
            .items
            .last()
            .map(|item| item.range.end)
            .unwrap_or(0);
        if jf as u64 * 8 < primary_end {
            return mercenary_stream_failure(
                evidence,
                "Mercenary marker overlaps the owned primary list",
            );
        }
        let owned = jf.saturating_add(2);
        // An unhired character has jf immediately followed by the standard footer.
        if bytes.get(owned..owned.saturating_add(2)) == Some(b"kf".as_slice())
            && MercenaryFooter::from_bytes(&bytes[owned..]).is_standard()
        {
            return MercenaryItemStream {
                items: Vec::new(),
                evidence,
            };
        }
        if bytes.get(owned..owned.saturating_add(2)) != Some(b"JM".as_slice()) {
            return mercenary_stream_failure(
                evidence,
                "Owned V3 mercenary list must begin immediately after jf",
            );
        }
        owned
    } else {
        let Some(pos) = jm_positions.iter().copied().find(|pos| *pos > jf) else {
            return MercenaryItemStream {
                items: Vec::new(),
                evidence,
            };
        };
        pos
    };
    let Some(header) = bytes.get(jm_pos..jm_pos.saturating_add(4)) else {
        return mercenary_stream_failure(evidence, "Mercenary list header is truncated");
    };
    let item_count = u16::from_le_bytes([header[2], header[3]]);
    evidence.declared_count = Some(item_count);
    let (Some(kf), Some(lf)) = (
        map.kf_pos.or_else(|| find_marker(bytes, b"kf")),
        map.lf_pos.or_else(|| find_marker(bytes, b"lf")),
    ) else {
        return mercenary_stream_failure(evidence, "Mercenary footer markers are missing");
    };
    let next_pos = jm_positions
        .iter()
        .copied()
        .find(|pos| *pos > jm_pos)
        .unwrap_or(bytes.len());
    let items_end = if dialect == ItemStreamDialect::D2rV3 {
        kf
    } else {
        kf.min(next_pos)
    };
    if kf > bytes.len()
        || !MercenaryFooter::from_bytes(&bytes[kf..]).is_standard()
        || lf < kf
        || jm_pos >= items_end
    {
        return mercenary_stream_failure(
            evidence,
            "Mercenary list has an unsupported footer envelope",
        );
    }
    evidence.source_range = Some((jm_pos as u64 + 4) * 8..items_end as u64 * 8);
    let items = if dialect == ItemStreamDialect::D2rV3 {
        let Some(raw_version) = bytes
            .get(4..8)
            .and_then(|raw| <[u8; 4]>::try_from(raw).ok())
        else {
            return mercenary_stream_failure(evidence, "Save version is truncated");
        };
        match crate::domain::item::serialization::read_counted_list_d2r_v3(
            &bytes[..items_end],
            jm_pos,
            huffman,
            u32::from_le_bytes(raw_version),
        ) {
            Ok(items) => items,
            Err(error) => return mercenary_stream_failure(evidence, error.to_string()),
        }
    } else {
        match Item::read_section(
            &bytes[jm_pos..items_end],
            jm_pos as u64 * 8,
            item_count,
            huffman,
            is_alpha,
            false,
        ) {
            Ok(items) => items,
            Err(error) => return mercenary_stream_failure(evidence, error.to_string()),
        }
    };
    evidence.parsed_count = items.len();
    evidence.status = "ok";
    if dialect == ItemStreamDialect::D2rV3 {
        let mut next = (jm_pos as u64 + 4) * 8;
        for (index, item) in items.iter().enumerate() {
            if item.range.start != next || item.range.end <= next || item.range.end % 8 != 0 {
                return mercenary_stream_failure(
                    evidence,
                    "V3 mercenary item source ranges are not contiguous",
                );
            }
            next = item.range.end;
            let slot = dialect.placement(item).equipment_slot;
            if !item.properties_complete
                || item.is_opaque()
                || item.is_semi_opaque()
                || !item.is_identified
                || !matches!(slot, Some("Head" | "Armor" | "Weapon1" | "Weapon2"))
            {
                evidence.status = "partial";
                evidence.diagnostics.push(format!(
                    "Mercenary item {index} has unresolved properties or placement"
                ));
            }
        }
        if items.len() != item_count as usize || next != items_end as u64 * 8 {
            return mercenary_stream_failure(
                evidence,
                "V3 mercenary count or footer closure disagrees with source",
            );
        }
    }

    MercenaryItemStream { items, evidence }
}

pub fn mercenary_item_slot(
    item: &Item,
    dialect: ItemStreamDialect,
) -> (String, &'static str, &'static str) {
    let trimmed = item.code.trim();
    let code_display = if trimmed.is_empty() { "unk" } else { trimmed };
    let is_alpha = dialect == ItemStreamDialect::Alpha;
    let merc_item = MercenaryEquipmentItem {
        code: code_display.to_string(),
        location: item.location,
        mode: item.mode,
        x: item.x,
        y: item.y,
    };
    if is_alpha {
        alpha_mercenary_slot_semantics(&merc_item, item.is_residue())
    } else if dialect == ItemStreamDialect::D2rV3 {
        let slot = match dialect.placement(item).equipment_slot {
            Some("Head") => "Helm",
            Some("Armor") => "Armor",
            Some("Weapon1") => "Weapon1",
            Some("Weapon2") => "Weapon2",
            _ => "Unknown",
        };
        (
            slot.to_string(),
            "d2r_v3_owned_counted_list",
            if slot == "Unknown" {
                "unresolved_d2r_v3_candidate"
            } else {
                "equipped_slot_candidate"
            },
        )
    } else {
        (
            merc_item.slot_name(),
            "shared_mercenary_slot_name",
            "legacy_candidate",
        )
    }
}

pub fn alpha_mercenary_slot_semantics(
    item: &MercenaryEquipmentItem,
    is_residue: bool,
) -> (String, &'static str, &'static str) {
    let slot = match (item.location, item.mode, item.x) {
        (3, 1, _) => Some("Armor"),
        (4, 1, _) => Some("Weapon1"),
        (1, 1, 1) => Some("Helm"),
        (1, 1, 3) => Some("Armor"),
        (1, 1, 4) => Some("Weapon1"),
        (1, 1, 5) => Some("Weapon2"),
        (4, 5, _) => Some("Weapon1"),
        (1, 4, _) => Some("Weapon2"),
        (4, 0, _) => Some("Helm"),
        (0, 3, _) => Some("Weapon1"), // Rogue Bow/Weapon candidate (observed mode 3)
        (0, 5, _) => Some("Weapon1"), // Rogue Bow/Weapon candidate (observed mode 5)
        _ => None,
    };

    match slot {
        Some(s) => {
            let kind = if is_residue {
                "parser_residue_in_valid_slot"
            } else if item.code == "unk" {
                "unknown_code_localization_gap"
            } else {
                "equipped_slot_candidate"
            };
            (s.to_string(), "alpha_v105_signature", kind)
        }
        None => (
            "Unknown".to_string(),
            "unclassified_alpha_v105_candidate",
            "parser_residue",
        ),
    }
}

fn find_marker(bytes: &[u8], marker: &[u8; 2]) -> Option<usize> {
    bytes.windows(2).position(|window| window == marker)
}

// Helper: get standard quality string representation

#[cfg(test)]
mod mercenary_items_tests {
    use super::*;
    use crate::save::map_core_sections;
    use d2r_macros::settled_contract_test;

    const HIRED: &[u8] = include_bytes!(
        "../../tests/fixtures/savegames/gameplay/normal/act3/TESTASSASSIN_Act3_ColdMercLvl21.d2s"
    );
    const UNHIRED: &[u8] = include_bytes!(
        "../../tests/fixtures/savegames/gameplay/normal/act1/TESTASSASSIN_Act1_FreshStart.d2s"
    );

    fn frame(bytes: &[u8]) -> (crate::save::SaveSectionMap, PlayerItemStream) {
        let map = map_core_sections(bytes).unwrap();
        let jf = map.jf_pos.or_else(|| find_marker(bytes, b"jf")).unwrap();
        // Only the supplied outer-container end is synthetic; save bytes remain original.
        let mut anchor = Item::empty_for_tests();
        anchor.range.end = jf as u64 * 8;
        (
            map,
            PlayerItemStream {
                dialect: ItemStreamDialect::D2rV3,
                items: vec![anchor],
                diagnostics: Vec::new(),
            },
        )
    }

    #[settled_contract_test]
    fn actual_source_frame_keeps_two_items_socket_children_and_slots() {
        let (map, stream) = frame(HIRED);
        let mut parsed = collect_mercenary_items(HIRED, &map, &HuffmanTree::new(), &stream);
        assert_eq!(parsed.evidence.status, "ok");
        assert_eq!(parsed.evidence.declared_count, Some(2));
        assert_eq!(parsed.items.len(), 2);
        let helm = parsed
            .items
            .iter()
            .find(|item| item.code.trim() == "fhl")
            .unwrap();
        assert_eq!(
            helm.socketed_items
                .iter()
                .map(|item| item.code.trim())
                .collect::<Vec<_>>(),
            ["r08", "r09"]
        );
        assert_eq!(
            mercenary_item_slot(helm, ItemStreamDialect::D2rV3),
            (
                "Helm".into(),
                "d2r_v3_owned_counted_list",
                "equipped_slot_candidate"
            )
        );
        assert_eq!(
            parsed.items.last().unwrap().range.end,
            parsed.evidence.source_range.as_ref().unwrap().end
        );
        parsed.evidence.record_exported_count(2);
        assert_eq!(parsed.evidence.exported_count, 2);
        assert_eq!(parsed.evidence.status, "ok");
    }

    #[settled_contract_test]
    fn unhired_source_retains_absent_evidence() {
        let (map, stream) = frame(UNHIRED);
        let mut parsed = collect_mercenary_items(UNHIRED, &map, &HuffmanTree::new(), &stream);
        assert_eq!(parsed.evidence.status, "absent");
        assert!(parsed.items.is_empty());
        assert_eq!(parsed.evidence.declared_count, None);
        parsed.evidence.record_exported_count(0);
        assert_eq!(parsed.evidence.status, "absent");
    }

    #[settled_contract_test]
    fn missing_owned_list_is_an_explicit_error_and_stays_error_after_export() {
        let (map, stream) = frame(HIRED);
        let jf = map.jf_pos.or_else(|| find_marker(HIRED, b"jf")).unwrap();
        let mut bytes = HIRED.to_vec();
        bytes[jf + 2] = b'X';
        let mut parsed = collect_mercenary_items(&bytes, &map, &HuffmanTree::new(), &stream);
        assert_eq!(parsed.evidence.status, "error");
        assert!(parsed.items.is_empty());
        parsed.evidence.record_exported_count(0);
        assert_eq!(parsed.evidence.status, "error");
        assert!(!parsed.evidence.diagnostics.is_empty());
    }

    #[settled_contract_test]
    fn unsupported_footer_is_not_silently_skipped() {
        let (map, stream) = frame(HIRED);
        let kf = map.kf_pos.or_else(|| find_marker(HIRED, b"kf")).unwrap();
        let mut bytes = HIRED.to_vec();
        bytes[kf] = b'X';
        let parsed = collect_mercenary_items(&bytes, &map, &HuffmanTree::new(), &stream);
        assert_eq!(parsed.evidence.status, "error");
        assert!(parsed.items.is_empty());
    }

    #[settled_contract_test]
    fn primary_overlap_and_wrong_declaration_reject_the_owned_frame() {
        let (map, mut stream) = frame(HIRED);
        stream.items.last_mut().unwrap().range.end += 8;
        let parsed = collect_mercenary_items(HIRED, &map, &HuffmanTree::new(), &stream);
        assert_eq!(parsed.evidence.status, "error");
        let (map, stream) = frame(HIRED);
        let jf = map.jf_pos.or_else(|| find_marker(HIRED, b"jf")).unwrap();
        let mut bytes = HIRED.to_vec();
        bytes[jf + 4..jf + 6].copy_from_slice(&1_u16.to_le_bytes());
        let parsed = collect_mercenary_items(&bytes, &map, &HuffmanTree::new(), &stream);
        assert_eq!(parsed.evidence.status, "error");
    }

    #[settled_contract_test]
    fn consumer_count_mismatch_is_partial_and_keeps_source_counts() {
        let (map, stream) = frame(HIRED);
        let mut parsed = collect_mercenary_items(HIRED, &map, &HuffmanTree::new(), &stream);
        parsed.evidence.record_exported_count(1);
        assert_eq!(parsed.evidence.status, "partial");
        assert_eq!(parsed.evidence.declared_count, Some(2));
        assert_eq!(parsed.evidence.parsed_count, 2);
        assert_eq!(parsed.evidence.exported_count, 1);
    }

    #[settled_contract_test]
    fn alpha_and_legacy_slot_evidence_retains_existing_classification() {
        let mut item = Item::empty_for_tests();
        item.code = "fhl".into();
        item.header.location = 1;
        item.header.mode = 1;
        item.header.x = 1;
        assert_eq!(
            mercenary_item_slot(&item, ItemStreamDialect::Alpha),
            (
                "Helm".into(),
                "alpha_v105_signature",
                "equipped_slot_candidate"
            )
        );
        item.code.clear();
        assert_eq!(
            mercenary_item_slot(&item, ItemStreamDialect::Alpha).2,
            "parser_residue_in_valid_slot"
        );
        item.code = "unk".into();
        assert_eq!(
            mercenary_item_slot(&item, ItemStreamDialect::Alpha).2,
            "unknown_code_localization_gap"
        );
        assert_eq!(
            mercenary_item_slot(&item, ItemStreamDialect::Legacy).2,
            "legacy_candidate"
        );
    }

    #[settled_contract_test]
    fn unclassified_v3_slot_is_not_promoted_to_equipped() {
        let mut item = Item::empty_for_tests();
        item.code = "fhl".into();
        item.body.mode = 7;
        assert_eq!(
            mercenary_item_slot(&item, ItemStreamDialect::D2rV3),
            (
                "Unknown".into(),
                "d2r_v3_owned_counted_list",
                "unresolved_d2r_v3_candidate"
            )
        );
    }
}
