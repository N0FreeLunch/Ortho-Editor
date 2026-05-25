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
    flags: u32,
    is_runeword: bool,
    is_v105_shadow: bool,
    is_personalized: bool,
    is_compact: bool,
    is_socketed: bool,
) -> ParsingResult<(Vec<ItemProperty>, bool, bool, Option<u8>, Option<Vec<bool>>, Option<u64>, Vec<crate::domain::item::Item>)> {
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
    if crate::item::item_trace_enabled() && matches!(trimmed_code, "jav" | "buc") {
        eprintln!(
            "[DEBUG-STATS] code={} header_compact={} axiom_compact={} socketed={} runeword={} personalized={} version={}",
            trimmed_code,
            is_compact,
            axiom.is_compact,
            is_socketed,
            is_runeword,
            is_personalized,
            version
        );
    }
    let is_alpha = axiom.is_alpha();

    let is_v105_shadow_final = alpha_mode && (version == 5 || version == 1) && is_v105_shadow;
    let is_shadow_container = alpha_mode && (trimmed_code == "xrs" || trimmed_code == "c8xr");
    
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

    let (props, complete, term, nested_items) = read_property_list(cursor, trimmed_code, version, section_recovery, huffman, is_runeword, is_v105_shadow_final || is_shadow_container, &axiom, Some(&child_offsets), |bytes, pos, huff, idx, alpha| {
        let peek_code = crate::item::peek_item_header_at(bytes, pos, huff, alpha, 0).map(|p| p.3);
        let code_hint = peek_code.as_deref();
        crate::domain::item::serialization::parse_item_at_with_limit(bytes, pos, 0, huff, idx, alpha, None, None, code_hint)
    })?;
    
    if alpha_mode && (version == 5 || version == 1) && (axiom.is_fragment(flags) || is_v105_shadow_final) {
        let w_axiom = crate::domain::forensic::v105::axioms::V105PropertyWidthAxiom::default();
        cursor.begin_segment(ItemSegmentType::ExtendedStats);
        cursor.push_context("AlphaV5RunewordExtra");
        let extra = cursor.read_bits::<u8>(w_axiom.v5_runeword_extra_bits() as u32)?;
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
        recorder.set_limit(u64::MAX);
    }

    let is_compact = axiom.is_compact || code.trim().is_empty();
    let preserve_trailing_align = axiom.is_alpha() && (version == 0 || version == 1);

    let depth = NESTED_DEPTH.with(|d| d.get());
    if depth > 10 {
        return Ok((props, saw_terminator, terminator_bit, nested_items));
    }

    loop {
        if is_compact && !is_actual_runeword {
            if props.len() >= 1 {
                break;
            }
        }
        if axiom.is_alpha() && !is_actual_runeword && (recorder.pos() - start_pos) > MAX_ALPHA_V105_ITEM_BITS {
            return Err(recorder.fail(crate::error::ParsingError::BitBudgetExceeded { bit_offset: recorder.pos() }));
        }
        if axiom.is_alpha() && alpha_runeword {
            let current_abs_pos = _section_recovery.item_start_bit + recorder.pos();
            let next_child_off = if let Some(offsets) = child_marker_offsets {
                offsets.iter().filter(|&&off| off >= current_abs_pos).min().cloned()
            } else {
                None
            };
            
            let target_peek_pos = next_child_off.unwrap_or(current_abs_pos);
            let is_close = if next_child_off.is_some() {
                target_peek_pos >= current_abs_pos && (target_peek_pos - current_abs_pos) < 17
            } else {
                true 
            };
            
            if is_close {
                if let Some(header_info) = crate::item::peek_item_header_at(_section_recovery.bytes, target_peek_pos, huffman, axiom.save_is_alpha, 0) {
                    let (mode, loc, _x, code_peek, flags, version_peek, _is_compact, _header_bits, _nudge, _has_checksum) = header_info;
                    if crate::item::is_plausible_item_header(mode, loc, code_peek.as_bytes(), flags, version_peek, axiom.save_is_alpha) {
                        let trimmed_peek = code_peek.trim();
                        let is_socketable = (trimmed_peek.starts_with('r') && trimmed_peek.len() <= 3)
                            || (trimmed_peek.starts_with('g') && trimmed_peek.len() == 3)
                            || trimmed_peek == "jew" || trimmed_peek == "tsc" || trimmed_peek == "isc";
                        if is_socketable {
                            saw_terminator = true;
                            terminator_bit = false;
                            break;
                        }
                    }
                }
            }
        }

        if let Some(limit) = recorder.limit() {
            if recorder.pos() >= limit {
                if axiom.is_alpha() && !saw_terminator {
                    saw_terminator = true;
                    terminator_bit = false; 
                }
                break;
            }
        }

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
                props.push(prop);
                nested_items.extend(items);
                if axiom.is_alpha() && is_actual_runeword && nested_items.len() >= 3 {
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
                        saw_terminator = true;
                    }
                    break;
                }
            }
        }
    }

    if axiom.is_alpha() && (alpha_runeword || code.trim() == "ucb8" || code.trim() == "bwcw") && axiom.is_socketed && nested_items.is_empty() {
        let mut child_idx = 0;
        loop {
            let mut current_pos = recorder.pos();
            let mut current_abs_pos = _section_recovery.item_start_bit + current_pos;
            
            if let Some(offsets) = child_marker_offsets {
                if let Some(&next_marker_off) = offsets.iter().filter(|&&off| off >= current_abs_pos).min() {
                    let diff = next_marker_off - current_abs_pos;
                    if diff > 0 {
                        recorder.skip_and_record(diff as u32)?;
                        current_pos = recorder.pos();
                        current_abs_pos = next_marker_off;
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
                    let is_socketable = (normalized_peek.starts_with('r') && normalized_peek.len() <= 3)
                        || (normalized_peek.starts_with('g') && normalized_peek.len() == 3)
                        || normalized_peek == "jew" || normalized_peek == "tsc" || normalized_peek == "isc";
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
                        let res = recovery_fn(_section_recovery.bytes, current_abs_pos, huffman, child_idx, axiom.save_is_alpha);
                        d.set(prev);
                        res
                    });

                    if let Ok((mut child, end_pos)) = result {
                        child.mode = 6;
                        child.header.mode = 6;
                        child.code = normalized_peek.to_string();
                        child.body.code = normalized_peek.to_string();
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
                        
                        let is_tome = code.trim() == "ucb8" || code.trim() == "bwcw";
                        let max_children = if is_tome { 20 } else { 3 };
                        if child_idx >= max_children {
                            break;
                        }
                        continue;
                    }
                }
            }
            break;
        }
    }

    if axiom.is_alpha() && is_actual_runeword {
        recorder.set_limit(recorder.pos() + 512);
    } else if let Some(l) = old_limit {
        recorder.set_limit(l);
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
    
    let id_bits = 9; 
    let stat_id = recorder.read_bits::<u32>(id_bits)?;
    
    let rhythm = axiom.property_rhythm(alpha_runeword, is_v105_shadow, is_compact, stat_id);
    
    let id_bits = rhythm.id_bits;
    let terminator = (1u32 << id_bits) - 1;

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
            true, 
            term_bit,
            Vec::new(),
        )));
    }


    let mut param = 0;
    let mut nested_items = Vec::new();

    let effective_width = if let Some(width) = rhythm.value_bits {
        axiom.stat_bit_width(stat_id, width)
    } else {
        let mapped_id = axiom.map_alpha_id(stat_id);
        let mut default_width = if let Some(stat) = crate::data::stat_costs::STAT_COSTS.iter().find(|s| s.id == mapped_id) {
            if stat.save_param_bits > 0 {
                param = recorder.read_bits::<u32>(stat.save_param_bits as u32)?;
            }
            stat.save_bits as u32
        } else {
            9
        };
        
        // Alpha v105 Version 0 and 1 items use a 17-bit rhythm (9-bit id + 8-bit value) for standard stats.
        if axiom.save_is_alpha && (_version == 0 || _version == 1) && rhythm.id_bits == 9 && default_width == 9 {
            default_width = 8;
        }

        axiom.stat_bit_width(stat_id, default_width)
    };

    let is_stat_317 = stat_id == 317 || axiom.map_alpha_id(stat_id) == 317;
    let is_stat_320 = stat_id == 320 || axiom.map_alpha_id(stat_id) == 320;
    let is_already_nested = recorder.context_stack().iter().any(|s| s == "nested");
    let mut handled = false;
    let raw_value;

    if axiom.is_alpha() && alpha_runeword && axiom.is_socketed && (is_stat_317 || is_stat_320) && !is_already_nested {
        let entry_pos = recorder.pos();
        let absolute_entry_pos = reader_ctx.item_start_bit + entry_pos;
        recorder.push_context("nested");
        
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
            let res = recovery_fn(reader_ctx.bytes, found_pos, huffman, nested_items.len(), axiom.save_is_alpha);
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
            raw_value = 0; 
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
