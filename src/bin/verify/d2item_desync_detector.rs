use d2r_core::item::HuffmanTree;
use d2r_core::verify::args::{ArgParser, ArgSpec};
use d2r_core::verify::desync::detect_desync;
use std::env;
use std::fs;

fn main() {
    let mut parser = ArgParser::new("d2item_desync_detector");
    parser.add_spec(ArgSpec::positional("save_file", "Path to save file"));
    parser.add_spec(ArgSpec::flag("json", None, Some("json"), "Output results in JSON format"));

    use d2r_core::verify::args::ArgError;
    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            std::process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}", e);
            eprintln!("\n{}", parser.usage());
            std::process::exit(1);
        }
    };

    let path = parsed.get("save_file").unwrap();
    let use_json = parsed.is_set("json");

    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            std::process::exit(1);
        }
    };

    let huffman = HuffmanTree::new();
    let is_alpha = bytes[4..8] == [0x69, 0, 0, 0];

    match detect_desync(&bytes, &huffman, is_alpha) {
        Ok(reports) => {
            if use_json {
                println!("{}", serde_json::to_string_pretty(&reports).unwrap());
            } else {
                println!("Cascading Desync Report for: {}", path);
                println!("Mode: {}", if is_alpha { "Alpha v105" } else { "Retail" });
                println!("{:-<100}", "");
                println!("{:>5} | {:>12} | {:>12} | {:>8} | {:>10} | {:>10} | {:>18} | {:>5}", "Index", "Oracle Start", "Parser Start", "Drift", "Oracle Code", "Parser Code", "First Divergence", "Match");
                println!("{:-<100}", "");
                
                let mut first_desync_report = None;
                for r in &reports {
                    let family = r
                        .first_divergence_family
                        .map(|f| f.as_str())
                        .unwrap_or("none");
                    println!("{:5} | {:12} | {:12} | {:8} | {:11} | {:11} | {:18} | {:5}", 
                        r.item_index, r.oracle_start, r.parser_start, r.drift, r.oracle_code, r.parser_code, family, if r.is_match { "OK" } else { "FAIL" }
                    );
                    if !r.is_match && first_desync_report.is_none() {
                        first_desync_report = Some(r.clone());
                    }
                }
                println!("{:-<100}", "");
                
                if let Some(r) = first_desync_report {
                    println!("[ALERT] First desync detected at Item Index {}.", r.item_index);
                    if let Some(family) = r.first_divergence_family {
                        println!("  First Divergence Family: {}", family.as_str());
                    }
                    println!("\nForensic Bit Comparison at Drift Point:");
                    if let Some(dump) = &r.bit_dump {
                        println!("  Oracle Start ({:12}): {}", r.oracle_start, dump);
                    }
                    println!("  Parser Start ({:12}): {}", r.parser_start, d2r_core::verify::desync::dump_bits_at(&bytes, r.parser_start, 64));
                    
                    if r.drift > 0 {
                        println!("\n  [Drift Analysis] Parser skipped {} bits.", r.drift);
                        println!("  Bits between boundaries: {}", d2r_core::verify::desync::dump_bits_at(&bytes, r.oracle_start, r.drift as u32));
                    } else if r.drift < 0 {
                        println!("\n  [Drift Analysis] Parser started {} bits early.", -r.drift);
                    }
                } else {
                    println!("[PASS] No bitstream drift detected across {} items.", reports.len());
                }
            }
        }
        Err(e) => {
            eprintln!("Error during desync detection: {}", e);
            std::process::exit(1);
        }
    }
}
