use d2r_core::verify::args::{ArgParser, ArgError};
use d2r_core::save::find_jm_markers;
use serde::Serialize;
use std::env;
use std::fs;

#[derive(Serialize)]
struct FuzzResult {
    jm_index: usize,
    jm_bit_pos: u64,
    candidates: Vec<Candidate>,
}

#[derive(Serialize)]
struct Candidate {
    bit_offset: u64,
    bit_shift: u64,
    code: String,
    hex: String,
}

fn is_printable(c: u8, strict: bool) -> bool {
    if strict {
        (c >= b'A' && c <= b'Z') || (c >= b'a' && c <= b'z') || (c >= b'0' && c <= b'9')
    } else {
        c >= 32 && c <= 126
    }
}

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2item_code_fuzzer")
        .description("Fuzzy search for ASCII item codes after JM markers using bit-shifting");

    parser.add_arg("save_file", "path to the save file (.d2s)");
    parser.add_opt("range", "bit range to scan after each JM marker")
        .short('r')
        .long("range")
        .default("1024");
    parser.add_flag("strict", "restrict printable characters to A-Z, 0-9")
        .short('s')
        .long("strict");

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            anyhow::bail!("{}\n\n{}", e, parser.usage());
        }
    };

    let path = parsed.get("save_file").unwrap();
    let scan_range: u64 = parsed.get("range").unwrap().parse()?;
    let is_json = parsed.is_json();
    let strict = parsed.is_set("strict");

    let bytes = fs::read(path)?;
    let jm_markers = find_jm_markers(&bytes);

    let mut all_results = Vec::new();

    for (jm_idx, &jm_byte_pos) in jm_markers.iter().enumerate() {
        let jm_bit_pos = (jm_byte_pos as u64) * 8;
        let start_bit = jm_bit_pos + 32;

        let mut candidates = Vec::new();

        for shift in 0..scan_range {
            let current_bit = start_bit + shift;
            
            for len in [3, 4] {
                let bit_len = len * 8;
                if current_bit + bit_len > (bytes.len() * 8) as u64 {
                    continue;
                }

                let mut code_bytes = Vec::new();
                let mut valid = true;
                
                for i in 0..len {
                    let char_start_bit = current_bit + (i * 8);
                    let mut char_byte: u8 = 0;
                    for b in 0..8 {
                        let bit_pos = char_start_bit + b;
                        let byte_idx = (bit_pos / 8) as usize;
                        let bit_idx = (bit_pos % 8) as usize;
                        let bit = (bytes[byte_idx] >> bit_idx) & 1;
                        char_byte |= bit << b;
                    }
                    
                    if is_printable(char_byte, strict) {
                        code_bytes.push(char_byte);
                    } else {
                        valid = false;
                        break;
                    }
                }

                if valid {
                    let code = String::from_utf8_lossy(&code_bytes).to_string();
                    // Basic heuristic: check if it looks like a code
                    let looks_like_code = code.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ');
                    
                    if looks_like_code {
                        candidates.push(Candidate {
                            bit_offset: current_bit,
                            bit_shift: shift,
                            code,
                            hex: hex::encode(&code_bytes),
                        });
                    }
                }
            }
        }

        // Noise control: if too many candidates, retry with strict if not already strict
        if candidates.len() > 100 && !strict {
            let mut strict_candidates = Vec::new();
            for shift in 0..scan_range {
                let current_bit = start_bit + shift;
                for len in [3, 4] {
                    let bit_len = len * 8;
                    if current_bit + bit_len > (bytes.len() * 8) as u64 { continue; }
                    let mut code_bytes = Vec::new();
                    let mut valid = true;
                    for i in 0..len {
                        let char_start_bit = current_bit + (i * 8);
                        let mut char_byte: u8 = 0;
                        for b in 0..8 {
                            let bit_pos = char_start_bit + b;
                            let byte_idx = (bit_pos / 8) as usize;
                            let bit_idx = (bit_pos % 8) as usize;
                            let bit = (bytes[byte_idx] >> bit_idx) & 1;
                            char_byte |= bit << b;
                        }
                        if is_printable(char_byte, true) { code_bytes.push(char_byte); } else { valid = false; break; }
                    }
                    if valid {
                        let code = String::from_utf8_lossy(&code_bytes).to_string();
                        if code.chars().all(|c| c.is_ascii_alphanumeric()) {
                            strict_candidates.push(Candidate { bit_offset: current_bit, bit_shift: shift, code, hex: hex::encode(&code_bytes) });
                        }
                    }
                }
            }
            candidates = strict_candidates;
        }

        all_results.push(FuzzResult {
            jm_index: jm_idx,
            jm_bit_pos,
            candidates,
        });
    }

    if is_json {
        println!("{}", serde_json::to_string_pretty(&all_results)?);
    } else {
        for res in all_results {
            println!("[JM #{}] at bit {}", res.jm_index, res.jm_bit_pos);
            for cand in res.candidates {
                println!("  Offset {} | Shift {} | Candidate '{}' | Hex {}", cand.bit_offset, cand.bit_shift, cand.code, cand.hex);
            }
            println!("{:-<60}", "");
        }
    }

    Ok(())
}
