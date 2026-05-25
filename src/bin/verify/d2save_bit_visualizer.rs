// d2r-core/src/bin/verify/d2save_bit_visualizer.rs

use anyhow::Context;
use d2r_core::verify::args::{ArgError, ArgParser};
use std::{env, fs, io::Write};

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2save_bit_visualizer");
    parser.add_opt("start", "Start bit offset").short('s').long("start");
    parser.add_opt("width", "Bit width to visualize").short('w').long("width");
    parser.add_opt("output", "Output file path (mandatory for >2048 bits)").short('o').long("output");
    parser.add_flag("pure-text", "Disable ANSI colors and strip formatting").long("pure-text");
    parser.add_flag("token-efficient", "Minimize terminal output for AI agent safety").long("token-efficient");
    parser.add_arg("file", "Save file to visualize");

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => anyhow::bail!("error: {}\n\n{}", e, parser.usage()),
    };

    let file_path = parsed.get("file").context("Missing save file argument")?;
    let start_bit: usize = parsed.get("start").and_then(|s| s.parse().ok()).unwrap_or(0);
    let width: usize = parsed.get("width").and_then(|w| w.parse().ok()).unwrap_or(256);
    let output_path = parsed.get("output");
    let pure_text = parsed.is_set("pure-text");
    let token_efficient = parsed.is_set("token-efficient");

    if (width > 2048 || token_efficient) && output_path.is_none() {
        anyhow::bail!("OOM Prevention: Large output or --token-efficient requires --output <file_path> to prevent terminal overflow.");
    }

    let bytes = fs::read(file_path).context("Failed to read save file")?;
    
    // Attempt to parse items for semantic labeling (Slice 2)
    let version = if bytes.len() >= 8 {
        u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]))
    } else {
        0
    };
    let alpha_mode = version == 105;
    
    // Axiom 0340: Enable item trace to capture BitSegments for semantic labeling
    unsafe { env::set_var("D2R_ITEM_TRACE", "1"); }
    let huffman = d2r_core::item::HuffmanTree::new();
    let items = d2r_core::item::Item::read_player_items(&bytes, &huffman, alpha_mode).unwrap_or_default();
    
    // Axiom 0340: Identify section headers (JM markers)
    let jm_markers = d2r_core::save::find_jm_markers(&bytes);

    if !items.is_empty() {
        eprintln!("Parsed {} items for semantic labeling.", items.len());
    } else {
        eprintln!("No items parsed (or parsing failed). Semantic labeling will be limited.");
    }

    let mut result = String::new();
    result.push_str(&format!("Visualizing bits from {} to {} (width: {}, version: {})\n", start_bit, start_bit + width, width, version));
    
    let mut last_label = String::new();

    for i in 0..width {
        let bit_idx = start_bit + i;
        let byte_idx = bit_idx / 8;
        let bit_in_byte = bit_idx % 8;
        
        if byte_idx >= bytes.len() {
            result.push_str("X"); // Out of bounds
            continue;
        }
        
        let bit = (bytes[byte_idx] >> bit_in_byte) & 1 == 1;
        
        // Semantic labeling (Recursive search)
        let mut semantic = items.iter().find_map(|it| {
            if let Some(s) = it.query_bit(bit_idx as u64) {
                Some(s.label)
            } else if (bit_idx as u64) >= it.range.start && (bit_idx as u64) < it.range.end {
                Some(format!("Item({})", it.code.trim()))
            } else {
                None
            }
        });

        // Label JM markers if not already in an item range
        if semantic.is_none() {
            for &jm_pos in &jm_markers {
                let jm_bit = (jm_pos as u64) * 8;
                if (bit_idx as u64) >= jm_bit && (bit_idx as u64) < jm_bit + 16 {
                    semantic = Some("JM Marker".to_string());
                    break;
                } else if (bit_idx as u64) >= jm_bit + 16 && (bit_idx as u64) < jm_bit + 32 {
                    semantic = Some("Item Count".to_string());
                    break;
                }
            }
        }
        
        // ANSI colors (Safe Guard)
        let use_colors = output_path.is_none() && !pure_text;
        
        if use_colors {
            if let Some(label) = &semantic {
                // Colorize based on semantic (Simplified)
                if label.contains("JM") { result.push_str("\x1b[91m"); } // Bright Red for JM
                else if label.contains("Stats") { result.push_str("\x1b[93m"); } // Bright Yellow for Stats
                else { result.push_str("\x1b[32m"); } // Green
            } else if bit {
                result.push_str("\x1b[32m"); // Green for 1
            } else {
                result.push_str("\x1b[34m"); // Blue for 0
            }
        }

        result.push(if bit { '1' } else { '0' });
        
        if use_colors { result.push_str("\x1b[0m"); }
        
        if (i + 1) % 8 == 0 { result.push(' '); }
        
        if let Some(label) = semantic {
            if label != last_label {
                last_label = label;
            }
        }

        if (i + 1) % 64 == 0 { 
            if !last_label.is_empty() {
                result.push_str(&format!(" | {}", last_label));
            }
            result.push('\n'); 
        }
    }
    result.push('\n');

    if let Some(out) = output_path {
        let final_output = if pure_text { strip_ansi_codes(&result) } else { result };
        let mut f = fs::File::create(out)?;
        f.write_all(final_output.as_bytes())?;
        println!("Visualization written to {}", out);
    } else {
        println!("{}", result);
    }

    Ok(())
}

/// Zero-Dependency ANSI Strip Helper
fn strip_ansi_codes(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_escape = false;
    for c in input.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c == 'm' || c == 'K' {
                in_escape = false;
            }
        } else {
            output.push(c);
        }
    }
    output
}
