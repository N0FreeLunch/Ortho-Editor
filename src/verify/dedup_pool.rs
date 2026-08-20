// d2r-core/src/verify/dedup_pool.rs
// Memory-based Item Deduplication Pool for multi-fixture audit acceleration.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use serde::{Deserialize, Serialize};

use crate::domain::item::entity::Item;
use crate::verify::symmetry::ItemDiff;

/// Unique signature representing the immutable bitstream identity of an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemSignature {
    /// 4-byte ASCII item code (e.g. b"hp1 ", b"tsc ", b"cm2 ")
    pub code: [u8; 4],
    /// Savegame format version (e.g. 5, 6, 105 for Alpha, 0..=99 for Retail)
    pub version: u8,
    /// Bitstream length in bits
    pub bit_len: usize,
    /// 64-bit fast hash of the raw item bitstream
    pub bit_hash: u64,
}

impl ItemSignature {
    /// Constructs an `ItemSignature` from an `Item` entity.
    pub fn from_item(item: &Item, is_alpha: bool) -> Self {
        let mut code_bytes = [b' '; 4];
        let trimmed = item.code.trim().as_bytes();
        let copy_len = trimmed.len().min(4);
        code_bytes[..copy_len].copy_from_slice(&trimmed[..copy_len]);

        let bit_len = item.bits.len();
        let mut hasher = DefaultHasher::new();
        hasher.write_u8(if is_alpha { 1 } else { 0 });
        hasher.write_u8(item.header.version);
        hasher.write_u32(item.header.flags);
        hasher.write_usize(bit_len);

        // Pack bits into 64-bit blocks for fast hashing
        for chunk in item.bits.chunks(64) {
            let mut val = 0u64;
            for (i, b) in chunk.iter().enumerate() {
                if b.bit {
                    val |= 1u64 << i;
                }
            }
            hasher.write_u64(val);
        }

        let bit_hash = hasher.finish();

        Self {
            code: code_bytes,
            version: if is_alpha { 105 } else { item.header.version },
            bit_len,
            bit_hash,
        }
    }

    /// Constructs an `ItemSignature` directly from raw parameters and bit slice.
    pub fn from_raw_bits(code: &str, version: u8, bits: &[bool], is_alpha: bool) -> Self {
        let mut code_bytes = [b' '; 4];
        let trimmed = code.trim().as_bytes();
        let copy_len = trimmed.len().min(4);
        code_bytes[..copy_len].copy_from_slice(&trimmed[..copy_len]);

        let bit_len = bits.len();
        let mut hasher = DefaultHasher::new();
        hasher.write_u8(if is_alpha { 1 } else { 0 });
        hasher.write_u8(version);
        hasher.write_usize(bit_len);

        for chunk in bits.chunks(64) {
            let mut val = 0u64;
            for (i, &b) in chunk.iter().enumerate() {
                if b {
                    val |= 1u64 << i;
                }
            }
            hasher.write_u64(val);
        }

        let bit_hash = hasher.finish();

        Self {
            code: code_bytes,
            version,
            bit_len,
            bit_hash,
        }
    }
}

/// Cached verification receipt storing the symmetry diff output for a verified signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedVerificationReceipt {
    pub is_match: bool,
    pub original_bit_len: usize,
    pub target_bit_len: usize,
    pub first_diff_offset: Option<u64>,
    pub mismatch_type: Option<String>,
    pub segment: Option<String>,
    pub fidelity_score: f32,
    pub diff: ItemDiff,
}

impl CachedVerificationReceipt {
    /// Creates a cached receipt from an evaluated `ItemDiff`.
    pub fn from_diff(diff: &ItemDiff) -> Self {
        Self {
            is_match: diff.is_match,
            original_bit_len: diff.original_len,
            target_bit_len: diff.target_len,
            first_diff_offset: diff.first_mismatch_offset,
            mismatch_type: diff.mismatch_type.clone(),
            segment: diff.segment.clone(),
            fidelity_score: diff.fidelity_score,
            diff: diff.clone(),
        }
    }

    /// Reconstructs an `ItemDiff` from this receipt for a given item label.
    pub fn to_item_diff(&self, label: String) -> ItemDiff {
        let mut diff = self.diff.clone();
        diff.label = label;
        diff
    }
}

/// Statistics snapshot for `ItemMemoizationPool`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PoolStats {
    pub hits: usize,
    pub misses: usize,
    pub unique_entries: usize,
    pub hit_ratio: f64,
}

impl PoolStats {
    pub fn total_queries(&self) -> usize {
        self.hits + self.misses
    }
}

/// Immutable, zero-lock deduplication table for parallel item verification receipts.
#[derive(Clone)]
pub struct ImmutableReceiptTable {
    receipts: Arc<HashMap<ItemSignature, CachedVerificationReceipt>>,
    hits: Arc<AtomicUsize>,
    misses: Arc<AtomicUsize>,
}

/// Type alias maintaining backward compatibility with `ItemMemoizationPool`.
pub type ItemMemoizationPool = ImmutableReceiptTable;

impl std::fmt::Debug for ImmutableReceiptTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImmutableReceiptTable")
            .field("entries", &self.len())
            .field("stats", &self.stats())
            .finish()
    }
}

impl Default for ImmutableReceiptTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ImmutableReceiptTable {
    /// Creates a new empty immutable receipt table.
    pub fn new() -> Self {
        Self {
            receipts: Arc::new(HashMap::new()),
            hits: Arc::new(AtomicUsize::new(0)),
            misses: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Creates an immutable receipt table from a pre-built receipts map.
    pub fn from_map(receipts: HashMap<ItemSignature, CachedVerificationReceipt>) -> Self {
        Self {
            receipts: Arc::new(receipts),
            hits: Arc::new(AtomicUsize::new(0)),
            misses: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Creates an empty builder for staged Map-Reduce construction.
    pub fn builder() -> ReceiptTableBuilder {
        ReceiptTableBuilder::new()
    }

    /// Queries the immutable table for a cached verification receipt without locks.
    #[inline]
    pub fn get(&self, sig: &ItemSignature) -> Option<&CachedVerificationReceipt> {
        if let Some(receipt) = self.receipts.get(sig) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            Some(receipt)
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Returns the number of unique signatures cached in this immutable table.
    #[inline]
    pub fn len(&self) -> usize {
        self.receipts.len()
    }

    /// Checks if the immutable table is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.receipts.is_empty()
    }

    /// Returns a snapshot of table efficiency statistics.
    pub fn stats(&self) -> PoolStats {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        let unique_entries = self.receipts.len();
        let hit_ratio = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };

        PoolStats {
            hits,
            misses,
            unique_entries,
            hit_ratio,
        }
    }
}

/// Staged builder for constructing an `ImmutableReceiptTable` across Map-Reduce phases.
#[derive(Debug, Default)]
pub struct ReceiptTableBuilder {
    receipts: HashMap<ItemSignature, CachedVerificationReceipt>,
}

impl ReceiptTableBuilder {
    /// Creates a new builder.
    pub fn new() -> Self {
        Self {
            receipts: HashMap::new(),
        }
    }

    /// Pre-allocates builder storage.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            receipts: HashMap::with_capacity(capacity),
        }
    }

    /// Registers a single verified signature and receipt into the builder.
    pub fn insert(&mut self, sig: ItemSignature, receipt: CachedVerificationReceipt) {
        self.receipts.insert(sig, receipt);
    }

    /// Extends the builder with verified receipts from an iterator.
    pub fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = (ItemSignature, CachedVerificationReceipt)>,
    {
        self.receipts.extend(iter);
    }

    /// Finalizes and seals the receipts into an `ImmutableReceiptTable` with 0-lock lookups.
    pub fn build(self) -> ImmutableReceiptTable {
        ImmutableReceiptTable::from_map(self.receipts)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::domain::item::entity::{Item, RecordedBit};

    #[test]
    fn test_item_signature_deterministic() {
        let mut item1 = Item::default();
        item1.code = "hp1 ".to_string();
        item1.header.version = 5;
        item1.bits = vec![
            RecordedBit { bit: true, offset: 0 },
            RecordedBit { bit: false, offset: 1 },
            RecordedBit { bit: true, offset: 2 },
        ];

        let mut item2 = Item::default();
        item2.code = "hp1".to_string();
        item2.header.version = 5;
        item2.bits = vec![
            RecordedBit { bit: true, offset: 0 },
            RecordedBit { bit: false, offset: 1 },
            RecordedBit { bit: true, offset: 2 },
        ];

        let sig1 = ItemSignature::from_item(&item1, false);
        let sig2 = ItemSignature::from_item(&item2, false);
        assert_eq!(sig1, sig2);
        assert_eq!(sig1.bit_hash, sig2.bit_hash);
    }

    #[test]
    fn test_immutable_table_builder_and_retrieval() {
        let mut builder = ImmutableReceiptTable::builder();
        let sig = ItemSignature::from_raw_bits("tsc", 5, &[true, false, true], false);

        let mut diff = ItemDiff::default();
        diff.label = "Original Item".to_string();
        diff.code = "tsc".to_string();
        diff.is_match = true;
        diff.original_len = 3;
        diff.target_len = 3;

        let receipt = CachedVerificationReceipt::from_diff(&diff);
        builder.insert(sig, receipt);

        let table = builder.build();
        assert_eq!(table.len(), 1);

        let unverified_sig = ItemSignature::from_raw_bits("hp1", 5, &[false, false], false);
        assert!(table.get(&unverified_sig).is_none());
        assert_eq!(table.stats().misses, 1);
        assert_eq!(table.stats().hits, 0);

        let cached = table.get(&sig).expect("Cached receipt must be found");
        assert!(cached.is_match);
        assert_eq!(cached.original_bit_len, 3);
        assert_eq!(table.stats().hits, 1);
        assert_eq!(table.stats().unique_entries, 1);

        let item_diff = cached.to_item_diff("Item 42".to_string());
        assert_eq!(item_diff.label, "Item 42");
        assert!(item_diff.is_match);
    }

    #[test]
    fn test_concurrent_zero_lock_access() {
        use std::sync::Arc;
        use std::thread;

        let mut builder = ImmutableReceiptTable::builder();
        let sig = ItemSignature::from_raw_bits("hp1", 5, &[true, true, false], false);

        let mut diff = ItemDiff::default();
        diff.code = "hp1".to_string();
        diff.is_match = true;
        builder.insert(sig, CachedVerificationReceipt::from_diff(&diff));

        let table = Arc::new(builder.build());

        let mut handles = Vec::new();
        for _ in 0..8 {
            let t = Arc::clone(&table);
            let s = sig;
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    let receipt = t.get(&s).unwrap();
                    assert!(receipt.is_match);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let stats = table.stats();
        assert_eq!(stats.hits, 800);
        assert_eq!(stats.unique_entries, 1);
    }
}
