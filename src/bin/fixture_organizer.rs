use d2r_core::save::Save;
use d2r_core::verify::args::{ArgParser, ArgSpec};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let mut parser = ArgParser::new("fixture-organizer")
        .description("Standardizes and organizes gameplay save fixtures based on internal data");

    parser.add_spec(ArgSpec::option(
        "char",
        Some('c'),
        Some("char"),
        "Character name (e.g. TESTDRUID)",
    ));
    parser.add_spec(ArgSpec::option(
        "quest",
        Some('q'),
        Some("quest"),
        "Quest/Progression context (e.g. Q1)",
    ));
    parser.add_spec(ArgSpec::option(
        "desc",
        Some('d'),
        Some("desc"),
        "Detailed description (e.g. DonePreAkara)",
    ));
    parser.add_spec(ArgSpec::flag(
        "dry-run",
        Some('n'),
        Some("dry-run"),
        "Show what would be done without moving files",
    ));
    parser.add_spec(ArgSpec::positional("src", "Source .d2s file to organize"));

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(err) => match err {
            d2r_core::verify::args::ArgError::Help(h) => {
                println!("{}", h);
                std::process::exit(0);
            }
            d2r_core::verify::args::ArgError::Error(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
    };

    let src_path_str = parsed.get("src").expect("Source path is required");
    let src_path = Path::new(src_path_str);

    if !src_path.exists() {
        eprintln!("Error: Source file does not exist: {}", src_path_str);
        std::process::exit(1);
    }

    let bytes = fs::read(src_path).expect("Failed to read source file");

    // Basic validation and parsing
    let save = match Save::from_bytes(&bytes) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: Not a valid D2R save file: {}", e);
            std::process::exit(1);
        }
    };

    // Intelligence Anchor (User Provided)
    // 161 (0xA1): Normal, 162 (0xA2): NM, 163 (0xA3): Hell
    // Values: 0x01-0x05 (Act 1-5), 0x80+ (Cleared)

    let active_act = bytes.get(21).copied().unwrap_or(0);
    let progress_flag = bytes.get(108).copied().unwrap_or(0);
    let normal_prog = bytes.get(161).copied().unwrap_or(0);
    let nm_prog = bytes.get(162).copied().unwrap_or(0);
    let hell_prog = bytes.get(163).copied().unwrap_or(0);

    println!("Source Analysis (Alpha v105 Focus):");
    println!("  Character Name:             {}", save.header.char_name);
    println!(
        "  Offset 021 (active_act):    0x{:02X} (Act {})",
        active_act,
        (active_act & 0x7) + 1
    );
    println!("  Offset 108 (progress_flag): 0x{:02X}", progress_flag);
    println!("  Offset 161 (Normal Prog):   0x{:02X}", normal_prog);
    println!("  Offset 162 (NM Prog):       0x{:02X}", nm_prog);
    println!("  Offset 163 (Hell Prog):     0x{:02X}", hell_prog);

    // Alpha v105 Difficulty Logic Refinement (Verified via Discussion 0156)
    // Offset 21 (active_act) bits 3-4 indicate difficulty in v105.
    // 0x00: Normal, 0x08: Nightmare (Bit 3), 0x10: Hell (Bit 4)
    const V105_DIFFICULTY_MASK: u8 = 0x18;
    const V105_DIFFICULTY_SHIFT: u8 = 3;
    const V105_DIFF_NIGHTMARE: u8 = 1;
    const V105_DIFF_HELL: u8 = 2;

    let diff_str = match (active_act & V105_DIFFICULTY_MASK) >> V105_DIFFICULTY_SHIFT {
        V105_DIFF_NIGHTMARE => "nightmare",
        V105_DIFF_HELL => "hell",
        _ => "normal",
    };

    let act_num = (active_act & 0x7) + 1;
    let act_str = format!("act{}", act_num);

    // Resolve naming components with automation fallbacks
    let char_val = parsed
        .get("char")
        .cloned()
        .unwrap_or_else(|| save.header.char_name.clone());
    let quest_val = parsed
        .get("quest")
        .cloned()
        .unwrap_or_else(|| format!("Act{}", act_num));
    let desc_val = parsed
        .get("desc")
        .cloned()
        .unwrap_or_else(|| "AutoOrganized".to_string());

    let new_filename = format!("{}_{}_{}.d2s", char_val, quest_val, desc_val);

    // Resolve target root
    let root = env::var("D2R_CORE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::current_dir().unwrap());

    let target_dir = root
        .join("tests/fixtures/savegames/gameplay")
        .join(diff_str)
        .join(&act_str);
    let target_path = target_dir.join(&new_filename);

    println!("\nProposed Placement:");
    println!("  Target Directory:  {}", target_dir.display());
    println!("  Target Filename:   {}", new_filename);

    if parsed.is_set("dry-run") {
        println!("\n[DRY RUN] No files were copied.");
        return;
    }

    if !target_dir.exists() {
        println!("  Creating directory...");
        fs::create_dir_all(&target_dir).expect("Failed to create target directory");
    }

    fs::copy(src_path, &target_path).expect("Failed to copy file to target");
    println!("\nSuccess: File organized to {}", target_path.display());
}
