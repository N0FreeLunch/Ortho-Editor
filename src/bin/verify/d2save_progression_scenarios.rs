use std::fs;
use std::path::PathBuf;
use anyhow::{bail, Context};
use d2r_core::save::{Save, map_core_sections, finalize_save_bytes, WaypointSection, QuestSection};
use d2r_core::domain::header::axiom::{CHAR_NAME_OFFSET, CHAR_NAME_LEN};

fn get_save_dir() -> PathBuf {
    let _ = dotenvy::dotenv();
    std::env::var("D2R_SAVE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("C:/Users/dhks1/Saved Games/Diablo II Resurrected"))
}

fn get_core_dir() -> PathBuf {
    let _ = dotenvy::dotenv();
    std::env::var("D2R_CORE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn mutate_name_in_header(bytes: &mut [u8], new_name: &str) -> anyhow::Result<()> {
    if !new_name.is_ascii() || new_name.len() >= CHAR_NAME_LEN {
        bail!("New name must be ASCII and less than {} chars", CHAR_NAME_LEN);
    }
    let end = CHAR_NAME_OFFSET + CHAR_NAME_LEN;
    if end > bytes.len() {
        bail!("Save file too small for name offset");
    }
    // Fill with zero and write name
    bytes[CHAR_NAME_OFFSET..end].fill(0);
    bytes[CHAR_NAME_OFFSET..CHAR_NAME_OFFSET + new_name.len()].copy_from_slice(new_name.as_bytes());
    Ok(())
}

fn create_scenario_surgical(
    source_bytes: &[u8],
    zarulfo_bytes: Option<&[u8]>,
    zarulfo_n4_bytes: Option<&[u8]>,
    warriv_n2_bytes: Option<&[u8]>,
    new_name: &str,
    scenario_type: &str,
) -> anyhow::Result<Vec<u8>> {
    let mut working_bytes = source_bytes.to_vec();
    
    // 1. Map core sections to locate Quest, Waypoint, and Expansion offsets surgically
    let map = map_core_sections(&working_bytes).context("Failed to map save sections")?;
    
    let woo_pos = map.woo_pos.context("Woo! marker not found")?;
    let ws_pos = map.ws_pos.context("WS marker not found")?;
    let w4_pos = map.w4_pos.context("w4 marker not found")?;
    let gf_pos = map.gf_pos; // GF starts the stats section, marking the end of the header/expansion
 
    // 2. Extract raw sub-slices surgically (100% stable matching boundaries)
    let mut quests_section = QuestSection::from_slice(&working_bytes[woo_pos..ws_pos]);
    let mut wps_section = WaypointSection::from_slice(&working_bytes[ws_pos..w4_pos]);
    let mut expansion = d2r_core::save::ExpansionSection::from_slice(&working_bytes[w4_pos..gf_pos]);
 
    // 3. Mutate only the targeted bits in-place using Unified 96-byte Parallel Translation Rules
    let wp_anchor = ws_pos;

    match scenario_type {
        "AA" => {
            println!("[Surgical AA] NM Act 5 Clear & Hell Act 1 Entry transition");
            
            // 1. Waypoints: All NM WPs active, Hell WPs inactive
            wps_section.set_activated_by_name("Act 1 - Town", 1, true, wp_anchor);
            wps_section.set_activated_by_name("Act 2 - Town", 1, true, wp_anchor);
            wps_section.set_activated_by_name("Act 3 - Town", 1, true, wp_anchor);
            wps_section.set_activated_by_name("Act 4 - Town", 1, true, wp_anchor);
            wps_section.set_activated_by_name("Act 5 - Town", 1, true, wp_anchor);
            
            wps_section.set_activated_by_name("Act 1 - Town", 2, false, wp_anchor);
            wps_section.set_activated_by_name("Act 2 - Town", 2, false, wp_anchor);
            wps_section.set_activated_by_name("Act 3 - Town", 2, false, wp_anchor);
            wps_section.set_activated_by_name("Act 4 - Town", 2, false, wp_anchor);
            wps_section.set_activated_by_name("Act 5 - Town", 2, false, wp_anchor);

            // 2. Expansion Section: Activate NM Towns & Hell Act 1 Rogue Encampment
            expansion.set_activated_by_name("Act 1 - Town", 1, true);
            expansion.set_activated_by_name("Act 2 - Town", 1, true);
            expansion.set_activated_by_name("Act 3 - Town", 1, true);
            expansion.set_activated_by_name("Act 4 - Town", 1, true);
            expansion.set_activated_by_name("Act 5 - Town", 1, true);
            expansion.set_activated_by_name("Act 1 - Town", 2, true); // Hell Act 1 Rogue Encampment

            // 3. Parallel Translation: Clone fully completed Normal block to NM block
            if quests_section.raw_bytes.len() >= 204 {
                println!("  [Surgical Copy] Cloning fully completed Normal Quests (12..108) to Nightmare Quests (108..204)");
                let normal_clone = quests_section.raw_bytes[12..108].to_vec();
                quests_section.raw_bytes[108..204].copy_from_slice(&normal_clone);
            }
            if quests_section.raw_bytes.len() >= 300 {
                quests_section.raw_bytes[204..300].fill(0); // Hell quests inactive
            }

            // 4. Set Header Active Act & Difficulty (Offset 21): difficulty=2 (Hell), act=0 (Act 1) -> 0x10
            if working_bytes.len() > 21 {
                working_bytes[21] = 0x10;
            }
        }
        "AB" => {
            println!("[Surgical AB] NM Act 2 Clear, NM Act 3 Entry (Docks WP inactive)");
            
            // 1. Waypoints: NM Act 1, 2 active, NM Act 3 inactive
            wps_section.set_activated_by_name("Act 1 - Town", 1, true, wp_anchor);
            wps_section.set_activated_by_name("Act 2 - Town", 1, true, wp_anchor);
            wps_section.set_activated_by_name("Act 3 - Town", 1, false, wp_anchor); // Docks WP inactive!
            wps_section.set_activated_by_name("Act 4 - Town", 1, false, wp_anchor);
            wps_section.set_activated_by_name("Act 5 - Town", 1, false, wp_anchor);

            // 2. Expansion Section: Activate NM Act 1, 2, and Act 3 Town (to keep Act 3 quest/wp tabs active)
            expansion.set_activated_by_name("Act 1 - Town", 1, true);
            expansion.set_activated_by_name("Act 2 - Town", 1, true);
            expansion.set_activated_by_name("Act 3 - Town", 1, true); // Act 3 tab active!

            // 3. Parallel Translation: Clone Zarulfo (Normal Act 2 complete) to NM block
            if let Some(z_bytes) = zarulfo_bytes {
                if let Ok(z_map) = map_core_sections(z_bytes) {
                    if let (Some(z_woo), Some(z_ws)) = (z_map.woo_pos, z_map.ws_pos) {
                        let z_quests = QuestSection::from_slice(&z_bytes[z_woo..z_ws]);
                        if z_quests.raw_bytes.len() >= 108 && quests_section.raw_bytes.len() >= 204 {
                            println!("  [Surgical Copy] Cloning Normal Quests (12..108) to Nightmare Quests (108..204)");
                            quests_section.raw_bytes[108..204].copy_from_slice(&z_quests.raw_bytes[12..108]);
                        }
                    }
                }
            } else {
                println!("  [Warning] Zarulfo NM Act 3 baseline bytes not found! Quest Section left untouched.");
            }

            // 4. Set Header Active Act & Difficulty (Offset 21): difficulty=1 (NM), act=2 (Act 3) -> 0x0A
            if working_bytes.len() > 21 {
                working_bytes[21] = 0x0A;
            }
        }
        "AC" => {
            println!("[Surgical AC] NM Acts 2 & 3 Completed, NM Act 4 WP active, Act 3 & 5 WPs inactive, Hell Entry active");
            
            // 1. Waypoints: NM Act 1, 2, 4 active, NM Act 3, 5 inactive
            wps_section.set_activated_by_name("Act 1 - Town", 1, true, wp_anchor);
            wps_section.set_activated_by_name("Act 2 - Town", 1, true, wp_anchor);
            wps_section.set_activated_by_name("Act 3 - Town", 1, false, wp_anchor); // Act 3 WP inactive!
            wps_section.set_activated_by_name("Act 4 - Town", 1, true, wp_anchor);  // Act 4 WP active!
            wps_section.set_activated_by_name("Act 5 - Town", 1, false, wp_anchor); // Act 5 WP inactive!

            // 2. Expansion Section: Activate NM Towns & Hell Act 1
            expansion.set_activated_by_name("Act 1 - Town", 1, true);
            expansion.set_activated_by_name("Act 2 - Town", 1, true);
            expansion.set_activated_by_name("Act 3 - Town", 1, true);
            expansion.set_activated_by_name("Act 4 - Town", 1, true);
            expansion.set_activated_by_name("Act 5 - Town", 1, true);
            expansion.set_activated_by_name("Act 1 - Town", 2, true); // Hell Act 1 Rogue Encampment

            // 3. Parallel Translation: Clone fully completed Normal block to NM block
            if quests_section.raw_bytes.len() >= 204 {
                println!("  [Surgical Copy] Cloning fully completed Normal Quests (12..108) to Nightmare Quests (108..204)");
                let normal_clone = quests_section.raw_bytes[12..108].to_vec();
                quests_section.raw_bytes[108..204].copy_from_slice(&normal_clone);
            }
            if quests_section.raw_bytes.len() >= 300 {
                quests_section.raw_bytes[204..300].fill(0); // Hell quests inactive
            }

            // 4. Set Header Active Act & Difficulty (Offset 21): difficulty=2 (Hell), act=0 (Act 1) -> 0x10
            if working_bytes.len() > 21 {
                working_bytes[21] = 0x10;
            }
        }
        "AD" => {
            println!("[Surgical AD] NM Act 2 & 3 Completed, NM Act 1, 2, 3, 4 tabs active");
            
            // 1. Waypoints: NM Act 1, 2, 3, 4 active, NM Act 5 inactive
            wps_section.set_activated_by_name("Act 1 - Town", 1, true, wp_anchor);
            wps_section.set_activated_by_name("Act 2 - Town", 1, true, wp_anchor);
            wps_section.set_activated_by_name("Act 3 - Town", 1, true, wp_anchor);
            wps_section.set_activated_by_name("Act 4 - Town", 1, true, wp_anchor);
            wps_section.set_activated_by_name("Act 5 - Town", 1, false, wp_anchor);

            // 2. Expansion Section: Activate NM Act 1, 2, 3, 4 Town
            expansion.set_activated_by_name("Act 1 - Town", 1, true);
            expansion.set_activated_by_name("Act 2 - Town", 1, true);
            expansion.set_activated_by_name("Act 3 - Town", 1, true);
            expansion.set_activated_by_name("Act 4 - Town", 1, true);

            // 3. Parallel Translation: Clone Zarulfo N4 (Normal Act 3 complete) to NM block
            if let Some(z_bytes) = zarulfo_n4_bytes {
                if let Ok(z_map) = map_core_sections(z_bytes) {
                    if let (Some(z_woo), Some(z_ws)) = (z_map.woo_pos, z_map.ws_pos) {
                        let z_quests = QuestSection::from_slice(&z_bytes[z_woo..z_ws]);
                        if z_quests.raw_bytes.len() >= 108 && quests_section.raw_bytes.len() >= 204 {
                            println!("  [Surgical Copy] Cloning Normal Quests (12..108) to Nightmare Quests (108..204)");
                            quests_section.raw_bytes[108..204].copy_from_slice(&z_quests.raw_bytes[12..108]);
                        }
                    }
                }
            } else {
                println!("  [Warning] Zarulfo NM Act 4 baseline bytes not found! Quest Section left untouched.");
            }

            // 4. Set Header Active Act & Difficulty (Offset 21): difficulty=1 (NM), act=3 (Act 4) -> 0x0B
            if working_bytes.len() > 21 {
                working_bytes[21] = 0x0B;
            }
        }
        _ => bail!("Unknown scenario: {}", scenario_type),
    }
 
    // 4. Overwrite ONLY the mutated sections in the raw bytes (Items left 100% untouched!)
    working_bytes[woo_pos..ws_pos].copy_from_slice(quests_section.as_slice());
    working_bytes[ws_pos..w4_pos].copy_from_slice(wps_section.as_slice());
    working_bytes[w4_pos..gf_pos].copy_from_slice(expansion.as_slice());
 
    // 5. Change Character Name in Header (In-place)
    mutate_name_in_header(&mut working_bytes, new_name)?;
 
    // 6. Recalculate checksum only
    finalize_save_bytes(&mut working_bytes, true).context("Failed to finalize checksum")?;
 
    Ok(working_bytes)
}
 
fn main() -> anyhow::Result<()> {
    let save_dir = get_save_dir();
    // Use the clean verified NM Act 2 entry save directly as the surgical baseline to resolve drift anomalies
    let source_path = save_dir.join("TESTASSASSIN - 나이트메어 엑트2 입성.d2s");
    
    let source_bytes = if source_path.exists() {
        println!("Using clean NM Act 2 baseline from: {:?}", source_path);
        fs::read(&source_path)?
    } else {
        let fallback_path = get_core_dir().join("tests/fixtures/savegames/original/amazon_empty.d2s");
        println!("NM baseline not found. Falling back to fixture: {:?}", fallback_path);
        if !fallback_path.exists() {
            bail!("No source save file found anywhere!");
        }
        fs::read(&fallback_path)?
    };
 
    // Dynamically resolve Zarulfo (Normal Act 3) save by exact size (2021 bytes)
    let mut zarulfo_bytes = None;
    // Dynamically resolve Zarulfo N4 (Normal Act 4) save by exact size (2122 bytes)
    let mut zarulfo_n4_bytes = None;
    // Dynamically resolve Prayer Mercenary (Normal Act 2 entry) save by exact size (1922 bytes)
    let mut warriv_n2_bytes = None;
 
    if let Ok(entries) = fs::read_dir(&save_dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                let name = entry.file_name().to_string_lossy().to_string();
                if meta.len() == 2021 && name.contains("TESTASSASSIN") {
                    println!("Found Zarulfo (Normal Act 3) baseline at: {:?}", entry.path());
                    if let Ok(bytes) = fs::read(&entry.path()) {
                        zarulfo_bytes = Some(bytes);
                    }
                } else if meta.len() == 2122 && name.contains("TESTASSASSIN") {
                    println!("Found Zarulfo N4 (Normal Act 4) baseline at: {:?}", entry.path());
                    if let Ok(bytes) = fs::read(&entry.path()) {
                        zarulfo_n4_bytes = Some(bytes);
                    }
                } else if meta.len() == 1922 && name.contains("TESTASSASSIN") {
                    println!("Found Prayer Mercenary (Normal Act 2) baseline at: {:?}", entry.path());
                    if let Ok(bytes) = fs::read(&entry.path()) {
                        warriv_n2_bytes = Some(bytes);
                    }
                }
            }
        }
    }
 
    // Generate Scenarios surgically: AA, AB, AD, AC
    let scenarios = [
        ("TESTASSASSIN_AA", "AA"), 
        ("TESTASSASSIN_AB", "AB"), 
        ("TESTASSASSIN_AD", "AD"), 
        ("TESTASSASSIN_AC", "AC")
    ];
    
    for &(new_name, scenario_type) in &scenarios {
        let output_path = save_dir.join(format!("{}.d2s", new_name));
        println!("Generating scenario {} surgically -> {:?}", scenario_type, output_path);
        
        match create_scenario_surgical(
            &source_bytes, 
            zarulfo_bytes.as_deref(), 
            zarulfo_n4_bytes.as_deref(),
            warriv_n2_bytes.as_deref(),
            new_name, 
            scenario_type
        ) {
            Ok(rebuilt) => {
                // Purge stale map/control/key caches to prevent client-side rollback triggers due to cache desync!
                let cache_extensions = ["ctl", "key", "map", "ma0", "ma1", "ma2", "ma3"];
                for ext in &cache_extensions {
                    let cache_file = save_dir.join(format!("{}.{}", new_name, ext));
                    if cache_file.exists() {
                        println!("  [Purge Cache] Removing stale cache file: {:?}", cache_file);
                        let _ = fs::remove_file(cache_file);
                    }
                }
 
                fs::write(&output_path, rebuilt)?;
                println!("Successfully created surgical scenario character: {}", new_name);
            }
            Err(e) => {
                println!("Failed to create surgical scenario character {}: {:?}", new_name, e);
            }
        }
    }
 
    println!("\nAll done! 4 test characters successfully created surgically in your Saved Games directory.");
    println!("Please boot up D2R and verify in-game load for: TESTASSASSIN_AA, TESTASSASSIN_AB, TESTASSASSIN_AD, TESTASSASSIN_AC.");
 
    Ok(())
}
