use bitstream_io::{BitRead, Numeric};
use crate::domain::item::{BitSegment, RecordedBit};
use crate::domain::item::quality::ItemQuality;
use crate::domain::header::entity::ItemSegmentType;
use crate::error::{BackingBitCursor, ParsingError, ParsingFailure, ParsingResult};

/// A bit-precision cursor that wraps a `BitRead` implementation and adds
/// positioning, checkpoint/rollback, and semantic recording capabilities.
pub struct BitCursor<R: BitRead> {
    inner: R,
    bit_pos: u64,
    pub base_pos: u64, // Added to support relative caching
    limit: Option<u64>,
    recorded_bits: Vec<RecordedBit>,
    segments: Vec<BitSegment>,
    context_stack: Vec<(String, u64, Option<u64>)>, // (label, start_bit, expected_bits)
    pub trace_enabled: bool,
    pub alpha_quality: Option<ItemQuality>,
}

impl<R: BitRead> BackingBitCursor for BitCursor<R> {
    fn pos(&self) -> u64 {
        self.bit_pos
    }

    fn context_stack(&self) -> Vec<String> {
        self.context_stack.iter().map(|(label, _, _)| label.clone()).collect()
    }

    fn current_context_start(&self) -> u64 {
        self.context_stack.last().map(|(_, start, _)| *start).unwrap_or(0)
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
    pub fn read_bits<T: Numeric + From<u8> + std::ops::BitOrAssign + std::ops::Shl<u32, Output = T>>(&mut self, count: u32) -> ParsingResult<T> {
        let mut value = T::from(0u8);
        let max_bits = (std::mem::size_of::<T>() * 8) as u32;
        for i in 0..count {
            if self.read_bit()? {
                if i < max_bits {
                    value |= T::from(1u8) << i;
                }
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
        self.context_stack.push((label.to_string(), self.bit_pos, None));
    }

    /// Begins a new semantic segment from an enum type.
    pub fn begin_segment(&mut self, segment_type: ItemSegmentType) {
        self.push_context(&format!("{:?}", segment_type));
    }

    /// Begins a new semantic segment with an expected bit length.
    pub fn begin_segment_with_expected(&mut self, segment_type: ItemSegmentType, expected: Option<u64>) {
        self.context_stack.push((format!("{:?}", segment_type), self.bit_pos, expected));
    }

    /// Ends the current semantic segment and records it.
    pub fn end_segment(&mut self) {
        if let Some((label, start, expected)) = self.context_stack.pop() {
            let _actual_bits = self.bit_pos - start;
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

    pub fn with_context<T, F>(&mut self, name: &str, mut f: F) -> ParsingResult<T>
    where F: FnMut(&mut Self) -> ParsingResult<T>
    {
        self.push_context(name);
        let res = f(self);
        self.pop_context();
        res
    }

    pub fn pop_context(&mut self) {
        self.end_segment();
    }

    pub fn context_stack(&self) -> Vec<String> {
        self.context_stack.iter().map(|(label, _, _)| label.clone()).collect()
    }

    pub fn get_context_count(&self) -> usize {
        self.context_stack.len()
    }

    /// Returns all recorded segments.
    pub fn segments(&self) -> &[BitSegment] {
        &self.segments
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
