#[cfg(test)]
mod tests {
    use d2r_core::item::{Item, ItemBitRange, RecordedBit};
    use d2r_core::domain::stats::entity::ItemProperty;
    use d2r_core::domain::vo::ItemStatValue;
    use d2r_core::domain::item::ItemQuality;

    #[test]
    fn test_alpha_v105_stat_mutation() {
        let mut item = Item::empty_for_tests();
        item.header.version = 5;
        item.header.quality = Some(ItemQuality::Normal);
        item.header.flags = 0x00000800; // Runeword bit (is_runeword)
        
        // Add a property that needs mapping in Alpha
        // Raw ID 289 -> Effective ID 9 (maxmana)
        let prop = ItemProperty {
            stat_id: 289,
            name: "maxmana".to_string(),
            param: 0,
            raw_value: 100,
            value: 100,
            range: ItemBitRange { start: 0, end: 0 },
        };
        item.properties.push(prop);
        item.stats.properties = item.properties.clone();
        
        // Set a dummy bit cache to verify invalidation
        item.bits = vec![RecordedBit { bit: true, offset: 0 }];
        
        // Mutate using Effective ID 9
        let new_val = ItemStatValue::new(150).unwrap();
        let found = item.set_property_value(9, new_val);
        
        assert!(found, "Should find and update property 9 (mapped from 289)");
        assert_eq!(item.properties[0].value, 150);
        assert!(item.bits.is_empty(), "Bit cache should be cleared after mutation");
    }
}
