/// Item placement coordinates and container mapping.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemPlacement {
    pub container: u8,
    pub page: Option<u8>,
    pub x: u8,
    pub y: u8,
}
