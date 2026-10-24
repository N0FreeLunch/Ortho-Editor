use crate::data::bit_cursor::BitCursor;
use crate::data::generated::layout_policy::{
    LegacyHeaderCodeTrialLayout as CodeTrialLayout,
    LEGACY_HEADER_COMPACT_CODE_POLICY as HEADER_COMPACT_CODES,
    LEGACY_HEADER_GAP_HINT_POLICY as HEADER_GAP_HINTS,
};
use crate::domain::item::serialization::peek_item_header_at_with_base;
use crate::domain::payload::HuffmanTree;
use bitstream_io::BitRead;
use std::io;

pub type ItemHeaderPeek = (u8, u8, u8, String, u32, u8, bool, u64, i8, bool);

/// Owned provisional observation inside the forensic Header boundary.
/// Code bytes/allocation and the signed trial gap are preserved verbatim;
/// this observation does not establish a canonical Header or new format truth.
pub(crate) struct ProvisionalHeaderPeek {
    mode: u8,
    location: u8,
    x: u8,
    code: String,
    flags: u32,
    version: u8,
    is_compact: bool,
    consumed_bits: u64,
    trial_gap_bits: i8,
    has_checksum: bool,
}

impl From<ItemHeaderPeek> for ProvisionalHeaderPeek {
    fn from(value: ItemHeaderPeek) -> Self {
        let (
            mode,
            location,
            x,
            code,
            flags,
            version,
            is_compact,
            consumed_bits,
            trial_gap_bits,
            has_checksum,
        ) = value;
        Self {
            mode,
            location,
            x,
            code,
            flags,
            version,
            is_compact,
            consumed_bits,
            trial_gap_bits,
            has_checksum,
        }
    }
}

impl From<ProvisionalHeaderPeek> for ItemHeaderPeek {
    fn from(value: ProvisionalHeaderPeek) -> Self {
        (
            value.mode,
            value.location,
            value.x,
            value.code,
            value.flags,
            value.version,
            value.is_compact,
            value.consumed_bits,
            value.trial_gap_bits,
            value.has_checksum,
        )
    }
}

/// Decodes one provisional checksum hypothesis; the caller owns checkpoint rollback.
fn probe_checksum_trial<R: BitRead>(
    cursor: &mut BitCursor<R>,
    huff: &HuffmanTree,
    alpha_mode: bool,
    is_first_item: bool,
    start_bit: u64,
    has_checksum: bool,
) -> Option<ProvisionalHeaderPeek> {
    let mut result = None;
    if let Ok((header, _, _)) = crate::domain::item::entity::parse_item_header(
        cursor,
        alpha_mode,
        None,
        None,
        is_first_item,
        None,
        Some(has_checksum),
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
            for _ in 0..CodeTrialLayout::HUFFMAN_CODE_SYMBOLS {
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
                for _ in 0..CodeTrialLayout::ASCII_CODE_BYTES {
                    match cursor.read_bits::<u8>(CodeTrialLayout::ASCII_BYTE_BITS) {
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
                    && (CodeTrialLayout::ASCII_OVERRIDE_CODES.contains(&trimmed_ascii)
                        || crate::domain::forensic::v105::axioms::is_v105_summary_code(&ascii_code))
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
                result = Some(ProvisionalHeaderPeek {
                    mode: header.mode,
                    location: header.location,
                    x: header.x,
                    code: trimmed,
                    flags: header.flags,
                    version: header.version,
                    is_compact,
                    consumed_bits: consumed,
                    trial_gap_bits: 0i8,
                    has_checksum: header.has_checksum,
                });
            }
        }
    }
    result
}

/// Obtains provisional hints with the inherited checksum trials and cursor effects.
pub(crate) fn probe_header_hints<R: BitRead>(
    cursor: &mut BitCursor<R>,
    huff: &HuffmanTree,
    ctx: Option<(&[u8], u64)>,
    alpha_mode: bool,
    idx: usize,
    start_bit: u64,
) -> Option<ProvisionalHeaderPeek> {
    let is_first_item = idx == 0;
    let mut peek = if alpha_mode && ctx.is_some() {
        let (bytes, rel_start_bit) = ctx.unwrap();
        peek_item_header_at_with_base(bytes, rel_start_bit, Some(start_bit), huff, true, idx)
            .map(ProvisionalHeaderPeek::from)
    } else {
        None
    };

    if alpha_mode && peek.is_none() {
        let saved = cursor.checkpoint();

        // Path A: Try parsing assuming has_checksum is true
        let true_result =
            probe_checksum_trial(cursor, huff, alpha_mode, is_first_item, start_bit, true);
        cursor.rollback(saved);

        // Path B: Try parsing assuming has_checksum is false
        let false_result =
            probe_checksum_trial(cursor, huff, alpha_mode, is_first_item, start_bit, false);
        cursor.rollback(saved);

        // Resolve: Select the path that yields a valid summary code.
        let is_trusted_compact_code = |code: &str| {
            HEADER_COMPACT_CODES
                .trusted_compact_codes
                .contains(&code.trim())
        };
        let true_is_summary = true_result
            .as_ref()
            .map(|r| crate::domain::forensic::v105::axioms::is_v105_summary_code(&r.code))
            .unwrap_or(false);
        let false_is_summary = false_result
            .as_ref()
            .map(|r| crate::domain::forensic::v105::axioms::is_v105_summary_code(&r.code))
            .unwrap_or(false);
        let true_is_trusted_compact = true_result
            .as_ref()
            .map(|r| is_trusted_compact_code(&r.code))
            .unwrap_or(false);
        let false_is_trusted_compact = false_result
            .as_ref()
            .map(|r| is_trusted_compact_code(&r.code))
            .unwrap_or(false);

        if true_is_summary && !false_is_summary {
            peek = true_result;
        } else if false_is_summary && !true_is_summary {
            peek = false_result;
        } else if true_is_summary && false_is_summary {
            // If both yield summary codes, prioritize the one with a standard version (< 7)
            let true_v = true_result.as_ref().map(|r| r.version).unwrap_or(7);
            let false_v = false_result.as_ref().map(|r| r.version).unwrap_or(7);
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

/// Borrowed, provisional parser arguments selected by forensic hint arbitration.
/// The code retains its original input allocation and whitespace.
pub(crate) struct ResolvedHeaderHints<'a> {
    pub code_peek: Option<&'a str>,
    pub gap_override: Option<usize>,
    pub forced_compact: Option<bool>,
    pub has_checksum_peek: Option<bool>,
}

/// Resolve inherited hint precedence without reading or changing a bit cursor.
pub(crate) fn resolve_header_hints<'a>(
    peek: &'a Option<ProvisionalHeaderPeek>,
    code_hint: Option<&'a str>,
    forced_compact: Option<bool>,
    alpha_mode: bool,
) -> ResolvedHeaderHints<'a> {
    let is_compact_peek = peek.as_ref().map(|p| p.is_compact).unwrap_or(false);
    let code_peek = code_hint
        .filter(|hint| !hint.trim().is_empty())
        .or(peek.as_ref().map(|p| p.code.as_str()));
    let gap_code = code_peek.map(crate::item::normalize_alpha_code_hint);
    let gap_override = peek.as_ref().and_then(|p| {
        if matches!(gap_code.map(|code| code.trim()), Some(code) if HEADER_GAP_HINTS.excluded_codes.contains(&code)) {
            return None;
        }

        // Reject unreliable gap guesses from malformed peek codes.
        // If peek produced a code with whitespace or length < 3, or if
        // an explicit code_hint was provided and peek's code does not match,
        // do not override the gap with peek's unverified 0-gap offset.
        let p_code_trimmed = p.code.trim();
        if p_code_trimmed.contains(' ') || p_code_trimmed.len() < 3 {
            return None;
        }
        if let Some(hint) = code_hint {
            let hint_trimmed = hint.trim();
            if !hint_trimmed.is_empty() && hint_trimmed != p_code_trimmed {
                return None;
            }
        }

        let mut gap = p.trial_gap_bits as usize;
        // Alpha v105 version 7 non-compact items reuse the legacy gap budget
        // differently from the earlier compact/summary cases. Keep the
        // original trial width for compact items, but trim the version-7
        // non-compact hints so other families stay aligned.
        if alpha_mode && p.version == HEADER_GAP_HINTS.alpha_noncompact_adjusted_header_version && !p.is_compact {
            gap = gap.saturating_sub(HEADER_GAP_HINTS.alpha_noncompact_reduction_bits);
        }
        Some(gap)
    });
    let has_checksum_peek = peek.as_ref().map(|p| p.has_checksum);

    let mut forced_compact = forced_compact.or(if is_compact_peek { Some(true) } else { None });
    if let Some(code) = code_peek {
        if HEADER_COMPACT_CODES
            .forced_compact_hint_codes
            .contains(&code.trim())
        {
            forced_compact = Some(true);
        }
    }
    ResolvedHeaderHints {
        code_peek,
        gap_override,
        forced_compact,
        has_checksum_peek,
    }
}

#[cfg(test)]
mod hint_resolution_tests {
    use super::{resolve_header_hints, ItemHeaderPeek, ProvisionalHeaderPeek};

    fn peek(code: &str, compact: bool, gap: i8) -> Option<ProvisionalHeaderPeek> {
        Some(ProvisionalHeaderPeek {
            mode: 0,
            location: 0,
            x: 0,
            code: code.to_string(),
            flags: 0,
            version: 7,
            is_compact: compact,
            consumed_bits: 0,
            trial_gap_bits: gap,
            has_checksum: true,
        })
    }

    #[d2r_macros::hypothesis_test]
    fn hint_resolution_preserves_borrowed_code_and_explicit_priority() {
        let peek = peek("rin", false, 50);
        let explicit = String::from("rin");
        let observed = &peek.as_ref().unwrap().code;
        assert_ne!(explicit.as_ptr(), observed.as_ptr());
        let resolved = resolve_header_hints(&peek, Some(&explicit), Some(false), true);
        assert_eq!(resolved.code_peek, Some("rin"));
        assert_eq!(resolved.code_peek.unwrap().as_ptr(), explicit.as_ptr());
        assert_eq!(resolved.gap_override, Some(5));
        assert_eq!(resolved.forced_compact, Some(false));
        assert_eq!(resolved.has_checksum_peek, Some(true));

        let padded = String::from("  rin  ");
        let resolved = resolve_header_hints(&peek, Some(&padded), None, true);
        assert_eq!(resolved.code_peek, Some("  rin  "));
        assert_eq!(resolved.code_peek.unwrap().as_ptr(), padded.as_ptr());
        assert_eq!(resolved.gap_override, Some(5));

        let resolved = resolve_header_hints(&peek, Some(" \u{2003}\t"), None, true);
        assert_eq!(resolved.code_peek.unwrap().as_ptr(), observed.as_ptr());
        assert_eq!(resolved.gap_override, Some(5));
        let missing = None;
        let resolved = resolve_header_hints(&missing, None, Some(false), false);
        assert_eq!(resolved.code_peek, None);
        assert_eq!(resolved.gap_override, None);
        assert_eq!(resolved.forced_compact, Some(false));
        assert_eq!(resolved.has_checksum_peek, None);
    }

    #[d2r_macros::hypothesis_test]
    fn hint_resolution_preserves_alias_gap_and_compact_precedence() {
        // Existing source policy: normalized gap admission and raw compact
        // admission intentionally use different representations of the code.
        let alias = peek("k g", false, 50);
        let resolved = resolve_header_hints(&alias, Some("k g"), Some(false), true);
        assert_eq!(resolved.code_peek, Some("k g"));
        assert_eq!(resolved.gap_override, None);
        assert_eq!(resolved.forced_compact, Some(false));
        let compact_code = peek("jav", false, -1);
        let resolved = resolve_header_hints(&compact_code, None, Some(false), false);
        assert_eq!(resolved.forced_compact, Some(true));
        assert_eq!(resolved.gap_override, None);

        let negative = peek("rin", false, -1);
        let resolved = resolve_header_hints(&negative, None, None, true);
        assert_eq!(
            resolved.gap_override,
            Some((-1_i8 as usize).saturating_sub(45))
        );
        let compact_peek = peek("rin", true, -1);
        let resolved = resolve_header_hints(&compact_peek, None, Some(false), true);
        assert_eq!(resolved.gap_override, Some(-1_i8 as usize));
        assert_eq!(resolved.forced_compact, Some(false));
        assert_eq!(
            resolve_header_hints(&negative, Some("amu"), None, true).gap_override,
            None
        );

        let byte_short = peek("\u{00e9}", false, 50);
        let byte_three = peek("\u{6f22}", false, 50);
        let internal_tab = peek("a\tb", false, 50);
        let internal_space = peek("a b", false, 50);
        assert_eq!(
            resolve_header_hints(&byte_short, None, None, true).gap_override,
            None
        );
        assert_eq!(
            resolve_header_hints(&byte_three, None, None, true).gap_override,
            Some(5)
        );
        assert_eq!(
            resolve_header_hints(&internal_tab, None, None, true).gap_override,
            Some(5)
        );
        assert_eq!(
            resolve_header_hints(&internal_space, None, None, true).gap_override,
            None
        );
    }


    #[d2r_macros::hypothesis_test]
    fn provisional_peek_legacy_bridge_preserves_each_field_and_string_allocation() {
        let mut code = String::with_capacity(64);
        code.push_str("  raw\0\u{2003}");
        let pointer = code.as_ptr();
        let capacity = code.capacity();
        let legacy: ItemHeaderPeek = (
            0x12,
            0x34,
            0x56,
            code,
            0x12345678,
            0x78,
            true,
            0x1020304050607080,
            -113,
            false,
        );
        let typed = ProvisionalHeaderPeek::from(legacy);
        assert_eq!(typed.mode, 0x12);
        assert_eq!(typed.location, 0x34);
        assert_eq!(typed.x, 0x56);
        assert_eq!(typed.code, "  raw\0\u{2003}");
        assert_eq!(typed.code.as_ptr(), pointer);
        assert_eq!(typed.code.capacity(), capacity);
        assert_eq!(typed.flags, 0x12345678);
        assert_eq!(typed.version, 0x78);
        assert!(typed.is_compact);
        assert_eq!(typed.consumed_bits, 0x1020304050607080);
        assert_eq!(typed.trial_gap_bits, -113);
        assert!(!typed.has_checksum);
        let roundtrip: ItemHeaderPeek = typed.into();
        assert_eq!(roundtrip.3.as_ptr(), pointer);
        assert_eq!(roundtrip.3.capacity(), capacity);
        assert_eq!(
            roundtrip,
            (
                0x12,
                0x34,
                0x56,
                "  raw\0\u{2003}".to_string(),
                0x12345678,
                0x78,
                true,
                0x1020304050607080,
                -113,
                false
            )
        );
    }
}
