use bitstream_io::BitRead;
use crate::domain::item::{ItemBitRange};
use crate::domain::stats::{
    ItemProperty, StatsAxiom,
};
use crate::data::bit_cursor::BitCursor;
use crate::item::{HuffmanTree, ParsingResult, PropertyReaderContext};

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

    loop {
        let result = parse_single_property_internal(recorder, version, huffman, alpha_runeword, is_compact, is_v105_shadow, axiom, &mut recovery_fn)?;
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
    parse_single_property_internal(recorder, version, huffman, alpha_runeword, false, false, axiom, section_recovery)
}

fn parse_single_property_internal<R, F>(
    recorder: &mut BitCursor<R>,
    _version: u8,
    _huffman: &HuffmanTree,
    alpha_runeword: bool,
    is_compact: bool,
    is_v105_shadow: bool,
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
            while recorder.pos() % 8 != 0 {
                let _p = recorder.read_bit()?;
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
        let effective_width = if axiom.is_alpha() && axiom.version == 5 {
            match stat_id {
                114 | 289 | 287 | 309 | 310 | 311 | 312 => 14,
                _ => width,
            }
        } else {
            width
        };
        raw_value = recorder.read_bits::<u32>(effective_width)?;
    } else {
        let mapped_id = axiom.map_alpha_id(stat_id);
        if let Some(stat) = crate::data::stat_costs::STAT_COSTS.iter().find(|s| s.id == mapped_id) {
            if stat.save_param_bits > 0 {
                param = recorder.read_bits::<u32>(stat.save_param_bits as u32)?;
            }
            raw_value = recorder.read_bits::<u32>(stat.save_bits as u32)?;
        } else {
            raw_value = recorder.read_bits::<u32>(9)?;
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
