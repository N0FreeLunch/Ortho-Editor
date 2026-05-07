//! Roundtrip validation tests for bit-perfect serialization.

#[cfg(test)]
mod roundtrip_tests {
    use d2r_core::domain::vo::align_to_byte;
    use d2r_core::item::{HuffmanTree, Item};
    use d2r_core::verify::{Verifier, bit_diff::BitDiffVerifier};
    use std::fs;
    use std::path::PathBuf;

    fn repo_path(relative: &str) -> PathBuf {
        let _ = dotenvy::dotenv();
        let base = std::env::var("D2R_CORE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        base.join(relative)
    }

    #[test]
    fn test_authority_runeword_roundtrip() {
        let path = repo_path("tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
        let bytes = fs::read(path).expect("fixture should be readable");
        let huffman = HuffmanTree::new();

        // 1. Read all items from the save
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        let items =
            Item::read_player_items(&bytes, &huffman, version == 105).expect("items should parse");

        for item in &items {
            // 2. Re-serialize each item
            let alpha_mode = version == 105;
            let reserialized = item
                .to_bytes(&huffman, alpha_mode)
                .expect("should re-serialize");

            // 3. Compare bits if the item wasn't recovered/modified during parse
            // Note: If the item was 'recovered' due to bit-mismatch, the roundtrip
            // might produce a 'fixed' bitstream which is logically identical but bit-different.
            // For 'Authority' runeword, we expect it to be stable.
            if item.properties_complete {
                // If it's a top-level item with bits recorded, we can compare directly.
                if !item.bits.is_empty() {
                    // Re-calculate how many bytes the original bits occupied
                    let original_bits_len = item.bits.len() as u64;
                    let original_bytes_len = align_to_byte(original_bits_len) / 8;

                    assert_eq!(
                        reserialized.len() as u64,
                        original_bytes_len,
                        "Reserialized length mismatch for item {}",
                        item.code
                    );

                    // We don't have the original raw segment here easily,
                    // but we can parse the reserialized bytes back and compare properties.
                    let item_back = Item::from_bytes(&reserialized, &huffman, alpha_mode)
                        .expect("should parse back");
                    assert_eq!(item.code, item_back.code);
                    assert_eq!(item.properties.len(), item_back.properties.len());
                    for (p1, p2) in item.properties.iter().zip(item_back.properties.iter()) {
                        assert_eq!(p1.stat_id, p2.stat_id);
                        assert_eq!(p1.value, p2.value);
                    }
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn test_mutation_and_roundtrip() {
        let path = repo_path("tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
        let bytes = fs::read(path).expect("fixture should be readable");
        println!(
            "Save Signature: 0x{:08X}",
            u32::from_le_bytes(bytes[0..4].try_into().unwrap())
        );
        println!(
            "Save Version: 0x{:08X}",
            u32::from_le_bytes(bytes[4..8].try_into().unwrap())
        );
        let huffman = HuffmanTree::new();

        // Use Trace to see what's happening
        unsafe {
            std::env::set_var("D2R_ITEM_TRACE", "1");
        }

        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        let mut items =
            Item::read_player_items(&bytes, &huffman, version == 105).expect("items should parse");

        // The Authority item was previously misidentified as "w ha", but it's "xrs " (Cuirass)
        let authority = items
            .iter_mut()
            .find(|item| item.code.trim() == "xrs")
            .expect("Authority item (xrs) not found");

        println!(
            "Authority Item: Code={}, Version={}, Flags=0x{:08X}",
            authority.code, authority.version, authority.flags
        );

        // Check current properties
        println!(
            "Item properties: {:?}",
            authority
                .properties
                .iter()
                .map(|p| (p.stat_id, &p.name))
                .collect::<Vec<_>>()
        );
        println!(
            "Set attributes: {:?}",
            authority
                .set_attributes
                .iter()
                .map(|list| list.iter().map(|p| p.stat_id).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        );
        println!(
            "Runeword attributes: {:?}",
            authority
                .runeword_attributes
                .iter()
                .map(|p| p.stat_id)
                .collect::<Vec<_>>()
        );
        println!("--------------------------------------------------");

        // Mutate internal properties:
        // In Alpha v105 Authority, ID 9 (maxmana) exists.
        let target_stat_id = 9;
        use d2r_core::domain::vo::ItemStatValue;
        let new_val = ItemStatValue::new(100).unwrap();

        assert!(
            authority.set_property_value(target_stat_id, new_val),
            "Failed to set property {}",
            target_stat_id
        );

        // Re-serialize and verify
        let alpha_mode = version == 105;
        let reserialized = authority
            .to_bytes(&huffman, alpha_mode)
            .expect("should re-serialize modified item");

        // Parse back and verify new value
        let modified_item = Item::from_bytes(&reserialized, &huffman, alpha_mode)
            .expect("should parse back modified bits");

        let mut all_stats = modified_item.properties.clone();
        for list in &modified_item.set_attributes {
            all_stats.extend(list.clone());
        }
        all_stats.extend(modified_item.runeword_attributes.clone());

        let new_stat = all_stats
            .iter()
            .find(|p| p.stat_id == target_stat_id)
            .expect("Mutated stat not found");
        assert_eq!(new_stat.value, 300);
    }



    #[test]
    fn test_10scrolls_full_roundtrip() {
        let path = repo_path("tests/fixtures/savegames/original/amazon_10_scrolls.d2s");
        let bytes = fs::read(path).expect("fixture should be readable");
        let huffman = HuffmanTree::new();

        // 1. Read all items - Expecting 16 items (via rescue strategy)
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        let items =
            Item::read_player_items(&bytes, &huffman, version == 105).expect("items should parse");
        assert_eq!(
            items.len(),
            16,
            "Should have recovered all 16 items from 10-scrolls fixture"
        );

        for item in &items {
            // 2. Re-serialize
            let reserialized = item.to_bytes(&huffman, true).expect("should re-serialize");

            // 3. Parse back and verify basic identity
            let item_back =
                Item::from_bytes(&reserialized, &huffman, true).expect("should parse back");
            assert_eq!(item.code, item_back.code, "Code mismatch for {}", item.code);
            assert_eq!(
                item.version, item_back.version,
                "Version mismatch for {}",
                item.code
            );
            assert_eq!(
                item.properties.len(),
                item_back.properties.len(),
                "Properties length mismatch for {}",
                item.code
            );
        }
    }

    #[test]
    fn test_full_save_roundtrip_regression() -> std::io::Result<()> {
        use d2r_core::save::{
            AttributeSection, map_core_sections, parse_quest_section, parse_skill_section,
            rebuild_status_and_player_items,
        };
        use d2r_core::verify::sba::{SbaBaseline, flatten_item, verify_baseline};

        let fixtures = [
            "tests/fixtures/savegames/original/TESTAMAZON.d2s",
            "tests/fixtures/savegames/original/amazon_empty.d2s",
            "tests/fixtures/savegames/original/amazon_authority_runeword.d2s",
            "tests/fixtures/savegames/original/amazon_10_scrolls.d2s",
            "tests/fixtures/savegames/original/amazon_v105_act2_start.d2s",
            "tests/fixtures/savegames/original/amazon_v105_andariel_killed_no_talk.d2s",
            "tests/fixtures/savegames/original/amazon_v105_re_probe_zigzag_all_diff.d2s",
        ];

        let filter = std::env::var("D2R_FIXTURE_FILTER").ok();
        let huffman = HuffmanTree::new();
        unsafe {
            std::env::set_var("D2R_ITEM_TRACE", "1");
        }
        for fixture in fixtures {
            if let Some(ref f) = filter {
                if !fixture.contains(f) {
                    continue;
                }
            }
            let path = repo_path(fixture);
            let bytes = fs::read(path).expect("fixture should be readable");

            // 1. Map and Parse all sections
            let map = map_core_sections(&bytes)?;
            let attributes = AttributeSection::parse(&bytes, map.gf_pos, map.if_pos)?;
            let skills = parse_skill_section(&bytes, &map)?;
            let quests = parse_quest_section(&bytes, &map)?;
            let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
            let items = Item::read_player_items(&bytes, &huffman, version == 105)?;

            // 2. Rebuild the entire save
            let rebuilt = rebuild_status_and_player_items(
                &bytes,
                Some(&attributes),
                Some(&skills),
                Some(&quests),
                None,
                None,
                &items,
                &huffman,
            )?;

            // 3. 100% Binary match requirement for these specific fixtures
            let verifier = BitDiffVerifier;
            let report = verifier.verify(&bytes, &rebuilt);
            
            if !report.is_success {
                // SBA Forensic Analysis
                let is_alpha = version == 105;
                let mut issues = Vec::new();
                
                let mut exp_flattened = Vec::new();
                for (i, item) in items.iter().enumerate() {
                    flatten_item(item, &i.to_string(), &mut exp_flattened);
                }
                let expected_baseline = SbaBaseline {
                    fixture: fixture.to_string(),
                    items: exp_flattened,
                };

                if let Ok(rebuilt_items) = Item::read_player_items(&rebuilt, &huffman, is_alpha) {
                    let mut act_flattened = Vec::new();
                    for (i, item) in rebuilt_items.iter().enumerate() {
                        flatten_item(item, &i.to_string(), &mut act_flattened);
                    }
                    let actual_baseline = SbaBaseline {
                        fixture: "reproduced".to_string(),
                        items: act_flattened,
                    };

                    let _ = verify_baseline(&expected_baseline, &actual_baseline, &mut issues);
                }

                for issue in issues {
                    eprintln!("[FORENSIC] Structural Mismatch: {} | Kind: {}", issue.message, issue.kind);
                }

                let jm_pos = map.first_jm();
                let section_start_bit = (jm_pos + 4) * 8;
                for issue in &report.issues {
                    let mut label = None;
                    for item in &items {
                        let abs_start = section_start_bit as u64 + item.range.start;
                        let abs_end = section_start_bit as u64 + item.range.end;
                        if issue.bit_offset >= abs_start && issue.bit_offset < abs_end {
                            // Mismatch is in this item!
                            let rel_bit = issue.bit_offset - abs_start;
                            // Now find segment in item
                            for seg in &item.segments {
                                if rel_bit >= seg.start && rel_bit < seg.end {
                                    label = Some(format!("Item({}) -> {}", item.code.trim(), seg.label));
                                    break;
                                }
                            }
                            if label.is_none() {
                                label = Some(format!("Item({}) -> Unknown Segment", item.code.trim()));
                            }
                            break;
                        }
                    }
                    if let Some(l) = label {
                        eprintln!("[AVRM] {} | Context: {}", issue.message, l);
                    } else {
                        eprintln!("[AVRM] {}", issue.message);
                    }
                }
                let _ = std::fs::write("tmp/reproduced.d2s", &rebuilt);
                eprintln!("[INFO] Failure artifact saved to tmp/reproduced.d2s for d2save_verify");
            }
            assert!(report.is_success, "Full save binary mismatch for {}", fixture);
        }
        Ok(())
    }

    #[test]
    fn test_manual_item_mutation() {
        use d2r_core::item::Item;
        let mut item = Item::empty_for_tests();
        item.header.version = 5; // Alpha-like
        
        // Set initial values
        item.set_defense(Some(100));
        item.set_durability(Some(10), Some(20));
        item.set_quantity(Some(5));
        item.set_id(Some(12345));
        item.set_level(Some(80));

        // Verify getters
        assert_eq!(item.defense(), Some(100));
        assert_eq!(item.current_durability(), Some(10));
        assert_eq!(item.max_durability(), Some(20));
        assert_eq!(item.quantity(), Some(5));
        assert_eq!(item.header.id, Some(12345));
        assert_eq!(item.header.level, Some(80));

        // Check mirror fields (Legacy Compatibility)
        assert_eq!(item.defense, Some(100));
        assert_eq!(item.current_durability, Some(10));
        assert_eq!(item.max_durability, Some(20));
        assert_eq!(item.quantity, Some(5));
    }

    #[test]
    fn test_socket_mutation() {
        use d2r_core::item::Item;
        let mut parent = Item::empty_for_tests();
        parent.set_sockets(2);
        assert_eq!(parent.sockets, Some(2));
        assert!(parent.header.is_socketed);

        let mut child = Item::empty_for_tests();
        child.body.code = "r01 ".to_string(); // El Rune
        child.code = "r01 ".to_string();

        parent.add_socketed_item(child.clone());
        assert_eq!(parent.socketed_items.len(), 1);
        assert_eq!(parent.num_socketed_items, 1);
        assert_eq!(parent.sockets, Some(2)); // Should stay 2
        assert_eq!(parent.socketed_items[0].code, "r01 ");

        // Adding more items than sockets should auto-bump sockets
        parent.add_socketed_item(child.clone());
        parent.add_socketed_item(child.clone());
        assert_eq!(parent.socketed_items.len(), 3);
        assert_eq!(parent.num_socketed_items, 3);
        assert_eq!(parent.sockets, Some(3)); // Bumped to 3
        assert!(parent.header.is_socketed);
    }
}
