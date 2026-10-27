//! Ortho Editor Core CLI Adapter for applying mutations and inspecting saves.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::exit;

use serde::Serialize;
use d2r_core::domain::contracts::{
    CharacterMutationContract, CharacterWireDto, NumericOp, ValidationErrorResponse,
    WaypointMutationContract, WaypointWireDto,
};
use d2r_core::item::HuffmanTree;
use d2r_core::save::{try_apply_character_mutation, try_apply_waypoint_mutation, Save};

#[derive(Debug, Serialize)]
struct MutationCliReport {
    status: String,
    input_file: String,
    output_file: Option<String>,
    dry_run: bool,
    character_name: Option<String>,
    character_level: Option<u8>,
    strength: Option<u32>,
    dexterity: Option<u32>,
    vitality: Option<u32>,
    energy: Option<u32>,
    stat_points: Option<u32>,
    gold_stash: Option<u32>,
    waypoints_unlocked: Option<bool>,
    new_checksum: Option<String>,
    error: Option<String>,
}

fn print_usage() {
    eprintln!(
        r#"Ortho Editor CLI - Save and Item Mutation Engine

Usage: ortho <save_file> [OPTIONS]

Options:
    --name <string>        Set character name (2-15 chars)
    --level <1-99>         Set character level
    --strength <1-1023>    Set base strength
    --dexterity <1-1023>   Set base dexterity
    --vitality <1-1023>    Set base vitality
    --energy <1-1023>      Set base energy
    --stat-points <0-1023> Set unspent stat points
    --gold-stash <0-2.5m>  Set private stash gold
    --gold-char <uint>     Set inventory gold
    --unlock-all-wps       Unlock all waypoints for selected or all difficulties
    --difficulty <0-2>     Select difficulty (0: Normal, 1: Nightmare, 2: Hell)
    --activate-wp <name>   Activate a specific waypoint by name
    --out <path>           Output path (default: <input>.mutated.d2s to protect original)
    --force-overwrite      Explicitly allow overwriting input file in-place
    --dry-run              Simulate mutation without writing to disk
    --json                 Output results as JSON
    -h, --help             Show this help message
"#
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 || args.contains(&"-h".to_string()) || args.contains(&"--help".to_string()) {
        print_usage();
        return;
    }

    let mut input_file: Option<PathBuf> = None;
    let mut out_file: Option<PathBuf> = None;
    let mut dry_run = false;
    let mut json_mode = false;
    let mut force_overwrite = false;

    let mut wire_dto = CharacterWireDto::default();
    let mut wp_dto = WaypointWireDto::default();
    let mut mutate_waypoints = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--dry-run" => dry_run = true,
            "--json" => json_mode = true,
            "--force-overwrite" => force_overwrite = true,
            "--unlock-all-wps" => {
                wp_dto.unlock_all = true;
                mutate_waypoints = true;
            }
            "--difficulty" => {
                i += 1;
                if i < args.len() {
                    wp_dto.difficulty = args[i].parse::<u8>().ok();
                    mutate_waypoints = true;
                }
            }
            "--activate-wp" => {
                i += 1;
                if i < args.len() {
                    wp_dto.activate_waypoints.push(args[i].clone());
                    mutate_waypoints = true;
                }
            }
            "--out" => {
                i += 1;
                if i < args.len() {
                    out_file = Some(PathBuf::from(&args[i]));
                }
            }
            "--name" => {
                i += 1;
                if i < args.len() {
                    wire_dto.name = Some(args[i].clone());
                }
            }
            "--level" => {
                i += 1;
                if i < args.len() {
                    wire_dto.level = args[i].parse::<NumericOp<u8>>().ok();
                }
            }
            "--strength" => {
                i += 1;
                if i < args.len() {
                    wire_dto.strength = args[i].parse::<NumericOp<u32>>().ok();
                }
            }
            "--dexterity" => {
                i += 1;
                if i < args.len() {
                    wire_dto.dexterity = args[i].parse::<NumericOp<u32>>().ok();
                }
            }
            "--vitality" => {
                i += 1;
                if i < args.len() {
                    wire_dto.vitality = args[i].parse::<NumericOp<u32>>().ok();
                }
            }
            "--energy" => {
                i += 1;
                if i < args.len() {
                    wire_dto.energy = args[i].parse::<NumericOp<u32>>().ok();
                }
            }
            "--stat-points" => {
                i += 1;
                if i < args.len() {
                    wire_dto.stat_points = args[i].parse::<NumericOp<u32>>().ok();
                }
            }
            "--gold-stash" => {
                i += 1;
                if i < args.len() {
                    wire_dto.gold_stash = args[i].parse::<NumericOp<u32>>().ok();
                }
            }
            "--gold-char" => {
                i += 1;
                if i < args.len() {
                    wire_dto.gold_character = args[i].parse::<NumericOp<u32>>().ok();
                }
            }
            arg if !arg.starts_with("--") && input_file.is_none() => {
                input_file = Some(PathBuf::from(arg));
            }
            other => {
                eprintln!("Unknown argument: {}", other);
                exit(1);
            }
        }
        i += 1;
    }

    let input_path = match input_file {
        Some(p) => p,
        None => {
            eprintln!("Error: input save file path is required");
            exit(1);
        }
    };

    let target_out_path = match out_file {
        Some(p) => p,
        None => {
            // Default: generate a safe non-destructive output path to protect original files
            let stem = input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("save");
            let parent = input_path.parent().unwrap_or_else(|| Path::new("."));
            parent.join(format!("{}.mutated.d2s", stem))
        }
    };

    if target_out_path == input_path && !force_overwrite && !dry_run {
        eprintln!(
            "Error: Target output path is identical to input path ('{}').\n\
            To protect version-controlled original saves, specify an output file via --out <path>,\n\
            or explicitly pass --force-overwrite to modify the file in-place.",
            input_path.display()
        );
        exit(1);
    }

    // 1. Convert Wire DTOs to domain contracts ("Parse, don't validate")
    let char_contract = match CharacterMutationContract::try_from(wire_dto) {
        Ok(c) => c,
        Err(err) => {
            if json_mode {
                let err_resp = ValidationErrorResponse::from(err);
                println!("{}", serde_json::to_string_pretty(&err_resp).unwrap());
            } else {
                eprintln!("Boundary validation error: {}", err);
            }
            exit(1);
        }
    };

    let wp_contract = if mutate_waypoints {
        match WaypointMutationContract::try_from(wp_dto.clone()) {
            Ok(c) => Some(c),
            Err(err) => {
                if json_mode {
                    let err_resp = ValidationErrorResponse::from(err);
                    println!("{}", serde_json::to_string_pretty(&err_resp).unwrap());
                } else {
                    eprintln!("Waypoint validation error: {}", err);
                }
                exit(1);
            }
        }
    } else {
        None
    };

    // 2. Read source save file
    let save_bytes = match fs::read(&input_path) {
        Ok(b) => b,
        Err(err) => {
            eprintln!("Failed to read input file '{}': {}", input_path.display(), err);
            exit(1);
        }
    };

    // 3. Apply mutation contracts sequentially
    let huffman = HuffmanTree::new();
    let after_char_bytes = match try_apply_character_mutation(&save_bytes, &char_contract, &huffman) {
        Ok(b) => b,
        Err(err) => {
            if json_mode {
                let err_resp = ValidationErrorResponse::from(err);
                println!("{}", serde_json::to_string_pretty(&err_resp).unwrap());
            } else {
                eprintln!("Failed to apply character mutation: {}", err);
            }
            exit(1);
        }
    };

    let mutated_bytes = if let Some(ref wpc) = wp_contract {
        match try_apply_waypoint_mutation(&after_char_bytes, wpc) {
            Ok(b) => b,
            Err(err) => {
                if json_mode {
                    let err_resp = ValidationErrorResponse::from(err);
                    println!("{}", serde_json::to_string_pretty(&err_resp).unwrap());
                } else {
                    eprintln!("Failed to apply waypoint mutation: {}", err);
                }
                exit(1);
            }
        }
    } else {
        after_char_bytes
    };

    // 4. Parse modified save for reporting and verification
    let modified_save = match Save::from_bytes(&mutated_bytes) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("Corrupted save generated: {}", err);
            exit(1);
        }
    };

    // 5. Write to output if not in dry-run mode
    if !dry_run {
        if let Err(err) = fs::write(&target_out_path, &mutated_bytes) {
            eprintln!("Failed to write mutated save to '{}': {}", target_out_path.display(), err);
            exit(1);
        }
    }

    // 6. Output report
    if json_mode {
        let map = d2r_core::save::map_core_sections(&mutated_bytes).ok();
        let attrs = map.and_then(|m| d2r_core::domain::stats::AttributeSection::parse(&mutated_bytes, m.gf_pos, m.if_pos).ok());
        let version = u32::from_le_bytes(mutated_bytes[4..8].try_into().unwrap_or([0; 4]));
        let is_alpha = version == 105;

        let final_str = attrs.as_ref().and_then(|a| a.actual_value(0, is_alpha)).map(|v| v as u32);
        let final_dex = attrs.as_ref().and_then(|a| a.actual_value(2, is_alpha)).map(|v| v as u32);
        let final_vit = attrs.as_ref().and_then(|a| a.actual_value(3, is_alpha)).map(|v| v as u32);
        let final_enr = attrs.as_ref().and_then(|a| a.actual_value(1, is_alpha)).map(|v| v as u32);
        let final_pts = attrs.as_ref().and_then(|a| a.actual_value(4, is_alpha)).map(|v| v as u32);
        let final_gold_stash = attrs.as_ref().and_then(|a| a.actual_value(15, is_alpha)).map(|v| v as u32);

        let report = MutationCliReport {
            status: "success".to_string(),
            input_file: input_path.display().to_string(),
            output_file: Some(target_out_path.display().to_string()),
            dry_run,
            character_name: Some(modified_save.header.char_name),
            character_level: Some(modified_save.header.char_level),
            strength: final_str,
            dexterity: final_dex,
            vitality: final_vit,
            energy: final_enr,
            stat_points: final_pts,
            gold_stash: final_gold_stash,
            waypoints_unlocked: if mutate_waypoints { Some(wp_dto.unlock_all) } else { None },
            new_checksum: Some(format!("{:#010X}", modified_save.header.checksum)),
            error: None,
        };
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("Mutation successfully applied!");
        println!("  File: {}", target_out_path.display());
        println!("  Name: {}", modified_save.header.char_name);
        println!("  Level: {}", modified_save.header.char_level);
        println!("  Checksum: {:#010X}", modified_save.header.checksum);
        if dry_run {
            println!("  [DRY-RUN] No bytes written to disk.");
        }
    }
}
