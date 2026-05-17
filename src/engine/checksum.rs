// This software is licensed under the PolyForm Noncommercial License 1.0.0.
// Required Notice: Copyright 2026 N0FreeLunch (https://github.com/N0FreeLunch/d2r-core)

use std::io;
use crate::domain::header::axiom::HeaderAxiom;

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

pub const FILE_SIZE_OFFSET: usize = 8;
pub const CHECKSUM_OFFSET: usize = 12;

pub struct Checksum;

impl Checksum {
    /// Calculates the D2S checksum for the given byte slice using the standard strategy.
    /// Note: The checksum field itself (bytes 12-15) must be zeroed before calculation.
    pub fn calculate(bytes: &[u8]) -> i32 {
        let axiom = HeaderAxiom::new(0); // Default version for legacy calculate
        axiom.checksum_strategy().calculate(bytes) as i32
    }

    /// Fixes the checksum in-place for a mutable byte slice using the provided strategy.
    /// Automatically zeroes the checksum field before calculation.
    pub fn fix_with_strategy(bytes: &mut [u8], strategy: &dyn ChecksumStrategy) {
        if bytes.len() < CHECKSUM_OFFSET + 4 {
            return;
        }
        // Zero out the checksum field
        bytes[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 4].fill(0);
        let cs = strategy.calculate(bytes);
        
        if crate::item::item_trace_enabled() {
            eprintln!(
                "[FTI Checksum] Fixed using strategy '{}' for range 0..{}",
                strategy.name(),
                bytes.len()
            );
        }

        bytes[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 4].copy_from_slice(&cs.to_le_bytes());
    }

    /// Fixes the checksum in-place for a mutable byte slice using the standard strategy.
    pub fn fix(bytes: &mut [u8]) {
        let version = if bytes.len() >= 8 {
            u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]))
        } else {
            0
        };
        let axiom = HeaderAxiom::new(version);
        Self::fix_with_strategy(bytes, axiom.checksum_strategy().as_ref());
    }
}

/// Recalculates the checksum for a save buffer without modifying it.
pub fn recalculate_checksum(bytes: &[u8]) -> io::Result<u32> {
    if bytes.len() < CHECKSUM_OFFSET + 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Save file is too small for checksum recalculation.",
        ));
    }

    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let axiom = HeaderAxiom::new(version);
    let mut calc_bytes = bytes.to_vec();
    calc_bytes[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 4].fill(0);
    Ok(axiom.checksum_strategy().calculate(&calc_bytes))
}

/// Finalizes the save bytes by updating the file size and fixing the checksum.
/// It determines the appropriate checksum strategy based on the version via HeaderAxiom.
pub fn finalize_save_bytes(bytes: &mut Vec<u8>, _force_fix: bool) -> io::Result<()> {
    if bytes.len() < 16 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Save bytes must be at least 16 bytes to finalize.",
        ));
    }

    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let len = bytes.len();
    if len > u32::MAX as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Save file is too large to store in u32 file_size.",
        ));
    }

    // Update file size field (offset 8)
    bytes[FILE_SIZE_OFFSET..FILE_SIZE_OFFSET + 4].copy_from_slice(&(len as u32).to_le_bytes());
    
    // Select strategy via HeaderAxiom
    let axiom = HeaderAxiom::new(version);
    let strategy = axiom.checksum_strategy();
    
    // Fix checksum
    Checksum::fix_with_strategy(bytes, strategy.as_ref());
    Ok(())
}
