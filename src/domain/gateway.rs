//! Gateway for external item data (Deep Links, Clipboard).
//! This module provides the "Sandbox" zone for validating incoming item payloads.

use crate::domain::inventory::get_item_size;
use crate::domain::vo::{InventoryCoordinate, InventoryPlacement, ItemSize};
use crate::item::{Item, HuffmanTree};
use std::io;

/// Payload represents the raw input from a Deep Link.
/// d2r-core://import/item?data=<hex_payload>
pub struct ItemGateway;

impl ItemGateway {
    /// Safe entry point for importing an item from a hex-encoded bitstream.
    /// This uses the VO-guarded parsing logic and returns a DiagnosticError
    /// if the payload is malicious or malformed.
    pub fn from_payload(hex_data: &str) -> Result<Item, io::Error> {
        let bytes = hex::decode(hex_data).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidInput, format!("Invalid HEX payload: {}", e))
        })?;

        let huffman = HuffmanTree::new();
        
        // Use the reinforced Item::read_single_item (Total Function approach)
        // to prevent panics and return structured errors.
        Item::from_bytes(&bytes, &huffman, false).map_err(Into::into)
    }

    /// Verifies whether the imported item fits within the supported inventory bounds.
    /// This is a boundary-only placement guard and does not inspect occupancy state.
    pub fn verify_placement(
        item: &Item,
        x: u8,
        y: u8,
    ) -> Result<InventoryPlacement, &'static str> {
        let (width, height) = get_item_size(item.code());
        let coordinate = InventoryCoordinate::new(x, y)?;
        let size = ItemSize::new(width, height)?;
        InventoryPlacement::new(coordinate, size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn load_fixture_item() -> Item {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/savegames/original/amazon_10_scrolls.d2s");
        let bytes = fs::read(&fixture_path)
            .unwrap_or_else(|err| panic!("failed to read fixture {}: {}", fixture_path.display(), err));
        let huffman = HuffmanTree::new();
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        let items = Item::read_player_items(&bytes, &huffman, version == 105)
            .expect("fixture should parse into player items");
        items
            .into_iter()
            .next()
            .expect("fixture should contain at least one item")
    }

    #[test]
    fn verify_placement_links_size_and_bounds() {
        let item = load_fixture_item();
        let expected_size = get_item_size(item.code());

        let placement = ItemGateway::verify_placement(&item, 0, 0)
            .expect("in-bounds placement should succeed");

        assert_eq!((placement.coordinate().x(), placement.coordinate().y()), (0, 0));
        assert_eq!(
            (placement.size().width(), placement.size().height()),
            expected_size
        );

        let err = ItemGateway::verify_placement(&item, 10, 0)
            .expect_err("out-of-bounds placement should fail");
        assert_eq!(err, "Item placement exceeds inventory boundaries");
    }
}
