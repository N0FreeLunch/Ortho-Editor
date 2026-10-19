use crate::domain::header::entity::ItemSegmentType;
use crate::domain::item::quality::ItemQuality;
use crate::domain::item::{BitSegment, RecordedBit};
use crate::error::{BackingBitCursor, ParsingError, ParsingFailure, ParsingResult};
use bitstream_io::{BitRead, Numeric};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BitReadTraceEvent {
    pub operation_label: String,
    pub pre_read_bit: u64,
    pub post_read_bit: u64,
    pub requested_width_bits: Option<u32>,
    pub context_stack: Vec<String>,
    pub outcome: String,
}

/// A bit-precision cursor that wraps a `BitRead` implementation and adds
/// positioning, checkpoint/rollback, and semantic recording capabilities.
pub struct BitCursor<R: BitRead> {
    inner: R,
    bit_pos: u64,
    pub base_pos: u64, // Added to support relative caching
    limit: Option<u64>,
    recorded_bits: Vec<RecordedBit>,
    segments: Vec<BitSegment>,
    operation_events: Vec<BitReadTraceEvent>,
    context_stack: Vec<(String, u64, Option<u64>)>, // (label, start_bit, expected_bits)
    pub trace_enabled: bool,
    pub alpha_quality: Option<ItemQuality>,
}

/// Owns contexts opened after an enclosing stack depth without owning cursor position.
pub(crate) struct CursorContextScope<'a, R: BitRead> {
    cursor: &'a mut BitCursor<R>,
    enclosing_depth: usize,
}

impl<R: BitRead> std::ops::Deref for CursorContextScope<'_, R> {
    type Target = BitCursor<R>;

    fn deref(&self) -> &Self::Target {
        self.cursor
    }
}

impl<R: BitRead> std::ops::DerefMut for CursorContextScope<'_, R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.cursor
    }
}

impl<R: BitRead> Drop for CursorContextScope<'_, R> {
    fn drop(&mut self) {
        while self.cursor.context_stack.len() > self.enclosing_depth {
            self.cursor.end_segment();
        }
    }
}

impl<R: BitRead> BackingBitCursor for BitCursor<R> {
    fn pos(&self) -> u64 {
        self.bit_pos
    }

    fn context_stack(&self) -> Vec<String> {
        self.context_stack
            .iter()
            .map(|(label, _, _)| label.clone())
            .collect()
    }

    fn current_context_start(&self) -> u64 {
        self.context_stack
            .last()
            .map(|(_, start, _)| *start)
            .unwrap_or(0)
    }
}

impl<R: BitRead> BitCursor<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            bit_pos: 0,
            base_pos: 0,
            limit: None,
            recorded_bits: Vec::new(),
            segments: Vec::new(),
            operation_events: Vec::new(),
            context_stack: Vec::new(),
            trace_enabled: false,
            alpha_quality: None,
        }
    }

    pub fn set_limit(&mut self, limit: u64) {
        self.limit = Some(limit);
    }

    pub fn set_trace(&mut self, enabled: bool) {
        self.trace_enabled = enabled;
    }

    pub fn fail(&self, error: ParsingError) -> ParsingFailure {
        ParsingFailure::new(error, self)
    }

    pub fn err<T>(&self, error: ParsingError) -> ParsingResult<T> {
        Err(self.fail(error))
    }

    pub fn set_pos(&mut self, pos: u64) {
        self.bit_pos = pos;
    }

    /// Returns the current bit position.
    pub fn pos(&self) -> u64 {
        self.bit_pos
    }

    pub fn limit(&self) -> Option<u64> {
        self.limit
    }

    /// Returns the number of bits remaining until the limit, or a large value if no limit is set.
    pub fn remaining(&self) -> u64 {
        if let Some(limit) = self.limit {
            limit.saturating_sub(self.bit_pos)
        } else {
            u64::MAX
        }
    }

    /// Alias for pos() to match BitRecorder's total_read
    pub fn total_read(&self) -> u64 {
        self.bit_pos
    }

    /// Reads a single bit from the stream.
    pub fn read_bit(&mut self) -> ParsingResult<bool> {
        if let Some(limit) = self.limit {
            if self.bit_pos >= limit {
                return Err(self.fail(ParsingError::Io("Bit limit exceeded (possible section boundary violation)".to_string()))
                    .with_hint("The parser tried to read beyond the allocated bits for this item or section."));
            }
        }

        // Slice 9: True rollback support via bit cache.
        // Forensic: We use the offset of the first recorded bit as the baseline for the cache.
        if let Some(first) = self.recorded_bits.first() {
            if self.bit_pos >= first.offset {
                let cache_idx = (self.bit_pos - first.offset) as usize;
                if cache_idx < self.recorded_bits.len() {
                    let bit = self.recorded_bits[cache_idx].bit;
                    self.bit_pos += 1;
                    return Ok(bit);
                }
            }
        }

        let bit = self.inner.read_bit().map_err(|e| {
            self.fail(ParsingError::Io(format!("Bit-level read failure: {}", e)))
                .with_hint("Possible end of bitstream reached unexpectedly.")
        })?;
        self.recorded_bits.push(RecordedBit {
            bit,
            offset: self.bit_pos,
        });
        self.bit_pos += 1;
        Ok(bit)
    }

    /// Reads multiple bits from the stream as a numeric type.
    pub fn read_bits<
        T: Numeric + From<u8> + std::ops::BitOrAssign + std::ops::Shl<u32, Output = T>,
    >(
        &mut self,
        count: u32,
    ) -> ParsingResult<T> {
        let pre_read_bit = self.bit_pos;
        let context_stack = self.context_stack();
        let operation_label = context_stack
            .last()
            .filter(|label| label.starts_with("ExtendedStats:") || *label == "Stats")
            .cloned();
        let mut value = T::from(0u8);
        let max_bits = (std::mem::size_of::<T>() * 8) as u32;
        for i in 0..count {
            match self.read_bit() {
                Ok(bit) => {
                    if bit && i < max_bits {
                        value |= T::from(1u8) << i;
                    }
                }
                Err(error) => {
                    if self.trace_enabled {
                        if let Some(operation_label) = operation_label.clone() {
                            self.operation_events.push(BitReadTraceEvent {
                                operation_label,
                                pre_read_bit,
                                post_read_bit: self.bit_pos,
                                requested_width_bits: Some(count),
                                context_stack: context_stack.clone(),
                                outcome: "failure".to_string(),
                            });
                        }
                    }
                    return Err(error);
                }
            }
        }
        if self.trace_enabled {
            if let Some(operation_label) = operation_label {
                self.operation_events.push(BitReadTraceEvent {
                    operation_label,
                    pre_read_bit,
                    post_read_bit: self.bit_pos,
                    requested_width_bits: Some(count),
                    context_stack,
                    outcome: "success".to_string(),
                });
            }
        }
        Ok(value)
    }

    /// Specific version for u64 to match BitRecorder's read_bits_u64
    pub fn read_bits_u64(&mut self, count: u32) -> ParsingResult<u64> {
        self.read_bits::<u64>(count)
    }

    pub fn read_bits_as_vec(&mut self, count: u32) -> ParsingResult<Vec<bool>> {
        let mut bits = Vec::with_capacity(count as usize);
        for _ in 0..count {
            bits.push(self.read_bit()?);
        }
        Ok(bits)
    }

    pub fn read_bits_as_vec_up_to(&mut self, count: u32) -> Vec<bool> {
        let mut bits = Vec::with_capacity(count as usize);
        for _ in 0..count {
            match self.read_bit() {
                Ok(bit) => bits.push(bit),
                Err(_) => break,
            }
        }
        bits
    }

    pub fn skip_and_record(&mut self, n: u32) -> ParsingResult<()> {
        for _ in 0..n {
            let _ = self.read_bit()?;
        }
        Ok(())
    }

    pub fn skip(&mut self, n: u64) -> ParsingResult<()> {
        for _ in 0..n {
            let _ = self.read_bit()?;
        }
        Ok(())
    }

    /// Begins a new semantic segment with a custom label.
    pub fn push_context(&mut self, label: &str) {
        self.context_stack
            .push((label.to_string(), self.bit_pos, None));
    }

    /// Begins a new semantic segment from an enum type.
    pub fn begin_segment(&mut self, segment_type: ItemSegmentType) {
        self.push_context(&format!("{:?}", segment_type));
    }

    /// Begins a new semantic segment with an expected bit length.
    pub fn begin_segment_with_expected(
        &mut self,
        segment_type: ItemSegmentType,
        expected: Option<u64>,
    ) {
        self.context_stack
            .push((format!("{:?}", segment_type), self.bit_pos, expected));
    }

    /// Ends the current semantic segment and records it.
    pub fn end_segment(&mut self) {
        if let Some((label, start, expected)) = self.context_stack.pop() {
            let _actual_bits = self.bit_pos.saturating_sub(start);
            if let Some(_expected_bits) = expected {
                // If trace enabled, we could log mismatches here.
            }

            if self.trace_enabled {
                self.segments.push(BitSegment {
                    start,
                    end: self.bit_pos,
                    label,
                    depth: self.context_stack.len(),
                });
            }
        }
    }

    /// Closes this scope and its pending descendants on return or unwind.
    /// Cursor position, consumed bits, limits, and existing error snapshots are retained.
    pub(crate) fn scoped_context(&mut self, name: &str) -> CursorContextScope<'_, R> {
        let enclosing_depth = self.context_stack.len();
        self.push_context(name);
        CursorContextScope {
            cursor: self,
            enclosing_depth,
        }
    }

    pub fn with_context<T, F>(&mut self, name: &str, mut f: F) -> ParsingResult<T>
    where
        F: FnMut(&mut Self) -> ParsingResult<T>,
    {
        let mut scope = self.scoped_context(name);
        f(&mut scope)
    }

    pub fn pop_context(&mut self) {
        self.end_segment();
    }

    pub fn context_stack(&self) -> Vec<String> {
        self.context_stack
            .iter()
            .map(|(label, _, _)| label.clone())
            .collect()
    }

    pub fn get_context_count(&self) -> usize {
        self.context_stack.len()
    }

    /// Returns all recorded segments.
    pub fn segments(&self) -> &[BitSegment] {
        &self.segments
    }

    pub fn operation_events(&self) -> &[BitReadTraceEvent] {
        &self.operation_events
    }

    /// Returns all active (unclosed) context segments.
    pub fn active_segments(&self) -> Vec<BitSegment> {
        self.context_stack
            .iter()
            .enumerate()
            .map(|(idx, (label, start, _))| BitSegment {
                start: *start,
                end: self.bit_pos,
                label: label.clone(),
                depth: idx,
            })
            .collect()
    }

    /// Returns all recorded bits.
    pub fn recorded_bits(&self) -> &[RecordedBit] {
        &self.recorded_bits
    }

    /// Returns all recorded bits as a Vec.
    pub fn recorded_bits_vec(&self) -> Vec<RecordedBit> {
        self.recorded_bits.clone()
    }

    /// Creates a checkpoint of the current state.
    pub fn checkpoint(&self) -> u64 {
        self.bit_pos
    }

    /// Rollback to a previous bit position.
    pub fn rollback(&mut self, checkpoint: u64) {
        self.bit_pos = checkpoint;
        // Slice 9: We no longer discard recorded bits/segments on rollback.
        // This allows re-reading from the cache.
        // self.recorded_bits.retain(|rb| rb.offset < checkpoint);
        // self.segments.retain(|s| s.end <= checkpoint);
    }

    pub fn byte_align(&mut self) -> ParsingResult<()> {
        let padding = (8 - (self.bit_pos % 8)) % 8;
        for _ in 0..padding {
            let _ = self.read_bit()?;
        }
        Ok(())
    }

    pub fn peek_bits_string(&mut self, _count: u32) -> ParsingResult<String> {
        Ok("peek_not_implemented".to_string())
    }
}

/// Forensic Implementation (Slice 7)
impl<'a> BitCursor<bitstream_io::BitReader<std::io::Cursor<&'a [u8]>, bitstream_io::LittleEndian>> {
    /// Creates a fresh BitCursor aligned to an absolute bit.
    /// This bypasses logical rollback limitations by physically seeking a new BitRead.
    pub fn from_raw_at(bytes: &'a [u8], bit_offset: u64) -> Self {
        use bitstream_io::{BitReader, LittleEndian};
        use std::io::Cursor;
        let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
        let _ = reader.skip(bit_offset as u32);
        let mut cursor = BitCursor::new(reader);
        cursor.bit_pos = bit_offset;
        cursor.base_pos = bit_offset;
        cursor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitstream_io::{BitReader, LittleEndian};
    use std::io::Cursor;

    use d2r_macros::hypothesis_test;

    #[hypothesis_test]
    fn refactor_context_scope_descendants_close_on_success_error_and_unwind() {
        let mut cases = 0;
        for trace in [false, true] {
            for offset in [0, 3] {
                for exit in 0..3 {
                    let bytes = [0b10101101; 4];
                    let mut cursor =
                        BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                    cursor.push_context("owner");
                    for _ in 0..offset {
                        cursor.read_bit().unwrap();
                    }
                    cursor.set_trace(trace);
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        cursor.with_context("scope", |cursor| {
                            cursor.push_context("leaf");
                            let value = cursor.read_bits::<u32>(2)?;
                            match exit {
                                0 => Ok(value),
                                1 => {
                                    cursor.err(ParsingError::Generic("scope error sentinel".into()))
                                }
                                _ => std::panic::panic_any("scope panic sentinel"),
                            }
                        })
                    }));
                    match exit {
                        0 => assert!(outcome.unwrap().is_ok()),
                        1 => {
                            let failure = outcome.unwrap().unwrap_err();
                            assert_eq!(failure.context_stack, vec!["owner", "scope", "leaf"]);
                            assert_eq!(
                                (failure.bit_offset, failure.context_relative_offset),
                                (offset + 2, 2)
                            );
                            assert_eq!(
                                failure.error,
                                ParsingError::Generic("scope error sentinel".into())
                            );
                        }
                        _ => assert_eq!(
                            outcome.unwrap_err().downcast_ref::<&str>(),
                            Some(&"scope panic sentinel")
                        ),
                    }
                    assert_eq!(cursor.context_stack(), vec!["owner"]);
                    assert_eq!((cursor.pos(), cursor.limit()), (offset + 2, None));
                    assert_eq!(cursor.recorded_bits().len(), (offset + 2) as usize);
                    assert_eq!(cursor.segments().len(), if trace { 2 } else { 0 });
                    if trace {
                        for (segment, label, depth) in [
                            (&cursor.segments()[0], "leaf", 2),
                            (&cursor.segments()[1], "scope", 1),
                        ] {
                            assert_eq!(segment.label, label);
                            assert_eq!(
                                (segment.start, segment.end, segment.depth),
                                (offset, offset + 2, depth)
                            );
                        }
                    }
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 12);
    }

    #[hypothesis_test]
    fn refactor_context_scope_nesting_and_explicit_close_preserve_ancestor() {
        let mut cases = 0;
        for trace in [false, true] {
            for explicit_close in [false, true] {
                let bytes = [0b10101101; 4];
                let mut cursor =
                    BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                cursor.set_trace(trace);
                cursor.push_context("owner");
                {
                    let mut outer = cursor.scoped_context("outer");
                    {
                        let mut inner = outer.scoped_context("inner");
                        inner.read_bits::<u32>(2).unwrap();
                        if explicit_close {
                            inner.pop_context();
                        }
                    }
                    assert_eq!(outer.context_stack(), vec!["owner", "outer"]);
                    if explicit_close {
                        outer.pop_context();
                    }
                }
                assert_eq!(cursor.context_stack(), vec!["owner"]);
                assert_eq!(cursor.pos(), 2);
                assert_eq!(cursor.segments().len(), if trace { 2 } else { 0 });
                if trace {
                    assert_eq!(cursor.segments()[0].label, "inner");
                    assert_eq!(cursor.segments()[1].label, "outer");
                }
                cases += 1;
            }
        }
        assert_eq!(cases, 4);
    }

    #[hypothesis_test]
    fn refactor_context_scope_rewind_does_not_replace_original_panic() {
        for trace in [false, true] {
            let bytes = [0b10101101; 4];
            let mut cursor = BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
            cursor.push_context("owner");
            cursor.read_bits::<u32>(3).unwrap();
            cursor.set_trace(trace);
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut context = cursor.scoped_context("rewound");
                context.rollback(0);
                std::panic::panic_any("rewind panic sentinel");
            }));
            assert_eq!(
                outcome.unwrap_err().downcast_ref::<&str>(),
                Some(&"rewind panic sentinel")
            );
            assert_eq!(cursor.context_stack(), vec!["owner"]);
            assert_eq!(cursor.pos(), 0);
            assert_eq!(cursor.recorded_bits().len(), 3);
            assert_eq!(cursor.segments().len(), usize::from(trace));
            if trace {
                let segment = &cursor.segments()[0];
                assert_eq!((segment.start, segment.end, segment.depth), (3, 0, 1));
                assert_eq!(segment.label, "rewound");
            }
        }
    }

    #[test]
    fn test_bit_cursor_basic_read() {
        let bytes = vec![0b10110010]; // 0xB2
        let reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let mut cursor = BitCursor::new(reader);

        assert_eq!(cursor.read_bit().unwrap(), false); // LSB of 0xB2 is 0
        assert_eq!(cursor.pos(), 1);
        assert_eq!(cursor.read_bit().unwrap(), true);
        assert_eq!(cursor.pos(), 2);
    }

    #[test]
    fn test_bit_cursor_segments() {
        let bytes = vec![0b00000000];
        let reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let mut cursor = BitCursor::new(reader);
        cursor.set_trace(true);

        cursor.begin_segment(ItemSegmentType::Header);
        let _ = cursor.read_bits::<u32>(4).unwrap();
        cursor.end_segment();

        assert_eq!(cursor.segments().len(), 1);
        assert_eq!(cursor.segments()[0].label, "Header");
        assert_eq!(cursor.segments()[0].start, 0);
        assert_eq!(cursor.segments()[0].end, 4);
    }

    #[test]
    fn test_extended_stats_read_bits_trace_events() {
        let bytes = vec![0b00000000];
        let reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let mut cursor = BitCursor::new(reader);
        cursor.set_trace(true);
        cursor.set_limit(2);

        cursor.push_context("ExtendedStats:id");
        let success = cursor.read_bits::<u32>(2);
        assert_eq!(success.unwrap(), 0);
        let failure = cursor.read_bits::<u32>(2);
        assert!(failure.is_err());
        cursor.pop_context();

        assert_eq!(cursor.operation_events().len(), 2);
        assert_eq!(cursor.operation_events()[0].outcome, "success");
        assert_eq!(cursor.operation_events()[0].pre_read_bit, 0);
        assert_eq!(cursor.operation_events()[0].post_read_bit, 2);
        assert_eq!(cursor.operation_events()[1].outcome, "failure");
        assert_eq!(cursor.operation_events()[1].pre_read_bit, 2);
        assert_eq!(cursor.operation_events()[1].post_read_bit, 2);
        assert_eq!(
            cursor.operation_events()[1].context_stack,
            vec!["ExtendedStats:id"]
        );

        let bytes = vec![0b00000000];
        let reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let mut non_extended = BitCursor::new(reader);
        non_extended.set_trace(true);
        non_extended.push_context("Other:id");
        assert!(non_extended.read_bits::<u32>(1).is_ok());
        assert!(non_extended.operation_events().is_empty());
    }

    #[test]
    fn test_stats_context_read_bits_trace_events() {
        let bytes = vec![0b00000000];
        let reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let mut cursor = BitCursor::new(reader);
        cursor.set_trace(true);
        cursor.set_limit(2);

        cursor.push_context("Stats");
        let success = cursor.read_bits::<u32>(2);
        assert_eq!(success.unwrap(), 0);
        let failure = cursor.read_bits::<u32>(2);
        assert!(failure.is_err());
        cursor.pop_context();

        assert_eq!(cursor.operation_events().len(), 2);
        assert_eq!(cursor.operation_events()[0].operation_label, "Stats");
        assert_eq!(cursor.operation_events()[0].requested_width_bits, Some(2));
        assert_eq!(cursor.operation_events()[0].pre_read_bit, 0);
        assert_eq!(cursor.operation_events()[0].post_read_bit, 2);
        assert_eq!(cursor.operation_events()[0].outcome, "success");
        assert_eq!(cursor.operation_events()[0].context_stack, vec!["Stats"]);
        assert_eq!(cursor.operation_events()[1].operation_label, "Stats");
        assert_eq!(cursor.operation_events()[1].requested_width_bits, Some(2));
        assert_eq!(cursor.operation_events()[1].pre_read_bit, 2);
        assert_eq!(cursor.operation_events()[1].post_read_bit, 2);
        assert_eq!(cursor.operation_events()[1].outcome, "failure");
        assert_eq!(cursor.operation_events()[1].context_stack, vec!["Stats"]);
    }

    #[test]
    fn test_nested_extended_stats_read_bits_trace_event_admission() {
        let bytes = vec![0b00000000];
        let reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let mut cursor = BitCursor::new(reader);
        cursor.set_trace(true);
        cursor.set_limit(2);

        cursor.push_context("Root");
        cursor.push_context("ExtendedStats");
        cursor.push_context("ExtendedStats:id");

        assert_eq!(cursor.read_bits::<u32>(2).unwrap(), 0);
        assert!(cursor.read_bits::<u32>(2).is_err());

        assert_eq!(cursor.operation_events().len(), 2);
        assert_eq!(cursor.operation_events()[0].outcome, "success");
        assert_eq!(cursor.operation_events()[1].outcome, "failure");
        assert_eq!(
            cursor.operation_events()[0].operation_label,
            "ExtendedStats:id"
        );
        assert_eq!(
            cursor.operation_events()[1].operation_label,
            "ExtendedStats:id"
        );
        assert_eq!(
            cursor.operation_events()[0].context_stack,
            vec!["Root", "ExtendedStats", "ExtendedStats:id"]
        );
        assert_eq!(
            cursor.operation_events()[1].context_stack,
            vec!["Root", "ExtendedStats", "ExtendedStats:id"]
        );
    }

    #[test]
    fn test_bit_cursor_rollback_interface() {
        let bytes = vec![0b00000000];
        let reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let mut cursor = BitCursor::new(reader);

        let checkpoint = cursor.checkpoint();
        let _ = cursor.read_bits::<u32>(4).unwrap();
        cursor.rollback(checkpoint);

        assert_eq!(cursor.pos(), 0);
        assert_eq!(cursor.recorded_bits().len(), 4);
    }
}
