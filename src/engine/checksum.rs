// This software is licensed under the PolyForm Noncommercial License 1.0.0.
// Required Notice: Copyright 2026 N0FreeLunch (https://github.com/N0FreeLunch/d2r-core)

//! Save-buffer checksum operations and compatible public strategy paths.
//!
//! ```
//! use d2r_core::domain::header::axiom::HeaderAxiom;
//! use d2r_core::engine::checksum::{
//!     ChecksumStrategy, StandardRollingSum, FILE_SIZE_OFFSET, CHECKSUM_OFFSET,
//! };
//!
//! let chosen: Box<dyn ChecksumStrategy> = HeaderAxiom::new(105).checksum_strategy();
//! let standard: &dyn ChecksumStrategy = &StandardRollingSum;
//! assert_eq!(chosen.name(), standard.name());
//! assert_eq!(standard.calculate(&[0, 0]), 0);
//! assert_eq!((FILE_SIZE_OFFSET, CHECKSUM_OFFSET), (8, 12));
//! ```

use crate::domain::header::axiom::HeaderAxiom;
pub use crate::domain::header::axiom::{CHECKSUM_OFFSET, FILE_SIZE_OFFSET};
pub use crate::domain::header::checksum::{ChecksumStrategy, StandardRollingSum};
use std::io;

fn strategy_for_save(bytes: &[u8]) -> Box<dyn ChecksumStrategy> {
    let version = crate::domain::header::axiom::try_read_save_version(bytes).unwrap_or(0);
    HeaderAxiom::new(version).checksum_strategy()
}

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
        Self::fix_with_strategy(bytes, strategy_for_save(bytes).as_ref());
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

    let strategy = strategy_for_save(bytes);
    let mut calc_bytes = bytes.to_vec();
    calc_bytes[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 4].fill(0);
    Ok(strategy.calculate(&calc_bytes))
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

    let len = bytes.len();
    if len > u32::MAX as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Save file is too large to store in u32 file_size.",
        ));
    }

    // Update file size field (offset 8)
    bytes[FILE_SIZE_OFFSET..FILE_SIZE_OFFSET + 4].copy_from_slice(&(len as u32).to_le_bytes());

    Checksum::fix(bytes);
    Ok(())
}
