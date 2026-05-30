use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use std::env;
use std::fs;
use std::process;

fn find_first_jm(bytes: &[u8]) -> Option<usize> {
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == b'J' && bytes[i + 1] == b'M' {
            return Some(i);
        }
    }
    None
}

fn main() {
    let mut parser = ArgParser::new("d2save_diff")
        .description("Compares two D2R save files and highlights byte-level differences in header and item sections");

    parser.add_spec(ArgSpec::positional(
        "file_a",
        "path to the first save file (.d2s)",
    ));
    parser.add_spec(ArgSpec::positional(
        "file_b",
        "path to the second save file (.d2s)",
    ));

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let path_a = parsed.get("file_a").unwrap();
    let path_b = parsed.get("file_b").unwrap();

    let bytes_a = match fs::read(path_a) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[ERROR] Cannot read '{}': {}", path_a, e);
            process::exit(1);
        }
    };
    let bytes_b = match fs::read(path_b) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[ERROR] Cannot read '{}': {}", path_b, e);
            process::exit(1);
        }
    };

    println!("=== d2save_diff ===");
    println!("  A: {} ({} bytes)", path_a, bytes_a.len());
    println!("  B: {} ({} bytes)", path_b, bytes_b.len());

    let jm_a = find_first_jm(&bytes_a);
    let jm_b = find_first_jm(&bytes_b);
    println!("  First JM in A: {:?}", jm_a);
    println!("  First JM in B: {:?}", jm_b);

    let common_len = bytes_a.len().min(bytes_b.len());
    let mut diffs = Vec::new();

    for i in 0..common_len {
        if bytes_a[i] != bytes_b[i] {
            diffs.push(i);
        }
    }

    println!();
    println!("[DIFF SUMMARY]");

    let header_end = jm_a.unwrap_or(0).min(jm_b.unwrap_or(0));
    let header_diffs: Vec<usize> = diffs.iter().copied().filter(|&i| i < header_end).collect();
    let item_diffs: Vec<usize> = diffs.iter().copied().filter(|&i| i >= header_end).collect();

    println!(
        "  Header diffs  (0..{header_end}): {} bytes",
        header_diffs.len()
    );
    println!(
        "  Item diffs    ({header_end}..{}): {} bytes",
        common_len,
        item_diffs.len()
    );
    if bytes_a.len() != bytes_b.len() {
        println!(
            "  Length diff: {} bytes",
            bytes_b.len() as isize - bytes_a.len() as isize
        );
    }

    println!();
    println!("[DETAILS] (first 30 diffs)");
    println!("  {:>8}  {:>10}  {:>10}", "Offset", "A (hex)", "B (hex)");
    println!("  {:->8}  {:->10}  {:->10}", "", "", "");
    for &i in diffs.iter().take(30) {
        let a_val = bytes_a[i];
        let b_val = bytes_b[i];
        let region = if i < header_end { "header" } else { "items " };
        println!(
            "  {:>8}  0x{:02X} ({:>3})   0x{:02X} ({:>3})   [{}]",
            i, a_val, a_val, b_val, b_val, region
        );
    }
    if diffs.len() > 30 {
        println!("  ... and {} more diffs", diffs.len() - 30);
    }
    if diffs.is_empty() {
        println!("  [IDENTICAL] No differences found.");
    }
}
