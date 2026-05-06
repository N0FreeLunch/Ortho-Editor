use bitstream_io::BitRead;
use crate::domain::item::{Item, ItemBitRange, ItemQuality};
use crate::domain::stats::{
    ItemProperty, StatsAxiom, ItemStats,
};
use crate::data::bit_cursor::BitCursor;
use crate::item::{self, HuffmanTree, ParsingResult, PropertyReaderContext, ItemHeader};
use crate::domain::header::entity::ItemSegmentType;

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
) -> ParsingResult<(Vec<ItemProperty>, bool, bool, Option<u8>, Option<Vec<bool>>, Option<u64>, Vec<crate::domain::item::Item>)> {
    let mut alpha_v5_runeword_extra = None;
    let mut alpha_shadow_skip_bits = None;
    cursor.begin_segment(ItemSegmentType::Stats);
    let trimmed_code = code.trim();
    let quality_val = quality.unwrap_or(ItemQuality::Normal);
    let axiom = StatsAxiom::new(version, quality_val, alpha_mode)
        .with_personalization(is_personalized)
        .with_code(trimmed_code);
    let is_alpha = axiom.is_alpha();

    let is_v105_shadow_final = alpha_mode && version == 5 && is_v105_shadow;
    let is_scroll = trimmed_code == "tsc" || trimmed_code == "isc";
    let is_potion = trimmed_code.starts_with('h') || trimmed_code.starts_with('m') || (version == 5 && trimmed_code.starts_with('7')) || (trimmed_code.starts_with('r') && trimmed_code.len() <= 3);

    if is_alpha && trimmed_code.is_empty() {
        return Ok((Vec::new(), true, false, None, None, None, Vec::new()));
    }

    // Removed version 4 early exit to allow parsing items with stats/nested children (e.g. mxh).

    if is_alpha && version == 5 && !is_v105_shadow_final && 
       (is_potion || is_scroll || quality_val < ItemQuality::Magic) {
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
    let (props, complete, term, nested_items) = read_property_list(cursor, trimmed_code, version, section_recovery, huffman, is_runeword, is_v105_shadow_final, &axiom, |bytes, pos, huff, idx, alpha| {
        crate::domain::item::serialization::parse_item_at_with_limit(bytes, pos, huff, idx, alpha, None)
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

pub fn read_property_list<R: BitRead, F>(
    recorder: &mut BitCursor<R>,
    code: &str,
    version: u8,
    _section_recovery: PropertyReaderContext,
    huffman: &HuffmanTree,
    alpha_runeword: bool,
    is_v105_shadow: bool,
    axiom: &StatsAxiom,
    mut recovery_fn: F,
) -> ParsingResult<(Vec<ItemProperty>, bool, bool, Vec<crate::domain::item::Item>)> 
where 
    F: FnMut(&[u8], u64, &HuffmanTree, usize, bool) -> ParsingResult<(crate::domain::item::Item, u64)>
{
    let mut props = Vec::new();
    let mut nested_items = Vec::new();
    let mut terminator_bit = false;
    let mut saw_terminator = false;

    // Heuristic for compact items in Alpha
    let is_compact = code.trim().is_empty() || code.len() < 3;

    let preserve_trailing_align = axiom.is_alpha() && version == 0 && code.trim().is_empty();

    // Track nesting depth
    let depth = recorder.get_context_count();
    if depth > 5 { return Ok((props, saw_terminator, terminator_bit, nested_items)); }

    loop {
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
        )?;
        match result {
            Some((prop, is_term, term_bit, items)) => {
                println!("[DEBUG] SLICE 11: Processed stat_id: {} at pos {}", prop.stat_id, recorder.pos());
                if is_term {
                    saw_terminator = true;
                    terminator_bit = term_bit;
                    break;
                }
                props.push(prop);
                nested_items.extend(items);
            }
            None => break,
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
    version: u8,
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
    let terminator = (1 << id_bits) - 1;

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
            Vec::new()
        )));
    }

    let raw_value;
    let mut param = 0;
    let mut nested_items = Vec::new();

    let mut effective_width = if let Some(width) = rhythm.value_bits {
        axiom.stat_bit_width(stat_id, width)
    } else {
        let mapped_id = axiom.map_alpha_id(stat_id);
        let default_width = if let Some(stat) = crate::data::stat_costs::STAT_COSTS.iter().find(|s| s.id == mapped_id) {
            if stat.save_param_bits > 0 {
                param = recorder.read_bits::<u32>(stat.save_param_bits as u32)?;
            }
            stat.save_bits as u32
        } else {
            9
        };
        axiom.stat_bit_width(stat_id, default_width)
    };

    // Slice 7: Force atomic skip for Stat 320 to prevent stream termination
    if axiom.is_alpha() && (stat_id == 320 || axiom.map_alpha_id(stat_id) == 320) {
        if crate::item::item_trace_enabled() {
            println!("[DEBUG] SLICE 7: Forcing atomic skip for Stat 320 at pos {}", recorder.pos());
        }
        // Stat 320 in Alpha v105 is known to be a 2871-bit blob
        let skip_bits = 2871; 
        recorder.skip_and_record(skip_bits)?;
        
        return Ok(Some((
            ItemProperty {
                stat_id,
                raw_value: 0,
                param: 0,
                name: "atomic_blob".to_string(),
                value: 0,
                range: ItemBitRange { start: entry_start, end: recorder.pos() },
            },
            false,
            false,
            Vec::new()
        )));
    }

    // Slice 7: Stat 317/320 nested recovery seam
    // Prevent recursive triggering of Stat 320 recovery by checking for "nested" context
    let is_already_nested = recorder.get_context_count() > 0;
    // Log stat identification for diagnosis
    if crate::item::item_trace_enabled() && (stat_id == 317 || axiom.map_alpha_id(stat_id) == 317) {
        println!("[DEBUG] SLICE 11: Stat 317 found, pos: {}, width: {}, already_nested: {}", recorder.pos(), effective_width, is_already_nested);
    }
    if axiom.is_alpha() && (stat_id == 317 || axiom.map_alpha_id(stat_id) == 317) && effective_width > 32 && !is_already_nested {
        if crate::item::item_trace_enabled() {
            println!("[DEBUG] SLICE 11 TRIGGER: Stat 317 nested recovery at pos {}, axiom_alpha: {}", recorder.pos(), axiom.is_alpha());
        }
        recorder.push_context("nested");
        if let Ok((child, end_pos)) = recovery_fn(
            reader_ctx.bytes,
            recorder.pos(),
            huffman,
            0,
            axiom.save_is_alpha,
        ) {
            println!("[DEBUG] SLICE 11: Child item parsed, end_pos: {}, consumed: {}", end_pos, end_pos - recorder.pos());
            if end_pos > recorder.pos() {
                let consumed = (end_pos - recorder.pos()) as u32;
                recorder.skip_and_record(consumed)?;
                effective_width = consumed;
                nested_items.push(child);
            }
        } else {
            println!("[DEBUG] SLICE 11: Failed to parse child item at pos {}", recorder.pos());
        }
        recorder.pop_context();
    }


    if effective_width > 32 {
        recorder.skip_and_record(effective_width)?;
        raw_value = 0; // Huge payload not preserved in raw_value
    } else {
        raw_value = recorder.read_bits::<u32>(effective_width)?;
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
