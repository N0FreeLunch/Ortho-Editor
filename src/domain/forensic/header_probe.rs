use crate::data::bit_cursor::BitCursor;
use crate::domain::item::serialization::peek_item_header_at_with_base;
use crate::domain::payload::HuffmanTree;
use bitstream_io::BitRead;
use std::io;

pub type ItemHeaderPeek = (u8, u8, u8, String, u32, u8, bool, u64, i8, bool);

/// Obtains provisional hints with the inherited checksum trials and cursor effects.
pub(crate) fn probe_header_hints<R: BitRead>(
    cursor: &mut BitCursor<R>,
    huff: &HuffmanTree,
    ctx: Option<(&[u8], u64)>,
    alpha_mode: bool,
    idx: usize,
    start_bit: u64,
) -> Option<ItemHeaderPeek> {
    let is_first_item = idx == 0;
    let mut peek = if alpha_mode && ctx.is_some() {
        let (bytes, rel_start_bit) = ctx.unwrap();
        peek_item_header_at_with_base(bytes, rel_start_bit, Some(start_bit), huff, true, idx)
    } else {
        None
    };

    if alpha_mode && peek.is_none() {
        let saved = cursor.checkpoint();

        // Path A: Try parsing assuming has_checksum is true
        let mut true_result = None;
        if let Ok((header, _, _)) = crate::domain::item::entity::parse_item_header(
            cursor,
            alpha_mode,
            None,
            None,
            is_first_item,
            None,
            Some(true),
            Some(start_bit),
        ) {
            let s_axiom = crate::domain::stats::axiom::StatsAxiom::new(
                header.version,
                header
                    .quality
                    .unwrap_or(crate::domain::item::quality::ItemQuality::Normal),
                alpha_mode,
            );
            let is_ho = s_axiom.is_header_only(header.flags, "");
            if !is_ho {
                let huffman_pos = cursor.pos();
                let mut decoded = String::new();
                let mut ok = true;
                for _ in 0..4 {
                    if let Ok(c) = huff.decode_internal(|| {
                        cursor
                            .read_bit()
                            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{:?}", e)))
                    }) {
                        decoded.push(c);
                    } else {
                        ok = false;
                        break;
                    }
                }
                if alpha_mode {
                    let saved_dec = cursor.checkpoint();
                    cursor.rollback(huffman_pos);
                    let mut ascii_code = String::new();
                    let mut success = true;
                    for _ in 0..3 {
                        match cursor.read_bits::<u8>(8) {
                            Ok(ch) => {
                                ascii_code.push(ch as char);
                            }
                            Err(_) => {
                                success = false;
                                break;
                            }
                        }
                    }
                    let trimmed_ascii = ascii_code.trim();
                    if success
                        && (trimmed_ascii == "xrs"
                            || trimmed_ascii == "mp1"
                            || crate::domain::forensic::v105::axioms::is_v105_summary_code(
                                &ascii_code,
                            ))
                    {
                        decoded = ascii_code;
                    } else {
                        cursor.rollback(saved_dec);
                    }
                }
                if ok {
                    let trimmed = decoded.trim().to_string();
                    let is_summary =
                        crate::domain::forensic::v105::axioms::is_v105_summary_code(&trimmed);
                    let is_compact = header.is_compact || is_summary;
                    let consumed = cursor.pos() - start_bit;
                    true_result = Some((
                        header.mode,
                        header.location,
                        header.x,
                        trimmed,
                        header.flags,
                        header.version,
                        is_compact,
                        consumed,
                        0i8,
                        header.has_checksum,
                    ));
                }
            }
        }
        cursor.rollback(saved);

        // Path B: Try parsing assuming has_checksum is false
        let mut false_result = None;
        if let Ok((header, _, _)) = crate::domain::item::entity::parse_item_header(
            cursor,
            alpha_mode,
            None,
            None,
            is_first_item,
            None,
            Some(false),
            Some(start_bit),
        ) {
            let s_axiom = crate::domain::stats::axiom::StatsAxiom::new(
                header.version,
                header
                    .quality
                    .unwrap_or(crate::domain::item::quality::ItemQuality::Normal),
                alpha_mode,
            );
            let is_ho = s_axiom.is_header_only(header.flags, "");
            if !is_ho {
                let huffman_pos = cursor.pos();
                let mut decoded = String::new();
                let mut ok = true;
                for _ in 0..4 {
                    if let Ok(c) = huff.decode_internal(|| {
                        cursor
                            .read_bit()
                            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{:?}", e)))
                    }) {
                        decoded.push(c);
                    } else {
                        ok = false;
                        break;
                    }
                }
                if alpha_mode {
                    let saved_dec = cursor.checkpoint();
                    cursor.rollback(huffman_pos);
                    let mut ascii_code = String::new();
                    let mut success = true;
                    for _ in 0..3 {
                        match cursor.read_bits::<u8>(8) {
                            Ok(ch) => {
                                ascii_code.push(ch as char);
                            }
                            Err(_) => {
                                success = false;
                                break;
                            }
                        }
                    }
                    let trimmed_ascii = ascii_code.trim();
                    if success
                        && (trimmed_ascii == "xrs"
                            || trimmed_ascii == "mp1"
                            || crate::domain::forensic::v105::axioms::is_v105_summary_code(
                                &ascii_code,
                            ))
                    {
                        decoded = ascii_code;
                    } else {
                        cursor.rollback(saved_dec);
                    }
                }
                if ok {
                    let trimmed = decoded.trim().to_string();
                    let is_summary =
                        crate::domain::forensic::v105::axioms::is_v105_summary_code(&trimmed);
                    let is_compact = header.is_compact || is_summary;
                    let consumed = cursor.pos() - start_bit;
                    false_result = Some((
                        header.mode,
                        header.location,
                        header.x,
                        trimmed,
                        header.flags,
                        header.version,
                        is_compact,
                        consumed,
                        0i8,
                        header.has_checksum,
                    ));
                }
            }
        }
        cursor.rollback(saved);

        // Resolve: Select the path that yields a valid summary code.
        let is_trusted_compact_code = |code: &str| matches!(code.trim(), "jav" | "buc" | "us g");
        let true_is_summary = true_result
            .as_ref()
            .map(|r| crate::domain::forensic::v105::axioms::is_v105_summary_code(&r.3))
            .unwrap_or(false);
        let false_is_summary = false_result
            .as_ref()
            .map(|r| crate::domain::forensic::v105::axioms::is_v105_summary_code(&r.3))
            .unwrap_or(false);
        let true_is_trusted_compact = true_result
            .as_ref()
            .map(|r| is_trusted_compact_code(&r.3))
            .unwrap_or(false);
        let false_is_trusted_compact = false_result
            .as_ref()
            .map(|r| is_trusted_compact_code(&r.3))
            .unwrap_or(false);

        if true_is_summary && !false_is_summary {
            peek = true_result;
        } else if false_is_summary && !true_is_summary {
            peek = false_result;
        } else if true_is_summary && false_is_summary {
            // If both yield summary codes, prioritize the one with a standard version (< 7)
            let true_v = true_result.as_ref().map(|r| r.5).unwrap_or(7);
            let false_v = false_result.as_ref().map(|r| r.5).unwrap_or(7);
            if true_v < 7 && false_v == 7 {
                peek = true_result;
            } else if false_v < 7 && true_v == 7 {
                peek = false_result;
            } else {
                peek = true_result;
            }
        } else if true_is_trusted_compact && !false_is_trusted_compact {
            peek = true_result;
        } else if false_is_trusted_compact && !true_is_trusted_compact {
            peek = false_result;
        } else if true_is_trusted_compact && false_is_trusted_compact {
            peek = true_result;
        } else {
            peek = None;
        }
    }
    peek
}
