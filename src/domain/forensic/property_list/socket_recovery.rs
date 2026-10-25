use crate::data::bit_cursor::BitCursor;
use crate::data::generated::layout_policy::legacy_alpha_child_code_alias;
use crate::domain::item::{HuffmanTree, Item};
use crate::domain::stats::parser::{
    ThreadLocalValueScope, nested_property_depth_failure, push_socket_trace_event,
    with_nested_property_depth,
};
use crate::error::ParsingResult;
use bitstream_io::BitRead;
use std::collections::HashMap;

/// Source-local probing and recovery for one property list's socket children.
pub(super) struct SocketRecovery<'a> {
    bytes: &'a [u8],
    huffman: &'a HuffmanTree,
    section_base_abs: u64,
    child_marker_offsets: Option<&'a [u64]>,
    child_marker_codes: Option<&'a HashMap<u64, String>>,
}

impl<'a> SocketRecovery<'a> {
    pub(super) fn new(
        bytes: &'a [u8],
        huffman: &'a HuffmanTree,
        section_base_abs: u64,
        child_marker_offsets: Option<&'a [u64]>,
        child_marker_codes: Option<&'a HashMap<u64, String>>,
    ) -> Self {
        Self {
            bytes,
            huffman,
            section_base_abs,
            child_marker_offsets,
            child_marker_codes,
        }
    }

    pub(super) fn should_stop_before_child(
        &self,
        current_rel_pos: u64,
        is_authority_host: bool,
    ) -> bool {
        let lookahead_window = 64u64;
        let next_child_rel_off = if let Some(offsets) = self.child_marker_offsets {
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
                if let Some(header_info) =
                    crate::item::peek_item_header_at(self.bytes, probe, self.huffman, true, 0)
                {
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
                    let normalized_peek =
                        legacy_alpha_child_code_alias(trimmed_peek).unwrap_or(trimmed_peek);
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
            return true;
        }
        false
    }

    pub(super) fn recover_children<R: BitRead, F>(
        &self,
        recorder: &mut BitCursor<R>,
        code: &str,
        is_alpha: bool,
        nested_items: &mut Vec<Item>,
        recovery_fn: &mut F,
    ) -> ParsingResult<()>
    where
        F: FnMut(&[u8], u64, &HuffmanTree, usize, bool) -> ParsingResult<(Item, u64)>,
    {
        push_socket_trace_event(
            "fallback_entry",
            recorder.pos().saturating_sub(self.section_base_abs),
            self.child_marker_offsets
                .and_then(|offsets| offsets.iter().min().copied()),
            None,
            "needs_socket_recovery_true",
        );
        let mut child_idx = nested_items.len();
        let mut last_rel_pos: Option<u64> = None;
        loop {
            // current_rel_pos: section-local offset. recorder.base_pos = section absolute start bit.
            let current_rel_pos = recorder.pos().saturating_sub(self.section_base_abs);
            if let Some(last) = last_rel_pos {
                if current_rel_pos <= last {
                    break;
                }
            }
            last_rel_pos = Some(current_rel_pos);

            // next_marker_rel_off: section-local offset of the next child marker
            let next_marker_local_off = if let Some(offsets) = self.child_marker_offsets {
                if let Some(&next_off) = offsets.iter().filter(|&&off| off >= current_rel_pos).min()
                {
                    let abs_target = self.section_base_abs + next_off;
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
            let scanner_code = self
                .child_marker_codes
                .and_then(|codes| codes.get(&target_local_pos))
                .map(|code| {
                    let trimmed = code.trim();
                    legacy_alpha_child_code_alias(trimmed).unwrap_or(trimmed)
                });
            let (normalized_code, force_from_scan) = if let Some(header_info) =
                crate::item::peek_item_header_at(
                    self.bytes,
                    target_local_pos,
                    self.huffman,
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
                let normalized_peek =
                    legacy_alpha_child_code_alias(trimmed_peek).unwrap_or(trimmed_peek);
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
                let _context = recorder.scoped_context("nested");
                let _recovery = ThreadLocalValueScope::replace(
                    &crate::domain::header::entity::IN_NESTED_RECOVERY,
                    true,
                );
                // Use is_alpha: ensure nested items use the correct mode
                recovery_fn(
                    self.bytes,
                    target_local_pos,
                    self.huffman,
                    child_idx,
                    is_alpha,
                )
            });

            // If recovery_fn fails but the offset was scanner-verified (force_from_scan),
            // create a minimal placeholder Item using the known scanner code.
            let (child_item, end_pos_val) = match result {
                Ok((item, ep)) => (Some(item), ep),
                Err(_) if force_from_scan && !normalized_code.is_empty() => {
                    // Recovery parse failed, but scanner confirmed this as a rune/gem.
                    // Create a minimal placeholder using the scanner-verified code.
                    let mut placeholder = crate::domain::item::Item::default();
                    let sc = legacy_alpha_child_code_alias(normalized_code.as_str())
                        .unwrap_or(normalized_code.as_str())
                        .to_string();
                    placeholder.code = sc.clone();
                    placeholder.body.code = sc;
                    // Estimate consumed bits: use gap to next marker, or fallback to 72
                    let est_end = if let Some(offsets) = self.child_marker_offsets {
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
                    match legacy_alpha_child_code_alias(c.as_str()) {
                        Some(canonical) => canonical.to_string(),
                        None => c,
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
                    recorder.pos().saturating_sub(self.section_base_abs),
                    self.child_marker_offsets.and_then(|offsets| {
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
                if let Some(offsets) = self.child_marker_offsets {
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
        Ok(())
    }
}
