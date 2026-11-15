// This software is licensed under the PolyForm Noncommercial License 1.0.0.
// Required Notice: Copyright 2026 N0FreeLunch (https://github.com/N0FreeLunch/d2r-core)

/// Trait for checksum calculation strategies.
pub trait ChecksumStrategy: Send + Sync {
    /// Returns the symbolic name of the strategy for tracing.
    fn name(&self) -> &str;
    /// Calculates the checksum for the given byte slice.
    fn calculate(&self, data: &[u8]) -> u32;
}

/// Standard rolling sum checksum strategy used in D2S v1.10+ and Alpha v105.
pub struct StandardRollingSum;

impl ChecksumStrategy for StandardRollingSum {
    fn name(&self) -> &str {
        "StandardRollingSum"
    }

    fn calculate(&self, data: &[u8]) -> u32 {
        let mut checksum: i32 = 0;
        for &byte in data {
            let carry = if checksum < 0 { 1 } else { 0 };
            checksum = (byte as i32)
                .wrapping_add(checksum.wrapping_mul(2))
                .wrapping_add(carry);
        }
        checksum as u32
    }
}
