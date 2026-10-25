//! Legacy width/layout policy and nested item emission outside the codec kernel.
use crate::domain::item::{HuffmanTree, Item};
use crate::domain::stats::StatsAxiom;
use crate::domain::stats::property_entry::{PropertyRhythm, legacy_property_value_width};
use crate::domain::stats::property_write::{NestedPropertyWriter, PropertyWritePolicy};
use crate::wire::BitEmitter;
use std::io;

pub(crate) struct LegacyWritePolicy<'a> {
    code: &'a str,
    version: u8,
    runeword: bool,
    shadow: bool,
    axiom: &'a StatsAxiom,
}
impl<'a> LegacyWritePolicy<'a> {
    pub(crate) fn new(
        code: &'a str,
        version: u8,
        runeword: bool,
        shadow: bool,
        axiom: &'a StatsAxiom,
    ) -> Self {
        Self {
            code,
            version,
            runeword,
            shadow,
            axiom,
        }
    }
}
impl PropertyWritePolicy for LegacyWritePolicy<'_> {
    fn rhythm(&self) -> PropertyRhythm {
        self.axiom
            .property_rhythm(self.runeword, self.shadow, self.axiom.is_compact, 0)
    }
    fn map_id(&self, id: u32) -> u32 {
        self.axiom.map_alpha_id(id)
    }
    fn omit_parameter(&self) -> bool {
        self.axiom.omits_property_parameter(self.code, self.version)
    }
    fn effective_width(&self, raw: u32, bits: Option<u32>, rhythm: PropertyRhythm) -> u32 {
        let default = self
            .axiom
            .default_property_value_width(self.version, bits, rhythm);
        legacy_property_value_width(
            self.code,
            raw,
            || self.axiom.map_alpha_id(raw),
            || self.axiom.stat_bit_width(raw, default),
        )
    }
    fn is_runeword(&self) -> bool {
        self.axiom.is_runeword(0)
    }
    fn allows_auto_terminator(&self) -> bool {
        !self.axiom.is_alpha()
            || crate::data::generated::layout_policy::LEGACY_PROPERTY_STREAM_POLICY
                .alpha_auto_terminator_header_versions
                .contains(&self.version)
    }
    fn is_compact(&self) -> bool {
        self.axiom.is_compact
    }
    fn tvs_padding_bits(&self) -> Option<u32> {
        self.axiom.tvs_padding_bits(self.runeword)
    }
}
pub(crate) struct LegacyNestedWriter<'a> {
    code: &'a str,
    nested_items: &'a [Item],
    huffman: &'a HuffmanTree,
    axiom: &'a StatsAxiom,
    item_idx: usize,
}
impl<'a> LegacyNestedWriter<'a> {
    pub(crate) fn new(
        code: &'a str,
        nested_items: &'a [Item],
        huffman: &'a HuffmanTree,
        axiom: &'a StatsAxiom,
    ) -> Self {
        Self {
            code,
            nested_items,
            huffman,
            axiom,
            item_idx: 0,
        }
    }
}
impl NestedPropertyWriter for LegacyNestedWriter<'_> {
    fn try_write(&mut self, emitter: &mut BitEmitter, raw_id: u32) -> io::Result<bool> {
        let code = self.code;
        let axiom = self.axiom;
        let nested_items = self.nested_items;
        let huffman = self.huffman;
        let mut handled = false;
        let is_nested_stat = (raw_id == 317 || axiom.map_alpha_id(raw_id) == 317)
            || (raw_id == 320 || axiom.map_alpha_id(raw_id) == 320)
            || (raw_id == 387 || axiom.map_alpha_id(raw_id) == 387);
        if axiom.is_alpha() && is_nested_stat {
            if self.item_idx < nested_items.len() {
                let child = &nested_items[self.item_idx];
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

                self.item_idx += 1;
                handled = true;
            }
        }

        Ok(handled)
    }
}
