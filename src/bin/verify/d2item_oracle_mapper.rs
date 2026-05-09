use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::item::{HuffmanTree, is_plausible_item_header, peek_item_header_at};
use d2r_core::save::find_jm_markers;
use d2r_core::verify::args::{ArgParser, ArgSpec, ArgError};
use serde::Serialize;
use std::env;
use std::fs;
use std::io::{self, Cursor};

use d2r_core::report::Report;

#[derive(Serialize, Debug, Clone)]
struct ScanAnchor {
    bit_offset: u64,
    code: String,
    flags: u32,
    version: u8,
    mode: u8,
    location: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    best_width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<i32>,
}

fn main() -> io::Result<()> {
    let mut parser = ArgParser::new("d2item_oracle_mapper")
        .description("Mathematically infer item bit-widths and map JM anchors in D2R save files");

    parser.add_spec(ArgSpec::positional("save_path", "path to the D2R save file (.d2s)"));
    parser.add_spec(ArgSpec::flag("list-anchors", None, Some("list-anchors"), "list all plausible JM item anchors"));
    parser.add_spec(ArgSpec::flag("auto-map", None, Some("auto-map"), "mathematically infer bit-widths for all items"));
    parser.add_spec(ArgSpec::flag("heatmap", None, Some("heatmap"), "show a bit-level heatmap for found items"));
    parser.add_spec(ArgSpec::option("width", Some('w'), Some("width"), "heatmap display width (default: 10)").with_default("10"));

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            std::process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}\n\n{}", e, parser.usage());
            std::process::exit(1);
        }
    };

    let path = parsed.get("save_path").unwrap();
    let list_anchors = parsed.is_set("list-anchors");
    let auto_map = parsed.is_set("auto-map");
    let heatmap = parsed.is_set("heatmap");
    let mut om = d2r_core::verify::OutputManager::new("d2item_oracle_mapper", &parsed);
    let show_json = om.is_json();
    let heatmap_width: u32 = parsed.get("width").and_then(|s| s.parse().ok()).unwrap_or(10);

    let bytes = fs::read(path)?;
    let huffman = HuffmanTree::new();

    let mut scan_results = Vec::new();

    let jm_markers = find_jm_markers(&bytes);
    for &jm_pos in &jm_markers {
        let section_start = (jm_pos + 4) * 8;
        let mut bit_cursor = section_start as u64;
        let section_end = (bytes.len() * 8) as u64;

        while bit_cursor < section_end {
            if let Some((mode, location, _, code, flags, version, _is_compact, header_bits, _nudge, _has_checksum)) =
                            peek_item_header_at(&bytes, bit_cursor, &huffman, true)
                        {
                if is_plausible_item_header(mode, location, &code, flags, version, true) {
                    let mut record = ScanAnchor {
                        bit_offset: bit_cursor,
                        code: code.trim().to_string(),
                        flags,
                        version,
                        mode,
                        location,
                        best_width: None,
                        score: None,
                    };

                    if auto_map {
                        let stats_offset = if record.version == 5 || record.version == 1 {
                            19
                        } else {
                            0
                        };
                        let (width, score) =
                            infer_bit_width(&bytes, bit_cursor + header_bits + stats_offset, version);
                        record.best_width = Some(width);
                        record.score = Some(score);
                    }

                    scan_results.push(record);
                    bit_cursor += header_bits;
                    continue;
                }
            }
            bit_cursor += 1;
        }
    }

    if list_anchors || auto_map {
        if show_json {
            let report = Report::new(path, scan_results.clone());
            om.println(&serde_json::to_string_pretty(&report).unwrap());
        } else {
            om.println("| Anchor Bit | Code | Flags      | Ver | Mode | Loc | Width | Score |");
            om.println("|------------|------|------------|-----|------|-----|-------|-------|");
            for a in &scan_results {
                let width_str = a
                    .best_width
                    .map(|w| w.to_string())
                    .unwrap_or_else(|| "-".to_string());
                let score_str = a
                    .score
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "-".to_string());
                om.println(&format!(
                    "| {:10} | {:4} | {:#010x} | {:3} | {:4} | {:3} | {:5} | {:5} |",
                    a.bit_offset,
                    a.code,
                    a.flags,
                    a.version,
                    a.mode,
                    a.location,
                    width_str,
                    score_str
                ));
            }
        }
    }

    if heatmap && !show_json {
        for a in &scan_results {
            if a.code == "gp" {
                continue;
            }
            om.println(&format!(
                "\n[Heatmap] Code: {}, Start: {}, Width: {}",
                a.code, a.bit_offset, heatmap_width
            ));
            let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
            let start = a.bit_offset + 60;
            if reader.skip(start as u32).is_err() {
                continue;
            }

            for row in 0..10 {
                let mut line = format!("  Row {:2}: ", row);
                for _ in 0..heatmap_width {
                    let bit = reader.read_bit().unwrap_or(false);
                    line.push(if bit { '1' } else { '0' });
                }
                om.println(&line);
            }
        }
    }

    om.summary("Scan complete.");
    Ok(())
}

fn infer_bit_width(bytes: &[u8], stats_start_bit: u64, _version: u8) -> (u32, i32) {
    let mut best_width = 10;
    let mut best_score = -10000;

    for width in 8..=24 {
        let mut score = 0;
        let mut found_terminator = false;

        let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
        if reader.skip(stats_start_bit as u32).is_err() {
            continue;
        }

        for _p in 0..128 {
            let stat_id = match reader.read_var::<u16>(9) {
                Ok(id) => id,
                Err(_) => break,
            };

            if stat_id == 0x1FF {
                found_terminator = true;
                score += 1000;
                break;
            }

            if stat_id < 512 {
                score += 20;
            } else {
                score -= 1000;
                break;
            }

            let val_bits = (width as i32 - 9).max(0) as u32;
            if reader.skip(val_bits).is_err() {
                break;
            }
        }

        if !found_terminator {
            score -= 5000;
        }

        if width == 10 {
            score += 1;
        }

        if score > best_score {
            best_score = score;
            best_width = width;
        }
    }

    (best_width, best_score)
}
