use bitstream_io::BitRead;
use crate::domain::item::{Item, ItemBitRange, ItemQuality};
use crate::domain::stats::{
    ItemProperty, StatsAxiom, ItemStats,
};
use crate::data::bit_cursor::BitCursor;
use crate::item::{HuffmanTree, ParsingResult, PropertyReaderContext, ItemHeader};
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
) -> ParsingResult<(Vec<ItemProperty>, bool, bool, Option<u8>, Option<Vec<bool>>, Option<u64>)> {
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
        return Ok((Vec::new(), true, false, None, None, None));
    }

    if is_alpha && version == 4 && !is_personalized {
        return Ok((Vec::new(), true, false, None, None, None));
    }

    if is_alpha && version == 5 && !is_v105_shadow_final && 
       (is_potion || is_scroll || quality_val < ItemQuality::Magic) {
          if trimmed_code == "7mgw" {
              let mut payload = Vec::new();
              for _ in 0..28 { payload.push(cursor.read_bit()?); }
              return Ok((Vec::new(), true, false, None, Some(payload), None));
          }
          return Ok((Vec::new(), true, false, None, None, None));
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
    let (props, complete, term) = read_property_list(cursor, trimmed_code, version, section_recovery, huffman, is_runeword, is_v105_shadow_final, &axiom, |_, _, _, _, _| {
        Ok((Item::default(), 0))
    })?;

    cursor.end_segment();
    Ok((props, complete, term, alpha_v5_runeword_extra, None, alpha_shadow_skip_bits))
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
) -> ParsingResult<(Vec<ItemProperty>, bool, bool)> 
where 
    F: FnMut(&[u8], u64, &HuffmanTree, usize, bool) -> ParsingResult<(crate::domain::item::Item, u64)>
{
    let mut props = Vec::new();
    let mut terminator_bit = false;
    let mut saw_terminator = false;

    // Heuristic for compact items in Alpha
    let is_compact = code.trim().is_empty() || code.len() < 3;

    let preserve_trailing_align = axiom.is_alpha() && version == 0 && code.trim().is_empty();
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
            &mut recovery_fn,
        )?;
        match result {
            Some((prop, is_term, term_bit)) => {
                if is_term {
                    saw_terminator = true;
                    terminator_bit = term_bit;
                    break;
                }
                props.push(prop);
            }
            None => break,
        }
    }

    Ok((props, saw_terminator, terminator_bit))
}

pub fn parse_single_property<R, F>(
    recorder: &mut BitCursor<R>,
    version: u8,
    huffman: &HuffmanTree,
    alpha_runeword: bool,
    axiom: &StatsAxiom,
    section_recovery: F,
) -> ParsingResult<Option<(ItemProperty, bool, bool)>>
where
    R: BitRead,
    F: FnMut(&[u8], u64, &HuffmanTree, usize, bool) -> ParsingResult<(crate::domain::item::Item, u64)>,
{
    parse_single_property_internal(recorder, version, huffman, alpha_runeword, false, false, false, axiom, section_recovery)
}

fn parse_single_property_internal<R, F>(
    recorder: &mut BitCursor<R>,
    _version: u8,
    _huffman: &HuffmanTree,
    alpha_runeword: bool,
    is_compact: bool,
    is_v105_shadow: bool,
    preserve_trailing_align: bool,
    axiom: &StatsAxiom,
    mut _section_recovery: F,
) -> ParsingResult<Option<(ItemProperty, bool, bool)>>
where
    R: BitRead,
    F: FnMut(&[u8], u64, &HuffmanTree, usize, bool) -> ParsingResult<(crate::domain::item::Item, u64)>,
{

    let entry_start = recorder.pos();
    let rhythm = axiom.property_rhythm(alpha_runeword, is_v105_shadow, is_compact);
    
    let id_bits = rhythm.id_bits;
    let terminator = (1 << id_bits) - 1;

    let stat_id = recorder.read_bits::<u32>(id_bits)?;
    
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
            term_bit
        )));
    }

    let raw_value;
    let mut param = 0;

    if let Some(width) = rhythm.value_bits {
        let effective_width = axiom.stat_bit_width(stat_id, width);

        if effective_width > 32 {
            recorder.skip_and_record(effective_width)?;
            raw_value = 0; // Huge payload not preserved in raw_value
        } else {
            raw_value = recorder.read_bits::<u32>(effective_width)?;
        }
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
        let effective_width = axiom.stat_bit_width(stat_id, default_width);
        if effective_width > 32 {
            recorder.skip_and_record(effective_width)?;
            raw_value = 0;
        } else {
            raw_value = recorder.read_bits::<u32>(effective_width)?;
        }
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
        false
    )))
}
