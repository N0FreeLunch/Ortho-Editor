//! Value Object (VO) layer.
//! This module defines opaque types that encapsulate core domain invariants.
//! These types will later be shared with Elm via `elm-rs`.

use serde::{Serialize, Deserialize};

/// Opaque wrapper for Item Stat Values.
/// Protects against invalid stat boundaries and mutations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStatValue(i32);

impl ItemStatValue {
    pub const MIN_SAFE: i32 = -100_000;
    pub const MAX_SAFE: i32 = 100_000;

    pub fn new(val: i32) -> Result<Self, &'static str> {
        if val < Self::MIN_SAFE || val > Self::MAX_SAFE {
            return Err("Item stat value out of safe boundaries");
        }
        Ok(Self(val))
    }

    pub fn value(&self) -> i32 {
        self.0
    }
}

impl TryFrom<i32> for ItemStatValue {
    type Error = &'static str;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Opaque wrapper for Inventory Coordinates.
/// Ensures valid grid placement bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryCoordinate {
    x: u8,
    y: u8,
}

impl InventoryCoordinate {
    pub const MAX_X: u8 = 10;
    pub const MAX_Y: u8 = 10;

    pub fn new(x: u8, y: u8) -> Result<Self, &'static str> {
        if x > Self::MAX_X || y > Self::MAX_Y {
            return Err("Inventory coordinates out of bounds");
        }
        Ok(Self { x, y })
    }

    pub fn x(&self) -> u8 {
        self.x
    }

    pub fn y(&self) -> u8 {
        self.y
    }
}

impl TryFrom<(u8, u8)> for InventoryCoordinate {
    type Error = &'static str;
    fn try_from(value: (u8, u8)) -> Result<Self, Self::Error> {
        Self::new(value.0, value.1)
    }
}

/// Opaque wrapper for Item Size (Width x Height).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemSize {
    width: u8,
    height: u8,
}

impl ItemSize {
    pub const MAX_DIM: u8 = 4; // Typical D2 maximum size (e.g., Bows/Staves are 4 high)

    pub fn new(width: u8, height: u8) -> Result<Self, &'static str> {
        if width == 0 || height == 0 || width > Self::MAX_DIM || height > Self::MAX_DIM {
            return Err("Item size out of valid range");
        }
        Ok(Self { width, height })
    }

    pub fn width(&self) -> u8 {
        self.width
    }

    pub fn height(&self) -> u8 {
        self.height
    }
}

/// Represents a validated placement of an item in the inventory.
/// Guarantees that the item fits within the 10x10 grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryPlacement {
    coordinate: InventoryCoordinate,
    size: ItemSize,
}

impl InventoryPlacement {
    pub fn new(coordinate: InventoryCoordinate, size: ItemSize) -> Result<Self, &'static str> {
        // Check if the bottom-right corner exceeds inventory bounds
        // coordinate.x + size.width must be <= 10 (MAX_X)
        // coordinate.y + size.height must be <= 10 (MAX_Y)
        if (coordinate.x() as u16 + size.width() as u16) > InventoryCoordinate::MAX_X as u16
            || (coordinate.y() as u16 + size.height() as u16) > InventoryCoordinate::MAX_Y as u16
        {
            return Err("Item placement exceeds inventory boundaries");
        }
        Ok(Self { coordinate, size })
    }

    pub fn coordinate(&self) -> InventoryCoordinate {
        self.coordinate
    }

    pub fn size(&self) -> ItemSize {
        self.size
    }
}

/// Container identifier for item placement.
/// Covers character inventory, Horadric Cube, and Stash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerLocation {
    /// Placed on ground (mode 0)
    Ground,
    /// Equipped on character (mode 1)
    Equipped(u8),
    /// Belt column and row (mode 2)
    Belt,
    /// Inventory grid (mode 0/4, page 0)
    Inventory,
    /// Horadric Cube container grid (page 3 or alpha v105 3x4 sub-grid)
    HoradricCube,
    /// Character private stash (page 1)
    Stash,
}

impl ContainerLocation {
    /// In D2 / D2R item headers, page 3 represents the Horadric Cube container.
    pub const CUBE_PAGE: u8 = 3;
    /// Maximum dimensions of the Horadric Cube interior (3 columns x 4 rows).
    pub const CUBE_MAX_X: u8 = 3;
    pub const CUBE_MAX_Y: u8 = 4;

    /// Determines container location from header page, mode, and location.
    pub fn from_header(page: u8, mode: u8, location: u8) -> Self {
        if page == Self::CUBE_PAGE {
            ContainerLocation::HoradricCube
        } else if page == 1 {
            ContainerLocation::Stash
        } else if mode == 1 {
            ContainerLocation::Equipped(location)
        } else if mode == 2 {
            ContainerLocation::Belt
        } else if mode == 0 {
            ContainerLocation::Ground
        } else {
            ContainerLocation::Inventory
        }
    }
}

/// Aligns a bit position to the next byte boundary.
/// 0 remains 0, 1-8 becomes 8, 9-16 becomes 16.
pub fn align_to_byte(bit_pos: u64) -> u64 {
    (bit_pos + 7) & !7
}


