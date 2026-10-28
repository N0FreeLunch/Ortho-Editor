use std::collections::HashMap;

use crate::domain::item::Item;
use crate::domain::payload::HuffmanTree;
use crate::error::ParsingResult;

pub(super) fn recover_child(
    bytes: &[u8],
    pos: u64,
    huff: &HuffmanTree,
    idx: usize,
    alpha: bool,
    section_base_abs: u64,
    child_offset_codes: &HashMap<u64, String>,
) -> ParsingResult<(Item, u64)> {
    // Use scanner-verified code as hint to ensure correct item identification
    let scanner_code = child_offset_codes.get(&pos).map(|s| s.as_str());
    let peek_res = crate::item::peek_item_header_at(bytes, pos, huff, alpha, 0);
    let peek_code = peek_res.as_ref().map(|p| p.3.as_str());
    let code_hint = scanner_code.or(peek_code);

    let mut limit = None;
    let mut forced_compact = None;
    if alpha {
        if let Some((_, _, _, code, flags, version, is_compact, _, _, _)) = &peek_res {
            if crate::item::item_trace_enabled() {
                eprintln!(
                    "[peek-child] code={} flags=0x{:08X} version={} compact={}",
                    code.trim(),
                    flags,
                    version,
                    is_compact
                );
            }
            let target_width = crate::domain::forensic::v105::axioms::get_v105_target_width(
                *version,
                code,
                *flags,
                Some(idx),
            );
            if target_width > 0 {
                limit = Some(target_width as u64);
            }
            if *is_compact {
                forced_compact = Some(true);
            }
        }
    }

    let (item, mut consumed) = crate::domain::item::serialization::parse_item_at_with_limit(
        bytes,
        pos,
        section_base_abs,
        huff,
        idx,
        alpha,
        limit,
        forced_compact,
        code_hint,
    )?;
    if let Some(l) = limit {
        consumed = consumed.max(l);
    }
    Ok((item, consumed))
}
