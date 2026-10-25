//! Property-list encoding beside the Stats parser and layout axioms.

use super::{ItemProperty, StatsAxiom};
use crate::domain::item::{BitEmitter, HuffmanTree, Item, ItemQuality};
use crate::domain::forensic::property_write::{LegacyNestedWriter, LegacyWritePolicy};
use crate::domain::stats::property_write::{NoNestedPropertyWriter, write_property_stream};
use crate::domain::stats::retail_write::RetailWritePolicy;
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
    if !axiom.save_is_alpha {
        let policy = RetailWritePolicy::new(code, &axiom.code, axiom.is_compact);
        return write_property_stream(
            emitter,
            props,
            terminator_bit,
            properties_complete,
            &policy,
            &mut NoNestedPropertyWriter,
        );
    }
    let policy = LegacyWritePolicy::new(code, version, alpha_runeword, is_v105_shadow, axiom);
    let mut children = LegacyNestedWriter::new(code, nested_items, huffman, axiom);
    write_property_stream(
        emitter,
        props,
        terminator_bit,
        properties_complete,
        &policy,
        &mut children,
    )
}
