use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{bail, Context, Result};
use serde_json::json;
use d2r_core::domain::header::axiom::{CHAR_NAME_OFFSET, CHAR_NAME_LEN};
use d2r_core::engine::checksum::Checksum;
use d2r_core::save::{Save, finalize_save_bytes};

fn get_save_dir() -> Option<PathBuf> {
    let _ = dotenvy::dotenv();
    std::env::var("D2R_SAVE_DIR").ok().map(PathBuf::from)
}

fn mutate_name_in_header(bytes: &mut [u8], new_name: &str) -> Result<()> {
    if !new_name.is_ascii() || new_name.len() >= CHAR_NAME_LEN {
        bail!("New name must be ASCII and less than {} chars", CHAR_NAME_LEN);
    }
    let end = CHAR_NAME_OFFSET + CHAR_NAME_LEN;
    if end > bytes.len() {
        bail!("Save file too small for name offset");
    }
    bytes[CHAR_NAME_OFFSET..end].fill(0);
    bytes[CHAR_NAME_OFFSET..CHAR_NAME_OFFSET + new_name.len()].copy_from_slice(new_name.as_bytes());
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut source_path = "tests/fixtures/savegames/gameplay/normal/act4/TESTDRUID_Act4_HellforgeComplete.d2s".to_string();
    let mut output_path = "tests/fixtures/savegames/gameplay/normal/act4/ProbeTester.d2s".to_string();
    let mut char_name = "ProbeTester".to_string();
    let mut copy_to_live_dir = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--source" => {
                if i + 1 < args.len() {
                    source_path = args[i + 1].clone();
                    i += 1;
                }
            }
            "--output" => {
                if i + 1 < args.len() {
                    output_path = args[i + 1].clone();
                    i += 1;
                }
            }
            "--char-name" => {
                if i + 1 < args.len() {
                    char_name = args[i + 1].clone();
                    i += 1;
                }
            }
            "--copy-to-save-dir" => {
                copy_to_live_dir = true;
            }
            "--help" | "-h" => {
                println!("Usage: d2save_oracle_mutate [options]");
                println!("  --source <path>        Source save file (default: TESTDRUID_Act4_HellforgeComplete.d2s)");
                println!("  --output <path>        Output save file (default: ProbeTester.d2s)");
                println!("  --char-name <name>     Target character name (default: ProbeTester)");
                println!("  --copy-to-save-dir     Also copy generated save into D2R_SAVE_DIR if configured");
                return Ok(());
            }
            _ => {}
        }
        i += 1;
    }

    println!("[Oracle Mutator] Loading base save: {}", source_path);
    let mut working_bytes = fs::read(&source_path)
        .with_context(|| format!("Failed to read source save: {}", source_path))?;

    let original_len = working_bytes.len();
    println!("  Original byte length: {}", original_len);

    // 1. Mutate character name in header (offset 299)
    mutate_name_in_header(&mut working_bytes, &char_name)?;
    println!("  Updated character name at offset {} to '{}'", CHAR_NAME_OFFSET, char_name);

    // 2. Finalize save bytes and recalculate checksum
    finalize_save_bytes(&mut working_bytes, true)
        .context("Failed to finalize save bytes / recalculate checksum")?;
    
    // Explicit rolling checksum verification
    let mut sum_bytes = working_bytes.clone();
    Checksum::fix(&mut sum_bytes);
    working_bytes[12..16].copy_from_slice(&sum_bytes[12..16]);

    // 3. Verify parseability with library
    let parsed_save = Save::from_bytes(&working_bytes)
        .context("Failed to parse newly generated oracle save bytes with Save::from_bytes")?;
    println!("  Verified: Parsed character name from mutated save: '{}'", parsed_save.header.char_name);

    // 4. Write output file
    if let Some(parent) = Path::new(&output_path).parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(&output_path, &working_bytes)
        .with_context(|| format!("Failed to write output save: {}", output_path))?;
    println!("[OK] Generated Oracle Witness Save: {} ({} bytes)", output_path, working_bytes.len());

    // 5. Optionally copy to live D2R save directory if configured
    if copy_to_live_dir {
        if let Some(save_dir) = get_save_dir() {
            if save_dir.exists() {
                let target_file = save_dir.join(format!("{}.d2s", char_name));
                fs::write(&target_file, &working_bytes)
                    .with_context(|| format!("Failed to copy to live save dir: {:?}", target_file))?;
                println!("  [Live Witness] Copied to live save game dir: {:?}", target_file);
            }
        }
    }

    // 6. Write manifest
    let manifest_path = "agent_artifacts/2026-09-15-4001-cycle/probe-tester-oracle.json";
    if let Some(parent) = Path::new(manifest_path).parent() {
        fs::create_dir_all(parent).ok();
    }
    let manifest = json!({
        "status": "ready",
        "source": source_path,
        "output": output_path,
        "character_name": char_name,
        "target_fixture_suite": "Alpha v105 Normal Gameplay",
        "carrier_items": [
            {
                "section_item_index": 13,
                "item_code": "rin",
                "property": {
                    "name": "item_fastercastrate",
                    "value": 10,
                    "stat_id": 105,
                    "stream_offset_absolute": 8944
                }
            },
            {
                "section_item_index": 11,
                "item_code": "amu",
                "property": {
                    "name": "slotted_alignment_envelope",
                    "tail_bits": 11,
                    "stream_offset_absolute": 8792
                }
            },
            {
                "section_item_index": 8,
                "item_code": "bst",
                "property": {
                    "name": "inventory_grid_soft_truncate",
                    "grid_coord": [3, 0],
                    "stream_offset_absolute": 8491
                }
            }
        ],
        "file_size": working_bytes.len(),
        "checksum": format!("0x{:08X}", u32::from_le_bytes(working_bytes[12..16].try_into().unwrap_or([0; 4])))
    });
    fs::write(manifest_path, serde_json::to_string_pretty(&manifest)?)?;
    println!("  Manifest written to: {}", manifest_path);

    Ok(())
}