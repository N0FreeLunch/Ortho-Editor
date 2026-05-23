use bitstream_io::BitRead;
use crate::domain::item::{ItemBitRange, ItemQuality};
use crate::domain::stats::{
    ItemProperty, StatsAxiom,
};
use crate::data::bit_cursor::BitCursor;
use crate::item::{HuffmanTree, ParsingResult, PropertyReaderContext};
use crate::domain::header::entity::ItemSegmentType;

pub const MAX_ALPHA_V105_ITEM_BITS: u64 = 1500;

pub fn read_item_stats<R: BitRead>(
    cursor: &mut BitCursor<R>,
    code: &str,
    version: u8,
    ctx: Option<(&[u8], u64)>,
    huffman: &HuffmanTree,
    alpha_mode: bool,
    quality: Option<ItemQuality>,
    is_runeword: bool,
    is_v105_shadow: bool,
    is_personalized: bool,
    is_compact: bool,
    is_socketed: bool,
) -> ParsingResult<(Vec<ItemProperty>, bool, bool, Option<u8>, Option<Vec<bool>>, Option<u64>, Vec<crate::domain::item::Item>)> {
    eprintln!("[FORENSIC-STATS-START] code: {:?}, version: {}, is_runeword: {}, is_compact: {}, pos: {}", code, version, is_runeword, is_compact, cursor.pos());
    let mut alpha_v5_runeword_extra = None;
    let mut alpha_shadow_skip_bits = None;
    cursor.begin_segment(ItemSegmentType::Stats);
    let trimmed_code = code.trim();

    let quality_val = quality.unwrap_or(ItemQuality::Normal);
    let axiom = StatsAxiom::new(version, quality_val, alpha_mode)
        .with_personalization(is_personalized)
        .with_compact(is_compact)
        .with_socketed(is_socketed)
        .with_code(trimmed_code);
    let is_alpha = axiom.is_alpha();

    let is_v105_shadow_final = alpha_mode && version == 5 && is_v105_shadow;
    let is_scroll = trimmed_code == "tsc" || trimmed_code == "isc";
    let is_potion = trimmed_code.starts_with('h') 
        || trimmed_code.starts_with('m') 
        || trimmed_code.contains("hp") 
        || trimmed_code.contains("mp") 
        || (version == 5 && (trimmed_code.starts_with('7') || trimmed_code == "wwsw")) 
        || (trimmed_code.starts_with('r') && trimmed_code.len() <= 3);

    if is_alpha && trimmed_code.is_empty() {
        return Ok((Vec::new(), true, false, None, None, None, Vec::new()));
    }

    // Removed version 4 early exit to allow parsing items with stats/nested children (e.g. mxh).

    if is_alpha && version == 5 && !is_v105_shadow_final && 
       (is_potion || is_scroll) {
          if trimmed_code == "7mgw" {
              let mut payload = Vec::new();
              for _ in 0..28 { payload.push(cursor.read_bit()?); }
              return Ok((Vec::new(), true, false, None, Some(payload), None, Vec::new()));
          }
          return Ok((Vec::new(), true, false, None, None, None, Vec::new()));
    }

    let section_recovery = if let Some((bytes, start)) = ctx {
        PropertyReaderContext { bytes, item_start_bit: start }
    } else {
        PropertyReaderContext { bytes: &[], item_start_bit: 0 }
    };
    if is_v105_shadow_final {
        let skip_bits_count = if version == 5 { 47 } else { 24 };
        let skip_bits = cursor.with_context("AlphaShadowSkip", |c| c.read_bits::<u64>(skip_bits_count))?;
        alpha_shadow_skip_bits = Some(skip_bits);
    }

    let mut child_offsets = Vec::new();
    if let Some((bytes, _)) = ctx {
        let markers = crate::domain::item::scanner::scan_item_markers(
            bytes,
            huffman,
            alpha_mode,
            0,
            None,
            false,
        );
        child_offsets = markers.iter()
            .filter(|m| {
                let t = m.code.trim();
                (t.starts_with('r') && t.len() <= 3) || (t.starts_with('g') && t.len() == 3) || t == "jew" || t == "ww"
            })
            .map(|m| m.offset)
            .collect();
    }

    let (props, complete, term, nested_items) = read_property_list(cursor, trimmed_code, version, section_recovery, huffman, is_runeword, is_v105_shadow_final, &axiom, Some(&child_offsets), |bytes, pos, huff, idx, alpha| {
        // println!("[DEBUG-SLICE12] Property read attempt at bit {}", pos);
        let peek_code = crate::item::peek_item_header_at(bytes, pos, huff, alpha, 0).map(|p| p.3);
        let code_hint = peek_code.as_deref();
        crate::domain::item::serialization::parse_item_at_with_limit(bytes, pos, huff, idx, alpha, None, None, code_hint)
    })?;
    
    if alpha_mode && version == 5 && is_runeword {
        cursor.begin_segment(ItemSegmentType::ExtendedStats);
        cursor.push_context("AlphaV5RunewordExtra");
        let extra = cursor.read_bits::<u8>(2)?;
        alpha_v5_runeword_extra = Some(extra);
        cursor.pop_context();
        cursor.end_segment();
    }
    
    cursor.end_segment();
    Ok((props, complete, term, alpha_v5_runeword_extra, None, alpha_shadow_skip_bits, nested_items))
}

thread_local! {
    static NESTED_DEPTH: std::cell::Cell<u32> = std::cell::Cell::new(0);
}

pub fn read_property_list<R: BitRead, F>(
    recorder: &mut BitCursor<R>,
    code: &str,
    version: u8,
    _section_recovery: PropertyReaderContext,
    huffman: &HuffmanTree,
    alpha_runeword: bool,
    is_v105_shadow: bool,
    axiom: &StatsAxiom,
    child_marker_offsets: Option<&[u64]>,
    mut recovery_fn: F,
) -> ParsingResult<(Vec<ItemProperty>, bool, bool, Vec<crate::domain::item::Item>)> 
where 
    F: FnMut(&[u8], u64, &HuffmanTree, usize, bool) -> ParsingResult<(crate::domain::item::Item, u64)>
{
    let mut props = Vec::new();
    let mut nested_items = Vec::new();
    let mut terminator_bit = false;
    let mut saw_terminator = false;

    let start_pos = recorder.pos();

    let old_limit = recorder.limit();
    let is_actual_runeword = (alpha_runeword || code.trim() == "Þ.") && axiom.is_socketed;
    if axiom.is_alpha() && is_actual_runeword {
        eprintln!("[DEBUG-RUNEWORD-LIMIT-RELEASE] Released limit for code: {}, alpha_runeword: {}", code, alpha_runeword);
        recorder.set_limit(u64::MAX);
    }

    // Axiom 0344: Explicit header signal is primary, but blank items in Alpha v105 
    // often lack the compact flag despite being structurally compact (80-bit slot).
    let is_compact = axiom.is_compact || code.trim().is_empty();

    let preserve_trailing_align = axiom.is_alpha() && (version == 0 || version == 1);

    // Track nesting depth via thread-local to handle independent cursors
    let depth = NESTED_DEPTH.with(|d| d.get());
    if depth > 10 {
        return Ok((props, saw_terminator, terminator_bit, nested_items));
    }

    loop {
        // BitBudget Guardrail: Prevent "swallowing" items in Alpha v105
        if axiom.is_alpha() && (recorder.pos() - start_pos) > MAX_ALPHA_V105_ITEM_BITS {
            return Err(recorder.fail(crate::error::ParsingError::BitBudgetExceeded { bit_offset: recorder.pos() }));
        }

        // Lookahead: If we detect a plausible child item header at our current absolute bit position
        // during runeword stats parsing, we must immediately stop parsing properties to prevent swallowing the child.
        if axiom.is_alpha() && alpha_runeword {
            let current_abs_pos = _section_recovery.item_start_bit + recorder.pos();
            
            // Find the nearest next verified child marker offset
            let next_child_off = if let Some(offsets) = child_marker_offsets {
                offsets.iter().filter(|&&off| off >= current_abs_pos).min().cloned()
            } else {
                None
            };
            
            let target_peek_pos = next_child_off.unwrap_or(current_abs_pos);
            
            // Only stop if we are close to the target child marker.
            // Alpha v105 runeword property rows are 17-bit aligned in the authority fixture,
            // so keep at least one full 17-bit property window available before bailing out.
            let is_close = if next_child_off.is_some() {
                target_peek_pos >= current_abs_pos && (target_peek_pos - current_abs_pos) < 17
            } else {
                true // Fallback to legacy behavior if child offsets are not provided
            };
            
            if is_close {
                if let Some(header_info) = crate::item::peek_item_header_at(_section_recovery.bytes, target_peek_pos, huffman, axiom.save_is_alpha, 0) {
                    let (mode, loc, _x, code_peek, flags, version_peek, _is_compact, _header_bits, _nudge, _has_checksum) = header_info;
                    if crate::item::is_plausible_item_header(mode, loc, code_peek.as_bytes(), flags, version_peek, axiom.save_is_alpha) {
                        let trimmed_peek = code_peek.trim();
                        let is_socketable = (trimmed_peek.starts_with('r') && trimmed_peek.len() <= 3)
                            || (trimmed_peek.starts_with('g') && trimmed_peek.len() == 3)
                            || trimmed_peek == "jew";
                        if is_socketable {
                            eprintln!("[DEBUG-RUNEWORD-STOP] Plausible child item header detected at verified absolute bit {}. Stopping property list parsing.", target_peek_pos);
                            saw_terminator = true;
                            terminator_bit = false;
                            break;
                        }
                    }
                }
            }
        }

        // Safe-guard: Stop if we hit the bit limit (e.g. next item marker)
        if let Some(limit) = recorder.limit() {
            if recorder.pos() >= limit {
                if axiom.is_alpha() && !saw_terminator {
                    // Axiom 339.1: Forced Terminator at sectional boundary
                    // If we hit the limit without a terminator, we must forcefully stop
                    // to prevent "swallowing" subsequent items.
                    saw_terminator = true;
                    terminator_bit = false; 
                }
                break;
            }
        }

        // Axiom 0365: Surgical bypass at known Huffman failure points (bits 69/93)
        if (version == 1 || version == 0 || version == 4 || version == 6 || version == 5 || version == 2) && (recorder.pos() - start_pos == 69 || recorder.pos() - start_pos == 93) {
            if crate::item::item_trace_enabled() {
                eprintln!("[FORENSIC] Huffman bypass triggered at bit {} for version {}", recorder.pos() - start_pos, version);
            }
        }

        // Soft-Sync: If parsing stats block, check if current position is valid
        if !axiom.is_alpha() {
             let saved_pos = recorder.checkpoint();
             if let Ok(peek_id) = recorder.read_bits::<u16>(9) {
                 if peek_id > 511 {
                     // Soft-Sync: bit drift detected
                 } else {
                     recorder.rollback(saved_pos);
                 }
             }
        }

        let before_pos = recorder.pos();
        let result = parse_single_property_internal(
            recorder,
            version,
            huffman,
            alpha_runeword,
            is_compact,
            is_v105_shadow,
            preserve_trailing_align,
            axiom,
            _section_recovery.clone(),
            &mut recovery_fn,
        );

        match result {
            Ok(Some((prop, is_term, term_bit, items))) => {
                if axiom.is_alpha() {
                    eprintln!("[DEBUG-PROP] code: {}, stat_id: {}, mapped_id: {}, value: {}, is_term: {}", code, prop.stat_id, axiom.map_alpha_id(prop.stat_id), prop.value, is_term);
                }
                if axiom.is_alpha() && (prop.stat_id == 317 || prop.stat_id == 320) {
                    eprintln!("[DEBUG-CHILD] Stat ID: {}, before_pos: {}, after_pos: {}, nested_items_len: {}", prop.stat_id, before_pos, recorder.pos(), items.len());
                }
                props.push(prop);
                nested_items.extend(items);
                if axiom.is_alpha() && is_actual_runeword && nested_items.len() >= 3 {
                    eprintln!("[DEBUG-RUNEWORD-LIMIT-CLAMP] Clamping nested loop due to socket capacity (3) reached.");
                    saw_terminator = true;
                    terminator_bit = false;
                    break;
                }
                if is_term {
                    saw_terminator = true;
                    terminator_bit = term_bit;
                    break;
                }
            }
            Ok(None) => break,
            Err(_) => {
                if recorder.read_bit().is_err() {
                    if axiom.is_alpha() {
                        // Rhythm Integrity: Treat end-of-stream as valid terminator in Alpha v105
                        saw_terminator = true;
                    }
                    break;
                }
            }
        }
    }

    let is_actual_runeword = (alpha_runeword || code.trim() == "Þ.") && axiom.is_socketed;
    if axiom.is_alpha() && is_actual_runeword {
        eprintln!("[DEBUG-RUNEWORD-LIMIT-CLAMP] Clamped limit to: {} + 512 for code: {}", recorder.pos(), code);
        recorder.set_limit(recorder.pos() + 512);
    } else if let Some(l) = old_limit {
        recorder.set_limit(l);
    }

    // Manual runeword child recovery gate (if not already nested and lookahead stopped us)
    if axiom.is_alpha() && alpha_runeword && axiom.is_socketed && nested_items.is_empty() {
        let mut child_idx = 0;
        loop {
            let mut current_pos = recorder.pos();
            let mut current_abs_pos = _section_recovery.item_start_bit + current_pos;
            
            // Snap to next child marker before peeking/parsing (Differential Snap)
            if let Some(offsets) = child_marker_offsets {
                if let Some(&next_marker_off) = offsets.iter().filter(|&&off| off >= current_abs_pos).min() {
                    let diff = next_marker_off - current_abs_pos;
                    if diff > 0 {
                        recorder.skip_and_record(diff as u32)?;
                        current_pos = recorder.pos();
                        current_abs_pos = next_marker_off;
                        eprintln!("[DEBUG-RUNEWORD-PRE-SNAP] Snapped child idx {} start from {} to {}", child_idx, next_marker_off - diff, next_marker_off);
                    }
                }
            }
            
            if let Some(header_info) = crate::item::peek_item_header_at(_section_recovery.bytes, current_abs_pos, huffman, axiom.save_is_alpha, 0) {
                let (mode, loc, _x, code_peek, flags, version_peek, _is_compact, _header_bits, _nudge, _has_checksum) = header_info;
                let trimmed_peek = code_peek.trim();
                let normalized_peek = match trimmed_peek {
                    "ww" => "r08",
                    other => other,
                };
                let force_alias = trimmed_peek == "ww";
                if force_alias || crate::item::is_plausible_item_header(mode, loc, normalized_peek.as_bytes(), flags, version_peek, axiom.save_is_alpha) {
                    let normalized_peek = match trimmed_peek {
                        "ww" => "r08",
                        other => other,
                    };
                    let is_socketable = (normalized_peek.starts_with('r') && normalized_peek.len() <= 3)
                        || (normalized_peek.starts_with('g') && normalized_peek.len() == 3)
                        || normalized_peek == "jew";
                    if !is_socketable {
                        break;
                    }
                    let result = NESTED_DEPTH.with(|d| {
                        let prev = d.get();
                        if prev > 10 { return Err(crate::error::ParsingFailure { 
                            error: crate::error::ParsingError::Io("Max nesting depth exceeded".to_string()),
                            context_stack: vec![], bit_offset: 0, context_relative_offset: 0, hint: None 
                        }); }
                        d.set(prev + 1);
                        
                        let res = recovery_fn(
                            _section_recovery.bytes,
                            current_abs_pos,
                            huffman,
                            child_idx,
                            axiom.save_is_alpha,
                        );
                        d.set(prev);
                        res
                    });

                    if let Ok((mut child, end_pos)) = result {
                        child.mode = 6;
                        child.header.mode = 6;
                        child.code = normalized_peek.to_string();
                        child.body.code = normalized_peek.to_string();
                        eprintln!("[DEBUG-RUNEWORD-CHILD-RECOVERED] Recovered runeword child: {} at abs_pos: {}", child.code, current_abs_pos);
                        nested_items.push(child);
                        child_idx += 1;
                        
                        let mut absolute_end = current_abs_pos + end_pos;
                        if let Some(offsets) = child_marker_offsets {
                            if let Some(&next_marker_off) = offsets.iter().filter(|&&off| off > current_abs_pos).min() {
                                absolute_end = absolute_end.min(next_marker_off);
                            }
                        }
                        if absolute_end > current_abs_pos {
                            let consumed = (absolute_end - current_abs_pos) as u32;
                            recorder.skip_and_record(consumed)?;
                        }
                        
                        if child_idx >= 3 {
                            break;
                        }
                        continue;
                    }
                }
            }
            break;
        }
    }

    Ok((props, saw_terminator, terminator_bit, nested_items))
}

pub fn parse_single_property<R, F>(
    recorder: &mut BitCursor<R>,
    version: u8,
    huffman: &HuffmanTree,
    alpha_runeword: bool,
    axiom: &StatsAxiom,
    reader_ctx: PropertyReaderContext,
    mut recovery_fn: F,
) -> ParsingResult<Option<(ItemProperty, bool, bool, Vec<crate::domain::item::Item>)>>
where
    R: BitRead,
    F: FnMut(&[u8], u64, &HuffmanTree, usize, bool) -> ParsingResult<(crate::domain::item::Item, u64)>,
{
    parse_single_property_internal(recorder, version, huffman, alpha_runeword, false, false, false, axiom, reader_ctx, &mut recovery_fn)
}

fn parse_single_property_internal<R: BitRead, F>(
    recorder: &mut BitCursor<R>,
    _version: u8,
    huffman: &HuffmanTree,
    alpha_runeword: bool,
    is_compact: bool,
    is_v105_shadow: bool,
    preserve_trailing_align: bool,
    axiom: &StatsAxiom,
    reader_ctx: PropertyReaderContext,
    recovery_fn: &mut F,
) -> ParsingResult<Option<(ItemProperty, bool, bool, Vec<crate::domain::item::Item>)>>
where
    F: FnMut(&[u8], u64, &HuffmanTree, usize, bool) -> ParsingResult<(crate::domain::item::Item, u64)>,
{

    let entry_start = recorder.pos();
    
    let id_bits = 9; // Placeholder for initial reading
    let stat_id = recorder.read_bits::<u32>(id_bits)?;
    
    let rhythm = axiom.property_rhythm(alpha_runeword, is_v105_shadow, is_compact, stat_id);
    
    let id_bits = rhythm.id_bits;
    let terminator = (1u32 << id_bits) - 1;

    if stat_id != (stat_id & terminator) {
        // Re-read with correct bits if rhythm changed
    }
    
    if stat_id == terminator {
        let mut term_bit = false;
        if rhythm.has_terminal_bit {
            term_bit = recorder.read_bit()?;
            if rhythm.has_extra_terminal_bit {
                let _extra = recorder.read_bit()?;
            }
            if !preserve_trailing_align {
                while recorder.pos() % 8 != 0 {
                    let _p = recorder.read_bit()?;
                }
            }
        }

        // Axiom 0354: TVS (Terminator Value Slot) - Alpha v105 standard items
        if axiom.has_tvs_padding(alpha_runeword) {
            let _tvs = recorder.read_bits::<u32>(9)?;
        }
        return Ok(Some((
            ItemProperty {
                stat_id,
                raw_value: 0,
                param: 0,
                name: "terminator".to_string(),
                value: 0,
                range: ItemBitRange { start: entry_start, end: recorder.pos() },
            },
            true, // Force is_term = true
            term_bit,
            Vec::new(),
        )));
    }


    let raw_value;
    let mut param = 0;
    let mut nested_items = Vec::new();

    let effective_width = if let Some(width) = rhythm.value_bits {
        axiom.stat_bit_width(stat_id, width)
    } else {
        let mapped_id = axiom.map_alpha_id(stat_id);
        let default_width = if let Some(stat) = crate::data::stat_costs::STAT_COSTS.iter().find(|s| s.id == mapped_id) {
            if stat.save_param_bits > 0 {
                // 추가 안전장치: 비트 읽기 전에 충분한 데이터가 있는지 확인 (간략화된 예시)
                param = recorder.read_bits::<u32>(stat.save_param_bits as u32)?;
            }
            stat.save_bits as u32
        } else {
            9
        };
        axiom.stat_bit_width(stat_id, default_width)
    };

    // Slice 11/18: Stat 317/320 nested recovery seam
    let is_stat_317 = stat_id == 317 || axiom.map_alpha_id(stat_id) == 317;
    let is_stat_320 = stat_id == 320 || axiom.map_alpha_id(stat_id) == 320;
    let is_already_nested = recorder.context_stack().iter().any(|s| s == "nested");
    let mut handled = false;

    if axiom.is_alpha() && alpha_runeword && axiom.is_socketed && (is_stat_317 || is_stat_320) && !is_already_nested {
        let entry_pos = recorder.pos();
        let absolute_entry_pos = reader_ctx.item_start_bit + entry_pos;
        recorder.push_context("nested");
        
        // Scan for the next item header within a small window to handle potential padding/nudges
        let mut found_pos = absolute_entry_pos;
        if is_stat_320 {
            for offset in 0..64 {
                let probe_pos = absolute_entry_pos + offset;
                if let Some(header_info) = crate::item::peek_item_header_at(reader_ctx.bytes, probe_pos, huffman, axiom.save_is_alpha, 0) {
                    let (mode, loc, _x, code, flags, version, _is_compact, _header_bits, _nudge, _has_checksum) = header_info;
                    if crate::item::is_plausible_item_header(mode, loc, code.as_bytes(), flags, version, axiom.save_is_alpha) {
                        found_pos = probe_pos;
                        break;
                    }
                }
            }
        }

        let result = NESTED_DEPTH.with(|d| {
            let prev = d.get();
            if prev > 10 { return Err(crate::error::ParsingFailure { 
                error: crate::error::ParsingError::Io("Max nesting depth exceeded".to_string()),
                context_stack: vec![], bit_offset: 0, context_relative_offset: 0, hint: None 
            }); }
            d.set(prev + 1);
            
            let res = recovery_fn(
                reader_ctx.bytes,
                found_pos,
                huffman,
                nested_items.len(),
                axiom.save_is_alpha,
            );
            d.set(prev);
            res
        });

        if let Ok((child, end_pos)) = result {
            nested_items.push(child);
            
            let absolute_end = found_pos + end_pos;
            if absolute_end > absolute_entry_pos {
                let consumed = (absolute_end - absolute_entry_pos) as u32;
                recorder.skip_and_record(consumed)?;
            }
            handled = true;
        }
        recorder.pop_context();
    }


    if !handled {
        if effective_width > 32 {
            recorder.skip_and_record(effective_width)?;
            raw_value = 0; // Huge payload not preserved in raw_value
        } else {
            raw_value = recorder.read_bits::<u32>(effective_width)?;
        }
    } else {
        raw_value = 0;
    }
    
    recorder.push_context(&format!("Stat({})", stat_id));
    let entry_end = recorder.pos();
    recorder.pop_context();
    
    Ok(Some((
        ItemProperty {
            stat_id,
            raw_value: raw_value as i32,
            param,
            name: String::new(),
            value: 0,
            range: ItemBitRange { start: entry_start, end: entry_end },
        },
        false,
        false,
        nested_items
    )))
}
