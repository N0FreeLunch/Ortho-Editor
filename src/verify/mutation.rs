// This software is licensed under the PolyForm Noncommercial License 1.0.0.
// Required Notice: Copyright 2026 N0FreeLunch (https://github.com/N0FreeLunch/d2r-core)

use std::io;
use crate::engine::checksum::finalize_save_bytes;
use crate::save::CHECKSUM_OFFSET;

/// Mode for mutation addressing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationMode {
    /// Absolute bit offset from the beginning of the file.
    Absolute,
    /// Logical path addressing (e.g., "Items.Item[2].Flags")
    Logical,
}

/// Resolves a logical address path to an absolute bit offset.
/// Currently only supports "Header.Checksum".
pub fn resolve_logical_address(path: &str) -> io::Result<usize> {
    match path {
        "Header.Checksum" => Ok(CHECKSUM_OFFSET * 8),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Unsupported logical address: {}", path),
        )),
    }
}

/// Options for a mutation operation.
#[derive(Debug, Clone)]
pub struct MutationOptions {
    pub bit_offset: usize,
    pub mode: MutationMode,
}

/// Flips a single bit at the given absolute bit offset in the provided byte slice.
/// bit_offset 0 is the LSB (least significant bit) of bytes[0].
pub fn flip_bit(bytes: &mut [u8], bit_offset: usize) {
    let byte_index = bit_offset / 8;
    let bit_index = bit_offset % 8;
    if byte_index < bytes.len() {
        bytes[byte_index] ^= 1 << bit_index;
    }
}

/// Mutates a single bit at the given absolute bit offset and finalizes the save bytes
/// (recalculating file size and checksum).
/// Returns a new Vec<u8> with the mutation applied and finalized.
pub fn mutate_absolute_bit_and_finalize(bytes: &[u8], bit_offset: usize) -> io::Result<Vec<u8>> {
    let mut mutated = bytes.to_vec();
    flip_bit(&mut mutated, bit_offset);
    finalize_save_bytes(&mut mutated, false)?;
    Ok(mutated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use crate::save::{Save, recalculate_checksum};

    fn fixture_bytes() -> Vec<u8> {
        let repo_root = env!("CARGO_MANIFEST_DIR");
        let path = Path::new(repo_root).join("tests/fixtures/savegames/original/amazon_empty.d2s");
        fs::read(path).expect("fixture should exist")
    }

    #[test]
    fn test_absolute_bit_flip_and_finalize() -> io::Result<()> {
        let original_bytes = fixture_bytes();
        
        // Choose a bit that is safe to flip. 
        // 299 is CHAR_NAME_OFFSET, which is typically a safe character string.
        let bit_offset = 299 * 8; 
        
        let mutated_bytes = mutate_absolute_bit_and_finalize(&original_bytes, bit_offset)?;
        
        // 1. Verify the bit flip actually happened in the mutated buffer
        assert_ne!(original_bytes[299], mutated_bytes[299]);
        
        // 2. Reparse with Save::from_bytes
        let save = Save::from_bytes(&mutated_bytes)?;
        
        // 3. Assert header file size matches actual length
        assert_eq!(save.header.file_size as usize, mutated_bytes.len());
        
        // 4. Assert stored checksum equals recalculate_checksum
        let calculated = recalculate_checksum(&mutated_bytes)?;
        assert_eq!(save.header.checksum, calculated);
        
        Ok(())
    }
}
