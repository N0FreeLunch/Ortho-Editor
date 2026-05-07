use crate::domain::item::Item;
use crate::domain::vo::ItemStatValue;

/// High-level Editor API for Item mutation.
/// This provides a safe, encapsulated interface for modifying items
/// while maintaining internal consistency.
pub struct ItemEditor<'a> {
    item: &'a mut Item,
}

impl<'a> ItemEditor<'a> {
    pub fn new(item: &'a mut Item) -> Self {
        Self { item }
    }

    /// Sets the item's defense value.
    pub fn set_defense(&mut self, value: u32) -> &mut Self {
        self.item.set_defense(Some(value));
        self
    }

    /// Sets the current and maximum durability.
    pub fn set_durability(&mut self, current: u32, max: u32) -> &mut Self {
        self.item.set_durability(Some(current), Some(max));
        self
    }

    /// Sets the stack quantity for stackable items.
    pub fn set_quantity(&mut self, value: u32) -> &mut Self {
        self.item.set_quantity(Some(value));
        self
    }

    /// Sets or updates a specific stat value by its ID.
    /// Handles Alpha v105 stat mapping internally.
    pub fn set_stat(&mut self, stat_id: u32, value: i32) -> &mut Self {
        // ItemStatValue::new returns a Result. For the editor API, we unwrap
        // as we expect the provided values to be valid for the given stat ID.
        if let Ok(v) = ItemStatValue::new(value) {
            self.item.set_property_value(stat_id, v);
        }
        self
    }

    /// Configures the total number of sockets.
    pub fn set_sockets(&mut self, count: u8) -> &mut Self {
        self.item.set_sockets(count);
        self
    }

    /// Adds a nested item into a socket.
    /// Automatically synchronizes the payload and updates socket flags.
    pub fn add_socketed_item(&mut self, child: Item) -> &mut Self {
        self.item.add_socketed_item(child);
        self
    }

    /// Finalizes the mutation and returns the mutated item reference.
    /// This ensures any final synchronization contracts are met.
    pub fn commit(&mut self) -> &mut Item {
        self.item.sync_socket_payload();
        self.item
    }
}

/// Extension trait to provide editor access directly on Item.
pub trait ItemEditorExt {
    fn edit(&mut self) -> ItemEditor<'_>;
}

impl ItemEditorExt for Item {
    fn edit(&mut self) -> ItemEditor<'_> {
        ItemEditor::new(self)
    }
}
