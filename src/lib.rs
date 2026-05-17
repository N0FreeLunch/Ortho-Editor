// This software is licensed under the PolyForm Noncommercial License 1.0.0.
// Required Notice: Copyright 2026 N0FreeLunch (https://github.com/N0FreeLunch/d2r-core)

pub mod data;
pub mod domain;
pub mod engine;
pub mod error;
pub mod inventory;
pub mod item;
pub mod save;
pub mod spec;
pub mod algo;
pub mod report;
pub mod verify;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::HuffmanTree;
    use crate::item::Item;
    use std::fs;
    use std::path::PathBuf;

    fn repo_path(relative: &str) -> PathBuf {
        let _ = dotenvy::dotenv();
        let base = std::env::var("D2R_CORE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        base.join(relative)
    }

    fn load_player_items(relative: &str) -> Vec<Item> {
        let bytes = fs::read(repo_path(relative)).expect("fixture should be readable");
        let huffman = HuffmanTree::new();
        let version_le = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        // Alpha v105 is file version 6 (sometimes) or 105 (in others). 
        // Our amazon_authority_runeword.d2s has 0x69 (105).
        let is_alpha = version_le == 6 || version_le == 105;
        Item::read_player_items(&bytes, &huffman, is_alpha).expect("item parse should succeed")
    }

    #[test]
    fn test_load_dlc_spec() {
        let _ = dotenvy::dotenv();
        let spec_path = std::env::var("D2R_SPEC_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| repo_path("../d2r-spec"));
        
        let yaml_path = spec_path.join("specification/v2_dlc_spec.yaml");
        let yaml_str = fs::read_to_string(yaml_path).expect("Should have been able to read the file");
        let spec: spec::DlcSpec = serde_yaml::from_str(&yaml_str).expect("Failed to parse YAML");

        assert_eq!(spec.name, "Reign of the Demonologist");
        assert_eq!(spec.character_classes[0].name, "Warlock");
        assert_eq!(spec.character_classes[0].id, 7);
        assert_eq!(spec.character_classes[0].skills.len(), 30);
    }

    #[test]
    fn test_runeword_socket_children_are_recovered() {
        let items =
            load_player_items("tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
        let real_items: Vec<_> = items.iter().filter(|it| !it.is_residue()).collect();

        assert_eq!(real_items.len(), 6);

        let authority = real_items
            .iter()
            .find(|item| item.code.trim() == "xrs" && item.is_runeword)
            .expect("authority base item (xrs) should be present");

        let child_codes: Vec<&str> = authority
            .socketed_items
            .iter()
            .map(|item| item.code.trim())
            .collect();

        assert_eq!(child_codes, vec!["r15", "r13", "r08"]);
    }

    #[test]
    fn test_plain_inventory_fixture_does_not_gain_socket_children() {
        let items = load_player_items("tests/fixtures/savegames/original/amazon_10_scrolls.d2s");
        let real_items: Vec<_> = items.iter().filter(|it| !it.is_residue()).collect();

        assert_eq!(real_items.len(), 16);
        assert!(real_items.iter().all(|item| item.socketed_items.is_empty()));
    }

    #[test]
    fn test_authority_runeword_children_stay_nested_with_expected_modes() {
        let items =
            load_player_items("tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
        let real_items: Vec<_> = items.iter().filter(|it| !it.is_residue()).collect();

        let top_level_codes: Vec<&str> = real_items.iter().map(|item| item.code.trim()).collect();
        assert_eq!(
            top_level_codes,
            vec!["hp1", "hp1", "hp1", "hp1", "xrs", "xrs"]
        );

        let authority = real_items.last().expect("authority base item should be last");
        assert_eq!(authority.code.trim(), "xrs");

        let child_summaries: Vec<(&str, u8)> = authority
            .socketed_items
            .iter()
            .map(|item| (item.code.trim(), item.mode))
            .collect();
        assert_eq!(child_summaries, vec![("r15", 6), ("r13", 6), ("r08", 6)]);

        assert!(
            items
                .iter()
                .all(|item| !matches!(item.code.trim(), "r15" | "r13" | "r08"))
        );
    }
    #[test]
    fn test_item_template_lookup() {
        use crate::item::Item;
        // This is a bit of a hack since item_template is private, 
        // but we can test it indirectly or make it pub(crate).
        // For now, let's just check if ITEM_TEMPLATES has what we need.
        let templates = crate::data::item_codes::ITEM_TEMPLATES;
        assert!(templates.iter().any(|t| t.code == "hp1"), "hp1 should be in templates");
        assert!(templates.iter().any(|t| t.code == "xrs"), "xrs should be in templates");
        assert!(templates.iter().any(|t| t.code == "r15"), "r15 should be in templates");
    }
    #[test]
    fn test_calc_alvl() {
        use crate::data::legitimacy::calc_alvl;
        // ilvl=50, qlvl=30, magic_lvl=0 -> temp=50, 50 < 99-30/2=84, alvl=50-15=35
        assert_eq!(calc_alvl(50, 30, 0), 35);
        // High level case: ilvl=99, qlvl=30, magic_lvl=0 -> temp=99, 99 >= 84, alvl=2*99-99=99
        assert_eq!(calc_alvl(99, 30, 0), 99);
        // Magic level case: ilvl=50, qlvl=30, magic_lvl=10 -> temp=50, alvl=50+10=60
        assert_eq!(calc_alvl(50, 30, 10), 60);
    }
    #[test]
    fn test_authority_properties_match_fuzzer_truth() {
        let items = load_player_items("tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
        let real_items: Vec<_> = items.iter().filter(|it| !it.is_residue()).collect();
        let truth_json = fs::read_to_string(repo_path("tests/fixtures/savegames/original/amazon_authority_runeword_truth.json"))
            .expect("truth file should exist");
        
        let truth: serde_json::Value = serde_json::from_str(&truth_json).expect("truth should be valid JSON");
        
        // Find the xrs item (Authority base)
        let xrs = real_items.iter().find(|it| it.code.trim() == "xrs" && it.is_runeword).expect("xrs item should be present");
        
        let truth_props = truth["properties"].as_array().expect("properties should be array");
        
        fn map_alpha_id(id: u32) -> u32 {
            match id {
                26 => 31,
                312 => 72,
                207 => 73,
                380 => 194,
                256 => 127,
                496 => 99,
                499 => 16,
                289 => 9,
                _ => id,
            }
        }

        for (i, p_truth) in truth_props.iter().enumerate() {
            let raw_id = p_truth["stat_id"].as_u64().unwrap() as u32;
            let expected_id = map_alpha_id(raw_id);
            let expected_val = p_truth["value"].as_u64().unwrap() as i32;
            
            let actual_prop = &xrs.properties[i];
            
            assert_eq!(actual_prop.stat_id, expected_id, "Stat ID mismatch at property index {}", i);
            assert_eq!(actual_prop.raw_value, expected_val, "Value mismatch at index {}", i);
        }
    }
}
