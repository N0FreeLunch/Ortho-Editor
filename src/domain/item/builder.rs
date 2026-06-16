use crate::domain::item::{Item, ItemHeader, ItemQuality};
use crate::domain::stats::ItemProperty;

#[derive(Debug, Clone)]
pub struct ItemBuilder {
    item: Item,
}

impl ItemBuilder {
    pub fn new() -> Self {
        Self {
            item: Item::empty_for_tests(),
        }
    }

    pub fn with_item(item: Item) -> Self {
        Self { item }
    }

    pub fn code(mut self, code: impl Into<String>) -> Self {
        let code = code.into();
        self.item.body.code = code.clone();
        self.item.code = code;
        self
    }

    pub fn version(mut self, version: u8) -> Self {
        self.item.header.version = version;
        self
    }

    pub fn is_compact(mut self, is_compact: bool) -> Self {
        self.item.header.is_compact = is_compact;
        self
    }

    pub fn quality(mut self, quality: ItemQuality) -> Self {
        self.item.header.quality = Some(quality);
        self
    }

    pub fn magic_affixes(mut self, prefix: Option<u16>, suffix: Option<u16>) -> Self {
        self.item.magic_prefix = prefix;
        self.item.magic_suffix = suffix;
        self
    }

    pub fn property(mut self, property: ItemProperty) -> Self {
        self.item.properties.push(property.clone());
        self.item.stats.properties.push(property);
        self
    }

    pub fn properties(mut self, properties: Vec<ItemProperty>) -> Self {
        self.item.stats.properties = properties.clone();
        self.item.properties = properties;
        self
    }

    pub fn properties_complete(mut self, complete: bool) -> Self {
        self.item.properties_complete = complete;
        self
    }

    pub fn header(mut self, header: ItemHeader) -> Self {
        self.item.header = header;
        self
    }

    pub fn build(self) -> Item {
        self.item
    }
}

impl Default for ItemBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::ItemBuilder;
    use crate::domain::item::{HuffmanTree, Item, ItemHeader, ItemQuality};

    fn make_serializable_header() -> ItemHeader {
        let mut header = ItemHeader::default();
        header.flags = 0x4D4A;
        header.version = 5;
        header.is_compact = false;
        header.save_is_alpha = true;
        header
    }

    #[test]
    fn builder_mirrors_code_into_body_and_legacy_fields() {
        let item = ItemBuilder::new().code("xrs").build();

        assert_eq!(item.body.code, "xrs");
        assert_eq!(item.code, "xrs");
        assert_eq!(item.code(), "xrs");
    }

    #[test]
    fn builder_keeps_header_fields_on_item_header() {
        let item = ItemBuilder::new().version(5).is_compact(true).build();

        assert_eq!(item.header.version, 5);
        assert!(item.header.is_compact);
    }

    #[test]
    fn builder_roundtrips_quality_and_stats() {
        let huffman = HuffmanTree::new();
        let item = ItemBuilder::new()
            .header(make_serializable_header())
            .code("ci0 ")
            .quality(ItemQuality::Magic)
            .magic_affixes(Some(12), Some(34))
            .build();

        let bytes = item
            .to_bytes(0, &huffman, true)
            .expect("builder-authored item should serialize");
        let parsed =
            Item::from_bytes(&bytes, &huffman, true).expect("serialized item should roundtrip");

        assert!(
            parsed.header.quality == Some(ItemQuality::Magic)
                || parsed.header.alpha_quality_raw == Some(ItemQuality::Magic as u8)
        );
        assert_eq!(parsed.magic_prefix, Some(12));
        assert_eq!(parsed.magic_suffix, Some(34));
    }
}
