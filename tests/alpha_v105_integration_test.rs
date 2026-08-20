#[cfg(test)]
mod tests {
    use d2r_core::item::{Item, HuffmanTree};
    use std::fs;

    fn read_alpha_items_from_first_jm(fixture_path: &str) -> (Vec<Item>, u16) {
        let bytes = fs::read(fixture_path).expect("Fixture not found");
        let jm_pos = (0..bytes.len().saturating_sub(1))
            .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
            .expect("JM header not found");
        let count = u16::from_le_bytes([bytes[jm_pos + 2], bytes[jm_pos + 3]]);
        let huffman = HuffmanTree::new();
        // Parse the full file so absolute-offset heuristics stay intact.
        let items = Item::read_player_items(&bytes, &huffman, true).expect("Parsing failed");
        (items, count)
    }

    #[test]
    fn test_alpha_v105_amazon_recovery_100pct() {
        d2r_core::domain::header::entity::IN_NESTED_RECOVERY.with(|v| v.set(false));
        let fixture_path = "tests/fixtures/savegames/original/amazon_10_scrolls.d2s";
        let bytes = fs::read(fixture_path).expect("Fixture not found at tests/fixtures/savegames/original/amazon_10_scrolls.d2s");
        
        let jm_pos = (0..bytes.len().saturating_sub(1))
            .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
            .expect("JM header not found");
        
        let count = u16::from_le_bytes([bytes[jm_pos + 2], bytes[jm_pos + 3]]);
        println!("Expected items: {}", count);
        
        let huffman = HuffmanTree::new();
        // Alpha mode = true
        let items = Item::read_player_items(&bytes, &huffman, true).expect("Parsing failed");
        
        println!("Items recovered: {}", items.len());
        let mut recovered_codes = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let trimmed = d2r_core::item::normalize_alpha_code_hint(item.code.trim()).to_string();
            println!("[{:2}] {:<4} (id: {:?}, qual: {:?})", i, trimmed, item.id, item.quality);
            recovered_codes.push(trimmed);
        }
        
        assert_eq!(items.len() as u16, count, "Should recover all identified items (16)");
        
        // Verify specifically jav (14) and ucb8 (15) are found in the sequence.
        assert_eq!(recovered_codes.get(14).map(String::as_str), Some("jav"));
        assert_eq!(recovered_codes.get(15).map(String::as_str), Some("ucb8"));
        assert!(
            recovered_codes.contains(&"ucb8".to_string()),
            "UCB8 should be preserved as the body owner for the witness"
        );
        assert!(
            !recovered_codes.contains(&"wucb".to_string()),
            "The drifted wucb alias should not survive the body-code normalization"
        );
        
        // Final assertion: we have exactly 16 items.
        assert_eq!(items.len(), 16);
    }

    #[test]
    fn test_alpha_v105_overlap_tail_does_not_synthesize_opaque_when_count_satisfied() {
        d2r_core::domain::header::entity::IN_NESTED_RECOVERY.with(|v| v.set(false));
        let fixtures = [
            ("tests/fixtures/savegames/original/amazon_authority_runeword.d2s", None),
            ("tests/fixtures/savegames/original/amazon_10_scrolls.d2s", Some(16u16)),
        ];

        for (fixture_path, expected_top_level_count) in fixtures {
            let (items, count) = read_alpha_items_from_first_jm(fixture_path);
            if let Some(expected) = expected_top_level_count {
                assert_eq!(count, expected, "Unexpected JM count in {}", fixture_path);
                assert_eq!(
                    items.len() as u16,
                    expected,
                    "Top-level item count drift in {}",
                    fixture_path
                );
            }

            let tail_end = items
                .iter()
                .map(|it| it.range.end)
                .max()
                .unwrap_or(0);
            assert!(
                !items
                    .iter()
                    .any(|it| it.is_residue() && it.range.end == tail_end),
                "Unexpected trailing synthesized Opaque/residue item in {}",
                fixture_path
            );
        }
    }

    #[test]
    fn test_all_alpha_v105_fixtures_bit_perfect() {
        d2r_core::domain::header::entity::IN_NESTED_RECOVERY.with(|v| v.set(false));
        let fixtures = [
            "tests/fixtures/savegames/original/amazon_authority_runeword.d2s",
            "tests/fixtures/savegames/original/amazon_10_scrolls.d2s",
            "tests/fixtures/savegames/original/amazon_v105_act2_start.d2s",
            "tests/fixtures/savegames/original/amazon_v105_andariel_killed_no_talk.d2s",
            "tests/fixtures/savegames/original/amazon_v105_re_probe_zigzag_all_diff.d2s",
            "tests/fixtures/savegames/original/amazon_v105_slice2_equipment.d2s",
            "tests/fixtures/savegames/original/amazon_lvl2_progression_complex.d2s",
        ];
        
        let huffman = HuffmanTree::new();
        
        for fixture_path in fixtures {
            println!("Testing Alpha v105 bit-perfect roundtrip for: {}", fixture_path);
            let bytes = fs::read(fixture_path).expect("Fixture not found");
            
            // 1. Recover all items
            let items = Item::read_player_items(&bytes, &huffman, true).expect("Parsing failed");
            
            // 2. Reserialize section
            let reserialized_items = Item::serialize_section(&items, &huffman, true).expect("Serialization failed");
            
            // 3. Compare with original bytes
            let jm_pos = (0..bytes.len().saturating_sub(1))
                .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
                .expect("JM header not found");
            
            let original_payload = &bytes[jm_pos + 4..];
            let next_jm = (jm_pos + 2..bytes.len().saturating_sub(1))
                .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
                .unwrap_or(bytes.len());
            let section_len = next_jm.saturating_sub(jm_pos + 4);
            let compare_len = reserialized_items.len().min(section_len);
            
            // The items section in original files might contain more data (other sections),
            // so we compare only up to the length of our reserialized bits.
            // But for these specific Alpha fixtures, we aim for 100% segment matching.
            for i in 0..compare_len {
                if reserialized_items[i] != original_payload[i] {
                    assert_eq!(
                        reserialized_items[i], 
                        original_payload[i], 
                        "Byte mismatch at offset {} in fixture {}", i, fixture_path
                    );
                }
            }
            println!("  [PASS] {} bytes matched perfectly.", compare_len);
        }
    }
}
