#[cfg(test)]
mod tests {
    use d2r_core::item::{Item, HuffmanTree};
    use d2r_core::domain::item::ItemQuality;

    #[test]
    fn test_alpha_v105_socket_synchronization() {
        let mut parent = Item::empty_for_tests();
        parent.header.version = 6; // Alpha Version 6 (stable flags)
        parent.header.quality = Some(ItemQuality::Normal);
        parent.body.code = "r01 ".to_string();
        parent.code = "r01 ".to_string();
        
        let mut child = Item::empty_for_tests();
        child.body.code = "r01 ".to_string();
        child.code = "r01 ".to_string();
        child.header.mode = 6; // Socketed
        
        // Add item to socket. This calls sync_socket_payload internally.
        parent.add_socketed_item(child);
        
        // Verify internal state before roundtrip
        assert_eq!(parent.socketed_items.len(), 1);
        assert!(parent.properties.iter().any(|p| p.stat_id == 317), "Should have Stat 317 property for child");
        assert_eq!(parent.header.is_socketed, true);
        assert!((parent.header.flags & (1 << 11)) != 0, "Socketed bit should be set in flags");
        
        let huffman = HuffmanTree::new();
        let bytes = parent.to_bytes(0, &huffman, true).expect("Should serialize to bytes");
        
        let item = Item::from_bytes(&bytes, &huffman, true).expect("Should parse back");
        
        // Verify state after roundtrip
        assert_eq!(item.socketed_items.len(), 1, "Child item should persist through roundtrip");
        assert_eq!(item.socketed_items[0].code.trim(), "r01");
    }
}
