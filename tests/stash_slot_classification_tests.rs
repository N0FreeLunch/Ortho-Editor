use d2r_core::item::HuffmanTree;
use d2r_core::save::{ItemSlotClass, collect_player_slots};
use std::fs;
use std::io;

mod common;
use common::repo_path;

fn load_fixture(path: &str) -> io::Result<Vec<u8>> {
    fs::read(repo_path(path))
}

#[d2r_macros::settled_contract_test]
fn stash_slot_classification_progression_fixture() -> io::Result<()> {
    let bytes =
        load_fixture("tests/fixtures/savegames/original/amazon_lvl2_progression_complex.d2s")?;
    let huffman = HuffmanTree::new();
    let slots = collect_player_slots(&bytes, &huffman)?;
    let top_level = slots
        .iter()
        .filter(|(_, class)| *class != ItemSlotClass::SocketChild)
        .count();
    assert_eq!(top_level, 16);
    assert!(
        slots
            .iter()
            .any(|(_, class)| *class == ItemSlotClass::SocketChild)
    );
    assert!(
        slots
            .iter()
            .any(|(_, class)| *class == ItemSlotClass::InventoryLike)
    );
    // The V3 source contains two stash roots; the old Alpha adapter hid them.
    let stash: Vec<_> = slots
        .iter()
        .filter(|(_, class)| *class == ItemSlotClass::StashLike)
        .collect();
    assert_eq!(stash.len(), 2);
    for ((item, _), (start, end, stored_offset)) in
        stash.iter().zip([(9072, 9240, 9122), (9240, 9800, 9290)])
    {
        assert_eq!(item.code.trim(), "xrs");
        assert_eq!((item.range.start, item.range.end), (start, end));
        let stored = (0..3).fold(0_u8, |value, bit| {
            let offset = stored_offset + bit;
            value | (((bytes[offset as usize / 8] >> (offset % 8)) & 1) << bit)
        });
        assert_eq!(stored, 5);
    }
    Ok(())
}

#[cfg(test)]
mod selected_stream_collector {
    use super::*;
    use d2r_core::domain::item::stream::{ItemStreamDialect, PlayerItemStream};
    use d2r_core::item::Item;
    use d2r_core::save::classify_item_slot;
    use d2r_macros::{invariant_test, settled_contract_test};

    fn flatten(mut item: Item, output: &mut Vec<Item>) {
        let children = std::mem::take(&mut item.socketed_items);
        output.push(item);
        for child in children {
            flatten(child, output);
        }
    }

    #[invariant_test]
    fn public_slots_preserve_selected_v3_models_and_source_spans() {
        let huffman = HuffmanTree::new();
        let mut rings = 0;
        let mut children = 0;
        for (relative, roots) in [
            ("act1/TESTASSASSIN_Act1_FreshStart.d2s", 8),
            ("act1/TESTDRUID_Quest1_AkaraRingObtained.d2s", 35),
            ("act1/TESTASSASSIN_Act1_BloodRavenKilled_PreKashya.d2s", 33),
            ("act4/TESTDRUID_Act4_Enter.d2s", 39),
        ] {
            let fixture = format!("tests/fixtures/savegames/gameplay/normal/{relative}");
            let bytes = load_fixture(&fixture).unwrap();
            let original = bytes.clone();
            let stream = PlayerItemStream::read(&bytes, &huffman).unwrap();
            assert_eq!(stream.dialect, ItemStreamDialect::D2rV3);
            assert_eq!(stream.items.len(), roots);
            let mut expected = Vec::new();
            for item in stream.items {
                flatten(item, &mut expected);
            }
            let slots = collect_player_slots(&bytes, &huffman).unwrap();
            assert_eq!(slots.len(), expected.len());
            for ((actual, class), expected) in slots.iter().zip(&expected) {
                assert_eq!(actual, expected, "{fixture}: complete flattened model");
                assert!(actual.socketed_items.is_empty());
                let source: Vec<_> = (actual.range.start..actual.range.end)
                    .map(|bit| bytes[bit as usize / 8] & (1 << (bit % 8)) != 0)
                    .collect();
                assert_eq!(
                    actual.bits.iter().map(|bit| bit.bit).collect::<Vec<_>>(),
                    source
                );
                if actual.mode == 1 {
                    assert_eq!(*class, ItemSlotClass::EquipmentLike);
                    if matches!(actual.location, 6 | 7) {
                        assert_eq!(actual.code.trim(), "rin");
                        rings += 1;
                    }
                }
                if actual.mode == 2 {
                    assert_eq!(*class, ItemSlotClass::InventoryLike);
                }
                if actual.mode == 0
                    && actual
                        .body
                        .d2r_v3_auxiliary
                        .as_ref()
                        .and_then(|a| a.stored_in)
                        == Some(4)
                {
                    assert_eq!(*class, ItemSlotClass::InventoryLike);
                }
                if actual.mode == 6 {
                    assert_eq!(*class, ItemSlotClass::SocketChild);
                    children += 1;
                }
            }
            if roots == 8 {
                assert_eq!(
                    slots
                        .iter()
                        .map(|(item, _)| item.code.trim())
                        .collect::<Vec<_>>(),
                    ["hp1", "hp1", "hp1", "hp1", "tsc", "isc", "ktr", "buc"]
                );
                assert_eq!((slots[6].0.range.start, slots[6].0.range.end), (7728, 7880));
                assert_eq!((slots[7].0.range.start, slots[7].0.range.end), (7880, 8040));
            }
            assert_eq!(bytes, original);
        }
        assert!(rings > 0, "Actual equipped ring coverage is required");
        assert!(children > 0, "Actual socket hierarchy coverage is required");
        println!(
            "selected V3 public collector: 4 fixtures, 115 roots, exact models/source spans; rings={rings}, children={children}"
        );
    }

    #[settled_contract_test]
    fn public_original_streams_keep_format_specific_contracts() {
        let huffman = HuffmanTree::new();
        let mut alpha = 0;
        let mut v3 = 0;
        for (name, count) in [
            ("amazon_authority_runeword", 5),
            ("amazon_10_scrolls", 16),
            ("amazon_v105_act2_start", 4),
            ("amazon_v105_andariel_killed_no_talk", 4),
            ("amazon_v105_re_probe_zigzag_all_diff", 4),
            ("amazon_v105_slice2_equipment", 16),
            ("amazon_lvl2_progression_complex", 16),
        ] {
            let bytes =
                load_fixture(&format!("tests/fixtures/savegames/original/{name}.d2s")).unwrap();
            let stream = PlayerItemStream::read(&bytes, &huffman).unwrap();
            assert_eq!(stream.items.len(), count);
            let dialect = stream.dialect;
            let mut expected = Vec::new();
            if name == "amazon_authority_runeword" {
                assert_eq!(dialect, ItemStreamDialect::Alpha);
                alpha += 1;
                for item in Item::read_player_items(&bytes, &huffman, true).unwrap() {
                    flatten(item, &mut expected);
                }
            } else {
                assert_eq!(dialect, ItemStreamDialect::D2rV3);
                v3 += 1;
                for item in stream.items {
                    flatten(item, &mut expected);
                }
            }
            let slots = collect_player_slots(&bytes, &huffman).unwrap();
            assert_eq!(slots.len(), expected.len());
            for ((actual, class), expected) in slots.iter().zip(&expected) {
                assert_eq!(actual, expected, "{name}");
                if dialect == ItemStreamDialect::Alpha {
                    assert_eq!(*class, classify_item_slot(expected));
                }
            }
        }
        assert_eq!((alpha, v3), (1, 6));
        println!("public original collector: 7 fixtures, 1 actual Alpha and 6 selected V3");
    }

    #[settled_contract_test]
    fn public_collector_rejects_truncated_and_invalid_headers_without_panicking() {
        let huffman = HuffmanTree::new();
        for length in 0..8 {
            let bytes = vec![0; length];
            assert_eq!(
                collect_player_slots(&bytes, &huffman).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
        let mut bytes = vec![0; 12];
        bytes[4..8].copy_from_slice(&105_u32.to_le_bytes());
        assert_eq!(
            collect_player_slots(&bytes, &huffman).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );

        // A valid empty legacy recovery stream retains the existing empty result contract.
        bytes[..4].copy_from_slice(&d2r_core::save::D2S_MAGIC.to_le_bytes());
        bytes[4..8].copy_from_slice(&96_u32.to_le_bytes());
        bytes[8..12].copy_from_slice(b"JM\0\0");
        assert_eq!(
            PlayerItemStream::read(&bytes, &huffman).unwrap().dialect,
            ItemStreamDialect::Legacy
        );
        assert!(collect_player_slots(&bytes, &huffman).unwrap().is_empty());
    }
}
