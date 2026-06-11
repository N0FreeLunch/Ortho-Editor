use anyhow::{Context, Result};
use d2r_core::domain::forensic::v105::axioms::V105SectionMarkerAxiom;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Debug)]
struct ScanResult {
    file_path: String,
    version: u32,
    offset_271: u8,
    is_expansion_flag: bool,
    has_woo: bool,
    has_ws: bool,
    has_w4: bool,
}

fn main() -> Result<()> {
    let mut parser = ArgParser::new("flag_paradox_scanner");
    parser.add_spec(
        ArgSpec::option(
            "fixtures",
            None,
            Some("fixtures"),
            "Path to fixtures directory",
        )
        .required(),
    );
    parser.add_spec(ArgSpec::option(
        "output-json",
        None,
        Some("output-json"),
        "Path to output JSON file",
    ));
    parser.add_spec(ArgSpec::option(
        "output-md",
        None,
        Some("output-md"),
        "Path to output Markdown file",
    ));

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}", e);
            eprintln!("\n{}", parser.usage());
            std::process::exit(1);
        }
    };

    let fixtures_dir = parsed.get("fixtures").unwrap();
    let mut results = Vec::new();

    scan_dir(Path::new(fixtures_dir), &mut results)?;

    if let Some(json_path) = parsed.get("output-json") {
        let json = serde_json::to_string_pretty(&results)?;
        fs::write(json_path, json)
            .with_context(|| format!("Failed to write JSON to {}", json_path))?;
    }

    if let Some(md_path) = parsed.get("output-md") {
        let mut md = String::from("# Flag Paradox Scan Results\n\n");
        md.push_str(
            "| File | Version | Offset 271 | Is Expansion | Has Woo! | Has WS | Has w4 |\n",
        );
        md.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
        for res in &results {
            md.push_str(&format!(
                "| {} | {} | 0x{:02X} | {} | {} | {} | {} |\n",
                res.file_path,
                res.version,
                res.offset_271,
                res.is_expansion_flag,
                res.has_woo,
                res.has_ws,
                res.has_w4
            ));
        }
        fs::write(md_path, md).with_context(|| format!("Failed to write MD to {}", md_path))?;
    } else {
        println!("{:<60} | Ver | 271  | Exp | Woo | WS  | w4", "File");
        println!("{:-<60}-|-----|------|-----|-----|-----|-----", "");
        for res in &results {
            println!(
                "{:<60} | {:<3} | 0x{:02X} | {:<3} | {:<3} | {:<3} | {:<3}",
                res.file_path,
                res.version,
                res.offset_271,
                res.is_expansion_flag,
                res.has_woo,
                res.has_ws,
                res.has_w4
            );
        }
    }

    Ok(())
}

fn scan_dir(dir: &Path, results: &mut Vec<ScanResult>) -> Result<()> {
    let axiom = V105SectionMarkerAxiom::default();

    // Check if it's a file directly (for single file scan)
    if dir.is_file() {
        if dir.extension().and_then(|s| s.to_str()) == Some("d2s") {
            scan_file(dir, &axiom, results)?;
        }
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            scan_dir(&path, results)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("d2s") {
            scan_file(&path, &axiom, results)?;
        }
    }
    Ok(())
}

fn scan_file(
    path: &Path,
    axiom: &V105SectionMarkerAxiom,
    results: &mut Vec<ScanResult>,
) -> Result<()> {
    let bytes = fs::read(path)?;
    if bytes.len() < 300 {
        return Ok(());
    }

    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let offset_271 = bytes[271];
    let is_expansion_flag = (offset_271 & 0x20) != 0;

    let has_woo = axiom.find_woo(&bytes).is_some();
    let has_ws = axiom.find_ws(&bytes).is_some();
    let has_w4 = axiom.find_w4(&bytes).is_some();

    results.push(ScanResult {
        file_path: path.to_string_lossy().into_owned(),
        version,
        offset_271,
        is_expansion_flag,
        has_woo,
        has_ws,
        has_w4,
    });
    Ok(())
}
