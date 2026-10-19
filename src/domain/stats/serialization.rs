//! Property-list encoding beside the Stats parser and layout axioms.

use super::{ItemProperty, StatsAxiom};
use crate::domain::item::{BitEmitter, HuffmanTree, Item, ItemQuality};
use std::io;

pub fn write_property_list(
    emitter: &mut BitEmitter,
    code: &str,
    props: &[ItemProperty],
    nested_items: &[Item],
    huffman: &HuffmanTree,
    version: u8,
    alpha_runeword: bool,
    terminator_bit: bool,
    properties_complete: bool,
    _quality: ItemQuality,
    is_v105_shadow: bool,
    axiom: &StatsAxiom,
) -> io::Result<()> {
    let _start_bits = emitter.written_bits();

    // Axiom 0344: Explicit header signal is primary, but blank items in Alpha v105
    // often lack the compact flag despite being structurally compact (80-bit slot).
    // The inference is now centralized in StatsAxiom::with_code.
    let is_compact = axiom.is_compact;
    let rhythm = axiom.property_rhythm(alpha_runeword, is_v105_shadow, is_compact, 0);
    let id_bits = rhythm.id_bits;
    let terminator = (1 << id_bits) - 1;
    let mut item_idx = 0;
    for prop in props {
        let raw_id = prop.stat_id;
        emitter.write_bits(raw_id, id_bits)?;

        let mut handled = false;
        let is_nested_stat = (raw_id == 317 || axiom.map_alpha_id(raw_id) == 317)
            || (raw_id == 320 || axiom.map_alpha_id(raw_id) == 320)
            || (raw_id == 387 || axiom.map_alpha_id(raw_id) == 387);
        if axiom.is_alpha() && is_nested_stat {
            if item_idx < nested_items.len() {
                let child = &nested_items[item_idx];
                let is_stat_320 = raw_id == 320 || axiom.map_alpha_id(raw_id) == 320;
                let is_stat_387 = raw_id == 387 || axiom.map_alpha_id(raw_id) == 387;

                if is_stat_320 || is_stat_387 {
                    let child_bits_vec = if axiom.save_is_alpha
                        && (!child.code.trim().is_ascii() || code.trim() == "wa2")
                    {
                        child.bits.iter().map(|b| b.bit).collect::<Vec<bool>>()
                    } else {
                        child.to_bits(0, huffman, axiom.save_is_alpha)?
                    };
                    let child_bits = child_bits_vec.len();
                    let registry_width = if is_stat_387 {
                        0
                    } else {
                        axiom.stat_bit_width(320, 0)
                    };

                    emitter.extend_bits(child_bits_vec)?;

                    if registry_width > 0 {
                        let budget = registry_width + 2;
                        if child_bits < budget as usize {
                            emitter.write_bits(0, (budget as usize - child_bits) as u32)?;
                        }
                    }
                } else {
                    // Variable budget (Stat 317)
                    let child_bits_vec = if axiom.save_is_alpha
                        && (!child.code.trim().is_ascii() || code.trim() == "wa2")
                    {
                        child.bits.iter().map(|b| b.bit).collect::<Vec<bool>>()
                    } else {
                        child.to_bits(0, huffman, axiom.save_is_alpha)?
                    };
                    emitter.extend_bits(child_bits_vec)?;
                }

                item_idx += 1;
                handled = true;
            }
        }

        if !handled {
            let mapped_id = axiom.map_alpha_id(raw_id);
            let stat_cost = crate::data::stat_costs::STAT_COSTS
                .iter()
                .find(|s| s.id == mapped_id);
            let suppress_authority_params = axiom.omits_property_parameter(code, version);
            if let Some(stat) = stat_cost {
                if stat.save_param_bits > 0 && !suppress_authority_params {
                    emitter.write_bits(prop.param as u32, stat.save_param_bits as u32)?;
                }
            }
            if raw_id != terminator {
                let default_width = axiom.default_property_value_width(
                    version,
                    stat_cost.map(|stat| u32::from(stat.save_bits)),
                    rhythm,
                );

                let effective_width = if code.trim() == "wa2"
                    && (raw_id == 320 || axiom.map_alpha_id(raw_id) == 320)
                {
                    0
                } else {
                    axiom.stat_bit_width(raw_id, default_width)
                };
                emitter.write_bits(prop.raw_value as u32, effective_width)?;
            }
        }
    }
    let already_has_term = props.iter().any(|p| p.stat_id == terminator);
    let is_rw = axiom.is_runeword(0);
    if properties_complete
        && !already_has_term
        && (!axiom.is_alpha()
            || crate::data::generated::layout_policy::LEGACY_PROPERTY_STREAM_POLICY
                .alpha_auto_terminator_header_versions
                .contains(&version))
        && !is_rw
        && !is_compact
    {
        emitter.write_bits(terminator, id_bits)?;
    }
    if properties_complete && rhythm.has_terminal_bit {
        emitter.write_bit(terminator_bit)?;
        if rhythm.has_extra_terminal_bit {
            emitter.write_bit(terminator_bit)?;
        }
        emitter.byte_align()?;
    }

    // Axiom 0354: TVS (Terminator Value Slot) - Alpha v105 standard items
    if properties_complete && let Some(tvs_bits) = axiom.tvs_padding_bits(alpha_runeword) {
        emitter.write_bits(0, tvs_bits)?;
    }

    Ok(())
}
