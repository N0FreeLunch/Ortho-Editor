// This software is licensed under the PolyForm Noncommercial License 1.0.0.
// Required Notice: Copyright 2026 N0FreeLunch (https://github.com/N0FreeLunch/d2r-core)

use std::io;

pub const FILE_SIZE_OFFSET: usize = 8;
pub const CHECKSUM_OFFSET: usize = 12;

pub struct Checksum;

impl Checksum {
    /// Calculates the D2S checksum for the given byte slice.
    /// Note: The checksum field itself (bytes 12-15) must be zeroed before calculation.
    pub fn calculate(bytes: &[u8]) -> i32 {
        let mut checksum: i32 = 0;
        for &byte in bytes {
            let carry = if checksum < 0 { 1 } else { 0 };
            checksum = (byte as i32)
                .wrapping_add(checksum.wrapping_mul(2))
                .wrapping_add(carry);
        }
        checksum
    }

    /// Fixes the checksum in-place for a mutable byte slice.
    /// Automatically zeroes the checksum field before calculation.
    pub fn fix(bytes: &mut [u8]) {
        if bytes.len() < 16 {
            return;
        }
        // Zero out the checksum field
        bytes[12] = 0;
        bytes[13] = 0;
        bytes[14] = 0;
        bytes[15] = 0;
        let cs = Self::calculate(bytes);
        bytes[12..16].copy_from_slice(&cs.to_le_bytes());
    }
}

/// Recalculates the checksum for a save buffer without modifying it.
pub fn recalculate_checksum(bytes: &[u8]) -> io::Result<u32> {
    if bytes.len() < 16 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Save file is too small for checksum recalculation.",
        ));
    }

    let mut calc_bytes = bytes.to_vec();
    calc_bytes[12..16].copy_from_slice(&[0, 0, 0, 0]);
    Ok(Checksum::calculate(&calc_bytes) as u32)
}

/// Finalizes the save bytes by updating the file size and fixing the checksum.
/// If `force_fix` is true, it will fix the checksum regardless of version.
pub fn finalize_save_bytes(bytes: &mut Vec<u8>, force_fix: bool) -> io::Result<()> {
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
    
    // Fix checksum
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    if version != 105 || force_fix {
        Checksum::fix(bytes);
    }
    Ok(())
}
