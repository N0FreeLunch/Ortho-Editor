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

    fn ensure_extended(&mut self) {
        self.item.header.is_compact = false;
        self.item.is_compact = false;
        self.item.header.flags &= !(1 << 5);
        self.item.bits.clear();
        self.item.body.alpha_alignment_padding.clear();
        self.item.body.alpha_header_gap_bits.clear();
        self.item.body.alpha_header_gap = None;
        self.item.body.alpha_body_gap_bits.clear();
    }

    /// Sets the item's defense value.
    pub fn set_defense(&mut self, value: u32) -> &mut Self {
        self.ensure_extended();
        self.item.set_defense(Some(value));
        self.item.body.alpha_alignment_padding.clear();
        self
    }

    /// Sets the current and maximum durability.
    pub fn set_durability(&mut self, current: u32, max: u32) -> &mut Self {
        self.ensure_extended();
        self.item.set_durability(Some(current), Some(max));
        self.item.body.alpha_alignment_padding.clear();
        self
    }

    /// Sets the stack quantity for stackable items.
    pub fn set_quantity(&mut self, value: u32) -> &mut Self {
        self.item.set_quantity(Some(value));
        self.item.body.alpha_alignment_padding.clear();
        self
    }

    /// Sets or updates a specific stat value by its ID.
    /// Handles Alpha v105 stat mapping internally.
    pub fn set_stat(&mut self, stat_id: u32, value: i32) -> &mut Self {
        self.ensure_extended();
        // ItemStatValue::new returns a Result. For the editor API, we unwrap
        // as we expect the provided values to be valid for the given stat ID.
        if let Ok(v) = ItemStatValue::new(value) {
            self.item.set_property_value(stat_id, v);
        }
        self.item.body.alpha_alignment_padding.clear();
        self
    }

    /// Configures the total number of sockets.
    pub fn set_sockets(&mut self, count: u8) -> &mut Self {
        self.ensure_extended();
        self.item.set_sockets(count);
        let mask = 1 << 11;
        if count > 0 {
            self.item.header.flags |= mask;
            self.item.header.is_socketed = true;
        } else {
            self.item.header.flags &= !mask;
            self.item.header.is_socketed = false;
        }
        self.item.bits.clear();
        self.item.body.alpha_alignment_padding.clear();
        self
    }

    /// Adds a nested item into a socket.
    /// Automatically synchronizes the payload and updates socket flags.
    pub fn add_socketed_item(&mut self, child: Item) -> &mut Self {
        self.ensure_extended();
        self.item.add_socketed_item(child);
        self.item.body.alpha_alignment_padding.clear();
        self
    }

    /// Empties all sockets, returning the removed items.
    /// This reflects the Horadric Cube recipe (Hel Rune + Scroll of Town Portal).
    pub fn remove_socketed_items(&mut self) -> Vec<Item> {
        let removed = std::mem::take(&mut self.item.socketed_items);
        self.item.sync_socket_payload();
        self.item.bits.clear();
        self.item.body.alpha_alignment_padding.clear();
        removed
    }

    /// Sets the ethereal status of the item.
    pub fn set_ethereal(&mut self, is_ethereal: bool) -> &mut Self {
        self.ensure_extended();
        self.item.is_ethereal = is_ethereal;
        self.item.header.is_ethereal = is_ethereal;
        let is_alpha = self.item.header.save_is_alpha;
        let mask = if is_alpha { 1 << 24 } else { 1 << 22 };
        if is_ethereal {
            self.item.header.flags |= mask;
        } else {
            self.item.header.flags &= !mask;
        }
        self.item.bits.clear();
        self.item.body.alpha_alignment_padding.clear();
        self
    }

    /// Sets the identification status of the item.
    pub fn set_identified(&mut self, is_identified: bool) -> &mut Self {
        self.item.is_identified = is_identified;
        self.item.header.is_identified = is_identified;
        let mask = 1 << 4;
        if is_identified {
            self.item.header.flags |= mask;
        } else {
            self.item.header.flags &= !mask;
        }
        self.item.bits.clear();
        self.item.body.alpha_alignment_padding.clear();
        self
    }

    /// Sets the item quality.
    pub fn set_quality(&mut self, quality: crate::domain::item::ItemQuality) -> &mut Self {
        self.ensure_extended();
        self.item.quality = Some(quality);
        self.item.header.quality = Some(quality);
        self.item.bits.clear();
        self.item.body.alpha_alignment_padding.clear();
        self
    }

    /// Appends a new property to the item.
    pub fn add_property(&mut self, stat_id: u32, value: i32) -> &mut Self {
        self.ensure_extended();
        let cost = crate::data::stat_costs::STAT_COSTS.iter().find(|s| s.id == stat_id);
        let name = cost.map(|c| c.name.to_string()).unwrap_or_else(|| format!("stat_{}", stat_id));
        let raw_value = cost.map(|c| value.wrapping_add(c.save_add)).unwrap_or(value);
        let prop = crate::domain::stats::ItemProperty {
            stat_id,
            raw_value,
            param: 0,
            name,
            value,
            range: crate::domain::item::ItemBitRange::default(),
            is_opaque: false,
            opaque_bits: None,
            maturity: crate::domain::stats::StatMaturityTier::Verified,
        };
        self.item.properties.push(prop.clone());
        self.item.stats.properties.push(prop);
        self.item.bits.clear();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::item::{Item, ItemQuality};

    #[test]
    fn test_editor_defense_durability_mutation() {
        let mut item = Item::empty_for_tests();
        item.edit()
            .set_defense(350)
            .set_durability(45, 50)
            .set_ethereal(true)
            .commit();

        assert_eq!(item.defense(), Some(350));
        assert_eq!(item.current_durability(), Some(45));
        assert_eq!(item.max_durability(), Some(50));
        assert!(item.is_ethereal);
    }

    #[test]
    fn test_editor_socket_insertion_and_removal() {
        let mut parent = Item::empty_for_tests();
        parent.code = "crs ".to_string(); // Crystal Sword
        parent.edit().set_sockets(4).commit();

        assert_eq!(parent.sockets, Some(4));
        assert!(parent.header.is_socketed);

        let mut rune1 = Item::empty_for_tests();
        rune1.code = "r01 ".to_string(); // El Rune
        let mut rune2 = Item::empty_for_tests();
        rune2.code = "r02 ".to_string(); // Eld Rune

        parent.edit()
            .add_socketed_item(rune1)
            .add_socketed_item(rune2)
            .commit();

        assert_eq!(parent.socketed_items.len(), 2);
        assert_eq!(parent.num_socketed_items, 2);

        // Remove socketed items (Cube recipe simulation)
        let removed = parent.edit().remove_socketed_items();
        assert_eq!(removed.len(), 2);
        assert_eq!(parent.socketed_items.len(), 0);
        assert_eq!(parent.num_socketed_items, 0);
    }

    #[test]
    fn test_editor_property_mutation() {
        let mut item = Item::empty_for_tests();
        item.edit()
            .add_property(0, 20) // Strength +20
            .add_property(17, 150) // Enhanced Damage 150%
            .commit();

        assert_eq!(item.properties.len(), 2);
        assert_eq!(item.properties[0].stat_id, 0);
        assert_eq!(item.properties[0].value, 20);

        // Update stat value
        item.edit().set_stat(0, 30).commit();
        assert_eq!(item.properties[0].value, 30);
    }
}
