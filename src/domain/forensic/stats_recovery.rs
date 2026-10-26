//! Legacy item-stat recovery composition around the property-stream reader.
//! This preserves existing recovery policy; it does not establish new format rules.
use crate::data::bit_cursor::BitCursor;
use crate::data::generated::layout_policy::{
    ALPHA_LEGACY_AUTHORITY_CODES, legacy_alpha_child_code_alias,
};
use crate::domain::header::entity::ItemSegmentType;
use crate::domain::item::quality::ItemQuality;
use crate::domain::payload::HuffmanTree;
use crate::domain::stats::parser::{
    PropertyReaderContext, read_property_list_with_pair_policy, take_socket_recovery_trace_events,
};
use crate::domain::stats::{ItemProperty, StatsAxiom};
use crate::error::ParsingResult;
use bitstream_io::BitRead;

pub(crate) struct LegacyItemStatsReadContext<'a> {
    pub(crate) code: &'a str,
    pub(crate) version: u8,
    pub(crate) ctx: Option<(&'a [u8], u64)>,
    pub(crate) huffman: &'a HuffmanTree,
    pub(crate) alpha_mode: bool,
    pub(crate) quality: Option<ItemQuality>,
    pub(crate) flags: u32,
    pub(crate) is_runeword: bool,
    pub(crate) is_v105_shadow: bool,
    pub(crate) is_personalized: bool,
    pub(crate) is_compact: bool,
    pub(crate) is_socketed: bool,
}

pub(crate) type LegacyItemStatsReadResult = (
    Vec<ItemProperty>,
    bool,
    bool,
    Option<u8>,
    Option<Vec<bool>>,
    Option<u64>,
    Vec<crate::domain::item::Item>,
);

pub(crate) fn normalize_legacy_item_properties(
    props: &mut [ItemProperty],
    code: &str,
    version: u8,
    save_is_alpha: bool,
) {
    let alpha_admitted = save_is_alpha
        && crate::data::generated::layout_policy::ALPHA_ITEM_HEADER_ADMISSION_POLICY
            .stats_versions
            .contains(&version);
    crate::domain::stats::property_normalize::normalize_legacy_properties_using(
        props,
        code,
        alpha_admitted,
        |raw_id| {
            crate::domain::forensic::registry::map_alpha_id_if_admitted(raw_id, alpha_admitted)
        },
    );
}

pub(crate) fn read_item_stats_normalized<R: BitRead>(
    cursor: &mut BitCursor<R>,
    request: LegacyItemStatsReadContext<'_>,
) -> ParsingResult<LegacyItemStatsReadResult> {
    let LegacyItemStatsReadContext {
        code,
        version,
        ctx,
        huffman,
        alpha_mode,
        quality,
        flags,
        is_runeword,
        is_v105_shadow,
        is_personalized,
        is_compact,
        is_socketed,
    } = request;
    let (mut props, complete, term, v5_extra, unused_bits, shadow_bits, nested_items) =
        read_item_stats(
            cursor,
            code,
            version,
            ctx,
            huffman,
            alpha_mode,
            quality,
            flags,
            is_runeword,
            is_v105_shadow,
            is_personalized,
            is_compact,
            is_socketed,
        )?;

    let quality_val = quality.unwrap_or(ItemQuality::Normal);
    let axiom = StatsAxiom::new(version, quality_val, alpha_mode)
        .with_compact(is_compact)
        .with_code(code);

    normalize_legacy_item_properties(&mut props, code, axiom.version, axiom.save_is_alpha);

    Ok((
        props,
        complete,
        term,
        v5_extra,
        unused_bits,
        shadow_bits,
        nested_items,
    ))
}

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
) -> ParsingResult<(
    Vec<ItemProperty>,
    bool,
    bool,
    Option<u8>,
    Option<Vec<bool>>,
    Option<u64>,
    Vec<crate::domain::item::Item>,
)> {
    read_item_stats_with_unidentified_policy(
        cursor,
        code,
        version,
        ctx,
        huffman,
        alpha_mode,
        quality,
        flags,
        is_runeword,
        is_v105_shadow,
        is_personalized,
        is_compact,
        is_socketed,
        false,
    )
}

pub fn read_item_stats_d2r_v3<R: BitRead>(
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
) -> ParsingResult<(
    Vec<ItemProperty>,
    bool,
    bool,
    Option<u8>,
    Option<Vec<bool>>,
    Option<u64>,
    Vec<crate::domain::item::Item>,
)> {
    read_item_stats_with_unidentified_policy(
        cursor,
        code,
        version,
        ctx,
        huffman,
        alpha_mode,
        quality,
        flags,
        is_runeword,
        is_v105_shadow,
        is_personalized,
        is_compact,
        is_socketed,
        true,
    )
}

fn read_item_stats_with_unidentified_policy<R: BitRead>(
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
    consume_unidentified: bool,
) -> ParsingResult<(
    Vec<ItemProperty>,
    bool,
    bool,
    Option<u8>,
    Option<Vec<bool>>,
    Option<u64>,
    Vec<crate::domain::item::Item>,
)> {
    let mut alpha_v5_runeword_extra = None;
    let mut alpha_shadow_skip_bits = None;
    let mut stats_context = cursor.scoped_context(&format!("{:?}", ItemSegmentType::Stats));
    let cursor = &mut *stats_context;
    let trimmed_code = code.trim();

    let quality_val = quality.unwrap_or(ItemQuality::Normal);
    let axiom = StatsAxiom::new(version, quality_val, alpha_mode)
        .with_personalization(is_personalized)
        .with_compact(is_compact)
        .with_socketed(is_socketed)
        .with_code(trimmed_code);
    let is_alpha = axiom.is_alpha();

    let is_v105_shadow_final = alpha_mode && is_v105_shadow;
    let is_authority_host =
        alpha_mode && ALPHA_LEGACY_AUTHORITY_CODES.contains(&trimmed_code) && !is_compact;
    let is_shadow_container = alpha_mode && matches!(trimmed_code, "xrs" | "c8xr") && !is_compact;

    let is_scroll = trimmed_code == "tsc" || trimmed_code == "isc";
    let is_potion = trimmed_code.starts_with('h')
        || trimmed_code.starts_with('m')
        || trimmed_code.contains("hp")
        || trimmed_code.contains("mp")
        || (version == 5 && (trimmed_code.starts_with('7') || trimmed_code == "wwsw"))
        || (trimmed_code.starts_with('r') && trimmed_code.len() <= 3);

    let is_identified = (flags & (1 << 4)) != 0;
    if !is_identified
        && !consume_unidentified
        && !is_runeword
        && !is_shadow_container
        && !is_authority_host
    {
        return Ok((Vec::new(), true, false, None, None, None, Vec::new()));
    }

    if is_alpha && trimmed_code.is_empty() {
        return Ok((Vec::new(), true, false, None, None, None, Vec::new()));
    }

    if is_alpha && version == 5 && !is_v105_shadow_final && (is_potion || is_scroll) {
        if trimmed_code == "7mgw" {
            let mut payload = Vec::new();
            for _ in 0..28 {
                payload.push(cursor.read_bit()?);
            }
            return Ok((
                Vec::new(),
                true,
                false,
                None,
                Some(payload),
                None,
                Vec::new(),
            ));
        }
        return Ok((Vec::new(), true, false, None, None, None, Vec::new()));
    }

    let allow_compact_recovery = is_runeword || is_shadow_container || is_authority_host;

    let section_recovery = if let Some((bytes, start)) = ctx {
        PropertyReaderContext {
            bytes,
            item_start_bit: start,
        }
    } else {
        PropertyReaderContext {
            bytes: &[],
            item_start_bit: 0,
        }
    };
    let item_start_rel = section_recovery
        .item_start_bit
        .saturating_sub(cursor.base_pos);
    if is_v105_shadow_final && trimmed_code != "wa2" {
        let skip_bits_count = if is_shadow_container { 30 } else { 47 };
        let skip_bits =
            cursor.with_context("AlphaShadowSkip", |c| c.read_bits::<u64>(skip_bits_count))?;
        alpha_shadow_skip_bits = Some(skip_bits);
    }

    let mut child_offsets = Vec::new();
    let mut child_offset_codes: std::collections::HashMap<u64, String> =
        std::collections::HashMap::new();
    let mut authority_offsets = Vec::new();
    if let Some((bytes, _)) = ctx {
        let markers = crate::domain::forensic::marker_scan::scan_item_markers(
            bytes,
            huffman,
            axiom.is_alpha(),
            section_recovery.item_start_bit,
            None,
            false,
        );
        for m in markers.iter() {
            let t = m.code.trim();
            if ALPHA_LEGACY_AUTHORITY_CODES.contains(&t) {
                authority_offsets.push(m.offset);
            }
            if (t.starts_with('r') && t.len() <= 3)
                || (t.starts_with('g') && t.len() == 3)
                || t == "jew"
            {
                child_offsets.push(m.offset);
                child_offset_codes.insert(m.offset, m.code.trim().to_string());
            }
        }
    }

    crate::item_trace!(
        "[socket-recovery] code={} alpha={} runeword={} authority_host={} socketed={} compact={} item_start_rel={} child_offsets={} authority_offsets={}",
        trimmed_code,
        axiom.is_alpha(),
        is_runeword,
        is_authority_host,
        is_socketed,
        is_compact,
        item_start_rel,
        child_offsets.len(),
        authority_offsets.len()
    );
    if crate::item::item_trace_enabled() {
        let child_preview: Vec<(u64, String)> = child_offsets
            .iter()
            .take(8)
            .map(|off| {
                (
                    *off,
                    child_offset_codes.get(off).cloned().unwrap_or_default(),
                )
            })
            .collect();
        eprintln!("[socket-recovery] child_preview={:?}", child_preview);
    }

    if is_alpha && is_compact && !is_runeword && !is_authority_host {
        if trimmed_code == "7mgw" {
            let mut payload = Vec::new();
            for _ in 0..28 {
                payload.push(cursor.read_bit()?);
            }
            return Ok((
                Vec::new(),
                true,
                false,
                None,
                Some(payload),
                None,
                Vec::new(),
            ));
        }

        let is_pure_fragment = matches!(trimmed_code, "xrs" | "c8xr" | "");
        if allow_compact_recovery && is_pure_fragment {
            let mut nested_items = Vec::new();
            for off in child_offsets
                .iter()
                .copied()
                .filter(|off| *off > item_start_rel)
                .take(3)
            {
                let raw_code = child_offset_codes
                    .get(&off)
                    .map(|s| s.as_str())
                    .unwrap_or("jew");
                let trimmed_code = raw_code.trim();
                let normalized =
                    legacy_alpha_child_code_alias(trimmed_code).unwrap_or(trimmed_code);

                let mut child = crate::domain::item::Item::default();
                child.code = normalized.to_string();
                child.body.code = normalized.to_string();
                child.mode = 6;
                child.header.mode = 6;
                nested_items.push(child);
            }

            if nested_items.len() < 3 {
                let scan_limit = if trimmed_code.contains(' ') {
                    1024
                } else {
                    512
                };
                if let Some((scanned_children, _)) =
                    crate::domain::item::serialization::scan_socket_children(
                        section_recovery.bytes,
                        item_start_rel,
                        huffman,
                        0,
                        axiom.is_alpha(),
                        scan_limit,
                    )
                {
                    if scanned_children.len() > nested_items.len() {
                        nested_items = scanned_children;
                    }
                }
            }

            if !nested_items.is_empty() {
                // Consume the bits for the shadow items until the next REAL marker
                let next_real_off = authority_offsets.iter().copied().find(|&off| {
                    off > item_start_rel && {
                        let raw_code = child_offset_codes
                            .get(&off)
                            .map(|s| s.as_str())
                            .unwrap_or("");
                        let trimmed = raw_code.trim();
                        !matches!(trimmed, "xrs" | "c8xr" | "rhd" | "" | "ww" | "gcw")
                    }
                });

                let total_consumed = if let Some(real_off) = next_real_off {
                    real_off - item_start_rel
                } else {
                    // Consume until the end of the items section budget
                    child_offsets
                        .iter()
                        .copied()
                        .filter(|off| *off > item_start_rel)
                        .last()
                        .map(|off| (off + 80) - item_start_rel)
                        .unwrap_or(0)
                };

                let current_pos = cursor.pos() - section_recovery.item_start_bit;
                if total_consumed > current_pos {
                    let to_skip = total_consumed - current_pos;
                    let _ = cursor.skip_and_record(to_skip as u32)?;
                }

                return Ok((Vec::new(), true, false, None, None, None, nested_items));
            }
        }

        return Ok((Vec::new(), true, false, None, None, None, Vec::new()));
    }

    // section_base_abs: absolute bit offset of the section start (= cursor.base_pos).
    // This is used as base_bit_offset in parse_item_at_with_limit so that
    // absolute_bit = section_base_abs + local_pos is computed correctly.
    let section_base_abs = cursor.base_pos;

    if crate::item::item_trace_enabled() {
        eprintln!(
            "[stats-start] code={} pos={} base_pos={}",
            trimmed_code,
            cursor.pos(),
            cursor.base_pos
        );
    }
    let (props, complete, term, nested_items) = read_property_list_with_pair_policy(
        cursor,
        trimmed_code,
        version,
        section_recovery.clone(),
        huffman,
        is_runeword,
        is_v105_shadow_final || is_shadow_container || is_runeword,
        &axiom,
        consume_unidentified && !alpha_mode,
        Some(&child_offsets),
        Some(&child_offset_codes),
        |bytes, pos, huff, idx, alpha| {
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

            let (item, mut consumed) =
                crate::domain::item::serialization::parse_item_at_with_limit(
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
        },
    )?;
    let mut nested_items = nested_items;
    if axiom.is_alpha() && is_runeword && nested_items.len() < 3 {
        let scan_limit = if trimmed_code.contains(' ') || is_authority_host || is_runeword {
            1024
        } else {
            512
        };
        if let Some((scanned_children, _)) =
            crate::domain::item::serialization::scan_socket_children(
                section_recovery.bytes,
                item_start_rel,
                huffman,
                0,
                axiom.is_alpha(),
                scan_limit,
            )
        {
            if scanned_children.len() > nested_items.len() {
                nested_items = scanned_children;
            }
        }
    }
    if alpha_mode
        && (version == 5 || version == 1)
        && (axiom.is_fragment(flags) || is_v105_shadow_final)
        && !is_shadow_container
    {
        let w_axiom = crate::domain::forensic::v105::axioms::V105PropertyWidthAxiom::default();
        let needed = w_axiom.v5_runeword_extra_bits() as u32;
        if !is_authority_host || cursor.remaining() >= needed as u64 {
            let mut extended_context =
                cursor.scoped_context(&format!("{:?}", ItemSegmentType::ExtendedStats));
            let mut extra_context = extended_context.scoped_context("AlphaV5RunewordExtra");
            let extra = extra_context.read_bits::<u8>(needed)?;
            alpha_v5_runeword_extra = Some(extra);
        }
    }

    if crate::item::item_trace_enabled() && trimmed_code == "xrs" {
        let events = take_socket_recovery_trace_events();
        for ev in &events {
            eprintln!(
                "[xrs-socket-trace] stage={} rel_pos={} next_marker={:?} observed={:?} note={}",
                ev.stage, ev.current_rel_pos, ev.next_marker, ev.observed_marker, ev.note
            );
        }
    }
    Ok((
        props,
        complete,
        term,
        alpha_v5_runeword_extra,
        None,
        alpha_shadow_skip_bits,
        nested_items,
    ))
}

