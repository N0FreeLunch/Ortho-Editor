use bitstream_io::BitRead;
use d2r_core::item::{HuffmanTree, Item, ItemQuality};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::{OutputManager, Report, ReportMetadata, ReportStatus};
use serde::Serialize;
use std::env;
use std::fs;
use std::io;
use std::process;

#[derive(Debug, Serialize)]
pub struct ChunkVerifyPayload {
    pub save_path: String,
    pub checksum: ChecksumInfo,
    pub sections: Vec<SectionInfo>,
    pub patterns: Vec<PatternMatch>,
    pub progression: ProgressionInfo,
    pub items: Vec<ItemSummary>,
}

#[derive(Debug, Serialize)]
pub struct ChecksumInfo {
    pub original: u32,
    pub calculated: u32,
    pub is_valid: bool,
}

#[derive(Debug, Serialize)]
pub struct SectionInfo {
    pub name: String,
    pub mark: String,
    pub start: usize,
    pub end: usize,
    pub length: usize,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct PatternMatch {
    pub name: String,
    pub offset: Option<usize>,
    pub found: bool,
}

#[derive(Debug, Serialize)]
pub struct ProgressionInfo {
    pub completed_quests: Vec<String>,
    pub activated_waypoints: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ItemSummary {
    pub index: usize,
    pub code: String,
    pub bits: usize,
    pub quality: String,
    pub is_runeword: bool,
    pub mode: u8,
    pub page: u8,
    pub location: u8,
}

fn main() -> io::Result<()> {
    let mut parser = ArgParser::new("d2item_chunk_verify")
        .description("Analyzes save file structure and character progression, with optional item detail/range view");

    parser.add_spec(ArgSpec::positional(
        "save_file",
        "path to the save file (.d2s)",
    ));
    parser.add_spec(ArgSpec::option(
        "range",
        Some('r'),
        Some("range"),
        "scan summary range encoded as START..END (default: 0..10)",
    ));
    parser.add_spec(ArgSpec::option(
        "detail",
        Some('d'),
        Some("detail"),
        "item index for detail view",
    ));

    let args_os: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args_os) {
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

    let mut out = OutputManager::try_new("d2item_chunk_verify", &parsed).unwrap_or_else(|error| {
        eprintln!("Failed to initialize output destination: {error}");
        std::process::exit(1);
    });
    let path = parsed.get("save_file").unwrap();
    let mut range_start = 0;
    let mut range_end = 10;
    let mut detail_index: Option<usize> = None;

    if let Some(range_str) = parsed.get("range") {
        let parts: Vec<&str> = range_str.split("..").collect();
        if parts.len() == 2 {
            range_start = parts[0].parse().unwrap_or(0);
            range_end = parts[1].parse().unwrap_or(10);
        }
    }

    if let Some(detail_str) = parsed.get("detail") {
        detail_index = detail_str.parse().ok();
    }

    let bytes = fs::read(path)?;
    let huffman = HuffmanTree::new();

    // Map save file sections
    let map = match d2r_core::save::map_core_sections(&bytes) {
        Ok(m) => m,
        Err(e) => {
            out.try_defer_println(&format!("[ERROR] Failed to map save sections: {}", e))?;
            out.try_finish()?;
            return Ok(());
        }
    };

    // Verify Checksum
    let original_checksum = u32::from_le_bytes(bytes[12..16].try_into().unwrap_or([0; 4]));
    let calculated_checksum = d2r_core::save::recalculate_checksum(&bytes).unwrap_or(0);
    let is_checksum_valid = original_checksum == calculated_checksum;
    let checksum_status = if is_checksum_valid {
        format!("VALID (0x{:08X})", original_checksum)
    } else {
        format!(
            "INVALID (Expected 0x{:08X}, Got 0x{:08X})",
            calculated_checksum, original_checksum
        )
    };

    let mut sections = Vec::new();
    // 1. Header
    sections.push(SectionInfo {
        name: "Header".to_string(),
        mark: "-".to_string(),
        start: 0,
        end: map.gf_pos,
        length: map.gf_pos,
        status: "[OK]".to_string(),
    });
    // 2. Attributes (gf)
    sections.push(SectionInfo {
        name: "Attributes".to_string(),
        mark: "gf".to_string(),
        start: map.gf_pos,
        end: map.if_pos,
        length: map.if_pos - map.gf_pos,
        status: "[OK]".to_string(),
    });
    // 3. Skills (if)
    let skill_len = 2 + d2r_core::save::SKILL_SECTION_LEN;
    let skill_end = map.if_pos + skill_len;
    sections.push(SectionInfo {
        name: "Skills".to_string(),
        mark: "if".to_string(),
        start: map.if_pos,
        end: skill_end,
        length: skill_len,
        status: "[OK]".to_string(),
    });
    // 4. Gap (Quest/Progression?)
    let jm0 = map.jm_positions[0];
    let gap_len = jm0.saturating_sub(skill_end);
    let gap_status = if gap_len > 0 {
        format!("[?? {} bytes]", gap_len)
    } else {
        "[None]".to_string()
    };
    sections.push(SectionInfo {
        name: "Gap (Quest?)".to_string(),
        mark: "-".to_string(),
        start: skill_end,
        end: jm0,
        length: gap_len,
        status: gap_status,
    });
    // 5. Items (First JM to End)
    sections.push(SectionInfo {
        name: "Items (JM total)".to_string(),
        mark: "JM".to_string(),
        start: jm0,
        end: bytes.len(),
        length: bytes.len() - jm0,
        status: format!("[{} Sects]", map.jm_positions.len()),
    });

    // Print Save Structure Table
    out.try_defer_println("=== Save File Structure ===")?;
    out.try_defer_println(&format!("Checksum: {}", checksum_status))?;
    out.try_defer_println("")?;
    out.try_defer_println(&format!(
        "{:<20} | {:<4} | {:<10} | {:<10} | {:<10} | {:<10}",
        "Section", "Mark", "Start(Hex)", "End(Hex)", "Len(Dec)", "Status"
    ))?;
    out.try_defer_println(
        "---------------------|------|------------|------------|------------|-----------",
    )?;

    for s in &sections {
        out.try_defer_println(&format!(
            "{:<20} | {:<4} | 0x{:08X} | 0x{:08X} | {:>10} | {}",
            s.name, s.mark, s.start, s.end, s.length, s.status
        ))?;
    }
    out.try_defer_println("")?;

    // === Progression Sections (Header) ===
    out.try_defer_println("=== Progression Sections (Header) ===")?;

    // Wide-range Signature Scanning
    let header_range = bytes.len().min(0x341);
    let header_slice = &bytes[..header_range];

    out.try_defer_println(&format!(
        "[Scanning signatures in 0x0000..0x{:04X}]",
        header_range - 1
    ))?;

    let mut patterns = Vec::new();
    // Normal Pattern: [02, FF, 02] (Odd ON)
    let mut normal_found = false;
    for (i, win) in header_slice.windows(3).enumerate() {
        if win == [0x02, 0xFF, 0x02] {
            out.try_defer_println(&format!("[PatternFound] Normal at 0x{:04X}", i))?;
            patterns.push(PatternMatch {
                name: "Normal".to_string(),
                offset: Some(i),
                found: true,
            });
            normal_found = true;
        }
    }
    if !normal_found {
        out.try_defer_println("[PatternFound] Normal not found in header range")?;
        patterns.push(PatternMatch {
            name: "Normal".to_string(),
            offset: None,
            found: false,
        });
    }
    // NM Pattern: [FF, 02, FF] (Even ON)
    let mut nm_found = false;
    for (i, win) in header_slice.windows(3).enumerate() {
        if win == [0xFF, 0x02, 0xFF] {
            out.try_defer_println(&format!("[PatternFound] NM at 0x{:04X}", i))?;
            patterns.push(PatternMatch {
                name: "NM".to_string(),
                offset: Some(i),
                found: true,
            });
            nm_found = true;
        }
    }
    if !nm_found {
        out.try_defer_println("[PatternFound] NM not found in header range")?;
        patterns.push(PatternMatch {
            name: "NM".to_string(),
            offset: None,
            found: false,
        });
    }
    // Hell Pattern: [FF, FF, FF, FF, 02] (5th ON)
    let mut hell_found = false;
    for (i, win) in header_slice.windows(5).enumerate() {
        if win == [0xFF, 0xFF, 0xFF, 0xFF, 0x02] {
            out.try_defer_println(&format!("[PatternFound] Hell at 0x{:04X}", i))?;
            patterns.push(PatternMatch {
                name: "Hell".to_string(),
                offset: Some(i),
                found: true,
            });
            hell_found = true;
        }
    }
    if !hell_found {
        out.try_defer_println("[PatternFound] Hell not found in header range")?;
        patterns.push(PatternMatch {
            name: "Hell".to_string(),
            offset: None,
            found: false,
        });
    }
    out.try_defer_println("")?;

    // Raw Header Dump (0x000..0x340)
    out.try_defer_println(&format!(
        "=== Raw Header Dump (0x000..0x{:04X}) ===",
        header_range - 1
    ))?;
    for row in 0..((header_range + 15) / 16) {
        let row_start = row * 16;
        let row_end = (row_start + 16).min(header_range);
        out.try_defer_print(&format!("  0x{:04X} |", row_start))?;
        // Hex
        for b in row_start..row_end {
            out.try_defer_print(&format!(" {:02X}", bytes[b]))?;
        }
        for _ in row_end..(row_start + 16) {
            out.try_defer_print("   ")?;
        }
        out.try_defer_print(" | ")?;
        // Binary (Compact)
        for b in row_start..row_end {
            // print bit string but slightly more compact
            let bin = format!("{:08b}", bytes[b]);
            out.try_defer_print(&format!("{} ", bin))?;
        }
        out.try_defer_println("")?;
    }
    out.try_defer_println("")?;

    let mut completed_quests = Vec::new();
    let mut activated_waypoints = Vec::new();

    // === Character Progression (Alpha v105 Engine) ===
    if env::var_os("D2R_ITEM_TRACE").is_some() {
        out.try_finish()?;
    }
    if let Ok(save) = d2r_core::save::Save::from_bytes(&bytes) {
        out.try_defer_println("=== Character Progression (Alpha v105) ===")?;
        let version = save.header.version;

        // Quests
        out.try_defer_println("Completed Quests:")?;
        let mut completed_any = false;
        if let Some(ref quests) = save.header.quests {
            let normal_anchor = d2r_core::domain::progression::axiom::PROG_START_FILE
                + d2r_core::domain::progression::axiom::V105QuestAxiom::normal_start();
            let act5_anchor = d2r_core::domain::progression::axiom::PROG_START_FILE
                + d2r_core::domain::progression::axiom::V105QuestAxiom::act5_start();

            for quest in d2r_core::data::quests::V105_QUESTS {
                if quests.is_v105_completed_by_name(quest.name, normal_anchor, act5_anchor) {
                    let diff_str = match quest.difficulty {
                        0 => "Normal",
                        1 => "NM",
                        2 => "Hell",
                        _ => "?",
                    };
                    out.try_defer_println(&format!(
                        "  [{:<6}] Act {} - {}",
                        diff_str, quest.act, quest.name
                    ))?;
                    completed_quests
                        .push(format!("[{}] Act {} - {}", diff_str, quest.act, quest.name));
                    completed_any = true;
                }
            }
        }
        if !completed_any {
            out.try_defer_println("  (None)")?;
        }
        out.try_defer_println("")?;

        // Waypoints
        out.try_defer_println("Activated Waypoints:")?;
        let mut wp_any = false;
        let wp_anchor = d2r_core::domain::progression::axiom::PROG_START_FILE
            + d2r_core::domain::progression::axiom::V105WaypointAxiom::start_offset();

        for diff in 0..3 {
            let mut diff_wps = Vec::new();
            for wp in d2r_core::data::waypoints::WAYPOINTS {
                let activated = if version == 105 {
                    save.header
                        .waypoints
                        .as_ref()
                        .map(|w| w.is_activated_by_name(wp.name, diff as u8, wp_anchor))
                        .unwrap_or(false)
                } else {
                    match diff {
                        0 => save
                            .header
                            .waypoints
                            .as_ref()
                            .map(|w| w.is_activated_by_name(wp.name, 0, wp_anchor))
                            .unwrap_or(false),
                        1 | 2 => save
                            .header
                            .expansion
                            .as_ref()
                            .map(|e| e.is_activated_by_name(diff as u8, wp.name))
                            .unwrap_or(false),
                        _ => false,
                    }
                };
                if activated {
                    let name_clean = wp.name.replace(&format!("Act {} - ", wp.act), "");
                    diff_wps.push(format!("(A{}){}", wp.act, name_clean));
                    let diff_tag = match diff {
                        0 => "Normal",
                        1 => "NM",
                        2 => "Hell",
                        _ => "?",
                    };
                    activated_waypoints.push(format!("[{}] {}", diff_tag, wp.name));
                }
            }
            if !diff_wps.is_empty() {
                let diff_str = match diff {
                    0 => "Normal",
                    1 => "Nightmare",
                    2 => "Hell",
                    _ => "?",
                };
                out.try_defer_println(&format!("  [{:>9}]: {}", diff_str, diff_wps.join(", ")))?;
                wp_any = true;
            }
        }
        if !wp_any {
            out.try_defer_println("  (None)")?;
        }
        out.try_defer_println("")?;
    }

    let mut all_items = Vec::new();
    let jm_positions = &map.jm_positions;
    for (jm_idx, &start_pos) in jm_positions.iter().enumerate() {
        let count_val = if start_pos + 3 < bytes.len() {
            u16::from_le_bytes([bytes[start_pos + 2], bytes[start_pos + 3]])
        } else {
            0
        };

        // Find next JM marker or end of file
        let end_marker = if jm_idx + 1 < jm_positions.len() {
            jm_positions[jm_idx + 1]
        } else {
            bytes.len()
        };

        out.try_defer_println(&format!(
            "JM Section at 0x{:04X}: {} items",
            start_pos, count_val
        ))?;
        if count_val > 0 {
            let section_data = &bytes[start_pos + 4..end_marker];

            if env::var("D2R_ITEM_TRACE").is_ok() {
                out.try_defer_println(&format!(
                    "  [Diagnostic] Performing bit-level probe for JM section at 0x{:04X}...",
                    start_pos
                ))?;
                let section_bits = (section_data.len() * 8) as u64;
                for b in 0..section_bits.saturating_sub(64) {
                    if is_terminator_like(section_data, b) {
                        out.try_defer_println(&format!(
                            "  [Probe] Possible Terminator at bit {}",
                            b
                        ))?;
                        dump_bit_window(&mut out, section_data, b)?;
                    }
                    let code = peek_code_minimal(section_data, b, &huffman);
                    if let Some(c) = code {
                        // Only dump window for plausible codes in the interesting region
                        if b >= 1000 && b <= 2000 {
                            out.try_defer_println(&format!(
                                "  [Probe] Plausible Header at bit {} (Code: '{}')",
                                b, c
                            ))?;
                            dump_bit_window(&mut out, section_data, b)?;
                        }
                    } else if b == 1127 {
                        // Explicitly requested bit window for bit 1127
                        out.try_defer_println("  [Probe] Target Diagnostic at bit 1127")?;
                        dump_bit_window(&mut out, section_data, b)?;
                    }
                }
            }

            let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
            if env::var_os("D2R_ITEM_TRACE").is_some() {
                out.try_finish()?;
            }
            let result =
                Item::read_section(section_data, 0, count_val, &huffman, version == 105, false);

            match result {
                Ok(sect_items) => all_items.extend(sect_items),
                Err(err) => {
                    out.try_defer_println(&format!("  [ERROR] JM @ 0x{:04X}: {}", start_pos, err))?;
                }
            }
        }
    }

    let items = all_items;

    if out.is_json() {
        let payload = ChunkVerifyPayload {
            save_path: path.clone(),
            checksum: ChecksumInfo {
                original: original_checksum,
                calculated: calculated_checksum,
                is_valid: is_checksum_valid,
            },
            sections,
            patterns,
            progression: ProgressionInfo {
                completed_quests,
                activated_waypoints,
            },
            items: items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let quality_str = match item.quality {
                        Some(ItemQuality::Normal) => "Normal",
                        Some(ItemQuality::Magic) => "Magic",
                        Some(ItemQuality::Set) => "Set",
                        Some(ItemQuality::Unique) => "Unique",
                        Some(ItemQuality::Rare) => "Rare",
                        Some(ItemQuality::Crafted) => "Crafted",
                        _ => "Other",
                    };
                    ItemSummary {
                        index: i,
                        code: item.code.clone(),
                        bits: item.bits.len(),
                        quality: quality_str.to_string(),
                        is_runeword: item.is_runeword,
                        mode: item.mode,
                        page: item.page,
                        location: item.location,
                    }
                })
                .collect(),
        };

        let metadata = ReportMetadata::new("d2item_chunk_verify", path, env!("CARGO_PKG_VERSION"));
        let report = Report::new(metadata, ReportStatus::Ok)
            .with_results(payload)
            .with_forensic_context();

        out.try_json(&serde_json::to_string_pretty(&report).unwrap())?;
    }

    if let Some(idx) = detail_index {
        if idx >= items.len() {
            out.try_finish()?;
            eprintln!("Index {} out of bounds (total items: {})", idx, items.len());
            std::process::exit(1);
        }
        print_detail(&mut out, idx, &items[idx])?;
    } else {
        print_summary(&mut out, &items, range_start, range_end)?;
    }

    out.try_finish()?;
    Ok(())
}

fn print_summary(
    out: &mut OutputManager,
    items: &[Item],
    start: usize,
    end: usize,
) -> io::Result<()> {
    let actual_end = end.min(items.len());
    out.try_defer_println(&format!("Total Items Found: {}", items.len()))?;
    out.try_defer_println(&format!("Scanning Range: {}..{}", start, actual_end))?;
    out.try_defer_println("")?;
    out.try_defer_println(&format!(
        "{:>5} | {:<5} | {:>4} | {:<10} | {:<4} | {:<8}",
        "Index", "Code", "Bits", "Quality", "RW", "Loc"
    ))?;
    out.try_defer_println("------|-------|------|------------|------|---------")?;

    for i in start..actual_end {
        let item = &items[i];
        let quality_str = match item.quality {
            Some(ItemQuality::Normal) => "Normal",
            Some(ItemQuality::Magic) => "Magic",
            Some(ItemQuality::Set) => "Set",
            Some(ItemQuality::Unique) => "Unique",
            Some(ItemQuality::Rare) => "Rare",
            Some(ItemQuality::Crafted) => "Crafted",
            _ => "Other",
        };

        out.try_defer_println(&format!(
            "{:>5} | {:<5} | {:>4} | {:<10} | {:<4} | G:{:<1} P:{:<1} L:{:<2} S:{:<1}",
            i,
            item.code,
            item.bits.len(),
            quality_str,
            if item.is_runeword { "YES" } else { "NO" },
            item.mode,
            item.page,
            item.location,
            item.socketed_items.len()
        ))?;

        for child in &item.socketed_items {
            out.try_defer_println(&format!(
                "      Socketed: '{}' ({} bits)",
                child.code,
                child.bits.len()
            ))?;
        }

        // Basic anomaly check
        if item.is_runeword && item.bits.len() < 100 {
            out.try_defer_println(&format!(
                "      [WARN] Runeword with suspicious short bit-length: {}",
                item.bits.len()
            ))?;
        }
        if item.quality == Some(ItemQuality::Normal) && item.bits.len() > 200 {
            out.try_defer_println(&format!(
                "      [WARN] Normal item with suspicious long bit-length: {}",
                item.bits.len()
            ))?;
        }
    }
    Ok(())
}

fn print_detail(out: &mut OutputManager, index: usize, item: &Item) -> io::Result<()> {
    out.try_defer_println(&format!("=== Detail View: Item Index {} ===", index))?;
    out.try_defer_println(&format!("Code: '{}'", item.code))?;
    out.try_defer_println(&format!("Bits Length: {}", item.bits.len()))?;
    out.try_defer_println(&format!("Flags: 0x{:08X}", item.flags))?;
    out.try_defer_println(&format!("Version: {}", item.version))?;
    out.try_defer_println(&format!("Socketed: {}", (item.flags & (1 << 11)) != 0))?;
    out.try_defer_println(&format!("Quality: {:?}", item.quality))?;
    out.try_defer_println(&format!("Runeword: {}", item.is_runeword))?;
    out.try_defer_println(&format!(
        "Location: Mode={} Page={} X={} Y={} Loc={}",
        item.mode, item.page, item.x, item.y, item.location
    ))?;
    out.try_defer_println(&format!(
        "Properties Complete: {}",
        item.properties_complete
    ))?;
    out.try_defer_println("")?;
    out.try_defer_println("Properties:")?;
    for prop in &item.properties {
        out.try_defer_println(&format!("  ID {:>3}: Value {}", prop.stat_id, prop.value))?;
    }
    if !item.runeword_attributes.is_empty() {
        out.try_defer_println("Runeword Attributes:")?;
        for prop in &item.runeword_attributes {
            out.try_defer_println(&format!("  ID {:>3}: Value {}", prop.stat_id, prop.value))?;
        }
    }
    Ok(())
}

fn dump_bit_window(out: &mut OutputManager, data: &[u8], pos: u64) -> io::Result<()> {
    let half = 32;
    let start = pos.saturating_sub(half);
    let end = (pos + half).min((data.len() * 8) as u64);

    out.try_defer_print(&format!("  [BitWindow] @{:>4}: ", pos))?;
    for b in start..end {
        let byte_idx = (b / 8) as usize;
        let bit_idx = (b % 8) as u8;
        let bit = (data[byte_idx] >> bit_idx) & 1 != 0;
        out.try_defer_print(if bit { "1" } else { "0" })?;
        if b % 8 == 7 {
            out.try_defer_print(" ")?;
        }
    }
    out.try_defer_println("")?;
    Ok(())
}

fn is_terminator_like(data: &[u8], bit_pos: u64) -> bool {
    // Look for 9 ones followed by 8 zeros (17 bits)
    let mut reader =
        bitstream_io::BitReader::endian(std::io::Cursor::new(data), bitstream_io::LittleEndian);
    if reader.skip(bit_pos as u32).is_err() {
        return false;
    }

    let mut val = 0u32;
    for i in 0..17 {
        if let Ok(bit) = reader.read_bit() {
            if bit {
                val |= 1 << i;
            }
        } else {
            return false;
        }
    }
    val == 0x1FF // Exactly FF 01 00 in little-endian bytes starting at bit_pos
}

fn peek_code_minimal(data: &[u8], start_bit: u64, huffman: &HuffmanTree) -> Option<String> {
    // Flags(32)+Ver(3)+Mode(3)+Loc(4)+X(4) = 46 bits
    // We'll just try decoding at offsets 46, 46+7 (if Loc 0), etc.
    for offset in [46u64, 46 + 7] {
        let mut reader =
            bitstream_io::BitReader::endian(std::io::Cursor::new(data), bitstream_io::LittleEndian);
        if reader.skip((start_bit + offset) as u32).is_err() {
            continue;
        }
        let mut code = String::new();
        let mut ok = true;
        for _ in 0..4 {
            if let Ok(c) = huffman.decode(&mut reader) {
                code.push(c);
            } else {
                ok = false;
                break;
            }
        }
        if ok {
            let trimmed = code.trim();
            if trimmed.len() >= 3 && trimmed.chars().all(|c| c.is_alphanumeric()) {
                let known = [
                    "jav", "buc", "rin", "amu", "key", "tsc", "isc", "hp1", "mp1",
                ];
                if known.contains(&trimmed) {
                    return Some(code);
                }
            }
        }
    }
    None
}
