use bitstream_io::{BitRead, BitReader as IoBitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::item::{BitSegment, HuffmanTree, Item, find_next_item_match};
use d2r_core::verify::args::{ArgParser, ArgSpec};
use d2r_core::verify::{Report, ReportIssue, ReportMetadata, ReportStatus};
use serde::Serialize;
use std::env;
use std::fs;
use std::io::Cursor;

#[derive(Serialize)]
struct BitPeekItemJson {
    index: usize,
    code: String,
    start_bit: u64,
    bits_len: usize,
    segments: Vec<BitSegment>,
}

#[derive(Serialize)]
struct BitPeekJsonPayload {
    path: String,
    offset: u64,
    count_bits: u32,
    value_bin: Option<String>,
    jm_pos: Option<usize>,
    item_count: Option<u16>,
    items: Option<Vec<BitPeekItemJson>>,
}

fn main() {
    let mut parser = ArgParser::new("d2item_bit_peek");
    parser.add_spec(ArgSpec::positional("save_file", "Path to save file"));
    parser.add_spec(
        ArgSpec::positional("offset", "Bit offset from start")
            .optional()
            .with_default("0"),
    );
    parser.add_spec(
        ArgSpec::positional("count_bits", "Number of bits to read")
            .optional()
            .with_default("64"),
    );

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

    let is_json = parsed.is_set("json");
    let path = parsed.get("save_file").unwrap();
    let offset = parsed
        .get("offset")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let count_bits = parsed
        .get("count_bits")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(64);

    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            if is_json {
                let metadata = ReportMetadata::new("d2item_bit_peek", path, "unknown");
                let report = Report::<BitPeekJsonPayload>::new(metadata, ReportStatus::Fail)
                    .with_issues(vec![ReportIssue {
                        kind: "io".to_string(),
                        message: format!("Failed to read file: {}", e),
                        bit_offset: None,
                    }])
                    .with_hints(vec![
                        "Ensure the file path is correct and accessible.".to_string(),
                    ]);
                println!("{}", serde_json::to_string(&report).unwrap());
                std::process::exit(1);
            } else {
                panic!("failed to read save file: {}", e);
            }
        }
    };

    let mut issues = Vec::new();
    let mut payload = BitPeekJsonPayload {
        path: path.clone(),
        offset,
        count_bits,
        value_bin: None,
        jm_pos: None,
        item_count: None,
        items: None,
    };

    if offset > 0 {
        let mut reader =
            IoBitReader::endian(Cursor::new(&bytes[(offset / 8) as usize..]), LittleEndian);
        let _ = reader.skip((offset % 8) as u32);
        let mut cursor = BitCursor::new(reader);
        let val: u64 = cursor.read_bits::<u64>(count_bits).unwrap_or(0);

        if is_json {
            payload.value_bin = Some(format!("{:0width$b}", val, width = count_bits as usize));
            let metadata = ReportMetadata::new("d2item_bit_peek", path, "unknown");
            let report = Report::new(metadata, ReportStatus::Ok)
                .with_results(payload)
                .with_issues(issues);
            println!("{}", serde_json::to_string(&report).unwrap());
        } else {
            println!(
                "Bits at offset {}: {:0width$b}",
                offset,
                val,
                width = count_bits as usize
            );
        }
        return;
    }

    let jm_pos = match (0..bytes.len() - 2).find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M') {
        Some(pos) => pos,
        None => {
            if is_json {
                let metadata = ReportMetadata::new("d2item_bit_peek", path, "unknown");
                let report = Report::<BitPeekJsonPayload>::new(metadata, ReportStatus::Fail)
                    .with_issues(vec![ReportIssue {
                        kind: "format".to_string(),
                        message: "No JM marker found".to_string(),
                        bit_offset: None,
                    }])
                    .with_hints(vec![
                        "Not a valid D2 character save or severely truncated.".to_string(),
                    ]);
                println!("{}", serde_json::to_string(&report).unwrap());
                std::process::exit(1);
            } else {
                panic!("No JM marker found");
            }
        }
    };

    let count = u16::from_le_bytes([bytes[jm_pos + 2], bytes[jm_pos + 3]]);
    payload.jm_pos = Some(jm_pos);
    payload.item_count = Some(count);

    if !is_json {
        println!("JM at byte {}, item count {}", jm_pos, count);
    }

    let huffman = HuffmanTree::new();
    let reader = IoBitReader::endian(Cursor::new(&bytes[jm_pos + 4..]), LittleEndian);
    let mut recorder = BitCursor::new(reader);
    recorder.set_trace(true);

    let is_alpha = bytes[4..8] == [0x69, 0, 0, 0];
    let mut json_items = Vec::new();

    for i in 0..count {
        let bit_start = (jm_pos + 4) * 8 + recorder.pos() as usize;
        match Item::from_reader_with_context(
            &mut recorder,
            &huffman,
            Some((&bytes, ((jm_pos + 4) * 8) as u64)),
            is_alpha,
            i == 0,
            None,
        ) {
            Ok(item) => {
                if is_json {
                    let range_bits = item.range.end - item.range.start;
                    if range_bits != item.bits.len() as u64 {
                        issues.push(ReportIssue {
                            kind: "desync".to_string(),
                            message: format!(
                                "BitRange desync: Item reported {} bits, but bits.len() is {}",
                                range_bits,
                                item.bits.len()
                            ),
                            bit_offset: Some(bit_start as u64),
                        });
                    }

                    json_items.push(BitPeekItemJson {
                        index: i as usize,
                        code: item.code.clone(),
                        start_bit: bit_start as u64,
                        bits_len: item.bits.len(),
                        segments: recorder.segments().to_vec(),
                    });
                } else {
                    println!(
                        "Item {}: '{}' (start_bit={}, len={} bits) version={}",
                        i,
                        item.code,
                        bit_start,
                        item.bits.len(),
                        item.header.version
                    );

                    // Forensic BitRange Verification (Slice S2)
                    let range_bits = item.range.end - item.range.start;
                    if range_bits != item.bits.len() as u64 {
                        println!(
                            "  [FATAL] BitRange desync: Item reported {} bits, but bits.len() is {}",
                            range_bits,
                            item.bits.len()
                        );
                    } else {
                        println!("  [PASS] BitRange consistent: {} bits", range_bits);
                    }

                    println!("  BSLV Layout Tree:");
                    let mut segments = recorder.segments().to_vec();
                    // Sort by start bit (asc) and then by length (desc) to keep parents outside children
                    segments.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| b.end.cmp(&a.end)));

                    for seg in segments {
                        if seg.start == seg.end && seg.label == "Item Code" {
                            continue; // Skip noise from empty codes
                        }
                        let indent = "    ".repeat(seg.depth);
                        let len = seg.end - seg.start;
                        println!(
                            "  {}[{:>4}..{:>4}] (len={:>2}) {}",
                            indent, seg.start, seg.end, len, seg.label
                        );
                    }

                    if i == 0 {
                        // [DEBUG] Future: Implementation of proper non-advancing peek required.
                    }
                }
            }
            Err(e) => {
                if is_json {
                    issues.push(ReportIssue {
                        kind: "parse".to_string(),
                        message: format!("Error at Item {}: {}", i, e),
                        bit_offset: Some(bit_start as u64),
                    });
                } else {
                    println!("Error at Item {}: {}", i, e);
                }

                // [Slice 3] Recovery Mode: Scan for the next item
                if is_alpha {
                    let current_bit_pos = recorder.pos();
                    let absolute_pos = ((jm_pos + 4) * 8) as u64 + current_bit_pos;
                    println!(
                        "  [DEBUG] Recovery Mode triggered at bit {}. Scanning for next item candidate...",
                        absolute_pos
                    );

                    let mut probe = absolute_pos + 1;
                    let limit = (bytes.len() * 8) as u64;
                    let mut found = false;
                    while probe < limit {
                        // Alpha v105 items are byte-aligned.
                        if probe % 8 != 0 {
                            probe += 1;
                            continue;
                        }

                        if let Some((
                            mode,
                            location,
                            x,
                            code,
                            flags,
                            version,
                            is_compact,
                            header_bits,
                            _nudge,
                            _has_checksum,
                        )) = d2r_core::item::peek_item_header_at(&bytes, probe, &huffman, true)
                        {
                            if d2r_core::item::is_plausible_item_header(
                                mode, location, &code, flags, version, true,
                            ) {
                                let gap_from_start = probe as i64 - bit_start as i64;
                                println!(
                                    "  [RECOVERY] Next item candidate '{}' found at bit {} (Gap from Item {} start: {} bits, version={})",
                                    code, probe, i, gap_from_start, version
                                );

                                // Re-sync recorder to the next item
                                let recorder_new_pos = probe - ((jm_pos + 4) * 8) as u64;
                                let skip_bits = recorder_new_pos - current_bit_pos;
                                let _ = recorder.skip_and_record(skip_bits as u32);
                                found = true;
                                break;
                            }
                        }
                        probe += 8;
                    }

                    if found {
                        continue;
                    } else {
                        println!(
                            "  [DEBUG] No next item candidate found after scanning to end of file."
                        );
                    }
                }
                break;
            }
        }
    }

    if is_json {
        payload.items = Some(json_items);
        let version = if is_alpha { "105" } else { "unknown" };
        let metadata = ReportMetadata::new("d2item_bit_peek", path, version);
        let report = Report::new(
            metadata,
            if issues.is_empty() {
                ReportStatus::Ok
            } else {
                ReportStatus::Fail
            },
        )
        .with_results(payload)
        .with_issues(issues);
        println!("{}", serde_json::to_string(&report).unwrap());
    }
}
