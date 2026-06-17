use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use serde_json::json;
use std::env;
use std::fs;
use std::io::Cursor;

fn print_bits_window(bytes: &[u8], start_bit: usize, bit_count: usize) {
    let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
    let _ = reader.skip(start_bit as u32).unwrap_or(());
    println!("Bits from {} ({} bits):", start_bit, bit_count);
    for i in 0..bit_count {
        let bit = reader.read_bit().unwrap_or(false);
        print!("{}", if bit { '1' } else { '0' });
        if (i + 1) % 8 == 0 {
            print!(" ");
        }
        if (i + 1) % 32 == 0 {
            println!("(bit {})", start_bit + i + 1);
        }
    }
    println!();
}

fn read_bits(reader: &mut BitReader<Cursor<&[u8]>, LittleEndian>, count: u32) -> u32 {
    reader.read_var(count).unwrap_or(0)
}

fn analyze_non_compact_item(bytes: &[u8], bit_start: usize, huffman: &HuffmanTree) {
    let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
    let _ = reader.skip(bit_start as u32).unwrap_or(());
    let mut offset = bit_start;

    let flags = read_bits(&mut reader, 32);
    println!(
        "  flags           {:>5}-{:>5} = 0x{:08X}",
        offset,
        offset + 32,
        flags
    );
    offset += 32;

    let version = read_bits(&mut reader, 3);
    let mode = read_bits(&mut reader, 3);
    let location = read_bits(&mut reader, 4);
    let x = read_bits(&mut reader, 4);
    let y = read_bits(&mut reader, 4);
    let page = read_bits(&mut reader, 3);
    println!(
        "  version         {:>5}-{:>5} = {}",
        offset,
        offset + 3,
        version
    );
    offset += 3;
    println!(
        "  mode            {:>5}-{:>5} = {}",
        offset,
        offset + 3,
        mode
    );
    offset += 3;
    println!(
        "  location        {:>5}-{:>5} = {}",
        offset,
        offset + 4,
        location
    );
    offset += 4;
    println!("  x               {:>5}-{:>5} = {}", offset, offset + 4, x);
    offset += 4;
    println!("  y               {:>5}-{:>5} = {}", offset, offset + 4, y);
    offset += 4;
    println!(
        "  page            {:>5}-{:>5} = {}",
        offset,
        offset + 3,
        page
    );
    offset += 3;

    let code_start = offset;
    let mut code = String::new();
    for _ in 0..4 {
        code.push(huffman.decode(&mut reader).unwrap_or('?'));
    }
    let code_end = reader.position_in_bits().unwrap_or(0) as usize;
    println!(
        "  code            {:>5}-{:>5} = '{}'",
        code_start, code_end, code
    );
    offset = code_end;

    let socketed_count = read_bits(&mut reader, 3);
    println!(
        "  post-code bits  {:>5}-{:>5} = {}",
        offset,
        offset + 3,
        socketed_count
    );
    offset += 3;

    let id = read_bits(&mut reader, 32);
    let level = read_bits(&mut reader, 7);
    let quality = read_bits(&mut reader, 4);
    let multi_graphics = read_bits(&mut reader, 1);
    println!(
        "  id              {:>5}-{:>5} = {}",
        offset,
        offset + 32,
        id
    );
    offset += 32;
    println!(
        "  level           {:>5}-{:>5} = {}",
        offset,
        offset + 7,
        level
    );
    offset += 7;
    println!(
        "  quality         {:>5}-{:>5} = {}",
        offset,
        offset + 4,
        quality
    );
    offset += 4;
    println!(
        "  has graphics    {:>5}-{:>5} = {}",
        offset,
        offset + 1,
        multi_graphics
    );
    offset += 1;
    if multi_graphics != 0 {
        let graphic_id = read_bits(&mut reader, 3);
        println!(
            "  graphic id      {:>5}-{:>5} = {}",
            offset,
            offset + 3,
            graphic_id
        );
        offset += 3;
    }

    let class_specific = read_bits(&mut reader, 1);
    println!(
        "  class specific  {:>5}-{:>5} = {}",
        offset,
        offset + 1,
        class_specific
    );
    offset += 1;
    if class_specific != 0 {
        let class_bits = read_bits(&mut reader, 11);
        println!(
            "  class data      {:>5}-{:>5} = {}",
            offset,
            offset + 11,
            class_bits
        );
        offset += 11;
    }

    println!("  next 64 bits from {}", offset);
    print_bits_window(bytes, offset, 64);

    println!("  0x1FF candidates after {}", offset);
    for delta in 0..48 {
        let mut probe = BitReader::endian(Cursor::new(bytes), LittleEndian);
        let _ = probe.skip((offset + delta) as u32).unwrap_or(());
        if probe.read::<9, u32>().unwrap_or(0) == 0x1FF {
            println!("    offset {} -> bit {}", delta, offset + delta);
        }
    }
}

fn item_to_json(item: &Item, provenance: Option<serde_json::Value>) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert("code".to_string(), json!(item.code.trim()));
    map.insert("bit_length".to_string(), json!(item.range.end - item.range.start));
    map.insert("stats".to_string(), json!(item.properties.iter().map(|p| {
        json!({
            "id": p.stat_id,
            "name": p.name,
            "is_unknown": p.name.starts_with("Unknown")
        })
    }).collect::<Vec<_>>()));
    
    let residue = item.modules.iter().find_map(|m| {
        if let d2r_core::item::ItemModule::Residue(bits) = m {
            Some(bits.iter().map(|&b| if b { '1' } else { '0' }).collect::<String>())
        } else {
            None
        }
    });
    map.insert("residue_bits".to_string(), json!(residue));
    
    if let Some(prov) = provenance {
        map.insert("provenance".to_string(), prov);
    }
    
    serde_json::Value::Object(map)
}

fn classify_trace_ownership(
    item: &Item,
    scanner_hint: &str,
    normalized_code: &str,
    final_code: &str,
    gap_len: usize,
    gap_source: &str,
    emitter_bypass: bool,
) -> (String, String) {
    let padding_signals = emitter_bypass
        || item.is_opaque()
        || item.is_semi_opaque()
        || gap_source == "normalization:opaque_fallback";
        
    let is_kk_seam_drift = (scanner_hint.starts_with("wc") || scanner_hint.contains("wc"))
        && (final_code == "wwsl" || final_code == "wwu8")
        && gap_source == "normalization:drift_realigned";

    let replay_signals = gap_source == "header_gap_lookup"
        || is_kk_seam_drift
        || (!scanner_hint.is_empty()
            && scanner_hint == normalized_code
            && normalized_code == final_code
            && gap_len > 0);

    let ownership_hint = match (replay_signals, padding_signals) {
        (true, false) => "capture_replay",
        (false, true) => "emission_padding",
        _ => "ambiguous",
    };

    let ownership_reason = match ownership_hint {
        "capture_replay" => {
            if is_kk_seam_drift {
                format!(
                    "k  k seam drift identified: scanner_hint='{}' misaligned to final_code='{}' under drift_realigned. This is a capture_replay parsing geometry mismatch.",
                    scanner_hint, final_code
                )
            } else {
                format!(
                    "Header-derived replay signals dominate here: scanner_hint='{}', normalized_code='{}', final_code='{}', gap_len={}, gap_source='{}'.",
                    scanner_hint, normalized_code, final_code, gap_len, gap_source
                )
            }
        }
        "emission_padding" => format!(
            "Padding-preserving emission signals dominate here: emitter_bypass={}, gap_source='{}', final_code='{}'.",
            emitter_bypass, gap_source, final_code
        ),
        _ => format!(
            "Signals remain split between replay and padding: scanner_hint='{}', normalized_code='{}', final_code='{}', gap_len={}, gap_source='{}', emitter_bypass={}.",
            scanner_hint, normalized_code, final_code, gap_len, gap_source, emitter_bypass
        ),
    };

    (ownership_hint.to_string(), ownership_reason)
}

fn main() {
    let mut parser = ArgParser::new("d2item_inspect")
        .description("Decomposes a .d2i or .d2s item into its bit-fields and props.");
    parser.add_spec(ArgSpec::positional("file", "Path to .d2i or .d2s file"));
    parser.add_spec(ArgSpec::flag(
        "json",
        None,
        Some("json"),
        "Output results in JSON format",
    ));
    parser.add_spec(ArgSpec::option(
        "bit-offset",
        None,
        Some("bit-offset"),
        "Start parsing at specific bit offset",
    ));
    parser.add_spec(ArgSpec::flag(
        "trace-provenance",
        None,
        Some("trace-provenance"),
        "Trace item code provenance (scanner hint, normalized code, final code)",
    ));

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return;
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}\n\n{}", e, parser.usage());
            std::process::exit(1);
        }
    };

    let path = parsed.get("file").unwrap();
    let is_json = parsed.is_json();
    let bit_offset = parsed
        .get("bit-offset")
        .and_then(|s| s.parse::<usize>().ok());
    let trace_provenance = parsed.is_set("trace-provenance");

    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            if is_json {
                println!(
                    "{}",
                    json!({"errors": [format!("Failed to read file: {}", e)]})
                );
            } else {
                eprintln!("Failed to read file: {}", e);
            }
            return;
        }
    };
    let huffman = HuffmanTree::new();

    let version_raw = if bytes.len() >= 8 {
        u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]))
    } else {
        0
    };
    let is_alpha = version_raw == 105 || version_raw == 6;

    if let Some(offset) = bit_offset {
        let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let _ = reader.skip(offset as u32).unwrap_or(());
        match Item::from_reader(&mut reader, &huffman, is_alpha) {
            Ok(item) => {
                let bit_end = reader.position_in_bits().unwrap_or(0) as usize;

                let provenance = if trace_provenance && is_alpha {
                    let scanner_hint = d2r_core::domain::item::serialization::peek_item_header_at_with_base(
                        &bytes,
                        offset as u64,
                        Some(offset as u64),
                        &huffman,
                        true,
                        0,
                    )
                    .map(|p| p.3.trim().to_string())
                    .unwrap_or_default();

                    let (normalized_code, gap_len, gap_source) = {
                        let mut reader2 = BitReader::endian(Cursor::new(&bytes), LittleEndian);
                        let _ = reader2.skip(offset as u32).unwrap_or(());
                        let mut cursor = d2r_core::data::bit_cursor::BitCursor::new(&mut reader2);
                        
                        let gap_override = d2r_core::domain::item::serialization::peek_item_header_at_with_base(
                            &bytes,
                            offset as u64,
                            Some(offset as u64),
                            &huffman,
                            true,
                            0,
                        ).map(|p| {
                            let mut gap = p.8 as usize;
                            if p.5 == 7 && !p.6 {
                                gap = gap.saturating_sub(45);
                            }
                            gap
                        });
                        
                        let has_checksum_peek = d2r_core::domain::item::serialization::peek_item_header_at_with_base(
                            &bytes,
                            offset as u64,
                            Some(offset as u64),
                            &huffman,
                            true,
                            0,
                        ).map(|p| p.9);

                        if let Ok((header, _, _)) = d2r_core::domain::item::entity::parse_item_header(
                            &mut cursor,
                            true,
                            Some(scanner_hint.as_str()),
                            gap_override,
                            true,
                            None,
                            has_checksum_peek,
                            Some(offset as u64),
                        ) {
                            if header.is_compact {
                                cursor.base_pos = offset as u64;
                            }
                            let s_axiom = d2r_core::domain::stats::axiom::StatsAxiom::new(
                                header.version,
                                header.quality.unwrap_or(d2r_core::domain::item::quality::ItemQuality::Normal),
                                true,
                            );
                            let is_ho = s_axiom.is_header_only(header.flags, Some(scanner_hint.as_str()).unwrap_or(""));

                            if is_ho {
                                (scanner_hint.clone(), 0usize, "header_only".to_string())
                            } else {
                                let gap_len = if scanner_hint.trim() == "buc" || matches!(header.version, 1) {
                                    0
                                } else {
                                    s_axiom.header_gap(&scanner_hint, header.flags)
                                };
                                if gap_len > 0 {
                                    let _ = cursor.skip(gap_len as u64);
                                }
                                let mut decoded = String::new();
                                let mut ok = true;
                                for _ in 0..4 {
                                    if let Ok(c) = huffman.decode(&mut reader2) {
                                        decoded.push(c);
                                    } else {
                                        ok = false;
                                        break;
                                    }
                                }
                                if ok {
                                    let decoded_trimmed = decoded.trim().to_string();
                                    let gap_source = if gap_len > 0 {
                                        "header_gap_lookup".to_string()
                                    } else {
                                        if item.is_opaque() || item.is_semi_opaque() {
                                            "normalization:opaque_fallback".to_string()
                                        } else if decoded_trimmed == item.code.trim() {
                                            "normalization:match_target".to_string()
                                        } else {
                                            "normalization:drift_realigned".to_string()
                                        }
                                    };
                                    (decoded_trimmed, gap_len as usize, gap_source)
                                } else {
                                    let gap_source = if item.is_opaque() || item.is_semi_opaque() {
                                        "normalization:opaque_fallback".to_string()
                                    } else {
                                        "normalization:drift_realigned".to_string()
                                    };
                                    ("".to_string(), gap_len as usize, gap_source)
                                }
                            }
                        } else {
                            ("".to_string(), 0usize, "unresolved".to_string())
                        }
                    };

                    let final_code = item.code.trim().to_string();
                    
                    let emitter_bypass = {
                        let trimmed_code = item.code.trim_matches(|c: char| c.is_whitespace() || c == '\0');
                        let is_target_blank = is_alpha && trimmed_code.is_empty();
                        item.is_opaque() || item.is_semi_opaque() || is_target_blank
                    };

                    let (ownership_hint, ownership_reason) = classify_trace_ownership(
                        &item,
                        &scanner_hint,
                        &normalized_code,
                        &final_code,
                        gap_len,
                        &gap_source,
                        emitter_bypass,
                    );

                    let clean_code = |s: &str| {
                        s.split_whitespace()
                            .next()
                            .unwrap_or("")
                            .trim_matches(|c: char| c.is_whitespace() || c == '\0')
                            .to_string()
                    };
                    let clean_scanner = clean_code(&scanner_hint);
                    let clean_final = clean_code(&final_code);

                    // DIAGNOSTICS-CONTRACT: Exposes registry overrides to prevent AI reasoning desync.
                    // Do not remove this block unless performing structural refactoring of the diagnostics channel.
                    let reg = d2r_core::domain::forensic::registry::get_registry();
                    let reg_override = reg.item_overrides.as_ref()
                        .and_then(|overrides| {
                            overrides.get(&clean_scanner)
                                .or_else(|| overrides.get(&clean_final))
                        })
                        .map(|map| json!(map))
                        .unwrap_or(serde_json::Value::Null);

                    Some(json!({
                        "scanner_hint": scanner_hint,
                        "normalized_code": normalized_code,
                        "final_code": final_code,
                        "gap_len": gap_len,
                        "gap_source": gap_source,
                        "emitter_bypass": emitter_bypass,
                        "ownership_hint": ownership_hint,
                        "ownership_reason": ownership_reason,
                        "registry_override": reg_override
                    }))
                } else {
                    None
                };

                if is_json {
                    println!(
                        "{}",
                        json!({ "item": item_to_json(&item, provenance), "errors": [], "range": {"start": offset, "end": bit_end} })
                    );
                } else {
                    println!(
                        "Parsed item at offset {}: '{}' bits {}-{} loc={} quality={:?}",
                        offset, item.code, offset, bit_end, item.location, item.header.quality
                    );
                    if let Some(ref prov) = provenance {
                        println!("  [PROVENANCE]");
                        println!("    Scanner Hint   : {}", prov["scanner_hint"].as_str().unwrap_or(""));
                        println!("    Normalized Code: {}", prov["normalized_code"].as_str().unwrap_or(""));
                        println!("    Final Code     : {}", prov["final_code"].as_str().unwrap_or(""));
                        println!("    Gap Len        : {}", prov["gap_len"].as_u64().unwrap_or(0));
                        println!("    Gap Source     : {}", prov["gap_source"].as_str().unwrap_or(""));
                        println!("    Emitter Bypass : {}", prov["emitter_bypass"].as_bool().unwrap_or(false));
                        println!("    Ownership Hint : {}", prov["ownership_hint"].as_str().unwrap_or(""));
                        println!("    Ownership Reason: {}", prov["ownership_reason"].as_str().unwrap_or(""));
                        if !prov["registry_override"].is_null() {
                            println!("    Registry Override: {:?}", prov["registry_override"]);
                        }
                    }
                    for prop in &item.properties {
                        println!(
                            "  Prop: id={} value={} param={} bits {}-{}",
                            prop.stat_id,
                            prop.raw_value,
                            prop.param,
                            prop.range.start,
                            prop.range.end
                        );
                    }
                }
            }
            Err(e) => {
                // DIAGNOSTICS-CONTRACT: Exposes registry overrides on parsing crash sites to triage geometry desync.
                // Do not remove this block unless performing structural refactoring of the diagnostics channel.
                let mut prescription = String::new();
                if is_alpha {
                    let peeked_code = d2r_core::domain::item::serialization::peek_item_header_at_with_base(
                        &bytes,
                        offset as u64,
                        Some(offset as u64),
                        &huffman,
                        true,
                        0,
                    ).map(|p| {
                        p.3.split_whitespace()
                            .next()
                            .unwrap_or("")
                            .trim_matches(|c: char| c.is_whitespace() || c == '\0')
                            .to_string()
                    });

                    if let Some(ref code) = peeked_code {
                        let reg = d2r_core::domain::forensic::registry::get_registry();
                        if let Some(overrides) = &reg.item_overrides {
                            if let Some(map) = overrides.get(code) {
                                prescription = format!(
                                    " [Prescription: Active Registry Override Detected for code '{}' at offset {}. Registry has overrides: {:?}. This might cause geometry parsing conflict/desync.]",
                                    code, offset, map
                                );
                            }
                        }
                    }
                }

                if is_json {
                    let err_msg = if prescription.is_empty() {
                        format!("Error at offset {}: {}", offset, e)
                    } else {
                        format!("Error at offset {}: {}{}", offset, e, prescription)
                    };
                    println!(
                        "{}",
                        json!({ "errors": [err_msg] })
                    );
                } else {
                    if prescription.is_empty() {
                        eprintln!("Error at offset {}: {}", offset, e);
                    } else {
                        eprintln!("Error at offset {}: {}{}", offset, e, prescription);
                    }
                    analyze_non_compact_item(&bytes, offset, &huffman);
                }
            }
        }
        return;
    }

    // 1. Try reading as player items (save file format)
    if let Ok(items) = Item::read_player_items(&bytes, &huffman, is_alpha) {
        if is_json {
            let item_objs: Vec<_> = items.iter().map(|it| item_to_json(it, None)).collect();
            println!("{}", json!({"items": item_objs, "errors": []}));
            return;
        }

        println!(
            "Library parse recovered {} top-level items from player section",
            items.len()
        );
        for (i, item) in items.iter().enumerate() {
            println!(
                "Item {:2}: '{:4}' mode={} loc={} flags=0x{:08X} name={:?} children={} range={}-{}",
                i,
                item.code,
                item.header.mode,
                item.header.location,
                item.flags,
                item.personalized_player_name,
                item.socketed_items.len(),
                item.range.start,
                item.range.end
            );
            for prop in &item.properties {
                println!(
                    "  Prop: id={} value={} param={} bits {}-{}",
                    prop.stat_id, prop.raw_value, prop.param, prop.range.start, prop.range.end
                );
            }

            for (socket_index, child) in item.socketed_items.iter().enumerate() {
                println!(
                    "  socket {:2}: '{}' mode={} loc={}",
                    socket_index, child.code, child.mode, child.location
                );
            }
        }
        return;
    }

    // 2. Fallback: search for JM markers or treat as raw item
    let jm_pos =
        (0..bytes.len().saturating_sub(2)).find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M');

    let (section_bytes, jm_offset_bits, item_count) = if let Some(pos) = jm_pos {
        let count = u16::from_le_bytes([
            bytes.get(pos + 2).cloned().unwrap_or(0),
            bytes.get(pos + 3).cloned().unwrap_or(0),
        ]);
        let next_jm = (pos + 4..bytes.len().saturating_sub(1))
            .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
            .unwrap_or(bytes.len());
        (&bytes[pos + 4..next_jm], (pos + 4) * 8, count)
    } else {
        (&bytes[..], 0, 1)
    };

    let section_bits = (section_bytes.len() * 8) as u64;
    let mut reader = BitReader::endian(Cursor::new(section_bytes), LittleEndian);
    let mut visible_items: Vec<(usize, usize, Item)> = Vec::new();
    let mut errors = Vec::new();
    let mut raw_index = 0usize;

    while reader.position_in_bits().unwrap_or(section_bits) < section_bits {
        let _ = reader.byte_align();
        let pos = reader.position_in_bits().unwrap_or(0);
        if pos >= section_bits {
            break;
        }
        let bit_start = jm_offset_bits + pos as usize;

        match Item::from_reader(&mut reader, &huffman, is_alpha) {
            Ok(item) => {
                let pos_end = reader.position_in_bits().unwrap_or(0);
                let bit_end = jm_offset_bits + pos_end as usize;
                if item.mode == 6 {
                    if let Some((_, _, parent)) = visible_items.last_mut() {
                        parent.socketed_items.push(item);
                    } else {
                        errors.push(format!(
                            "Error at raw item {}: socketed item without a parent",
                            raw_index
                        ));
                        break;
                    }
                } else {
                    visible_items.push((bit_start, bit_end, item));
                }
            }
            Err(e) => {
                if visible_items.len() >= item_count as usize {
                    break;
                }
                errors.push(format!("Error at raw item {}: {}", raw_index, e));
                if !is_json {
                    analyze_non_compact_item(&bytes, bit_start, &huffman);
                }
                break;
            }
        }
        raw_index += 1;
    }

    if is_json {
        let item_objs: Vec<_> = visible_items
            .iter()
            .map(|(_, _, it)| item_to_json(it, None))
            .collect();
        if item_objs.len() == 1 {
            println!("{}", json!({ "item": item_objs[0], "errors": errors }));
        } else {
            println!("{}", json!({ "items": item_objs, "errors": errors }));
        }
    } else {
        println!(
            "Parsed {} visible items from a section expecting {} top-level items",
            visible_items.len(),
            item_count
        );

        for (i, (bit_start, bit_end, item)) in visible_items.iter().enumerate() {
            println!(
                "Item {:2}: '{}' bits {}-{} loc={} socketed_children={}",
                i,
                item.code,
                bit_start,
                bit_end,
                item.location,
                item.socketed_items.len()
            );
            for (socket_index, child) in item.socketed_items.iter().enumerate() {
                println!(
                    "  socket {:2}: '{}' loc={}",
                    socket_index, child.code, child.location
                );
            }
        }
    }
}
