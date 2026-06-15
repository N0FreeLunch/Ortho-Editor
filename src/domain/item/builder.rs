use crate::domain::item::{Item, ItemHeader};

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
}
