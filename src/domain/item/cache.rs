use crate::domain::item::entity::Item;
use crate::domain::item::quality::ItemQuality;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub type PropertyTuple = (u32, u32, i32, i32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemClassProjection {
    pub code: String,
    pub quality: u8,
    pub magic_prefix: Option<u16>,
    pub magic_suffix: Option<u16>,
    pub rare_name_1: Option<u8>,
    pub rare_name_2: Option<u8>,
    pub rare_affixes: [Option<u16>; 6],
    pub unique_id: Option<u16>,
    pub runeword_id: Option<u16>,
    pub runeword_level: Option<u8>,
    pub is_ethereal: bool,
    pub properties: Vec<PropertyTuple>,
    pub set_attributes: Vec<Vec<PropertyTuple>>,
    pub runeword_attributes: Vec<PropertyTuple>,
}

impl ItemClassProjection {
    pub fn extract(item: &Item) -> Option<Self> {
        if item.header.is_compact
            || item.is_opaque()
            || item.is_semi_opaque()
            || item.is_residue()
            || !item.socketed_items.is_empty()
            || item.defense().is_some()
            || item.max_durability().is_some()
        {
            return None;
        }

        let mut properties: Vec<PropertyTuple> = item
            .properties
            .iter()
            .map(|p| (p.stat_id, p.param, p.raw_value, p.value))
            .collect();
        properties.sort();

        let set_attributes: Vec<Vec<PropertyTuple>> = item
            .set_attributes
            .iter()
            .map(|list| {
                let mut inner: Vec<PropertyTuple> = list
                    .iter()
                    .map(|p| (p.stat_id, p.param, p.raw_value, p.value))
                    .collect();
                inner.sort();
                inner
            })
            .collect();

        let mut runeword_attributes: Vec<PropertyTuple> = item
            .runeword_attributes
            .iter()
            .map(|p| (p.stat_id, p.param, p.raw_value, p.value))
            .collect();
        runeword_attributes.sort();

        let quality = item.header.quality.unwrap_or(ItemQuality::Normal) as u8;

        Some(ItemClassProjection {
            code: item.body.code.trim().to_string(),
            quality,
            magic_prefix: item.magic_prefix,
            magic_suffix: item.magic_suffix,
            rare_name_1: item.rare_name_1,
            rare_name_2: item.rare_name_2,
            rare_affixes: item.rare_affixes,
            unique_id: item.unique_id,
            runeword_id: item.runeword_id,
            runeword_level: item.runeword_level,
            is_ethereal: item.header.is_ethereal,
            properties,
            set_attributes,
            runeword_attributes,
        })
    }
}

pub struct ItemClassRegistry {
    entries: HashMap<u64, Vec<Arc<ItemClassProjection>>>,
    collision_count: u64,
}

impl Default for ItemClassRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ItemClassRegistry {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            collision_count: 0,
        }
    }

    pub fn collision_count(&self) -> u64 {
        self.collision_count
    }

    pub fn len(&self) -> usize {
        self.entries.values().map(|v| v.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get_or_insert(&mut self, projection: ItemClassProjection) -> Arc<ItemClassProjection> {
        let digest = Self::compute_digest(&projection);
        let bucket = self.entries.entry(digest).or_default();

        if let Some(existing) = bucket.iter().find(|entry| entry.as_ref() == &projection) {
            return Arc::clone(existing);
        }

        if !bucket.is_empty() {
            self.collision_count += 1;
        }

        let arc = Arc::new(projection);
        bucket.push(Arc::clone(&arc));
        arc
    }

    fn compute_digest(projection: &ItemClassProjection) -> u64 {
        let mut hasher = DefaultHasher::new();
        projection.hash(&mut hasher);
        hasher.finish()
    }

    #[cfg(test)]
    pub(crate) fn force_insert_with_digest(
        &mut self,
        digest: u64,
        projection: ItemClassProjection,
    ) -> Arc<ItemClassProjection> {
        let bucket = self.entries.entry(digest).or_default();

        if let Some(existing) = bucket.iter().find(|entry| entry.as_ref() == &projection) {
            return Arc::clone(existing);
        }

        if !bucket.is_empty() {
            self.collision_count += 1;
        }

        let arc = Arc::new(projection);
        bucket.push(Arc::clone(&arc));
        arc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extraction_and_reuse() {
        let mut item1 = Item::empty_for_tests();
        item1.body.code = "hgl ".to_string();
        item1.code = "hgl ".to_string();

        let mut item2 = Item::empty_for_tests();
        item2.body.code = "hgl ".to_string();
        item2.code = "hgl ".to_string();

        let proj1 = ItemClassProjection::extract(&item1).expect("item1 should be eligible");
        let proj2 = ItemClassProjection::extract(&item2).expect("item2 should be eligible");

        let mut registry = ItemClassRegistry::new();
        let arc1 = registry.get_or_insert(proj1);
        let arc2 = registry.get_or_insert(proj2);

        assert!(Arc::ptr_eq(&arc1, &arc2));
        assert_eq!(registry.collision_count(), 0);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_rejection_paths() {
        // Compact item
        let mut compact_item = Item::empty_for_tests();
        compact_item.header.is_compact = true;
        assert!(ItemClassProjection::extract(&compact_item).is_none());

        // Opaque item
        let mut opaque_item = Item::empty_for_tests();
        opaque_item.modules.push(ItemModule::Opaque(vec![true]));
        assert!(ItemClassProjection::extract(&opaque_item).is_none());

        // Semi-opaque item
        let mut semi_opaque_item = Item::empty_for_tests();
        semi_opaque_item.modules.push(ItemModule::SemiOpaque {
            body_bits: vec![true],
            reason: "test".to_string(),
        });
        assert!(ItemClassProjection::extract(&semi_opaque_item).is_none());

        // Residue item
        let mut residue_item = Item::empty_for_tests();
        residue_item.body.code = "ww".to_string();
        residue_item.code = "ww".to_string();
        residue_item.header.save_is_alpha = true;
        assert!(ItemClassProjection::extract(&residue_item).is_none());

        // Socketed item
        let mut socketed_item = Item::empty_for_tests();
        socketed_item.socketed_items.push(Item::empty_for_tests());
        assert!(ItemClassProjection::extract(&socketed_item).is_none());

        // Item with defense
        let mut defense_item = Item::empty_for_tests();
        defense_item.body.defense = Some(100);
        assert!(ItemClassProjection::extract(&defense_item).is_none());

        // Item with max durability
        let mut durability_item = Item::empty_for_tests();
        durability_item.body.max_durability = Some(50);
        assert!(ItemClassProjection::extract(&durability_item).is_none());
    }

    #[test]
    fn test_forced_hash_collision() {
        let proj1 = ItemClassProjection {
            code: "abc".to_string(),
            quality: 2,
            magic_prefix: None,
            magic_suffix: None,
            rare_name_1: None,
            rare_name_2: None,
            rare_affixes: [None; 6],
            unique_id: None,
            runeword_id: None,
            runeword_level: None,
            is_ethereal: false,
            properties: vec![],
            set_attributes: vec![],
            runeword_attributes: vec![],
        };

        let mut proj2 = proj1.clone();
        proj2.code = "def".to_string();

        let mut registry = ItemClassRegistry::new();
        let arc1 = registry.force_insert_with_digest(999, proj1);
        let arc2 = registry.force_insert_with_digest(999, proj2);

        assert!(!Arc::ptr_eq(&arc1, &arc2));
        assert_eq!(arc1.code, "abc");
        assert_eq!(arc2.code, "def");
        assert_eq!(registry.collision_count(), 1);
        assert_eq!(registry.len(), 2);
    }
}
