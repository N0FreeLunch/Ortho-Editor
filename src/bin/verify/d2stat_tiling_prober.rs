use d2r_core::data::{StatSchemaRegistry, VersionStatSchema};
use serde::Serialize;
use std::fs;
use std::path::Path;

mod clap_stub {
    pub struct Args {
        pub file_path: Option<String>,
        pub bit_offset: usize,
        pub bit_string: Option<String>,
        pub max_depth: usize,
        pub alpha: bool,
        pub json: bool,
        pub help: bool,
    }

    pub fn parse_args() -> Args {
        let args: Vec<String> = std::env::args().collect();
        let mut file_path = None;
        let mut bit_offset = 0;
        let mut bit_string = None;
        let mut max_depth = 24;
        let mut alpha = false;
        let mut json = false;
        let mut help = false;

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--file" | "-f" => {
                    if i + 1 < args.len() {
                        file_path = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "--offset" | "-o" => {
                    if i + 1 < args.len() {
                        if let Ok(val) = args[i + 1].parse::<usize>() {
                            bit_offset = val;
                        }
                        i += 1;
                    }
                }
                "--bit-string" | "-b" => {
                    if i + 1 < args.len() {
                        bit_string = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "--max-depth" | "-d" => {
                    if i + 1 < args.len() {
                        if let Ok(val) = args[i + 1].parse::<usize>() {
                            max_depth = val;
                        }
                        i += 1;
                    }
                }
                "--alpha" | "-a" => {
                    alpha = true;
                }
                "--json" => {
                    json = true;
                }
                "--help" | "-h" => {
                    help = true;
                }
                _ => {}
            }
            i += 1;
        }

        Args {
            file_path,
            bit_offset,
            bit_string,
            max_depth,
            alpha,
            json,
            help,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct DecodedProperty {
    stat_id: u16,
    canonical_id: u16,
    name: String,
    param: u32,
    raw_value: i32,
    value: i32,
    bits_consumed: usize,
}

#[derive(Debug, Clone, Serialize)]
struct TilingPath {
    properties: Vec<DecodedProperty>,
    start_offset: usize,
    terminal_offset: usize,
    total_bits_consumed: usize,
    trailing_padding_bits: usize,
}

#[derive(Debug, Serialize)]
struct TilingReport {
    input_file: Option<String>,
    start_bit_offset: usize,
    total_bits_evaluated: usize,
    total_solutions: usize,
    is_unique: bool,
    pruned_branches_count: usize,
    solutions: Vec<TilingPath>,
    alpha_mode: bool,
}

fn read_u32_from_bits(bits: &[bool], start: usize, count: usize) -> Option<u32> {
    if start + count > bits.len() {
        return None;
    }
    let mut value = 0u32;
    for i in 0..count {
        if bits[start + i] {
            value |= 1 << i;
        }
    }
    Some(value)
}

fn bytes_to_bits(bytes: &[u8]) -> Vec<bool> {
    let mut bits = Vec::with_capacity(bytes.len() * 8);
    for &byte in bytes {
        for bit_idx in 0..8 {
            bits.push((byte & (1 << bit_idx)) != 0);
        }
    }
    bits
}

fn bitstring_to_bits(s: &str) -> Vec<bool> {
    s.chars().filter_map(|c| match c {
        '0' => Some(false),
        '1' => Some(true),
        _ => None,
    }).collect()
}

// Translate Alpha v105 raw stat ID to canonical stat ID if applicable
fn map_alpha_stat_id(raw_id: u16, is_alpha: bool) -> u16 {
    if !is_alpha {
        return raw_id;
    }
    match raw_id {
        26 => 31,   // item_defense_percent
        312 => 72,  // item_durability
        207 => 73,  // item_maxdurability
        380 => 194, // item_indestructible
        256 => 127, // item_allskills
        496 => 99,  // item_fastergethitrate
        499 => 16,  // item_enandefense_percent
        289 => 9,   // maxmana
        _ => raw_id,
    }
}

struct TilingSolverContext<'a> {
    bits: &'a [bool],
    schema: &'a VersionStatSchema,
    max_depth: usize,
    is_alpha: bool,
    solutions: Vec<TilingPath>,
    pruned_count: usize,
}

impl<'a> TilingSolverContext<'a> {
    fn search(&mut self, offset: usize, current_path: &mut Vec<DecodedProperty>, depth: usize) {
        if depth > self.max_depth {
            self.pruned_count += 1;
            return;
        }

        // Check if remaining bits are enough to read 9-bit ID
        if offset + 9 > self.bits.len() {
            self.pruned_count += 1;
            return;
        }

        let raw_id = match read_u32_from_bits(self.bits, offset, 9) {
            Some(v) => v as u16,
            None => {
                self.pruned_count += 1;
                return;
            }
        };

        // Check for 0x1FF (511) terminator
        if raw_id == 0x1FF {
            let term_end = offset + 9;
            let total_consumed = term_end;
            let trailing = if self.bits.len() >= term_end {
                self.bits.len() - term_end
            } else {
                0
            };

            self.solutions.push(TilingPath {
                properties: current_path.clone(),
                start_offset: 0,
                terminal_offset: term_end,
                total_bits_consumed: total_consumed,
                trailing_padding_bits: trailing,
            });
            return;
        }

        let canonical_id = map_alpha_stat_id(raw_id, self.is_alpha);

        // Lookup in VersionStatSchema
        if let Some(rule) = self.schema.get_rule(canonical_id) {
            let mut cur_pos = offset + 9;
            let mut valid = true;

            let param = if rule.save_param_bits > 0 {
                if let Some(p) = read_u32_from_bits(self.bits, cur_pos, rule.save_param_bits) {
                    cur_pos += rule.save_param_bits;
                    p
                } else {
                    valid = false;
                    0
                }
            } else {
                0
            };

            let raw_value = if valid && rule.save_bits > 0 {
                if let Some(v) = read_u32_from_bits(self.bits, cur_pos, rule.save_bits) {
                    cur_pos += rule.save_bits;
                    v as i32
                } else {
                    valid = false;
                    0
                }
            } else {
                0
            };

            if valid {
                let value = raw_value - rule.save_add;
                let consumed = cur_pos - offset;

                current_path.push(DecodedProperty {
                    stat_id: raw_id,
                    canonical_id,
                    name: rule.name.clone(),
                    param,
                    raw_value,
                    value,
                    bits_consumed: consumed,
                });

                // Recurse to next property
                self.search(cur_pos, current_path, depth + 1);

                // Backtrack
                current_path.pop();
            } else {
                self.pruned_count += 1;
            }
        } else {
            // Unrecognized stat ID -> Prune
            self.pruned_count += 1;
        }
    }
}

fn main() -> anyhow::Result<()> {
    let args = clap_stub::parse_args();

    if args.help {
        println!("d2stat_tiling_prober - Backtracking CSP Tiling Prober for Property Reachability");
        println!();
        println!("USAGE:");
        println!("    d2stat_tiling_prober [OPTIONS]");
        println!();
        println!("OPTIONS:");
        println!("    -f, --file <PATH>         Path to .d2s or raw binary item file");
        println!("    -o, --offset <BITS>       Starting bit offset (default: 0)");
        println!("    -b, --bit-string <BITS>   Direct 0101 bitstring input");
        println!("    -d, --max-depth <N>       Maximum recursion depth (default: 24)");
        println!("    -a, --alpha               Enable Alpha v105 stat ID alias mapping");
        println!("        --json                Output structured JSON report");
        println!("    -h, --help                Print help information");
        return Ok(());
    }

    let bits = if let Some(ref bit_str) = args.bit_string {
        bitstring_to_bits(bit_str)
    } else if let Some(ref file_path) = args.file_path {
        let bytes = fs::read(Path::new(file_path))?;
        bytes_to_bits(&bytes)
    } else {
        eprintln!("Error: Please specify --file <PATH> or --bit-string <BITS>");
        std::process::exit(1);
    };

    let slice_bits = if args.bit_offset < bits.len() {
        &bits[args.bit_offset..]
    } else {
        &[]
    };

    let schema = StatSchemaRegistry::default_v99();
    let mut ctx = TilingSolverContext {
        bits: slice_bits,
        schema,
        max_depth: args.max_depth,
        is_alpha: args.alpha,
        solutions: Vec::new(),
        pruned_count: 0,
    };

    let mut current_path = Vec::new();
    ctx.search(0, &mut current_path, 0);

    let total_solutions = ctx.solutions.len();
    let report = TilingReport {
        input_file: args.file_path.clone(),
        start_bit_offset: args.bit_offset,
        total_bits_evaluated: slice_bits.len(),
        total_solutions,
        is_unique: total_solutions == 1,
        pruned_branches_count: ctx.pruned_count,
        solutions: ctx.solutions,
        alpha_mode: args.alpha,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("=== Tiling Prober Report ===");
        if let Some(ref path) = args.file_path {
            println!("File: {}", path);
        }
        println!("Start Bit Offset: {}", args.bit_offset);
        println!("Bits Evaluated: {}", slice_bits.len());
        println!("Total Reachable Paths: {}", total_solutions);
        println!("Unique Path: {}", if report.is_unique { "YES" } else { "NO" });
        println!("Pruned Branches: {}", report.pruned_branches_count);
        println!("Alpha Mode: {}", args.alpha);
        println!();

        for (idx, path) in report.solutions.iter().enumerate() {
            println!("--- Solution #{} (Consumed {} bits, Trailing padding {} bits) ---", 
                idx + 1, path.total_bits_consumed, path.trailing_padding_bits);
            for prop in &path.properties {
                println!("  Stat {}: {} (param={}, raw={}, val={}, bits={})", 
                    prop.stat_id, prop.name, prop.param, prop.raw_value, prop.value, prop.bits_consumed);
            }
        }
    }

    Ok(())
}