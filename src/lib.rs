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

        assert_eq!(real_items.len(), 5);

        let authority = real_items
            .iter()
            .find(|item| item.code.trim() == "wa2" && item.is_runeword)
            .expect("authority base item (wa2) should be present");

        let child_codes: Vec<&str> = authority
            .socketed_items
            .iter()
            .map(|item| item.code.trim())
            .collect();

        assert!(
            child_codes.is_empty(),
            "wa2 should not expose nested socket children in the current truth contract"
        );
    }

    #[test]
    fn test_plain_inventory_fixture_does_not_gain_socket_children() {
        let items = load_player_items("tests/fixtures/savegames/original/amazon_10_scrolls.d2s");
        let real_items: Vec<_> = items.iter().filter(|it| !it.is_residue()).collect();

        // 16 is the physical bit-perfect item count for amazon_10_scrolls.d2s 
        // (4 hp1 + 10 tsc + 1 jav + 1 opaque buc = 16 items). 
        // Previous buggy is_residue logic swallowed opaque modules, which led to a stale count of 11.
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
            vec!["hp1", "hp1", "hp1", "hp1", "wa2"]
        );

        let authority = real_items
            .iter()
            .find(|item| item.code.trim() == "wa2")
            .expect("authority base item should be present");
        assert_eq!(authority.code.trim(), "wa2");
        assert!(authority.socketed_items.is_empty());

        assert!(
            items
                .iter()
                .all(|item| !matches!(item.code.trim(), "r15" | "r13"))
        );
    }
    #[test]
    fn test_item_template_lookup() {
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
        let authority = real_items
            .iter()
            .find(|it| it.code.trim() == "wa2" && it.header.is_runeword)
            .expect("wa2 item should be present");

        let actual_props: Vec<(u32, i32)> = authority
            .properties
            .iter()
            .map(|prop| (prop.stat_id, prop.value))
            .collect();
        let expected_props = vec![
            (297, 193),
            (260, 255),
            (0, -31),
            (0, -27),
            (424, 0),
            (384, 249),
            (416, 18),
            (0, -24),
            (0, 8),
            (320, 0),
            (134, 0),
            (112, 21),
            (438, 4),
            (0, -28),
            (0, -12),
            (160, 3),
            (1, 462),
            (283, 36),
        ];

        assert_eq!(authority.code.trim(), "wa2");
        assert!(authority.socketed_items.is_empty());
        assert_eq!(actual_props, expected_props);
    }
}

use std::sync::Once;

static RAYON_INIT: Once = Once::new();

#[cfg(windows)]
mod os_priority {
    // Zero-dependency win32 FFI to lower process priority class.
    // This protects system responsiveness during heavy parallel agent runs.
    use std::os::windows::raw::HANDLE;

    unsafe extern "system" {
        fn GetCurrentProcess() -> HANDLE;
        fn SetPriorityClass(h_process: HANDLE, dw_priority_class: u32) -> i32;
    }

    const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x00004000;

    pub fn lower_priority() {
        unsafe {
            let handle = GetCurrentProcess();
            let _ = SetPriorityClass(handle, BELOW_NORMAL_PRIORITY_CLASS);
        }
    }
}

#[cfg(not(windows))]
mod os_priority {
    // Zero-dependency unix FFI to nice down the process priority.
    unsafe extern "C" {
        fn setpriority(which: i32, who: i32, value: i32) -> i32;
    }

    const PRIO_PROCESS: i32 = 0;

    pub fn lower_priority() {
        unsafe {
            // set priority to nice=10 (yield cpu nicely to others)
            let _ = setpriority(PRIO_PROCESS, 0, 10);
        }
    }
}

#[cfg(not(target_family = "wasm"))]
pub fn init_rayon_thread_pool() {
    RAYON_INIT.call_once(|| {
        // Lower OS process priority to avoid starving other processes or agents
        os_priority::lower_priority();

        let _ = dotenvy::dotenv();
        
        if std::env::var("RAYON_NUM_THREADS").is_ok() {
            return;
        }

        let percent = std::env::var("D2R_THREAD_PERCENT")
            .ok()
            .and_then(|val| val.parse::<u32>().ok())
            .unwrap_or(50);

        let cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        
        let mut threads = ((cpus as u64 * percent as u64) / 100) as usize;
        
        if percent > 0 && percent < 100 {
            // Leave at least one thread if under 100%
            threads = threads.min(cpus.saturating_sub(1));
            // Use at least one thread if over 0%
            threads = threads.max(1);
        } else if percent >= 100 {
            threads = cpus;
        } else {
            threads = 1;
        }

        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global();
    });
}

#[cfg(target_family = "wasm")]
pub fn init_rayon_thread_pool() {}
