use std::fs;
use std::env;
use serde::Serialize;
use anyhow::{Result, Context, anyhow};

#[derive(Serialize)]
struct DiffResult {
    alignment_anchor: String,
    divergence_offset_from_jm: Option<u64>,
    base_bit_at_divergence: Option<u8>,
    mutated_bit_at_divergence: Option<u8>,
    bit_delta_summary: String,
}

fn read_bits(path: &str) -> Result<Vec<bool>> {
    let content = fs::read(path).with_context(|| format!("Failed to read {}", path))?;
    
    // Check if it's a .bits file (ASCII 0/1)
    if path.ends_with(".bits") {
        let content_str = String::from_utf8_lossy(&content);
        let mut bits = Vec::new();
        for line in content_str.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("Dumping") || line.starts_with("#") {
                continue;
            }
            
            for &b in line.as_bytes() {
                match b {
                    b'0' => bits.push(false),
                    b'1' => bits.push(true),
                    _ if b.is_ascii_whitespace() => continue,
                    _ => {
                        // Stop reading bits from this line if we hit a non-bit char
                        // (e.g. comments like " (72 bits)" or "#")
                        break;
                    }
                }
            }
        }
        return Ok(bits);
    }

    // Otherwise treat as binary, Little-Endian bits
    let mut bits = Vec::with_capacity(content.len() * 8);
    for &byte in &content {
        for i in 0..8 {
            bits.push(((byte >> i) & 1) != 0);
        }
    }
    Ok(bits)
}

fn find_jm(bits: &[bool]) -> Option<usize> {
    // JM = 0x4A, 0x4D
    // 0x4A (74) -> LSB: 0, 1, 0, 1, 0, 0, 1, 0
    // 0x4D (77) -> LSB: 1, 0, 1, 1, 0, 0, 1, 0
    let pattern = [
        false, true, false, true, false, false, true, false,
        true, false, true, true, false, false, true, false,
    ];

    if bits.len() < 16 {
        return None;
    }

    for i in 0..=(bits.len() - 16) {
        if bits[i..i + 16] == pattern {
            return Some(i);
        }
    }
    None
}

fn main() -> Result<()> {
    let args_vec: Vec<String> = env::args().collect();
    
    let mut base_path = None;
    let mut mutated_path = None;
    let mut json_mode = false;

    let mut i = 1;
    while i < args_vec.len() {
        match args_vec[i].as_str() {
            "--base" => {
                if i + 1 < args_vec.len() {
                    base_path = Some(&args_vec[i + 1]);
                    i += 1;
                }
            }
            "--mutated" => {
                if i + 1 < args_vec.len() {
                    mutated_path = Some(&args_vec[i + 1]);
                    i += 1;
                }
            }
            "--json" => {
                json_mode = true;
            }
            _ => {}
        }
        i += 1;
    }

    let base_path = base_path.ok_or_else(|| anyhow!("Missing --base"))?;
    let mutated_path = mutated_path.ok_or_else(|| anyhow!("Missing --mutated"))?;

    let base_bits = read_bits(base_path)?;
    let mutated_bits = read_bits(mutated_path)?;

    let base_jm = find_jm(&base_bits);
    let mutated_jm = find_jm(&mutated_bits);

    let mut result = DiffResult {
        alignment_anchor: "JM".to_string(),
        divergence_offset_from_jm: None,
        base_bit_at_divergence: None,
        mutated_bit_at_divergence: None,
        bit_delta_summary: String::new(),
    };

    match (base_jm, mutated_jm) {
        (Some(b_jm), Some(m_jm)) => {
            let mut offset = 0;
            let mut found_divergence = false;
            
            loop {
                let b_idx = b_jm + offset;
                let m_idx = m_jm + offset;

                if b_idx >= base_bits.len() || m_idx >= mutated_bits.len() {
                    break;
                }

                if base_bits[b_idx] != mutated_bits[m_idx] {
                    result.divergence_offset_from_jm = Some(offset as u64);
                    result.base_bit_at_divergence = Some(if base_bits[b_idx] { 1 } else { 0 });
                    result.mutated_bit_at_divergence = Some(if mutated_bits[m_idx] { 1 } else { 0 });
                    result.bit_delta_summary = format!(
                        "Divergence found at offset {} from JM (Base: {}, Mutated: {})",
                        offset,
                        result.base_bit_at_divergence.unwrap(),
                        result.mutated_bit_at_divergence.unwrap()
                    );
                    found_divergence = true;
                    break;
                }
                offset += 1;
            }

            if !found_divergence {
                result.bit_delta_summary = "No divergence found within available bits from JM anchor.".to_string();
            }
        }
        _ => {
            let mut missing = Vec::new();
            if base_jm.is_none() { missing.push("base"); }
            if mutated_jm.is_none() { missing.push("mutated"); }
            result.bit_delta_summary = format!("Failed to locate JM marker in {} sequence(s).", missing.join(" and "));
        }
    }

    if json_mode {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Alignment Anchor: {}", result.alignment_anchor);
        println!("{}", result.bit_delta_summary);
        if let Some(offset) = result.divergence_offset_from_jm {
            println!("Divergence Offset: {}", offset);
            println!("Base Bit:          {}", result.base_bit_at_divergence.unwrap());
            println!("Mutated Bit:       {}", result.mutated_bit_at_divergence.unwrap());
        }
    }

    Ok(())
}
