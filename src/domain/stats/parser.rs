use crate::data::bit_cursor::BitCursor;
use crate::domain::header::entity::ItemSegmentType;
use crate::domain::item::{HuffmanTree, ItemBitRange, ItemQuality};
use crate::domain::stats::{ItemProperty, StatsAxiom};
use crate::error::ParsingResult;
use bitstream_io::BitRead;
use std::cell::{Cell, RefCell};

#[derive(Debug, Clone)]
pub struct PropertyReaderContext<'a> {
    pub bytes: &'a [u8],
    pub item_start_bit: u64,
}

pub const MAX_ALPHA_V105_ITEM_BITS: u64 = 1500;
pub(crate) const D2R_V3_PAIRED_STAT_IDS: &[(u32, &[u32])] = &[
    (17, &[18]),
    (48, &[49]),
    (50, &[51]),
    (52, &[53]),
    (54, &[55, 56]),
    (57, &[58, 59]),
];

#[derive(Clone, Debug)]
pub struct SocketRecoveryTraceEvent {
    pub stage: &'static str,
    pub current_rel_pos: u64,
    pub next_marker: Option<u64>,
    pub observed_marker: Option<u64>,
    pub note: &'static str,
}

thread_local! {
    static SOCKET_TRACE_ENABLED: Cell<bool> = const { Cell::new(false) };
    static SOCKET_TRACE_EVENTS: RefCell<Vec<SocketRecoveryTraceEvent>> = const { RefCell::new(Vec::new()) };
}

pub fn set_socket_recovery_trace_enabled(enabled: bool) {
    SOCKET_TRACE_ENABLED.with(|f| f.set(enabled));
}

pub fn clear_socket_recovery_trace_events() {
    SOCKET_TRACE_EVENTS.with(|events| events.borrow_mut().clear());
}

pub fn take_socket_recovery_trace_events() -> Vec<SocketRecoveryTraceEvent> {
    SOCKET_TRACE_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
}

fn push_socket_trace_event(
    stage: &'static str,
    current_rel_pos: u64,
    next_marker: Option<u64>,
    observed_marker: Option<u64>,
    note: &'static str,
) {
    SOCKET_TRACE_ENABLED.with(|enabled| {
        if !enabled.get() {
            return;
        }
        SOCKET_TRACE_EVENTS.with(|events| {
            events.borrow_mut().push(SocketRecoveryTraceEvent {
                stage,
                current_rel_pos,
                next_marker,
                observed_marker,
                note,
            });
        });
    });
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
    cursor.begin_segment(ItemSegmentType::Stats);
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
        alpha_mode && matches!(trimmed_code, "xrs" | "c8xr" | "rhd" | "wa2") && !is_compact;
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
        let markers = crate::domain::item::scanner::scan_item_markers(
            bytes,
            huffman,
            axiom.is_alpha(),
            section_recovery.item_start_bit,
            None,
            false,
        );
        for m in markers.iter() {
            let t = m.code.trim();
            if matches!(t, "xrs" | "c8xr" | "rhd" | "wa2") {
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
                let normalized = match raw_code.trim() {
                    "ww" => "r13",
                    "gcw" => "r15",
                    other => other,
                };

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
            cursor.begin_segment(ItemSegmentType::ExtendedStats);
            cursor.push_context("AlphaV5RunewordExtra");
            let extra = cursor.read_bits::<u8>(needed)?;
            alpha_v5_runeword_extra = Some(extra);
            cursor.pop_context();
            cursor.end_segment();
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
    cursor.end_segment();
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

// Legacy admission checks the caller depth: current depth 10 may enter depth 11.
const NESTED_PROPERTY_DEPTH_THRESHOLD: u32 = 10;

thread_local! {
    static NESTED_DEPTH: std::cell::Cell<u32> = std::cell::Cell::new(0);
}

fn nested_property_depth_exhausted(current_depth: u32) -> bool {
    current_depth > NESTED_PROPERTY_DEPTH_THRESHOLD
}

fn nested_property_depth_failure() -> crate::error::ParsingFailure {
    crate::error::ParsingFailure {
        error: crate::error::ParsingError::Io("Max nesting depth exceeded".to_string()),
        context_stack: vec![],
        bit_offset: 0,
        context_relative_offset: 0,
        hint: None,
    }
}

fn with_nested_property_depth<T, E>(
    overflow_error: impl FnOnce() -> E,
    operation: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    NESTED_DEPTH.with(|depth| {
        let previous = depth.get();
        if nested_property_depth_exhausted(previous) {
            return Err(overflow_error());
        }
        depth.set(previous + 1);
        let result = operation();
        depth.set(previous);
        result
    })
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
    child_marker_offsets: Option<&[u64]>,
    child_marker_codes: Option<&std::collections::HashMap<u64, String>>,
    recovery_fn: F,
) -> ParsingResult<(
    Vec<ItemProperty>,
    bool,
    bool,
    Vec<crate::domain::item::Item>,
)>
where
    F: FnMut(
        &[u8],
        u64,
        &HuffmanTree,
        usize,
        bool,
    ) -> ParsingResult<(crate::domain::item::Item, u64)>,
{
    read_property_list_with_pair_policy(
        recorder,
        code,
        version,
        _section_recovery,
        huffman,
        alpha_runeword,
        is_v105_shadow,
        axiom,
        false,
        child_marker_offsets,
        child_marker_codes,
        recovery_fn,
    )
}

fn read_property_list_with_pair_policy<R: BitRead, F>(
    recorder: &mut BitCursor<R>,
    code: &str,
    version: u8,
    _section_recovery: PropertyReaderContext,
    huffman: &HuffmanTree,
    alpha_runeword: bool,
    is_v105_shadow: bool,
    axiom: &StatsAxiom,
    consume_d2r_v3_paired_stats: bool,
    child_marker_offsets: Option<&[u64]>,
    child_marker_codes: Option<&std::collections::HashMap<u64, String>>,
    mut recovery_fn: F,
) -> ParsingResult<(
    Vec<ItemProperty>,
    bool,
    bool,
    Vec<crate::domain::item::Item>,
)>
where
    F: FnMut(
        &[u8],
        u64,
        &HuffmanTree,
        usize,
        bool,
    ) -> ParsingResult<(crate::domain::item::Item, u64)>,
{
    let mut props = Vec::new();
    let mut nested_items = Vec::new();
    let mut terminator_bit = false;
    let mut saw_terminator = false;

    let start_pos = recorder.pos();

    let old_limit = recorder.limit();
    let is_authority_host =
        axiom.is_alpha() && matches!(code.trim(), "xrs" | "c8xr" | "rhd" | "wa2");
    let is_actual_runeword =
        (alpha_runeword || code.trim() == "Þ." || is_authority_host) && axiom.is_socketed;
    if axiom.is_alpha() && is_actual_runeword && old_limit.is_none() {
        recorder.set_limit(u64::MAX);
    }

    let is_compact = axiom.is_compact || code.trim().is_empty();
    let section_base_abs = if is_compact {
        recorder
            .base_pos
            .saturating_sub(_section_recovery.item_start_bit)
    } else {
        recorder.base_pos
    };

    let depth = NESTED_DEPTH.with(|d| d.get());
    if nested_property_depth_exhausted(depth) {
        return Ok((props, saw_terminator, terminator_bit, nested_items));
    }

    loop {
        if is_compact && !is_actual_runeword {
            if props.len() >= 1 {
                break;
            }
        }
        if axiom.is_alpha()
            && !is_actual_runeword
            && (recorder.pos() - start_pos) > MAX_ALPHA_V105_ITEM_BITS
        {
            return Err(
                recorder.fail(crate::error::ParsingError::BitBudgetExceeded {
                    bit_offset: recorder.pos(),
                }),
            );
        }
        if axiom.is_alpha() && is_actual_runeword {
            let current_rel_pos = recorder.pos().saturating_sub(section_base_abs);
            let lookahead_window = 64u64;
            let next_child_rel_off = if let Some(offsets) = child_marker_offsets {
                offsets
                    .iter()
                    .filter(|&&off| off >= current_rel_pos)
                    .min()
                    .cloned()
            } else {
                None
            };
            push_socket_trace_event(
                "pre_loop",
                current_rel_pos,
                next_child_rel_off,
                None,
                "main_loop_check",
            );

            let mut snap_target: Option<(u64, &'static str)> = None;
            if let Some(rel_off) = next_child_rel_off {
                if rel_off > current_rel_pos && (rel_off - current_rel_pos) <= lookahead_window {
                    snap_target = Some((rel_off, "main_loop_break_on_child_marker"));
                }
            }

            if snap_target.is_none() && !is_authority_host {
                let mut probe = current_rel_pos.saturating_add(1);
                let probe_limit = current_rel_pos.saturating_add(lookahead_window);
                while probe <= probe_limit {
                    // Use alpha=true for detection here: we are already in the Alpha-only
                    // seam, and the boundary we want to snap to may only be visible in
                    // Alpha header form at a non-byte-aligned bit offset.
                    if let Some(header_info) = crate::item::peek_item_header_at(
                        _section_recovery.bytes,
                        probe,
                        huffman,
                        true,
                        0,
                    ) {
                        let (
                            mode,
                            loc,
                            _x,
                            code_peek,
                            flags,
                            version_peek,
                            _is_compact,
                            _header_bits,
                            _nudge,
                            _has_checksum,
                        ) = header_info;
                        let trimmed_peek = code_peek.trim();
                        let normalized_peek = match trimmed_peek {
                            "ww" => "r13",
                            "gcw" => "r15",
                            other => other,
                        };
                        if crate::item::is_plausible_item_header(
                            mode,
                            loc,
                            normalized_peek.as_bytes(),
                            flags,
                            version_peek,
                            true,
                        ) {
                            snap_target = Some((probe, "main_loop_break_on_plausible_header"));
                            break;
                        }
                    }
                    probe = probe.saturating_add(1);
                }
            }

            if let Some((target_local_pos, note)) = snap_target {
                push_socket_trace_event(
                    "post_loop",
                    current_rel_pos,
                    next_child_rel_off,
                    Some(target_local_pos),
                    note,
                );
                saw_terminator = true;
                terminator_bit = false;
                break;
            }
        }

        if let Some(limit) = old_limit {
            if recorder.pos()
                + u64::from(crate::data::generated::layout_policy::LEGACY_PROPERTY_STREAM_POLICY.stat_id_bits)
                >= limit
            {
                if axiom.is_alpha() && !saw_terminator {
                    saw_terminator = true;
                    terminator_bit = false;
                }
                break;
            }
        }

        let result = parse_single_property_internal(
            recorder,
            version,
            huffman,
            alpha_runeword,
            is_compact,
            is_v105_shadow,
            consume_d2r_v3_paired_stats,
            axiom,
            _section_recovery.clone(),
            &mut recovery_fn,
        );

        match result {
            Ok(Some((parsed_properties, is_term, term_bit, items))) => {
                props.extend(parsed_properties);
                nested_items.extend(items);
                if axiom.is_alpha() && is_actual_runeword && nested_items.len() >= 3 {
                    saw_terminator = true;
                    terminator_bit = false;
                    break;
                }
                if is_term {
                    saw_terminator = true;
                    terminator_bit = term_bit;
                    break;
                }
            }
            Ok(None) => break,
            Err(e) => {
                let is_eof = match &e.error {
                    crate::error::ParsingError::Io(msg) => {
                        msg.contains("failed to fill whole buffer")
                            || msg.contains("unexpected end of file")
                    }
                    _ => false,
                };
                if is_eof {
                    return Ok((props, true, false, nested_items));
                }
                return Err(e);
            }
        }
    }

    let is_authority_host =
        axiom.is_alpha() && matches!(code.trim(), "xrs" | "c8xr" | "rhd" | "wa2");
    let needs_socket_recovery = axiom.is_alpha()
        && (alpha_runeword || code.trim() == "ucb8" || code.trim() == "bwcw" || is_authority_host)
        && axiom.is_socketed
        && child_marker_offsets
            .map(|offsets| nested_items.len() < offsets.len())
            .unwrap_or(false);
    if needs_socket_recovery {
        push_socket_trace_event(
            "fallback_entry",
            recorder.pos().saturating_sub(section_base_abs),
            child_marker_offsets.and_then(|offsets| offsets.iter().min().copied()),
            None,
            "needs_socket_recovery_true",
        );
        let mut child_idx = nested_items.len();
        let mut last_rel_pos: Option<u64> = None;
        loop {
            // current_rel_pos: section-local offset. recorder.base_pos = section absolute start bit.
            let current_rel_pos = recorder.pos().saturating_sub(section_base_abs);
            if let Some(last) = last_rel_pos {
                if current_rel_pos <= last {
                    break;
                }
            }
            last_rel_pos = Some(current_rel_pos);

            // next_marker_rel_off: section-local offset of the next child marker
            let next_marker_local_off = if let Some(offsets) = child_marker_offsets {
                if let Some(&next_off) = offsets.iter().filter(|&&off| off >= current_rel_pos).min()
                {
                    let abs_target = section_base_abs + next_off;
                    if let Some(lim) = recorder.limit() {
                        if abs_target > lim {
                            break;
                        }
                    }
                    let diff = next_off - current_rel_pos;
                    if diff > 0 {
                        recorder.skip_and_record(diff as u32)?;
                    }
                    next_off
                } else {
                    break;
                }
            } else {
                break;
            };
            push_socket_trace_event(
                "per_child",
                current_rel_pos,
                Some(next_marker_local_off),
                Some(next_marker_local_off),
                "attempt_recovery_at_marker",
            );

            // target_local_pos: section-local offset for peek_item_header_at (bytes = section_bytes)
            // and for parse_item_at_with_limit(bytes, bit=local, base=section_abs, ...)
            let target_local_pos = next_marker_local_off;

            // These offsets were already validated by scan_item_markers as rune candidates.
            // Attempt to peek to get the code for normalization, but fall back to direct parse
            // if peek fails. Use retail(false) peek which is empirically reliable for rune detection.
            let scanner_code = child_marker_codes
                .and_then(|codes| codes.get(&target_local_pos))
                .map(|code| match code.trim() {
                    "ww" => "r13",
                    "gcw" => "r15",
                    other => other,
                });
            let (normalized_code, force_from_scan) = if let Some(header_info) =
                crate::item::peek_item_header_at(
                    _section_recovery.bytes,
                    target_local_pos,
                    huffman,
                    false,
                    0,
                ) {
                let (
                    mode,
                    loc,
                    _x,
                    code_peek,
                    flags,
                    version_peek,
                    _is_compact,
                    _header_bits,
                    _nudge,
                    _has_checksum,
                ) = header_info;
                let trimmed_peek = code_peek.trim();
                let normalized_peek = match trimmed_peek {
                    "ww" => "r13",
                    "gcw" => "r15",
                    other => other,
                };
                let force_alias = trimmed_peek == "ww" || trimmed_peek == "gcw";
                let is_plausible = force_alias
                    || crate::item::is_plausible_item_header(
                        mode,
                        loc,
                        normalized_peek.as_bytes(),
                        flags,
                        version_peek,
                        false,
                    );
                if is_plausible {
                    (normalized_peek.to_string(), false)
                } else if let Some(scanner_code) = scanner_code {
                    (scanner_code.to_string(), true)
                } else {
                    (normalized_peek.to_string(), true)
                }
            } else {
                // peek failed — scanner-verified position, force the attempt
                (scanner_code.unwrap_or("").to_string(), true)
            };

            let is_socketable = (normalized_code.starts_with('r') && normalized_code.len() <= 3)
                || (normalized_code.starts_with('g') && normalized_code.len() == 3)
                || normalized_code == "jew";

            // If peek gave a non-socketable plausible item, stop (e.g. reached next equipment slot).
            // If force_from_scan=true, we trust the scanner and attempt recovery anyway.
            if !is_socketable && !force_from_scan {
                break;
            }

            let result = with_nested_property_depth(nested_property_depth_failure, || {
                recorder.push_context("nested");
                let prev_nested = crate::domain::header::entity::IN_NESTED_RECOVERY.with(|v| {
                    let p = v.get();
                    v.set(true);
                    p
                });
                // Use axiom.is_alpha(): ensure nested items use the correct mode
                let res = recovery_fn(
                    _section_recovery.bytes,
                    target_local_pos,
                    huffman,
                    child_idx,
                    axiom.is_alpha(),
                );
                crate::domain::header::entity::IN_NESTED_RECOVERY.with(|v| v.set(prev_nested));
                recorder.pop_context();
                res
            });

            // If recovery_fn fails but the offset was scanner-verified (force_from_scan),
            // create a minimal placeholder Item using the known scanner code.
            let (child_item, end_pos_val) = match result {
                Ok((item, ep)) => (Some(item), ep),
                Err(_) if force_from_scan && !normalized_code.is_empty() => {
                    // Recovery parse failed, but scanner confirmed this as a rune/gem.
                    // Create a minimal placeholder using the scanner-verified code.
                    let mut placeholder = crate::domain::item::Item::default();
                    let sc = match normalized_code.as_str() {
                        "ww" => "r13".to_string(),
                        "gcw" => "r15".to_string(),
                        c => c.to_string(),
                    };
                    placeholder.code = sc.clone();
                    placeholder.body.code = sc;
                    // Estimate consumed bits: use gap to next marker, or fallback to 72
                    let est_end = if let Some(offsets) = child_marker_offsets {
                        if let Some(&next_local) =
                            offsets.iter().filter(|&&off| off > target_local_pos).min()
                        {
                            next_local - target_local_pos
                        } else {
                            72
                        }
                    } else {
                        72
                    };
                    (Some(placeholder), est_end)
                }
                Err(_) => (None, 0),
            };

            if let Some(mut child) = child_item {
                let end_pos = end_pos_val;
                // Normalize code: use peek result if available, otherwise use child's parsed code.
                let final_code = if !normalized_code.is_empty()
                    && (normalized_code.starts_with('r')
                        || normalized_code.starts_with('g')
                        || normalized_code == "jew")
                {
                    normalized_code.clone()
                } else {
                    let c = child.code.trim().to_string();
                    match c.as_str() {
                        "ww" => "r13".to_string(),
                        "gcw" => "r15".to_string(),
                        _ => c,
                    }
                };
                let final_is_socketable = (final_code.starts_with('r') && final_code.len() <= 3)
                    || (final_code.starts_with('g') && final_code.len() == 3)
                    || final_code == "jew";
                // force_from_scan: scanner already verified this offset as a rune/gem.
                // Skip the socketable check in that case to avoid false negatives from retail parse.
                if !final_is_socketable && !force_from_scan {
                    // Parsed item is not a rune/gem — stop recovery
                    break;
                }
                child.mode = 6;
                child.header.mode = 6;
                child.code = final_code.clone();
                child.body.code = final_code;
                nested_items.push(child);
                child_idx += 1;
                push_socket_trace_event(
                    "per_child",
                    recorder.pos().saturating_sub(section_base_abs),
                    child_marker_offsets.and_then(|offsets| {
                        offsets
                            .iter()
                            .filter(|&&off| off > target_local_pos)
                            .min()
                            .copied()
                    }),
                    Some(target_local_pos),
                    "recovery_success",
                );

                // end_pos is bits consumed from target_local_pos;
                // next marker offset is also local, so comparison is consistent.
                let mut local_end = target_local_pos + end_pos;
                if let Some(offsets) = child_marker_offsets {
                    if let Some(&next_local) =
                        offsets.iter().filter(|&&off| off > target_local_pos).min()
                    {
                        local_end = local_end.min(next_local);
                    }
                }
                if local_end > target_local_pos {
                    let consumed = (local_end - target_local_pos) as u32;
                    recorder.skip_and_record(consumed)?;
                }

                let is_tome = code.trim() == "ucb8" || code.trim() == "bwcw";
                let max_children = if is_tome { 20 } else { 3 };
                if child_idx >= max_children {
                    break;
                }
                continue;
            }
            break;
        }
    }

    if axiom.is_alpha() && is_actual_runeword {
        let recovery_limit = recorder.pos().saturating_add(512);
        recorder.set_limit(old_limit.map_or(recovery_limit, |limit| limit.min(recovery_limit)));
    } else if let Some(l) = old_limit {
        recorder.set_limit(l);
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
    F: FnMut(
        &[u8],
        u64,
        &HuffmanTree,
        usize,
        bool,
    ) -> ParsingResult<(crate::domain::item::Item, u64)>,
{
    let parsed = parse_single_property_internal(
        recorder,
        version,
        huffman,
        alpha_runeword,
        false,
        false,
        false,
        axiom,
        reader_ctx,
        &mut recovery_fn,
    )?;
    Ok(parsed.map(|(mut properties, is_term, term_bit, items)| {
        let property = properties.remove(0);
        (property, is_term, term_bit, items)
    }))
}

fn parse_single_property_internal<R: BitRead, F>(
    recorder: &mut BitCursor<R>,
    _version: u8,
    huffman: &HuffmanTree,
    alpha_runeword: bool,
    is_compact: bool,
    is_v105_shadow: bool,
    consume_d2r_v3_paired_stats: bool,
    axiom: &StatsAxiom,
    reader_ctx: PropertyReaderContext,
    recovery_fn: &mut F,
) -> ParsingResult<
    Option<(
        Vec<ItemProperty>,
        bool,
        bool,
        Vec<crate::domain::item::Item>,
    )>,
>
where
    F: FnMut(
        &[u8],
        u64,
        &HuffmanTree,
        usize,
        bool,
    ) -> ParsingResult<(crate::domain::item::Item, u64)>,
{
    let entry_start = recorder.pos();
    let id_bits = u32::from(
        crate::data::generated::layout_policy::LEGACY_PROPERTY_STREAM_POLICY.stat_id_bits,
    );
    let stat_id = recorder.read_bits::<u32>(id_bits)?;
    if entry_start >= 7800 && entry_start <= 8305 && crate::item::item_trace_enabled() {
        eprintln!(
            "[prop-read-any] pos={} stat_id={} code={}",
            entry_start, stat_id, axiom.code
        );
    }

    let rhythm = axiom.property_rhythm(alpha_runeword, is_v105_shadow, is_compact, stat_id);

    let id_bits = rhythm.id_bits;
    let terminator = (1u32 << id_bits) - 1;

    if stat_id == terminator {
        let is_authority_host =
            axiom.save_is_alpha && matches!(axiom.code.trim(), "xrs" | "c8xr" | "rhd" | "wa2");
        let mut term_bit = false;
        if rhythm.has_terminal_bit {
            if !is_authority_host || recorder.remaining() >= 1 {
                term_bit = recorder.read_bit()?;
            }
            if rhythm.has_extra_terminal_bit {
                if !is_authority_host || recorder.remaining() >= 1 {
                    let _extra = recorder.read_bit()?;
                }
            }
            while recorder.pos() % 8 != 0 {
                if is_authority_host && recorder.remaining() == 0 {
                    break;
                }
                let _p = recorder.read_bit()?;
            }
        }

        if let Some(tvs_bits) = axiom.tvs_padding_bits(alpha_runeword) {
            if !is_authority_host || recorder.remaining() >= u64::from(tvs_bits) {
                let _tvs = recorder.read_bits::<u32>(tvs_bits)?;
            }
        }
        return Ok(Some((
            vec![ItemProperty {
                stat_id,
                raw_value: 0,
                param: 0,
                name: "terminator".to_string(),
                value: 0,
                range: ItemBitRange {
                    start: entry_start,
                    end: recorder.pos(),
                },
                is_opaque: false,
                opaque_bits: None,
                maturity: crate::domain::stats::StatMaturityTier::Verified,
            }],
            true,
            term_bit,
            Vec::new(),
        )));
    }

    let mut param = 0;
    let mut nested_items = Vec::new();
    let mapped_id = axiom.map_alpha_id(stat_id);
    let stat_cost = crate::data::stat_costs::STAT_COSTS
        .iter()
        .find(|s| s.id == mapped_id);
    let suppress_authority_params = axiom.omits_property_parameter(&axiom.code, _version);
    if let Some(stat) = stat_cost {
        if stat.save_param_bits > 0 && !suppress_authority_params {
            param = recorder.read_bits::<u32>(stat.save_param_bits as u32)?;
        }
    }

    let default_width = axiom.default_property_value_width(
        _version,
        stat_cost.map(|stat| u32::from(stat.save_bits)),
        rhythm,
    );

    let mut effective_width = axiom.stat_bit_width(stat_id, default_width);
    if axiom.code.trim() == "wa2" && (stat_id == 320 || axiom.map_alpha_id(stat_id) == 320) {
        effective_width = 0;
    }

    let is_stat_317 = stat_id == 317 || axiom.map_alpha_id(stat_id) == 317;
    let is_stat_320 = stat_id == 320 || axiom.map_alpha_id(stat_id) == 320;
    let is_stat_387 = stat_id == 387 || axiom.map_alpha_id(stat_id) == 387;
    let is_already_nested = recorder.context_stack().iter().any(|s| s == "nested");
    let mut handled = false;
    let raw_value;

    if axiom.is_alpha()
        && alpha_runeword
        && axiom.is_socketed
        && (is_stat_317 || is_stat_320 || is_stat_387)
        && !is_already_nested
    {
        let entry_pos = recorder.pos();
        let absolute_entry_pos = reader_ctx.item_start_bit + entry_pos;
        recorder.push_context("nested");

        let mut found_pos = absolute_entry_pos;
        if is_stat_320 {
            for offset in 0..64 {
                let probe_pos = absolute_entry_pos + offset;
                if let Some(header_info) = crate::item::peek_item_header_at(
                    reader_ctx.bytes,
                    probe_pos,
                    huffman,
                    axiom.save_is_alpha,
                    0,
                ) {
                    let (
                        mode,
                        loc,
                        _x,
                        code,
                        flags,
                        version,
                        _is_compact,
                        _header_bits,
                        _nudge,
                        _has_checksum,
                    ) = header_info;
                    if crate::item::is_plausible_item_header(
                        mode,
                        loc,
                        code.as_bytes(),
                        flags,
                        version,
                        axiom.save_is_alpha,
                    ) {
                        found_pos = probe_pos;
                        break;
                    }
                }
            }
        }

        let result = with_nested_property_depth(nested_property_depth_failure, || {
            recovery_fn(
                reader_ctx.bytes,
                found_pos,
                huffman,
                nested_items.len(),
                axiom.save_is_alpha,
            )
        });

        if let Ok((child, end_pos)) = result {
            nested_items.push(child);

            let absolute_end = found_pos + end_pos;
            if absolute_end > absolute_entry_pos {
                let consumed = (absolute_end - absolute_entry_pos) as u32;
                recorder.skip_and_record(consumed)?;
            }
            handled = true;
        }
        recorder.pop_context();
    }

    if !handled {
        if effective_width > 32 {
            recorder.skip_and_record(effective_width)?;
            raw_value = 0;
        } else {
            raw_value = recorder.read_bits::<u32>(effective_width)? as i32;
        }
    } else {
        raw_value = 0;
    }

    recorder.push_context(&format!("Stat({})", stat_id));
    let entry_end = recorder.pos();
    recorder.pop_context();
    let is_unknown_stat = stat_cost.is_none();
    let logical_value = if handled || effective_width > 32 {
        0
    } else {
        stat_cost
            .map(|stat| raw_value.wrapping_sub(stat.save_add))
            .unwrap_or(raw_value)
    };

    let property = ItemProperty {
        stat_id,
        raw_value: raw_value as i32,
        param,
        name: if is_unknown_stat {
            "opaque_property".to_string()
        } else {
            stat_cost.map(|s| s.name.to_string()).unwrap_or_default()
        },
        value: logical_value,
        range: ItemBitRange {
            start: entry_start,
            end: entry_end,
        },
        is_opaque: is_unknown_stat,
        opaque_bits: None,
        maturity: axiom.stat_maturity(stat_id, !is_unknown_stat),
    };
    let mut properties = vec![property];

    if consume_d2r_v3_paired_stats && !axiom.is_alpha() {
        let companion_stat_ids =
            D2R_V3_PAIRED_STAT_IDS
                .iter()
                .find_map(|(paired_stat_id, companion_stat_ids)| {
                    (*paired_stat_id == stat_id).then_some(*companion_stat_ids)
                });
        if let Some(companion_stat_ids) = companion_stat_ids {
            for companion_stat_id in companion_stat_ids.iter().copied() {
                if let Some(companion_stat) = crate::data::stat_costs::STAT_COSTS
                    .iter()
                    .find(|stat| stat.id == companion_stat_id)
                {
                    let companion_start = recorder.pos();
                    let companion_width = companion_stat.save_bits as u32;
                    let companion_raw_value = if companion_width > 32 {
                        recorder.skip_and_record(companion_width)?;
                        0
                    } else {
                        recorder.read_bits::<u32>(companion_width)? as i32
                    };
                    let companion_end = recorder.pos();
                    properties.push(ItemProperty {
                        stat_id: companion_stat_id,
                        raw_value: companion_raw_value,
                        param: 0,
                        name: companion_stat.name.to_string(),
                        value: companion_raw_value.wrapping_sub(companion_stat.save_add),
                        range: ItemBitRange {
                            start: companion_start,
                            end: companion_end,
                        },
                        is_opaque: false,
                        opaque_bits: None,
                        maturity: axiom.stat_maturity(companion_stat_id, true),
                    });
                }
            }
        }
    }

    Ok(Some((properties, false, false, nested_items)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitstream_io::{BitReader, LittleEndian};
    use std::io::Cursor;

    use d2r_macros::hypothesis_test;

    // Operational snapshots exercise the actual callers; they are not game-format axioms.
    struct DepthWitnessState {
        depth: u32,
        nested: bool,
    }

    impl DepthWitnessState {
        fn enter(depth: u32, nested: bool) -> Self {
            let previous = Self {
                depth: NESTED_DEPTH.with(|value| value.get()),
                nested: crate::domain::header::entity::IN_NESTED_RECOVERY.with(|value| value.get()),
            };
            NESTED_DEPTH.with(|value| value.set(depth));
            crate::domain::header::entity::IN_NESTED_RECOVERY.with(|value| value.set(nested));
            previous
        }
    }

    impl Drop for DepthWitnessState {
        fn drop(&mut self) {
            NESTED_DEPTH.with(|value| value.set(self.depth));
            crate::domain::header::entity::IN_NESTED_RECOVERY.with(|value| value.set(self.nested));
        }
    }

    fn depth_witness_prefix(offset: usize) -> Vec<bool> {
        (0..offset).map(|index| index % 2 == 0).collect()
    }

    fn depth_witness_packed(bits: &[bool]) -> Vec<u8> {
        bits.chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .enumerate()
                    .fold(0, |byte, (index, &bit)| byte | (u8::from(bit) << index))
            })
            .collect()
    }

    fn depth_witness_failure() -> crate::error::ParsingFailure {
        crate::error::ParsingFailure {
            error: crate::error::ParsingError::Io("depth callback sentinel".into()),
            context_stack: vec!["callback sentinel".into()],
            bit_offset: 77,
            context_relative_offset: 3,
            hint: Some("depth witness".into()),
        }
    }

    fn depth_witness_state(depth: u32, nested: bool) {
        assert_eq!(NESTED_DEPTH.with(|value| value.get()), depth);
        assert_eq!(
            crate::domain::header::entity::IN_NESTED_RECOVERY.with(|value| value.get()),
            nested
        );
    }

    #[hypothesis_test]
    fn refactor_depth_legacy_list_entry_preserves_threshold_cursor_and_flags() {
        let tree = HuffmanTree::new();
        let mut cases = 0;
        for depth in [0, 9, 10, 11, u32::MAX] {
            for nested in [false, true] {
                for alpha in [false, true] {
                    for offset in [0, 3] {
                        let _state = DepthWitnessState::enter(depth, nested);
                        let mut bits = depth_witness_prefix(offset);
                        bits.extend([true; 9]);
                        if !alpha {
                            bits.push(true);
                            while bits.len() % 8 != 0 {
                                bits.push(false);
                            }
                        }
                        let terminal_end = bits.len();
                        bits.extend([false, true, true, false, true]);
                        let bytes = depth_witness_packed(&bits);
                        let mut recorder =
                            BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                        for bit in depth_witness_prefix(offset) {
                            assert_eq!(recorder.read_bit().unwrap(), bit);
                        }
                        recorder.set_limit(bits.len() as u64);
                        recorder.push_context("owner");
                        let mut a = StatsAxiom::new(5, ItemQuality::Normal, alpha);
                        a.code = "plain".into();
                        let (props, complete, terminal_bit, children) = read_property_list(
                            &mut recorder,
                            "plain",
                            5,
                            PropertyReaderContext {
                                bytes: &bytes,
                                item_start_bit: 0,
                            },
                            &tree,
                            false,
                            false,
                            &a,
                            None,
                            None,
                            |_, _, _, _, _| panic!("terminal-only entry must not recover a child"),
                        )
                        .unwrap();
                        let expected_end = if depth > 10 { offset } else { terminal_end };
                        if depth > 10 {
                            assert!(
                                props.is_empty()
                                    && !complete
                                    && !terminal_bit
                                    && children.is_empty()
                            );
                        } else {
                            assert!(complete && children.is_empty());
                            assert_eq!(terminal_bit, !alpha);
                            assert_eq!(props.len(), 1);
                            assert_eq!(props[0].stat_id, 511);
                            assert_eq!(
                                (props[0].range.start, props[0].range.end),
                                (offset as u64, terminal_end as u64)
                            );
                        }
                        depth_witness_state(depth, nested);
                        assert_eq!(recorder.context_stack(), vec!["owner"]);
                        assert_eq!(recorder.limit(), Some(bits.len() as u64));
                        assert_eq!(recorder.pos(), expected_end as u64);
                        assert_eq!(
                            recorder
                                .recorded_bits()
                                .iter()
                                .map(|bit| bit.bit)
                                .collect::<Vec<_>>(),
                            bits[..expected_end]
                        );
                        for bit in &bits[expected_end..] {
                            assert_eq!(recorder.read_bit().unwrap(), *bit);
                        }
                        assert_eq!(
                            recorder
                                .recorded_bits()
                                .iter()
                                .map(|bit| bit.bit)
                                .collect::<Vec<_>>(),
                            bits
                        );
                        cases += 1;
                    }
                }
            }
        }
        assert_eq!(cases, 40);
    }

    #[hypothesis_test]
    fn refactor_depth_legacy_inline_child_preserves_origin_fallback_and_restoration() {
        let registry = crate::domain::forensic::registry::get_registry();
        assert!(!registry.stats.contains_key("320"));
        for id in [317, 320, 387] {
            assert!(!registry.mappings.contains_key(&id.to_string()));
            assert!(
                crate::data::stat_costs::STAT_COSTS
                    .iter()
                    .find(|stat| stat.id == id)
                    .is_none_or(|stat| stat.save_param_bits == 0)
            );
            assert!(
                registry
                    .item_overrides
                    .as_ref()
                    .and_then(|items| items.get("plain"))
                    .and_then(|row| row.get(&id.to_string()))
                    .is_none()
            );
        }
        let tree = HuffmanTree::new();
        let mut cases = 0;
        for depth in [0, 9, 10, 11, u32::MAX] {
            for nested in [false, true] {
                for callback_ok in [false, true] {
                    for id in [317, 320, 387] {
                        for offset in [0, 3] {
                            for origin in [0, 40] {
                                let _state = DepthWitnessState::enter(depth, nested);
                                let mut bits = depth_witness_prefix(offset);
                                bits.extend((0..9).map(|index| id & (1 << index) != 0));
                                bits.extend([
                                    true, false, true, false, true, false, true, false, true,
                                ]);
                                bits.extend([false, true, true, false, true]);
                                let bytes = depth_witness_packed(&bits);
                                let mut recorder = BitCursor::new(BitReader::endian(
                                    Cursor::new(&bytes),
                                    LittleEndian,
                                ));
                                for bit in depth_witness_prefix(offset) {
                                    assert_eq!(recorder.read_bit().unwrap(), bit);
                                }
                                recorder.set_limit(bits.len() as u64);
                                recorder.push_context("owner");
                                let mut a = StatsAxiom::new(5, ItemQuality::Normal, true);
                                a.code = "plain".into();
                                a.is_socketed = true;
                                let mut calls = 0;
                                let (property, terminal, term_bit, children) =
                                    parse_single_property(
                                        &mut recorder,
                                        5,
                                        &tree,
                                        true,
                                        &a,
                                        PropertyReaderContext {
                                            bytes: &[],
                                            item_start_bit: origin,
                                        },
                                        |data, position, _, index, alpha| {
                                            calls += 1;
                                            depth_witness_state(depth + 1, nested);
                                            assert!(data.is_empty() && alpha);
                                            assert_eq!(position, origin + offset as u64 + 9);
                                            assert_eq!(index, 0);
                                            if !callback_ok {
                                                return Err(depth_witness_failure());
                                            }
                                            let mut child = crate::domain::item::Item::default();
                                            child.code = "r09".into();
                                            child.body.code = "r09".into();
                                            child.header.id = Some(77);
                                            Ok((child, 5))
                                        },
                                    )
                                    .unwrap()
                                    .unwrap();
                                let handled = depth <= 10 && callback_ok;
                                assert_eq!(calls, usize::from(depth <= 10));
                                let field_bits = if handled {
                                    5
                                } else if id == 320 {
                                    0
                                } else {
                                    9
                                };
                                let expected_end = offset + 9 + field_bits;
                                assert!(!terminal && !term_bit);
                                assert_eq!((property.stat_id, property.param), (id, 0));
                                assert_eq!(
                                    property.raw_value,
                                    if handled || id == 320 { 0 } else { 341 }
                                );
                                assert_eq!(
                                    (property.range.start, property.range.end),
                                    (offset as u64, expected_end as u64)
                                );
                                assert_eq!(children.len(), usize::from(handled));
                                if handled {
                                    assert_eq!(children[0].code, "r09");
                                    assert_eq!(children[0].header.id, Some(77));
                                }
                                depth_witness_state(depth, nested);
                                assert_eq!(recorder.context_stack(), vec!["owner"]);
                                assert_eq!(recorder.limit(), Some(bits.len() as u64));
                                assert_eq!(recorder.pos(), expected_end as u64);
                                assert_eq!(
                                    recorder
                                        .recorded_bits()
                                        .iter()
                                        .map(|bit| bit.bit)
                                        .collect::<Vec<_>>(),
                                    bits[..expected_end]
                                );
                                for bit in &bits[expected_end..] {
                                    assert_eq!(recorder.read_bit().unwrap(), *bit);
                                }
                                assert_eq!(
                                    recorder
                                        .recorded_bits()
                                        .iter()
                                        .map(|bit| bit.bit)
                                        .collect::<Vec<_>>(),
                                    bits
                                );
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 240);
    }

    #[hypothesis_test]
    fn refactor_depth_legacy_scanner_fallback_preserves_flag_placeholder_and_cursor() {
        let tree = HuffmanTree::new();
        let mut cases = 0;
        for depth in [0, 9, 10, 11, u32::MAX] {
            for nested in [false, true] {
                for callback_ok in [false, true] {
                    for offset in [0, 3] {
                        let _state = DepthWitnessState::enter(depth, nested);
                        let mut bits = depth_witness_prefix(offset);
                        bits.extend((0..96).map(|index| index % 3 == 1));
                        bits.extend([false, true, true, false, true]);
                        let bytes = depth_witness_packed(&bits);
                        let mut recorder =
                            BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                        for bit in depth_witness_prefix(offset) {
                            assert_eq!(recorder.read_bit().unwrap(), bit);
                        }
                        recorder.set_limit(bits.len() as u64);
                        recorder.push_context("owner");
                        let mut a = StatsAxiom::new(5, ItemQuality::Normal, true);
                        a.code = "plain".into();
                        a.is_socketed = true;
                        let target = offset as u64 + 8;
                        let offsets = [target];
                        let codes = std::collections::HashMap::from([(target, "r01".to_string())]);
                        let mut calls = 0;
                        let (props, complete, terminal_bit, children) = read_property_list(
                            &mut recorder,
                            "plain",
                            5,
                            PropertyReaderContext {
                                bytes: &[],
                                item_start_bit: 0,
                            },
                            &tree,
                            true,
                            false,
                            &a,
                            Some(&offsets),
                            Some(&codes),
                            |data, position, _, index, alpha| {
                                calls += 1;
                                depth_witness_state(depth + 1, true);
                                assert!(data.is_empty() && alpha);
                                assert_eq!((position, index), (target, 0));
                                if !callback_ok {
                                    return Err(depth_witness_failure());
                                }
                                let mut child = crate::domain::item::Item::default();
                                child.code = "r09".into();
                                child.body.code = "r09".into();
                                child.header.id = Some(77);
                                Ok((child, 5))
                            },
                        )
                        .unwrap();
                        assert!(props.is_empty() && !terminal_bit);
                        let expected_end = if depth > 10 {
                            offset
                        } else {
                            offset + 8 + if callback_ok { 5 } else { 72 }
                        };
                        assert_eq!(calls, usize::from(depth <= 10));
                        assert_eq!(complete, depth <= 10);
                        assert_eq!(children.len(), usize::from(depth <= 10));
                        if depth <= 10 {
                            assert_eq!(
                                (
                                    children[0].code.as_str(),
                                    children[0].body.code.as_str(),
                                    children[0].header.mode
                                ),
                                ("r01", "r01", 6)
                            );
                            assert_eq!(
                                children[0].header.id,
                                if callback_ok { Some(77) } else { None }
                            );
                        }
                        depth_witness_state(depth, nested);
                        assert_eq!(recorder.context_stack(), vec!["owner"]);
                        assert_eq!(recorder.limit(), Some(bits.len() as u64));
                        assert_eq!(recorder.pos(), expected_end as u64);
                        assert_eq!(
                            recorder
                                .recorded_bits()
                                .iter()
                                .map(|bit| bit.bit)
                                .collect::<Vec<_>>(),
                            bits[..expected_end]
                        );
                        for bit in &bits[expected_end..] {
                            assert_eq!(recorder.read_bit().unwrap(), *bit);
                        }
                        assert_eq!(
                            recorder
                                .recorded_bits()
                                .iter()
                                .map(|bit| bit.bit)
                                .collect::<Vec<_>>(),
                            bits
                        );
                        cases += 1;
                    }
                }
            }
        }
        assert_eq!(cases, 40);
    }

    #[hypothesis_test]
    fn refactor_depth_scope_contract_preserves_payload_and_overflow_metadata() {
        let mut cases = 0;
        for depth in [0, 9, 10, 11, u32::MAX] {
            for nested in [false, true] {
                for callback_ok in [false, true] {
                    let _state = DepthWitnessState::enter(depth, nested);
                    let sentinel = depth_witness_failure();
                    let mut calls = 0;
                    let result: ParsingResult<u32> =
                        with_nested_property_depth(nested_property_depth_failure, || {
                            calls += 1;
                            depth_witness_state(depth + 1, nested);
                            // A normal return must restore the caller even if child code changes depth.
                            NESTED_DEPTH.with(|value| value.set(123));
                            if callback_ok {
                                Ok(7)
                            } else {
                                Err(sentinel.clone())
                            }
                        });
                    assert_eq!(calls, usize::from(depth <= 10));
                    if depth > 10 {
                        let failure = result.unwrap_err();
                        assert_eq!(
                            failure.error,
                            crate::error::ParsingError::Io("Max nesting depth exceeded".into())
                        );
                        assert!(failure.context_stack.is_empty() && failure.hint.is_none());
                        assert_eq!(
                            (failure.bit_offset, failure.context_relative_offset),
                            (0, 0)
                        );
                    } else if callback_ok {
                        assert_eq!(result.unwrap(), 7);
                    } else {
                        let failure = result.unwrap_err();
                        assert_eq!(failure.error, sentinel.error);
                        assert_eq!(failure.context_stack, sentinel.context_stack);
                        assert_eq!(failure.bit_offset, sentinel.bit_offset);
                        assert_eq!(
                            failure.context_relative_offset,
                            sentinel.context_relative_offset
                        );
                        assert_eq!(failure.hint, sentinel.hint);
                    }
                    depth_witness_state(depth, nested);
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 20);
    }

    #[test]
    fn test_amazon_wa2_runeword_property_alignment() {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/savegames/beta/amazon_v105_slice2_equipment.d2s"
        );
        let bytes = std::fs::read(fixture).expect("save must exist");
        let huffman = HuffmanTree::new();
        let items = crate::domain::item::Item::read_player_items(&bytes, &huffman, true)
            .expect("items must parse");
        let wa2 = items
            .iter()
            .find(|it| it.code.trim() == "wa2")
            .expect("wa2 item must exist");
        assert_eq!(wa2.header.version, 2);
        assert_eq!(wa2.code.trim(), "wa2");
        assert!(wa2.header.is_runeword);
        assert_eq!(wa2.range.start, 8030);
        assert_eq!(wa2.range.end, 8542);
        assert_eq!(wa2.total_bits, 512);
        assert_eq!(wa2.properties.len(), 19);
    }

    #[test]
    fn test_druid_bst_cathan_rule_property_alignment() {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/savegames/gameplay/normal/act1/TESTDRUID_PostWarriv_Act1.d2s"
        );
        let bytes = std::fs::read(fixture).expect("save must exist");
        let huffman = HuffmanTree::new();
        let items = crate::domain::item::Item::read_player_items(&bytes, &huffman, true)
            .expect("items must parse");
        let bst = items
            .iter()
            .find(|it| it.code.trim() == "bst")
            .expect("bst item must exist");
        assert_eq!(bst.header.version, 7);
        assert_eq!(bst.code.trim(), "bst");
        assert!(!bst.header.is_identified);
    }

    #[test]
    fn test_unknown_stat_id_marked_as_opaque() {
        // Construct a bitstream containing:
        // 9-bit stat_id: 509 (unmapped arbitrary ID)
        // 9-bit value: 42
        // 9-bit terminator: 0x1FF (511)
        let mut bits = Vec::new();
        // Stat ID 509 = 0b111111101
        for i in 0..9 {
            bits.push((509 >> i) & 1 == 1);
        }
        // Value 42 = 0b000101010
        for i in 0..9 {
            bits.push((42 >> i) & 1 == 1);
        }
        // Terminator 511 = 0b111111111
        for _ in 0..9 {
            bits.push(true);
        }

        // Pack bits into bytes
        let mut bytes = Vec::new();
        for chunk in bits.chunks(8) {
            let mut byte = 0u8;
            for (i, &b) in chunk.iter().enumerate() {
                if b {
                    byte |= 1 << i;
                }
            }
            bytes.push(byte);
        }

        let reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let mut cursor = BitCursor::new(reader);
        let huffman = HuffmanTree::new();
        let axiom = StatsAxiom::new(5, ItemQuality::Normal, false);

        let result = parse_single_property(
            &mut cursor,
            5,
            &huffman,
            false,
            &axiom,
            PropertyReaderContext {
                bytes: &bytes,
                item_start_bit: 0,
            },
            |_b, _pos, _h, _idx, _alpha| {
                Err(crate::error::ParsingFailure {
                    error: crate::error::ParsingError::Io("mock".to_string()),
                    context_stack: vec![],
                    bit_offset: 0,
                    context_relative_offset: 0,
                    hint: None,
                })
            },
        );

        let (prop, is_term, _, _) = result
            .expect("should parse property")
            .expect("should have prop");
        assert!(!is_term, "first entry should not be terminator");
        assert_eq!(prop.stat_id, 509);
        assert!(prop.is_opaque, "unknown stat id must be marked as opaque");
        assert_eq!(prop.name, "opaque_property");
    }

    #[test]
    fn test_alpha_authority_runeword_keeps_caller_limit() {
        // The terminator and its Alpha alignment padding fit within this caller bound.
        let bytes = vec![0xFF, 0x01];
        let reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let mut cursor = BitCursor::new(reader);
        cursor.set_limit(18);

        let huffman = HuffmanTree::new();
        let axiom = StatsAxiom::new(5, ItemQuality::Normal, true)
            .with_code("wa2")
            .with_socketed(true)
            .with_compact(false);

        let result = read_property_list(
            &mut cursor,
            "wa2",
            5,
            PropertyReaderContext {
                bytes: &bytes,
                item_start_bit: 0,
            },
            &huffman,
            false,
            false,
            &axiom,
            None,
            None,
            |_b, _pos, _h, _idx, _alpha| {
                Err(crate::error::ParsingFailure {
                    error: crate::error::ParsingError::Io("mock".to_string()),
                    context_stack: vec![],
                    bit_offset: 0,
                    context_relative_offset: 0,
                    hint: None,
                })
            },
        );

        assert!(
            result.is_ok(),
            "bounded runeword property list should parse"
        );
        assert!(
            cursor.pos() <= 18,
            "property list must not cross the caller limit"
        );
        assert_eq!(cursor.limit(), Some(18));
    }
}
