//! Ortho Editor Core CLI Adapter for applying mutations and inspecting saves.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::exit;

use serde::Serialize;
use d2r_core::domain::contracts::{
    BatchItemWireDto, CharacterWireDto, ItemStatOpWireDto, ItemWireDto, NumericOp,
    SaveMutationDto, SaveMutationPipeline, SaveMutationValidator, SkillOpWireDto, SkillWireDto,
    WaypointWireDto,
};
use d2r_core::item::HuffmanTree;
use d2r_core::save::Save;

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
    skills_mutated: Option<bool>,
    respec_applied: Option<bool>,
    item_mutated: Option<bool>,
    batch_operations: Option<Vec<String>>,
    new_checksum: Option<String>,
    error: Option<String>,
}

fn print_usage() {
    eprintln!(
        r#"Ortho Editor CLI - Save and Item Mutation Engine

Usage: ortho <save_file> [OPTIONS]

Payload Option:
    --payload <path|json>  Load entire SaveMutationDto from JSON file or literal JSON string

Character Options:
    --name <string>        Set character name (2-15 chars)
    --level <1-99>         Set character level
    --strength <1-1023>    Set base strength
    --dexterity <1-1023>   Set base dexterity
    --vitality <1-1023>    Set base vitality
    --energy <1-1023>      Set base energy
    --stat-points <0-1023> Set unspent stat points
    --gold-stash <0-2.5m>  Set private stash gold
    --gold-char <uint>     Set inventory gold

Waypoint Options:
    --unlock-all-wps       Unlock all waypoints for selected or all difficulties
    --difficulty <0-2>     Select difficulty (0: Normal, 1: Nightmare, 2: Hell)
    --activate-wp <name>   Activate a specific waypoint by name

Skill & Respec Options:
    --skill <id:op>        Set/shift skill level (e.g. 54:=20, 64:+1, 36:=0)
    --skill-points <op>    Set/shift unspent skill points (stat 5)
    --respec               Reset all 30 class skills to 0 and refund all spent points

Batch Item Options:
    --repair-all           Fully repair current durability on all items
    --identify-all         Identify all unidentified items
    --socket-all <count>   Add sockets to eligible unsocketed items (up to base/ilvl limit)

Single Item Options:
    --item-index <uint>    Target item by 0-based index
    --item-code <string>   Target item by 3-letter code (e.g. amu, rin, mau)
    --item-slot <string>   Target item by slot (e.g. head, neck, torso)
    --item-defense <val>   Set or shift item defense (e.g. 250, +50, -10)
    --item-durability <v>  Set or shift current durability (e.g. 50, +10)
    --item-max-durability  Set or shift max durability
    --item-quantity <val>  Set or shift item stack quantity
    --item-sockets <0-6>   Set item socket count
    --item-ethereal <bool> Set ethereal status (true/false)
    --item-identified <b>  Set identified status (true/false)
    --item-ed <op>         Set or shift enhanced damage % (e.g. 150, +30)
    --item-min-damage <op> Set or shift flat min damage (e.g. 15, +5)
    --item-max-damage <op> Set or shift flat max damage (e.g. 30, +10)
    --item-rep-dur <op>    Set or shift replenish durability rate (e.g. 3)
    --item-rep-quant <op>  Set or shift replenish quantity rate (e.g. 1)
    --item-indestructible  Set indestructible flag (true/false)
    --item-stat <id:op>    Set/shift stat property (e.g. 9:+20, 127:=2, 31:-5)

General Options:
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
    let mut payload_arg: Option<String> = None;
    let mut dry_run = false;
    let mut json_mode = false;
    let mut force_overwrite = false;

    let mut wire_dto = CharacterWireDto::default();
    let mut mutate_character = false;

    let mut wp_dto = WaypointWireDto::default();
    let mut mutate_waypoints = false;

    let mut skill_dto = SkillWireDto::default();
    let mut mutate_skills = false;

    let mut batch_dto = BatchItemWireDto::default();
    let mut mutate_batch = false;

    let mut item_dto = ItemWireDto::default();
    let mut mutate_item = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--dry-run" => dry_run = true,
            "--json" => json_mode = true,
            "--force-overwrite" => force_overwrite = true,
            "--payload" => {
                i += 1;
                if i < args.len() {
                    payload_arg = Some(args[i].clone());
                }
            }
            // Waypoints
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
            // Skills & Respec
            "--skill" => {
                i += 1;
                if i < args.len() {
                    match args[i].parse::<SkillOpWireDto>() {
                        Ok(op_dto) => {
                            skill_dto.skills.push(op_dto);
                            mutate_skills = true;
                        }
                        Err(err) => {
                            eprintln!("Invalid --skill parameter: {}", err);
                            exit(1);
                        }
                    }
                }
            }
            "--skill-points" => {
                i += 1;
                if i < args.len() {
                    skill_dto.skill_points = args[i].parse::<NumericOp<u8>>().ok();
                    mutate_skills = true;
                }
            }
            "--respec" => {
                skill_dto.respec = Some(true);
                mutate_skills = true;
            }
            // Batch Item Operations
            "--repair-all" => {
                batch_dto.repair_all = Some(true);
                mutate_batch = true;
            }
            "--identify-all" => {
                batch_dto.identify_all = Some(true);
                mutate_batch = true;
            }
            "--socket-all" => {
                i += 1;
                if i < args.len() {
                    batch_dto.socket_all = args[i].parse::<u8>().ok();
                    mutate_batch = true;
                }
            }
            // Single Item Operations
            "--item-index" => {
                i += 1;
                if i < args.len() {
                    item_dto.target.index = args[i].parse::<usize>().ok();
                    mutate_item = true;
                }
            }
            "--item-code" => {
                i += 1;
                if i < args.len() {
                    item_dto.target.code = Some(args[i].clone());
                    mutate_item = true;
                }
            }
            "--item-slot" => {
                i += 1;
                if i < args.len() {
                    item_dto.target.slot = Some(args[i].clone());
                    mutate_item = true;
                }
            }
            "--item-defense" => {
                i += 1;
                if i < args.len() {
                    item_dto.defense = args[i].parse::<NumericOp<u32>>().ok();
                    mutate_item = true;
                }
            }
            "--item-durability" => {
                i += 1;
                if i < args.len() {
                    item_dto.durability = args[i].parse::<NumericOp<u32>>().ok();
                    mutate_item = true;
                }
            }
            "--item-max-durability" => {
                i += 1;
                if i < args.len() {
                    item_dto.max_durability = args[i].parse::<NumericOp<u32>>().ok();
                    mutate_item = true;
                }
            }
            "--item-quantity" => {
                i += 1;
                if i < args.len() {
                    item_dto.quantity = args[i].parse::<NumericOp<u32>>().ok();
                    mutate_item = true;
                }
            }
            "--item-sockets" => {
                i += 1;
                if i < args.len() {
                    item_dto.sockets = args[i].parse::<u8>().ok();
                    mutate_item = true;
                }
            }
            "--item-ethereal" => {
                i += 1;
                if i < args.len() {
                    item_dto.is_ethereal = args[i].parse::<bool>().ok();
                    mutate_item = true;
                }
            }
            "--item-identified" => {
                i += 1;
                if i < args.len() {
                    item_dto.is_identified = args[i].parse::<bool>().ok();
                    mutate_item = true;
                }
            }
            "--item-ed" | "--item-enhanced-damage" => {
                i += 1;
                if i < args.len() {
                    item_dto.enhanced_damage = args[i].parse::<NumericOp<i32>>().ok();
                    mutate_item = true;
                }
            }
            "--item-min-damage" => {
                i += 1;
                if i < args.len() {
                    item_dto.min_damage = args[i].parse::<NumericOp<i32>>().ok();
                    mutate_item = true;
                }
            }
            "--item-max-damage" => {
                i += 1;
                if i < args.len() {
                    item_dto.max_damage = args[i].parse::<NumericOp<i32>>().ok();
                    mutate_item = true;
                }
            }
            "--item-rep-dur" | "--item-replenish-durability" => {
                i += 1;
                if i < args.len() {
                    item_dto.replenish_durability = args[i].parse::<NumericOp<i32>>().ok();
                    mutate_item = true;
                }
            }
            "--item-rep-quant" | "--item-replenish-quantity" => {
                i += 1;
                if i < args.len() {
                    item_dto.replenish_quantity = args[i].parse::<NumericOp<i32>>().ok();
                    mutate_item = true;
                }
            }
            "--item-indestructible" => {
                i += 1;
                if i < args.len() {
                    item_dto.is_indestructible = args[i].parse::<bool>().ok();
                    mutate_item = true;
                }
            }
            "--item-stat" => {
                i += 1;
                if i < args.len() {
                    match args[i].parse::<ItemStatOpWireDto>() {
                        Ok(op_dto) => {
                            item_dto.stats.push(op_dto);
                            mutate_item = true;
                        }
                        Err(err) => {
                            eprintln!("Invalid --item-stat parameter: {}", err);
                            exit(1);
                        }
                    }
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
                    mutate_character = true;
                }
            }
            "--level" => {
                i += 1;
                if i < args.len() {
                    wire_dto.level = args[i].parse::<NumericOp<u8>>().ok();
                    mutate_character = true;
                }
            }
            "--strength" => {
                i += 1;
                if i < args.len() {
                    wire_dto.strength = args[i].parse::<NumericOp<u32>>().ok();
                    mutate_character = true;
                }
            }
            "--dexterity" => {
                i += 1;
                if i < args.len() {
                    wire_dto.dexterity = args[i].parse::<NumericOp<u32>>().ok();
                    mutate_character = true;
                }
            }
            "--vitality" => {
                i += 1;
                if i < args.len() {
                    wire_dto.vitality = args[i].parse::<NumericOp<u32>>().ok();
                    mutate_character = true;
                }
            }
            "--energy" => {
                i += 1;
                if i < args.len() {
                    wire_dto.energy = args[i].parse::<NumericOp<u32>>().ok();
                    mutate_character = true;
                }
            }
            "--stat-points" => {
                i += 1;
                if i < args.len() {
                    wire_dto.stat_points = args[i].parse::<NumericOp<u32>>().ok();
                    mutate_character = true;
                }
            }
            "--gold-stash" => {
                i += 1;
                if i < args.len() {
                    wire_dto.gold_stash = args[i].parse::<NumericOp<u32>>().ok();
                    mutate_character = true;
                }
            }
            "--gold-char" => {
                i += 1;
                if i < args.len() {
                    wire_dto.gold_character = args[i].parse::<NumericOp<u32>>().ok();
                    mutate_character = true;
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

    // 1. Build root SaveMutationDto from payload and/or CLI flags
    let mut root_dto = if let Some(ref payload) = payload_arg {
        let content = if Path::new(payload).is_file() {
            fs::read_to_string(payload).unwrap_or_else(|e| {
                eprintln!("Failed to read payload file '{}': {}", payload, e);
                exit(1);
            })
        } else {
            payload.clone()
        };

        serde_json::from_str::<SaveMutationDto>(&content).unwrap_or_else(|e| {
            eprintln!("Failed to parse payload as SaveMutationDto JSON: {}", e);
            exit(1);
        })
    } else {
        SaveMutationDto::default()
    };

    // Merge CLI flags into root_dto (flags augment/override payload)
    if mutate_character {
        root_dto.character = Some(wire_dto);
    }
    if mutate_waypoints {
        root_dto.waypoints = Some(wp_dto.clone());
    }
    if mutate_skills {
        root_dto.skills = Some(skill_dto);
    }
    if mutate_batch {
        root_dto.batch_items = Some(batch_dto);
    }
    if mutate_item {
        root_dto.items.push(item_dto);
    }

    // 2. Read source save file
    let save_bytes = match fs::read(&input_path) {
        Ok(b) => b,
        Err(err) => {
            eprintln!("Failed to read input file '{}': {}", input_path.display(), err);
            exit(1);
        }
    };

    let huffman = HuffmanTree::new();

    // 3. Validate root SaveMutationDto across all domains simultaneously
    let validated_contract = match SaveMutationValidator::validate(&root_dto, &save_bytes, &huffman) {
        Ok(c) => c,
        Err(err_resp) => {
            if json_mode {
                println!("{}", serde_json::to_string_pretty(&err_resp).unwrap());
            } else {
                eprintln!("Validation error (HTTP 422): {}", err_resp.message);
                for (field, violations) in &err_resp.errors {
                    for v in violations {
                        eprintln!("  - [{}]: {} ({})", field, v.message, v.hint);
                    }
                }
            }
            exit(1);
        }
    };

    // 4. Apply mutations atomically via SaveMutationPipeline
    let mutated_bytes = match SaveMutationPipeline::execute(&save_bytes, &validated_contract, &huffman) {
        Ok(b) => b,
        Err(err_resp) => {
            if json_mode {
                println!("{}", serde_json::to_string_pretty(&err_resp).unwrap());
            } else {
                eprintln!("Pipeline execution error: {}", err_resp.message);
                for (field, violations) in &err_resp.errors {
                    for v in violations {
                        eprintln!("  - [{}]: {}", field, v.message);
                    }
                }
            }
            exit(1);
        }
    };

    // 5. Parse modified save for reporting and verification
    let modified_save = match Save::from_bytes(&mutated_bytes) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("Corrupted save generated: {}", err);
            exit(1);
        }
    };

    // 6. Write to output if not in dry-run mode
    if !dry_run {
        if let Err(err) = fs::write(&target_out_path, &mutated_bytes) {
            eprintln!("Failed to write mutated save to '{}': {}", target_out_path.display(), err);
            exit(1);
        }
    }

    // 7. Output report
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
            waypoints_unlocked: if mutate_waypoints || root_dto.waypoints.is_some() { Some(true) } else { None },
            skills_mutated: if mutate_skills || root_dto.skills.is_some() { Some(true) } else { None },
            respec_applied: root_dto.skills.as_ref().map(|s| s.respec.unwrap_or(false)),
            item_mutated: if mutate_item || !root_dto.items.is_empty() { Some(true) } else { None },
            batch_operations: root_dto.batch_items.as_ref().map(|b| {
                let mut ops = Vec::new();
                if b.repair_all.unwrap_or(false) { ops.push("repair_all".to_string()); }
                if b.identify_all.unwrap_or(false) { ops.push("identify_all".to_string()); }
                if let Some(sock) = b.socket_all { ops.push(format!("socket_all({})", sock)); }
                ops
            }),
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
