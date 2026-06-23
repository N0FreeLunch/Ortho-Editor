//! Gateway for external item data (Deep Links, Clipboard).
//! This module provides the "Sandbox" zone for validating incoming item payloads.

use crate::domain::inventory::{get_item_size, InventoryGrid};
use crate::domain::vo::{InventoryCoordinate, InventoryPlacement, ItemSize};
use crate::item::{Item, HuffmanTree};
use std::io;

/// Payload represents the raw input from a Deep Link.
/// d2r-core://import/item?data=<hex_payload>
pub struct ItemGateway;

fn build_inventory_occupancy_grid(items: &[Item]) -> InventoryGrid {
    let mut grid = InventoryGrid::new_inventory();
    let inventory_items: Vec<Item> = items
        .iter()
        .filter(|item| item.location == 0)
        .cloned()
        .collect();
    grid.scan_items(&inventory_items);
    grid
}

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

    /// Verifies whether the imported item fits within the supported inventory bounds
    /// and does not overlap the current inventory occupancy snapshot.
    pub fn verify_placement_with_current_items(
        item: &Item,
        x: u8,
        y: u8,
        current_items: &[Item],
    ) -> Result<InventoryPlacement, &'static str> {
        let placement = Self::verify_placement(item, x, y)?;
        let occupancy = build_inventory_occupancy_grid(current_items);

        if matches!(
            occupancy.bitboard_collision_u64(
                placement.coordinate().x(),
                placement.coordinate().y(),
                placement.size().width(),
                placement.size().height()
            ),
            Some(true)
        ) {
            return Err("Item placement overlaps occupied inventory cells");
        }

        Ok(placement)
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

    fn load_fixture_items() -> Vec<Item> {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/savegames/original/amazon_10_scrolls.d2s");
        let bytes = fs::read(&fixture_path)
            .unwrap_or_else(|err| panic!("failed to read fixture {}: {}", fixture_path.display(), err));
        let huffman = HuffmanTree::new();
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        Item::read_player_items(&bytes, &huffman, version == 105)
            .expect("fixture should parse into player items")
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

    #[test]
    fn verify_placement_rejects_occupied_inventory_cells() {
        let items = load_fixture_items();
        let inventory_items: Vec<Item> = items
            .iter()
            .filter(|item| item.location == 0)
            .cloned()
            .collect();
        let occupancy = build_inventory_occupancy_grid(&inventory_items);
        let (free_x, free_y) = occupancy
            .find_free_slot(1, 1)
            .expect("fixture should leave at least one free inventory cell");
        let occupied_item = inventory_items
            .iter()
            .find(|item| item.location == 0)
            .expect("fixture should contain at least one inventory item");

        let mut candidate = Item::empty_for_tests();
        candidate.code = "hp1".to_string();

        let placement = ItemGateway::verify_placement_with_current_items(
            &candidate,
            free_x,
            free_y,
            &items,
        )
        .expect("free inventory cell should be accepted");
        assert_eq!(
            (placement.coordinate().x(), placement.coordinate().y()),
            (free_x, free_y)
        );

        let err = ItemGateway::verify_placement_with_current_items(
            &candidate,
            occupied_item.x,
            occupied_item.y,
            &items,
        )
        .expect_err("occupied inventory cell should be rejected");
        assert_eq!(err, "Item placement overlaps occupied inventory cells");
    }
}
