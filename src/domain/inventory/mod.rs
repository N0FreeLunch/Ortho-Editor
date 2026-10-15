use crate::item::Item;
mod placement;
use placement::ItemPlacement;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerKind {
    Inventory,
    Stash,
    Cube,
}

impl fmt::Display for ContainerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inventory => write!(f, "Inventory (10x4)"),
            Self::Stash => write!(f, "Stash (10x10)"),
            Self::Cube => write!(f, "Horadric Cube (3x4)"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerIntegrityError {
    OutOfBounds {
        container: ContainerKind,
        item_code: String,
        x: u8,
        y: u8,
        w: u8,
        h: u8,
    },
    Collision {
        container: ContainerKind,
        item_a: String,
        item_b: String,
        x: u8,
        y: u8,
    },
    CubeDependency(CubeIntegrityError),
}

impl fmt::Display for ContainerIntegrityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds {
                container,
                item_code,
                x,
                y,
                w,
                h,
            } => {
                write!(
                    f,
                    "[{}] Item '{}' at ({},{}) size {}x{} exceeds boundary",
                    container, item_code, x, y, w, h
                )
            }
            Self::Collision {
                container,
                item_a,
                item_b,
                x,
                y,
            } => {
                write!(
                    f,
                    "[{}] Items '{}' and '{}' collide at ({},{})",
                    container, item_a, item_b, x, y
                )
            }
            Self::CubeDependency(err) => write!(f, "{}", err),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CubeIntegrityError {
    /// Cube items exist, but the player has no Horadric Cube container (`box`).
    CubeNotFoundWithContainedItems,
    /// Player possesses more than one Horadric Cube container.
    MultipleCubesDetected(usize),
    /// An item inside the cube exceeds the 3x4 grid boundaries.
    OutOfBounds {
        item_code: String,
        x: u8,
        y: u8,
        w: u8,
        h: u8,
    },
    /// Two items inside the cube collide/overlap in one or more grid cells.
    Collision {
        item_a: String,
        item_b: String,
        x: u8,
        y: u8,
    },
}

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
    CubeIntegrity(CubeIntegrityError),
    ContainerIntegrity(ContainerIntegrityError),
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
            Self::CubeIntegrity(err) => {
                write!(f, "[CUBE_INTEGRITY] {}", err)
            }
            Self::ContainerIntegrity(err) => {
                write!(f, "[CONTAINER_INTEGRITY] {}", err)
            }
        }
    }
}

impl fmt::Display for CubeIntegrityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CubeNotFoundWithContainedItems => {
                write!(f, "Cube items exist but character has no Horadric Cube ('box')")
            }
            Self::MultipleCubesDetected(count) => {
                write!(f, "Multiple Horadric Cubes detected ({count} found, max 1 allowed)")
            }
            Self::OutOfBounds { item_code, x, y, w, h } => {
                write!(f, "Item '{item_code}' at ({x},{y}) size {w}x{h} exceeds 3x4 Cube grid")
            }
            Self::Collision { item_a, item_b, x, y } => {
                write!(f, "Items '{item_a}' and '{item_b}' collide in Cube at ({x},{y})")
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

    /// Default 3x4 Horadric Cube container grid
    pub fn new_cube() -> Self {
        Self::new(3, 4)
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
        use crate::domain::item::stream::{ItemContainer, ItemStreamDialect, PlayerItemStream};

        let mut grid = Self::new_inventory();
        if let Ok(stream) = PlayerItemStream::read(bytes, huffman) {
            for item in &stream.items {
                let (x, y) = if stream.dialect == ItemStreamDialect::D2rV3 {
                    let placement = stream.placement(item);
                    if placement.container != ItemContainer::Inventory {
                        continue;
                    }
                    (placement.x, placement.y)
                } else {
                    // Preserve legacy projection until Alpha placement has independent authority.
                    if item.location != 0 {
                        continue;
                    }
                    (item.x, item.y)
                };
                let (width, height) = get_item_size(&item.code);
                grid.occupy(x, y, width, height);
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

        // Enforce Horadric Cube 4-invariants integrity harness
        if let Err(cube_err) = Self::validate_cube_integrity(items) {
            errors.push(InventoryError::CubeIntegrity(cube_err));
        }

        errors
    }

    /// Enforces the 4 Horadric Cube physical and logical invariants across an item collection with ZERO heap allocations:
    /// 1. Container Existence Dependency: Cube items cannot exist if player has no cube container (`box`).
    /// 2. Container Uniqueness: Player cannot possess multiple Horadric Cubes simultaneously.
    /// 3. 3x4 Grid Containment: All items inside cube must fit inside 3 columns x 4 rows.
    /// 4. 2D Mutual Exclusivity: No two items inside cube may overlap/collide in any grid cell (u16 bitmask).
    pub fn validate_cube_integrity(items: &[Item]) -> Result<(), CubeIntegrityError> {
        let mut cube_count = 0usize;
        let mut occupied_mask: u16 = 0;
        let mut cell_occupant: [Option<&str>; 12] = [None; 12];
        let mut cube_items_count = 0usize;

        for item in items {
            let it = ItemPlacement::new(item);
            if it.should_skip() {
                continue;
            }

            if it.is_cube_container() {
                cube_count += 1;
                if cube_count > 1 {
                    return Err(CubeIntegrityError::MultipleCubesDetected(cube_count));
                }
                continue;
            }

            if it.is_in_cube() {
                cube_items_count += 1;
                let (w, h) = it.size();
                let it_code = it.code();

                // Out-of-bounds check (x + w <= 3, y + h <= 4)
                if it.x() + w > 3 || it.y() + h > 4 {
                    return Err(CubeIntegrityError::OutOfBounds {
                        item_code: it_code.to_string(),
                        x: it.x(),
                        y: it.y(),
                        w,
                        h,
                    });
                }

                // Check collision using 12-bit register mask
                let mut item_mask = 0u16;
                for r in it.y()..(it.y() + h) {
                    for c in it.x()..(it.x() + w) {
                        let bit = (r as usize) * 3 + (c as usize);
                        item_mask |= 1u16 << bit;
                    }
                }

                if occupied_mask & item_mask != 0 {
                    for r in it.y()..(it.y() + h) {
                        for c in it.x()..(it.x() + w) {
                            let bit = (r as usize) * 3 + (c as usize);
                            if (occupied_mask & (1u16 << bit)) != 0 {
                                if let Some(existing_code) = cell_occupant[bit] {
                                    return Err(CubeIntegrityError::Collision {
                                        item_a: existing_code.to_string(),
                                        item_b: it_code.to_string(),
                                        x: c,
                                        y: r,
                                    });
                                }
                            }
                        }
                    }
                }

                occupied_mask |= item_mask;
                for r in it.y()..(it.y() + h) {
                    for c in it.x()..(it.x() + w) {
                        cell_occupant[(r as usize) * 3 + (c as usize)] = Some(it_code);
                    }
                }
            }
        }

        // Invariant 0: Container Existence Dependency
        if cube_count == 0 && cube_items_count > 0 {
            return Err(CubeIntegrityError::CubeNotFoundWithContainedItems);
        }

        Ok(())
    }

    /// Enforces Personal Stash physical invariants across all items stored in stash (`page == 1`):
    /// 1. 10x10 Grid Containment: All stash items must fit inside 10 columns x 10 rows (x + w <= 10, y + h <= 10).
    /// 2. 2D Mutual Exclusivity: No two stash items may overlap in any of the 100 cells (u128 bitmask).
    pub fn validate_stash_integrity(items: &[Item]) -> Result<(), ContainerIntegrityError> {
        let mut occupied_mask: u128 = 0;
        let mut cell_occupant: [Option<&str>; 100] = [None; 100];

        for item in items {
            let it = ItemPlacement::new(item);
            if it.should_skip() {
                continue;
            }

            if it.is_stash() {
                let (w, h) = it.size();
                let it_code = it.code();

                if it.x() + w > 10 || it.y() + h > 10 {
                    return Err(ContainerIntegrityError::OutOfBounds {
                        container: ContainerKind::Stash,
                        item_code: it_code.to_string(),
                        x: it.x(),
                        y: it.y(),
                        w,
                        h,
                    });
                }

                let mut item_mask = 0u128;
                for r in it.y()..(it.y() + h) {
                    for c in it.x()..(it.x() + w) {
                        let bit = (r as usize) * 10 + (c as usize);
                        item_mask |= 1u128 << bit;
                    }
                }

                if occupied_mask & item_mask != 0 {
                    for r in it.y()..(it.y() + h) {
                        for c in it.x()..(it.x() + w) {
                            let bit = (r as usize) * 10 + (c as usize);
                            if (occupied_mask & (1u128 << bit)) != 0 {
                                if let Some(existing_code) = cell_occupant[bit] {
                                    return Err(ContainerIntegrityError::Collision {
                                        container: ContainerKind::Stash,
                                        item_a: existing_code.to_string(),
                                        item_b: it_code.to_string(),
                                        x: c,
                                        y: r,
                                    });
                                }
                            }
                        }
                    }
                }

                occupied_mask |= item_mask;
                for r in it.y()..(it.y() + h) {
                    for c in it.x()..(it.x() + w) {
                        cell_occupant[(r as usize) * 10 + (c as usize)] = Some(it_code);
                    }
                }
            }
        }

        Ok(())
    }

    /// Enforces Player Inventory physical invariants across items placed in inventory (`page == 0 && location == 0`):
    /// 1. 10x4 Grid Containment: All inventory items must fit inside 10 columns x 4 rows (x + w <= 10, y + h <= 4).
    /// 2. 2D Mutual Exclusivity: No two inventory items may overlap in any of the 40 cells (u64 bitmask).
    pub fn validate_inventory_integrity(items: &[Item]) -> Result<(), ContainerIntegrityError> {
        let mut occupied_mask: u64 = 0;
        let mut cell_occupant: [Option<&str>; 40] = [None; 40];

        for item in items {
            let it = ItemPlacement::new(item);
            if it.should_skip() {
                continue;
            }

            if it.is_inventory() {
                let (w, h) = it.size();
                let it_code = it.code();

                if it.x() + w > 10 || it.y() + h > 4 {
                    return Err(ContainerIntegrityError::OutOfBounds {
                        container: ContainerKind::Inventory,
                        item_code: it_code.to_string(),
                        x: it.x(),
                        y: it.y(),
                        w,
                        h,
                    });
                }

                let mut item_mask = 0u64;
                for r in it.y()..(it.y() + h) {
                    for c in it.x()..(it.x() + w) {
                        let bit = (r as usize) * 10 + (c as usize);
                        item_mask |= 1u64 << bit;
                    }
                }

                if occupied_mask & item_mask != 0 {
                    for r in it.y()..(it.y() + h) {
                        for c in it.x()..(it.x() + w) {
                            let bit = (r as usize) * 10 + (c as usize);
                            if (occupied_mask & (1u64 << bit)) != 0 {
                                if let Some(existing_code) = cell_occupant[bit] {
                                    return Err(ContainerIntegrityError::Collision {
                                        container: ContainerKind::Inventory,
                                        item_a: existing_code.to_string(),
                                        item_b: it_code.to_string(),
                                        x: c,
                                        y: r,
                                    });
                                }
                            }
                        }
                    }
                }

                occupied_mask |= item_mask;
                for r in it.y()..(it.y() + h) {
                    for c in it.x()..(it.x() + w) {
                        cell_occupant[(r as usize) * 10 + (c as usize)] = Some(it_code);
                    }
                }
            }
        }

        Ok(())
    }

    /// Enforces all container spatial and logical invariants (Inventory, Stash, Cube) in a single unified sweep.
    pub fn validate_all_containers_integrity(items: &[Item]) -> Result<(), ContainerIntegrityError> {
        // 1. Horadric Cube Invariants
        if let Err(cube_err) = Self::validate_cube_integrity(items) {
            return Err(ContainerIntegrityError::CubeDependency(cube_err));
        }

        // 2. Personal Stash Invariants
        Self::validate_stash_integrity(items)?;

        // 3. Player Inventory Invariants
        Self::validate_inventory_integrity(items)?;

        Ok(())
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
    let code = code.trim();
    if let Some(template) = crate::data::item_codes::ITEM_TEMPLATES
        .iter()
        .find(|template| template.code == code)
    {
        let types = crate::engine::validation::get_all_item_types(code);
        if template.is_armor {
            for (item_type, category) in [
                ("glov", "Gloves"),
                ("boot", "Boots"),
                ("belt", "Belt"),
                ("helm", "Helmet"),
                ("shld", "Shield"),
                ("tors", "Body Armor"),
            ] {
                if types.contains(&item_type) {
                    return category;
                }
            }
            return "Armor";
        }
        if template.is_weapon {
            if types.contains(&"tpot") {
                return "Throwing Potion";
            }
            if types.contains(&"bow") || types.contains(&"xbow") {
                return "Long Weapon/Bow";
            }
            // Preserve established coarse weapon labels when source data confirms a weapon.
            return match legacy_item_category(code) {
                category @ ("Large Weapon" | "Medium Weapon" | "Long Weapon/Bow") => category,
                _ => "Weapon",
            };
        }
        // Rejuvenation inherits both healing and mana; classify the subtype first.
        for (item_type, category) in [
            ("rpot", "Rejuvenation Potion"),
            ("hpot", "Healing Potion"),
            ("mpot", "Mana Potion"),
            ("poti", "Special Potion"),
            ("scro", "Scroll"),
            ("book", "Book"),
            ("key", "Keys"),
            ("bowq", "Ammo"),
            ("xboq", "Ammo"),
            ("scha", "Small Charm"),
            ("mcha", "Large Charm"),
            ("lcha", "Grand Charm"),
            ("char", "Charm"),
            ("jewl", "Jewel"),
            ("gem", "Gem/Skull"),
            ("rune", "Rune"),
            ("ring", "Ring"),
            ("amul", "Amulet"),
            ("ques", "Quest Item"),
            ("gold", "Gold"),
            ("body", "Body Part"),
            ("play", "Body Part"),
            ("misc", "Miscellaneous"),
        ] {
            if types.contains(&item_type) {
                return category;
            }
        }
    }
    // Codes without current type authority retain the existing compatibility fallback.
    legacy_item_category(code)
}

fn legacy_item_category(code: &str) -> &'static str {
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

    #[test]
    fn test_cube_integrity_invariants_harness() {
        use crate::domain::header::entity::ItemHeader;
        use crate::domain::item::entity::ItemBody;
        use crate::domain::vo::ContainerLocation;

        fn make_mock_item(code: &str, page: u8, x: u8, y: u8) -> Item {
            let mut header = ItemHeader::default();
            header.flags = 0x10; // Valid item flag
            header.page = page;
            header.x = x;
            header.y = y;
            Item {
                bits: Vec::new(),
                code: code.to_string(),
                header,
                body: ItemBody {
                    code: code.to_string(),
                    page,
                    x,
                    y,
                    ..Default::default()
                },
                ..Default::default()
            }
        }

        // 1. Invariant 0: Cube items exist but no cube container
        let item_in_cube = make_mock_item("tsc", ContainerLocation::CUBE_PAGE, 0, 0); // 1x1 scroll
        let res_no_cube = InventoryGrid::validate_cube_integrity(&[item_in_cube.clone()]);
        assert_eq!(
            res_no_cube,
            Err(CubeIntegrityError::CubeNotFoundWithContainedItems)
        );

        // 2. Invariant 1: Multiple cubes detected
        let cube1 = make_mock_item("box", 0, 0, 0);
        let cube2 = make_mock_item("box", 0, 2, 2);
        let res_multi_cubes = InventoryGrid::validate_cube_integrity(&[cube1.clone(), cube2]);
        assert_eq!(
            res_multi_cubes,
            Err(CubeIntegrityError::MultipleCubesDetected(2))
        );

        // 3. Valid cube with valid items
        let item1 = make_mock_item("tsc", ContainerLocation::CUBE_PAGE, 0, 0); // 1x1 at (0,0)
        let item2 = make_mock_item("tbk", ContainerLocation::CUBE_PAGE, 1, 0); // 1x2 at (1,0)
        let res_valid = InventoryGrid::validate_cube_integrity(&[cube1.clone(), item1, item2]);
        assert_eq!(res_valid, Ok(()));

        // 4. Invariant 2: Out of bounds (e.g. 2x2 item at x=2 in 3x4 cube exceeds width 3)
        let oob_item = make_mock_item("buc", ContainerLocation::CUBE_PAGE, 2, 0); // 2x2 buckler at (2,0) -> x+w = 4 > 3
        let res_oob = InventoryGrid::validate_cube_integrity(&[cube1.clone(), oob_item]);
        assert_eq!(
            res_oob,
            Err(CubeIntegrityError::OutOfBounds {
                item_code: "buc".to_string(),
                x: 2,
                y: 0,
                w: 2,
                h: 2,
            })
        );

        // 5. Invariant 3: Collision / Overlap between items inside cube
        let item_a = make_mock_item("buc", ContainerLocation::CUBE_PAGE, 0, 0); // 2x2 at (0,0) occupies (0,0),(1,0),(0,1),(1,1)
        let item_b = make_mock_item("tsc", ContainerLocation::CUBE_PAGE, 1, 1); // 1x1 at (1,1) collides with buc
        let res_collision = InventoryGrid::validate_cube_integrity(&[cube1, item_a, item_b]);
        assert_eq!(
            res_collision,
            Err(CubeIntegrityError::Collision {
                item_a: "buc".to_string(),
                item_b: "tsc".to_string(),
                x: 1,
                y: 1,
            })
        );
    }

    #[test]
    fn test_stash_and_inventory_integrity_harness() {
        use crate::domain::header::entity::ItemHeader;
        use crate::domain::item::entity::ItemBody;

        fn make_mock_item(code: &str, page: u8, location: u8, x: u8, y: u8) -> Item {
            let mut header = ItemHeader::default();
            header.flags = 0x10; // Valid item flag
            header.mode = 4; // Stored in container
            header.page = page;
            header.location = location;
            header.x = x;
            header.y = y;
            Item {
                bits: Vec::new(),
                code: code.to_string(),
                header,
                body: ItemBody {
                    code: code.to_string(),
                    mode: 4,
                    page,
                    location,
                    x,
                    y,
                    ..Default::default()
                },
                ..Default::default()
            }
        }

        // 1. Personal Stash Out-of-bounds (10x10)
        let oob_stash_item = make_mock_item("axe", 1, 0, 9, 8); // 2x4 axe at (9,8) in 10x10 exceeds x+w=11
        let res_stash_oob = InventoryGrid::validate_stash_integrity(&[oob_stash_item]);
        assert_eq!(
            res_stash_oob,
            Err(ContainerIntegrityError::OutOfBounds {
                container: ContainerKind::Stash,
                item_code: "axe".to_string(),
                x: 9,
                y: 8,
                w: 2,
                h: 3, // template size
            })
        );

        // 2. Personal Stash Collision
        let stash_a = make_mock_item("buc", 1, 0, 0, 0); // 2x2
        let stash_b = make_mock_item("tsc", 1, 0, 1, 1); // 1x1 at (1,1)
        let res_stash_col = InventoryGrid::validate_stash_integrity(&[stash_a, stash_b]);
        assert_eq!(
            res_stash_col,
            Err(ContainerIntegrityError::Collision {
                container: ContainerKind::Stash,
                item_a: "buc".to_string(),
                item_b: "tsc".to_string(),
                x: 1,
                y: 1,
            })
        );

        // 3. Player Inventory Out-of-bounds (10x4)
        let oob_inv_item = make_mock_item("axe", 0, 0, 0, 2); // 2x3 axe at (0,2) in 10x4 exceeds y+h=5 > 4
        let res_inv_oob = InventoryGrid::validate_inventory_integrity(&[oob_inv_item]);
        assert_eq!(
            res_inv_oob,
            Err(ContainerIntegrityError::OutOfBounds {
                container: ContainerKind::Inventory,
                item_code: "axe".to_string(),
                x: 0,
                y: 2,
                w: 2,
                h: 3,
            })
        );

        // 4. Valid Unified Containers
        let inv_valid = make_mock_item("tsc", 0, 0, 0, 0);
        let stash_valid = make_mock_item("tbk", 1, 0, 0, 0);
        let cube_box = make_mock_item("box", 0, 0, 5, 0);
        let cube_item = make_mock_item("tsc", 3, 0, 0, 0);
        let res_all = InventoryGrid::validate_all_containers_integrity(&[inv_valid, stash_valid, cube_box, cube_item]);
        assert_eq!(res_all, Ok(()));
    }
}

#[cfg(test)]
mod source_category_contract_tests {
    use super::get_item_category;
    use d2r_macros::settled_contract_test;

    #[settled_contract_test]
    fn captured_gameplay_codes_use_source_equipment_and_item_classes() {
        for (code, expected) in [
            ("ktr", "Weapon"),
            ("wrb", "Weapon"),
            ("sst", "Weapon"),
            ("spr", "Weapon"),
            ("ob1", "Weapon"),
            ("wnd", "Weapon"),
            ("mac", "Weapon"),
            ("gpl", "Throwing Potion"),
            ("hbw", "Long Weapon/Bow"),
            ("hla", "Body Armor"),
            ("brs", "Body Armor"),
            ("vgl", "Gloves"),
            ("mbl", "Belt"),
            ("skp", "Helmet"),
            ("dr1", "Helmet"),
            ("tow", "Shield"),
            ("box", "Quest Item"),
        ] {
            assert_eq!(get_item_category(code), expected, "{code}");
        }
    }

    #[settled_contract_test]
    fn armor_source_families_keep_belts_boots_and_class_specific_descendants_distinct() {
        let groups = [
            ("glov", "Gloves"),
            ("boot", "Boots"),
            ("belt", "Belt"),
            ("helm", "Helmet"),
            ("circ", "Helmet"),
            ("pelt", "Helmet"),
            ("phlm", "Helmet"),
            ("shie", "Shield"),
            ("ashd", "Shield"),
            ("head", "Shield"),
            ("tors", "Body Armor"),
        ];
        for (source_type, expected) in groups {
            let members: Vec<_> = crate::data::item_codes::ITEM_TEMPLATES
                .iter()
                .filter(|template| template.item_type == Some(source_type))
                .collect();
            assert!(
                members.len() >= 3,
                "normal and upgraded source family {source_type}"
            );
            for template in members {
                assert!(template.is_armor);
                assert_eq!(
                    get_item_category(template.code),
                    expected,
                    "{}",
                    template.code
                );
            }
        }
    }

    #[settled_contract_test]
    fn potion_subtype_precedes_both_parents_and_socketables_keep_their_source_classes() {
        for (code, expected) in [
            ("rvs", "Rejuvenation Potion"),
            ("rvl", "Rejuvenation Potion"),
            ("hp1", "Healing Potion"),
            ("mp1", "Mana Potion"),
            ("vps", "Special Potion"),
            ("r01", "Rune"),
            ("r33", "Rune"),
            ("gpr", "Gem/Skull"),
            ("cm1", "Small Charm"),
            ("cm2", "Large Charm"),
            ("cm3", "Grand Charm"),
            ("jew", "Jewel"),
            ("rin", "Ring"),
            ("amu", "Amulet"),
            ("tsc", "Scroll"),
            ("tbk", "Book"),
            ("aqv", "Ammo"),
            ("cqv", "Ammo"),
            ("key", "Keys"),
            ("gld", "Gold"),
        ] {
            assert_eq!(get_item_category(code), expected, "{code}");
        }
    }

    #[settled_contract_test]
    fn missing_source_context_retains_compatibility_without_inventing_item_classes() {
        assert_eq!(get_item_category(" vgl "), "Gloves");
        assert_eq!(get_item_category(" lbt "), "Boots");
        assert_eq!(get_item_category(" tbl "), "Belt");
        for (code, expected) in [
            ("wwa7", "Medium Weapon"),
            ("ssw", "Medium Weapon"),
            ("fsm", "Shield"),
            ("not-a-code", "Misc/Unknown"),
            ("", "Misc/Unknown"),
            ("VGL", "Misc/Unknown"),
        ] {
            assert_eq!(get_item_category(code), expected, "{code}");
        }
    }
}

#[cfg(test)]
mod inventory_loader_contract_tests {
    use super::InventoryGrid;
    use crate::domain::item::stream::{ItemContainer, ItemStreamDialect, PlayerItemStream};
    use crate::item::HuffmanTree;
    use d2r_macros::settled_contract_test;

    fn fixture(relative: &str) -> Vec<u8> {
        let root = std::env::var_os("D2R_CORE_PATH")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        std::fs::read(root.join("tests/fixtures/savegames").join(relative)).unwrap()
    }

    fn grid_mask(bytes: &[u8]) -> u64 {
        InventoryGrid::from_save_bytes(bytes, &HuffmanTree::new())
            .bitboard_mask_u64()
            .unwrap()
    }

    #[settled_contract_test]
    fn captured_v3_inventory_matches_raw_header_masks_without_belt_or_stash_cells() {
        // Masks come from source mode/storage coordinates and raw item dimensions.
        let cases = [
            (
                "gameplay/normal/act1/TESTASSASSIN_Act1_FreshStart.d2s",
                0x8020000000,
            ),
            ("original/amazon_lvl2_progression_complex.d2s", 0x0000000c03),
        ];
        for (file, expected) in cases {
            assert_eq!(grid_mask(&fixture(file)), expected, "{file}");
        }
    }

    #[settled_contract_test]
    fn selected_alpha_retains_legacy_grid_projection() {
        // Alpha potion placement remains unvalidated; retain the existing public mask.
        let bytes = fixture("original/amazon_authority_runeword.d2s");
        let stream = PlayerItemStream::read(&bytes, &HuffmanTree::new()).unwrap();
        assert_eq!(stream.dialect, ItemStreamDialect::Alpha);
        assert_eq!(grid_mask(&bytes), 0x0006018071);
    }

    #[settled_contract_test]
    fn controlled_storage_header_excludes_cube_stash_and_unknown_from_inventory() {
        let original = fixture("gameplay/normal/act1/TESTASSASSIN_Act1_FreshStart.d2s");
        let huffman = HuffmanTree::new();
        let stream = PlayerItemStream::read(&original, &huffman).unwrap();
        assert_eq!(stream.dialect, ItemStreamDialect::D2rV3);
        let scroll = &stream.items[4];
        assert_eq!(scroll.code.trim(), "tsc");
        let storage_start = scroll.range.start + 50;
        for (storage, container) in [
            (0u8, ItemContainer::Unknown),
            (1, ItemContainer::Inventory),
            (4, ItemContainer::Cube),
            (5, ItemContainer::Stash),
        ] {
            let mut changed = original.clone();
            for bit in 0..3 {
                let offset = storage_start + bit;
                let byte = &mut changed[(offset / 8) as usize];
                *byte = (*byte & !(1 << (offset % 8))) | (((storage >> bit) & 1) << (offset % 8));
            }
            let reparsed = PlayerItemStream::read(&changed, &huffman).unwrap();
            assert_eq!(reparsed.dialect, ItemStreamDialect::D2rV3);
            assert_eq!(reparsed.items.len(), stream.items.len());
            assert_eq!(reparsed.items[4].range, scroll.range);
            assert_eq!(reparsed.placement(&reparsed.items[4]).container, container);
            for (index, (before, after)) in original.iter().zip(&changed).enumerate() {
                let delta = before ^ after;
                for bit in 0..8 {
                    if delta & (1 << bit) != 0 {
                        let offset = index as u64 * 8 + bit;
                        assert!((storage_start..storage_start + 3).contains(&offset));
                    }
                }
            }
            // Controlled decoder input only; no game-load legality is asserted.
            let expected = if storage == 1 {
                0x8020000000
            } else {
                0x0020000000
            };
            assert_eq!(grid_mask(&changed), expected, "storage={storage}");
        }
    }

    #[settled_contract_test]
    fn short_or_invalid_save_headers_return_empty_grid_without_panicking() {
        for bytes in [
            &b""[..],
            &b"J"[..],
            &b"JM"[..],
            &b"JM00000"[..],
            &b"JM000000"[..],
        ] {
            assert_eq!(grid_mask(bytes), 0, "{bytes:?}");
        }
    }
}
