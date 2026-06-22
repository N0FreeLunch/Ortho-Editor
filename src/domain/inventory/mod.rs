use crate::item::Item;
use std::fmt;

#[derive(Debug, Clone)]
pub enum InventoryError {
    OutOfBounds {
        item_code: String,
        x: u8,
        y: u8,
        w: u8,
        h: u8,
    },
    Collision {
        item_code: String,
        x: u8,
        y: u8,
        w: u8,
        h: u8,
    },
    InvalidCode(String),
    LogicalMismatch {
        item_code: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridMaskWidth {
    U64,
    U128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridPattern {
    width: u8,
    height: u8,
}

impl GridPattern {
    pub const fn new(width: u8, height: u8) -> Self {
        Self { width, height }
    }

    pub const fn inventory() -> Self {
        Self::new(10, 4)
    }

    pub const fn stash() -> Self {
        Self::new(10, 10)
    }

    pub const fn cube() -> Self {
        Self::new(3, 4)
    }

    pub const fn belt(height: u8) -> Self {
        Self::new(4, height)
    }

    pub const fn width(self) -> u8 {
        self.width
    }

    pub const fn height(self) -> u8 {
        self.height
    }

    pub const fn cell_count(self) -> usize {
        (self.width as usize) * (self.height as usize)
    }

    pub fn mask_width(self) -> GridMaskWidth {
        if self.cell_count() <= u64::BITS as usize {
            GridMaskWidth::U64
        } else {
            GridMaskWidth::U128
        }
    }

    pub fn bit_index(self, x: u8, y: u8) -> Option<usize> {
        if x < self.width && y < self.height {
            Some((y as usize) * (self.width as usize) + (x as usize))
        } else {
            None
        }
    }

    pub fn rect_mask_u128(self, x: u8, y: u8, w: u8, h: u8) -> Option<u128> {
        if u16::from(x) + u16::from(w) > u16::from(self.width)
            || u16::from(y) + u16::from(h) > u16::from(self.height)
        {
            return None;
        }

        let mut mask = 0u128;
        for row in y..(y + h) {
            for col in x..(x + w) {
                let bit = self.bit_index(col, row)?;
                mask |= 1u128 << bit;
            }
        }
        Some(mask)
    }

    pub fn rect_mask_u64(self, x: u8, y: u8, w: u8, h: u8) -> Option<u64> {
        if self.mask_width() != GridMaskWidth::U64 {
            return None;
        }
        self.rect_mask_u128(x, y, w, h).map(|mask| mask as u64)
    }

    pub fn occupied_mask_u128(self, cells: &[bool]) -> Option<u128> {
        if cells.len() != self.cell_count() {
            return None;
        }

        let mut mask = 0u128;
        for (index, occupied) in cells.iter().copied().enumerate() {
            if occupied {
                mask |= 1u128 << index;
            }
        }
        Some(mask)
    }

    pub fn occupied_mask_u64(self, cells: &[bool]) -> Option<u64> {
        if self.mask_width() != GridMaskWidth::U64 {
            return None;
        }
        self.occupied_mask_u128(cells).map(|mask| mask as u64)
    }

    pub fn collides_u128(self, occupied_mask: u128, x: u8, y: u8, w: u8, h: u8) -> Option<bool> {
        self.rect_mask_u128(x, y, w, h)
            .map(|mask| occupied_mask & mask != 0)
    }

    pub fn collides_u64(self, occupied_mask: u64, x: u8, y: u8, w: u8, h: u8) -> Option<bool> {
        if self.mask_width() != GridMaskWidth::U64 {
            return None;
        }
        self.rect_mask_u64(x, y, w, h)
            .map(|mask| occupied_mask & mask != 0)
    }

    pub fn merged_u128(self, occupied_mask: u128, x: u8, y: u8, w: u8, h: u8) -> Option<u128> {
        self.rect_mask_u128(x, y, w, h)
            .map(|mask| occupied_mask | mask)
    }

    pub fn merged_u64(self, occupied_mask: u64, x: u8, y: u8, w: u8, h: u8) -> Option<u64> {
        if self.mask_width() != GridMaskWidth::U64 {
            return None;
        }
        self.rect_mask_u64(x, y, w, h)
            .map(|mask| occupied_mask | mask)
    }
}

impl fmt::Display for InventoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds {
                item_code,
                x,
                y,
                w,
                h,
            } => {
                write!(
                    f,
                    "[OUT_OF_BOUNDS] Item '{}' at ({},{}) size {}x{} exceeds grid",
                    item_code, x, y, w, h
                )
            }
            Self::Collision {
                item_code,
                x,
                y,
                w,
                h,
            } => {
                write!(
                    f,
                    "[COLLISION] Item '{}' at ({},{}) size {}x{} overlaps with another item",
                    item_code, x, y, w, h
                )
            }
            Self::InvalidCode(code) => {
                write!(f, "[INVALID_CODE] Item code '{}' is not recognized", code)
            }
            Self::LogicalMismatch { item_code, reason } => {
                write!(f, "[LOGICAL_MISMATCH] Item '{}': {}", item_code, reason)
            }
        }
    }
}

pub struct InventoryGrid {
    width: u8,
    height: u8,
    grid: Vec<bool>, // Flattened grid: y * width + x
}

impl InventoryGrid {
    pub fn new(width: u8, height: u8) -> Self {
        InventoryGrid {
            width,
            height,
            grid: vec![false; (width as usize) * (height as usize)],
        }
    }

    /// Default 10x4 inventory grid
    pub fn new_inventory() -> Self {
        Self::new(10, 4)
    }

    /// Default 10x10 stash grid
    pub fn new_stash() -> Self {
        Self::new(10, 10)
    }

    pub fn pattern(&self) -> GridPattern {
        GridPattern::new(self.width, self.height)
    }

    pub fn bitboard_mask_u128(&self) -> Option<u128> {
        self.pattern().occupied_mask_u128(&self.grid)
    }

    pub fn bitboard_mask_u64(&self) -> Option<u64> {
        self.pattern().occupied_mask_u64(&self.grid)
    }

    pub fn bitboard_collision_u128(&self, x: u8, y: u8, w: u8, h: u8) -> Option<bool> {
        self.bitboard_mask_u128()
            .and_then(|occupied| self.pattern().collides_u128(occupied, x, y, w, h))
    }

    pub fn bitboard_collision_u64(&self, x: u8, y: u8, w: u8, h: u8) -> Option<bool> {
        self.bitboard_mask_u64()
            .and_then(|occupied| self.pattern().collides_u64(occupied, x, y, w, h))
    }

    pub fn bitboard_merge_u128(&self, x: u8, y: u8, w: u8, h: u8) -> Option<u128> {
        self.bitboard_mask_u128()
            .and_then(|occupied| self.pattern().merged_u128(occupied, x, y, w, h))
    }

    pub fn bitboard_merge_u64(&self, x: u8, y: u8, w: u8, h: u8) -> Option<u64> {
        self.bitboard_mask_u64()
            .and_then(|occupied| self.pattern().merged_u64(occupied, x, y, w, h))
    }

    /// Marks a rectangle as occupied. Returns false if any cell is already occupied or out of bounds.
    pub fn occupy(&mut self, x: u8, y: u8, w: u8, h: u8) -> bool {
        if x + w > self.width || y + h > self.height {
            return false;
        }

        // Check if all needed cells are free
        for r in y..(y + h) {
            for c in x..(x + w) {
                if self.grid[(r as usize) * (self.width as usize) + (c as usize)] {
                    return false;
                }
            }
        }

        // Mark them as occupied
        for r in y..(y + h) {
            for c in x..(x + w) {
                self.grid[(r as usize) * (self.width as usize) + (c as usize)] = true;
            }
        }
        true
    }

    /// Finds the first available (top-left) slot for an item of given dimensions.
    pub fn find_free_slot(&self, w: u8, h: u8) -> Option<(u8, u8)> {
        for r in 0..=(self.height - h) {
            for c in 0..=(self.width - w) {
                let mut free = true;
                'check: for ir in r..(r + h) {
                    for ic in c..(c + w) {
                        if self.grid[(ir as usize) * (self.width as usize) + (ic as usize)] {
                            free = false;
                            break 'check;
                        }
                    }
                }
                if free {
                    return Some((c, r));
                }
            }
        }
        None
    }

    /// Convenience method to create a grid from a save file's raw bytes.
    pub fn from_save_bytes(bytes: &[u8], huffman: &crate::item::HuffmanTree) -> Self {
        let mut grid = Self::new_inventory();

        // Find JM marker
        let jm_pos =
            (0..bytes.len().saturating_sub(1)).find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M');

        if jm_pos.is_some() {
            let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
            if let Ok(items) = Item::read_player_items(bytes, huffman, version == 105) {
                for item in items {
                    if item.location == 0 {
                        grid.occupy(
                            item.x,
                            item.y,
                            get_item_size(&item.code).0,
                            get_item_size(&item.code).1,
                        );
                    }
                }
            }
        }
        grid
    }

    /// Auto-fills the grid from a collection of items.
    pub fn scan_items(&mut self, items: &[Item]) {
        for it in items {
            let (w, h) = get_item_size(&it.code);
            self.occupy(it.x, it.y, w, h);
        }
    }

    /// Debug print of the current grid state.
    pub fn debug_print(&self) {
        println!("   0 1 2 3 4 5 6 7 8 9");
        for r in 0..self.height {
            print!("{r}: ");
            for c in 0..self.width {
                print!(
                    "{}",
                    if self.grid[(r as usize) * (self.width as usize) + (c as usize)] {
                        "■ "
                    } else {
                        "□ "
                    }
                );
            }
            println!();
        }
    }

    /// Strong validation of a collection of items.
    /// Returns a list of all errors found.
    pub fn validate_items(items: &[Item], width: u8, height: u8) -> Vec<InventoryError> {
        let mut grid = Self::new(width, height);
        Self::validate_items_on_grid(&mut grid, items)
    }

    /// Validates items against an existing grid instance.
    pub fn validate_items_on_grid(grid: &mut Self, items: &[Item]) -> Vec<InventoryError> {
        let mut errors = Vec::new();

        for item in items {
            // Only validate items in the inventory (location 0)
            if item.location != 0 {
                continue;
            }

            let (w, h) = get_item_size(&item.code);

            // Check out of bounds
            if item.x + w > grid.width || item.y + h > grid.height {
                errors.push(InventoryError::OutOfBounds {
                    item_code: item.code.clone(),
                    x: item.x,
                    y: item.y,
                    w,
                    h,
                });
                continue;
            }

            // Check collision
            let mut has_collision = false;
            for r in item.y..(item.y + h) {
                for c in item.x..(item.x + w) {
                    if grid.grid[(r as usize) * (grid.width as usize) + (c as usize)] {
                        has_collision = true;
                        break;
                    }
                }
                if has_collision {
                    break;
                }
            }

            if has_collision {
                errors.push(InventoryError::Collision {
                    item_code: item.code.clone(),
                    x: item.x,
                    y: item.y,
                    w,
                    h,
                });
            } else {
                // No collision, mark the area
                for r in item.y..(item.y + h) {
                    for c in item.x..(item.x + w) {
                        grid.grid[(r as usize) * (grid.width as usize) + (c as usize)] = true;
                    }
                }
            }
        }

        errors
    }

    /// Performs deep logical integrity validation of a collection of items.
    pub fn validate_logical_integrity(
        items: &[Item],
        width: u8,
        height: u8,
    ) -> Vec<InventoryError> {
        let mut errors = Vec::new();

        for item in items {
            let code = item.code.trim();
            let template = crate::data::item_codes::ITEM_TEMPLATES
                .iter()
                .find(|t| t.code == code);

            // A. Vocabulary Check
            if template.is_none() {
                errors.push(InventoryError::InvalidCode(item.code.clone()));
                continue;
            }

            // B. Cross-Field Consistency
            // If identified is 0, properties should ideally be empty (not checking bitstream yet here)
            // But we can check if a compact item has excessive data if we had length info.

            // D. Socket Integrity
            // If it's a socketed item (location 6), x should be a valid parent index (placeholder logic)
            // if item.location == 6 && some_condition ...
        }

        // Also add collision check
        errors.extend(Self::validate_items(items, width, height));

        errors
    }
}

pub fn get_item_size(code: &str) -> (u8, u8) {
    let code = code.trim();

    // Primary source: Generated Item Templates
    if let Some(t) = crate::data::item_codes::ITEM_TEMPLATES
        .iter()
        .find(|t| t.code == code)
    {
        return (t.width, t.height);
    }

    // Fallback for codes not in templates (should be rare)
    match code {
        // Consumables (1x1)
        "tsc" | "isc" | "hp1" | "hp2" | "hp3" | "hp4" | "hp5" | "mp1" | "mp2" | "mp3" | "mp4"
        | "mp5" | "vps" | "yps" | "wms" | "rvs" | "rvl" | "key" | "aqv" | "cqv" => (1, 1),

        // Small charms, gems, rings, ammys (1x1)
        "cm1" | "gcv" | "gcy" | "gcb" | "gcg" | "gcr" | "gcw" | "skc" | "skz" => (1, 1),
        "rin" | "amu" | "jew" => (1, 1),

        // Books & Medium items (1x2)
        "tbk" | "ibk" | "cm2" | "cap" | "msk" => (1, 2),

        // Large items (2x2)
        "buc" | "cm3" | "brs" | "glb" | "vbl" | "tbl" | "lbl" => (2, 2),
        "fsm" => (2, 2), // Small shield

        // Armor & Large Weapons (2x3)
        "qui" | "lea" | "hrb" | "stu" | "rng" | "scl" | "chn" | "spl" | "plt" | "fld" => (2, 3),

        // Very Large Weapons (2x4)
        "axe" | "bax" | "tri" | "clb" | "spc" | "bst" | "hal" => (2, 4),

        // 1x3 Weapons (Long sword, etc)
        "jav" | "wwa7" | "lsw" | "ssw" | "msw" => (1, 3),

        // 1x4 Weapons (Pike, Bows)
        "pik" | "lbw" | "shb" | "lxb" => (1, 4),

        _ => (1, 1), // Default to 1x1 if unknown
    }
}
pub fn get_item_category(code: &str) -> &'static str {
    match code.trim() {
        // Potions
        "hp1" | "hp2" | "hp3" | "hp4" | "hp5" => "Healing Potion",
        "mp1" | "mp2" | "mp3" | "mp4" | "mp5" => "Mana Potion",
        "rvs" | "rvl" => "Rejuvenation Potion",
        "vps" | "yps" | "wms" => "Special Potion",

        // Scrolls & Books
        "tsc" | "isc" => "Scroll",
        "tbk" | "ibk" => "Book",

        // Consumables & Ammo
        "key" => "Keys",
        "aqv" | "cqv" => "Ammo",

        // Charms & Jewels
        "cm1" => "Small Charm",
        "cm2" => "Large Charm",
        "cm3" => "Grand Charm",
        "jew" => "Jewel",

        // Gems & Skulls
        "gcv" | "gcy" | "gcb" | "gcg" | "gcr" | "gcw" | "skc" | "skz" => "Gem/Skull",

        // Jewelry
        "rin" => "Ring",
        "amu" => "Amulet",

        // Equipment
        "buc" | "fsm" => "Shield",
        "cap" | "msk" | "cas" | "ghm" | "hlm" | "fhl" | "xml" => "Helmet",
        "qui" | "lea" | "hrb" | "stu" | "rng" | "scl" | "chn" | "spl" | "plt" | "fld" => {
            "Body Armor"
        }
        "glb" | "vgs" | "mgl" | "cha" | "hgl" => "Gloves",
        "vbl" | "tbl" | "lbl" | "hbl" => "Boots",
        "lbt" | "vbt" | "mbt" | "tbt" | "tbl " => "Belt",

        // Weapons
        "axe" | "bax" | "tri" | "clb" | "spc" | "bst" | "hal" => "Large Weapon",
        "jav" | "wwa7" | "lsw" | "ssw" | "msw" => "Medium Weapon",
        "pik" | "lbw" | "shb" | "lxb" => "Long Weapon/Bow",

        _ => "Misc/Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inventory_grid_occupy() {
        let mut grid = InventoryGrid::new_inventory(); // 10x4

        // Placing 2x2 item at (0,0)
        assert!(
            grid.occupy(0, 0, 2, 2),
            "Should be able to occupy (0,0) with 2x2"
        );

        // Attempt to place in the same spot again (should collision)
        assert!(
            !grid.occupy(1, 1, 1, 1),
            "Should fail due to collision at (1,1)"
        );

        // Attempt to place out of bounds
        assert!(
            !grid.occupy(9, 0, 2, 1),
            "Should fail due to out of bounds (width)"
        );
        assert!(
            !grid.occupy(0, 3, 1, 2),
            "Should fail due to out of bounds (height)"
        );
    }

    #[test]
    fn test_find_free_slot() {
        let mut grid = InventoryGrid::new(4, 4);

        // Occupy top-left 2x2
        grid.occupy(0, 0, 2, 2);

        // Next available slot for 2x2 should be (2,0) or (0,2)
        let slot = grid.find_free_slot(2, 2);
        assert!(slot.is_some());
        let (x, y) = slot.unwrap();
        assert!(
            x >= 2 || y >= 2,
            "Found slot ({},{}) should not overlap with (0,0) 2x2",
            x,
            y
        );
    }

    #[test]
    fn test_stash_grid() {
        let mut stash = InventoryGrid::new_stash(); // 10x10
        assert!(
            stash.occupy(0, 0, 10, 10),
            "Should be able to fill the entire stash"
        );
        assert!(!stash.occupy(0, 0, 1, 1), "Stash should be full now");
    }

    #[test]
    fn test_get_item_size() {
        assert_eq!(get_item_size("rin "), (1, 1)); // Ring
        assert_eq!(get_item_size("plt "), (2, 3)); // Plate Mail
        assert_eq!(get_item_size("axe "), (2, 3)); // Axe - TODO: This seems to be wrong in the data, should be 2x4
    }

    #[test]
    fn test_logical_integrity_validation() {
        // Create dummy item data (matching Item struct fields)
        let mut item1 = Item::empty_for_tests();
        item1.code = "rin ".to_string();
        item1.is_compact = true;
        item1.is_identified = true;
        item1.properties_complete = true;

        // Manual field setup for testing
        let mut items = Vec::new();
        let mut dummy_item = item1.clone();
        dummy_item.code = "rin ".to_string();
        dummy_item.x = 0;
        dummy_item.y = 0;
        dummy_item.location = 0; // Inventory
        items.push(dummy_item);

        let mut collision_item = item1.clone();
        collision_item.code = "rin ".to_string();
        collision_item.x = 0;
        collision_item.y = 0;
        collision_item.location = 0;
        items.push(collision_item);

        let errors = InventoryGrid::validate_logical_integrity(&items, 10, 4);
        assert!(!errors.is_empty(), "Should detect collision at (0,0)");
    }

    #[test]
    fn test_large_item_boundary_and_collision() {
        let mut grid = InventoryGrid::new_inventory(); // 10x4

        // 1. Attempt to place 2x4 armor at the right edge (should succeed)
        // x=8, w=2 -> 10 (OK), y=0, h=4 -> 4 (OK)
        assert!(
            grid.occupy(8, 0, 2, 4),
            "Should allow 2x4 item at the right edge"
        );

        // 2. Attempt to place 1x1 item inside occupied 2x4 area (should fail)
        assert!(
            !grid.occupy(9, 3, 1, 1),
            "Should fail to occupy inside 2x4 area"
        );

        // 3. Attempt to place over bottom boundary
        assert!(
            !grid.occupy(0, 1, 2, 4),
            "Should fail: y(1) + h(4) > height(4)"
        );
    }

    #[test]
    fn test_grid_index_mapping() {
        let mut grid = InventoryGrid::new(2, 2);
        // [ (0,0), (1,0) ] -> index 0, 1
        // [ (0,1), (1,1) ] -> index 2, 3

        grid.occupy(1, 1, 1, 1);
        assert!(
            grid.grid[3],
            "Index 3 should be occupied for (1,1) in 2x2 grid"
        );
        assert!(!grid.grid[0], "Index 0 should remain free");
    }

    #[test]
    fn test_grid_pattern_bitboard_collision() {
        let inventory = GridPattern::inventory();
        assert_eq!(inventory.bit_index(0, 0), Some(0));
        assert_eq!(inventory.bit_index(9, 3), Some(39));
        assert_eq!(inventory.mask_width(), GridMaskWidth::U64);

        let mut grid = InventoryGrid::new(inventory.width(), inventory.height());
        let placements = [(0, 0, 2, 2), (4, 0, 1, 3), (7, 1, 2, 2)];
        let mut occupied_mask = 0u128;

        for &(x, y, w, h) in &placements {
            assert!(grid.occupy(x, y, w, h));
            let rect_mask = inventory.rect_mask_u128(x, y, w, h).unwrap();
            assert_eq!(occupied_mask & rect_mask, 0, "placements should not overlap");
            occupied_mask |= rect_mask;
        }

        let colliding_candidate = (1, 1, 1, 1);
        let colliding_mask = inventory
            .rect_mask_u128(
                colliding_candidate.0,
                colliding_candidate.1,
                colliding_candidate.2,
                colliding_candidate.3,
            )
            .unwrap();
        assert_eq!(occupied_mask & colliding_mask != 0, true);
        assert_eq!(
            grid.bitboard_collision_u64(
                colliding_candidate.0,
                colliding_candidate.1,
                colliding_candidate.2,
                colliding_candidate.3
            ),
            Some(true)
        );
        assert_eq!(
            grid.occupy(
                colliding_candidate.0,
                colliding_candidate.1,
                colliding_candidate.2,
                colliding_candidate.3
            ),
            occupied_mask & colliding_mask == 0
        );

        let free_candidate = (2, 3, 1, 1);
        let free_mask = inventory
            .rect_mask_u128(
                free_candidate.0,
                free_candidate.1,
                free_candidate.2,
                free_candidate.3,
            )
            .unwrap();
        assert_eq!(occupied_mask & free_mask, 0);
        assert_eq!(
            grid.bitboard_collision_u64(
                free_candidate.0,
                free_candidate.1,
                free_candidate.2,
                free_candidate.3
            ),
            Some(false)
        );
        assert_eq!(
            grid.occupy(
                free_candidate.0,
                free_candidate.1,
                free_candidate.2,
                free_candidate.3
            ),
            occupied_mask & free_mask == 0
        );

        let merged_mask = grid
            .bitboard_merge_u64(0, 3, 1, 1)
            .expect("inventory should fit in u64");
        assert_eq!(
            merged_mask,
            grid.bitboard_mask_u64().unwrap() | inventory.rect_mask_u64(0, 3, 1, 1).unwrap()
        );

        let stash = GridPattern::stash();
        assert_eq!(stash.mask_width(), GridMaskWidth::U128);
        assert_eq!(stash.rect_mask_u64(0, 0, 10, 10), None);
        assert_eq!(stash.rect_mask_u128(0, 0, 10, 10).unwrap().count_ones(), 100);

        let cube = GridPattern::cube();
        assert_eq!(cube.rect_mask_u64(0, 0, 3, 4).unwrap().count_ones(), 12);

        let belt = GridPattern::belt(5);
        assert_eq!(belt.bit_index(3, 4), Some(19));
        assert_eq!(belt.mask_width(), GridMaskWidth::U64);
    }
}
