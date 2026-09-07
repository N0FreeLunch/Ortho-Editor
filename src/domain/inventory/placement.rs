use crate::domain::item::Item;

pub(super) struct ItemPlacement<'a> {
    item: &'a Item,
}

impl<'a> ItemPlacement<'a> {
    pub(super) fn new(item: &'a Item) -> Self {
        Self { item }
    }

    pub(super) fn code(&self) -> &'a str {
        self.item.code.trim()
    }

    pub(super) fn x(&self) -> u8 {
        self.item.x
    }

    pub(super) fn y(&self) -> u8 {
        self.item.y
    }

    pub(super) fn size(&self) -> (u8, u8) {
        super::get_item_size(&self.item.code)
    }

    pub(super) fn should_skip(&self) -> bool {
        let code = self.code();
        self.item.is_residue()
            || code.is_empty()
            || matches!(code, "wyws" | "b7ts" | "ks d")
            || self.item.header.flags == 0
            || crate::data::item_codes::ITEM_TEMPLATES
                .iter()
                .all(|template| template.code != code)
    }

    pub(super) fn is_cube_container(&self) -> bool {
        self.item.is_cube_container()
    }

    pub(super) fn is_in_cube(&self) -> bool {
        self.item.is_in_cube()
    }

    pub(super) fn is_stash(&self) -> bool {
        let item = self.item;
        let is_stored = item.header.mode == 4 || item.body.mode == 4;
        is_stored
            && (item.header.page == 1
                || item.body.page == 1
                || item.header.location == 4
                || item.body.location == 4
                || crate::verify::alpha_inventory_routing::alpha_inventory_route(item, true)
                    == crate::verify::alpha_inventory_routing::AlphaInventoryRoute::Stash)
    }

    pub(super) fn is_inventory(&self) -> bool {
        let item = self.item;
        let is_stored = item.header.mode == 4 || item.body.mode == 4;
        is_stored
            && !self.is_stash()
            && (item.header.page == 0 && item.body.page == 0)
            && (item.header.location == 0 && item.body.location == 0)
            && (item.header.mode != 1 && item.body.mode != 1)
            && (item.header.mode != 2 && item.body.mode != 2)
            && item.header.y < 4
            && item.y < 4
            && crate::verify::alpha_inventory_routing::alpha_inventory_route(item, true)
                == crate::verify::alpha_inventory_routing::AlphaInventoryRoute::Inventory
    }
}

#[cfg(test)]
mod tests {
    use super::ItemPlacement;
    use crate::domain::header::entity::ItemHeader;
    use crate::domain::inventory::{
        ContainerIntegrityError, ContainerKind, CubeIntegrityError, InventoryGrid,
    };
    use crate::domain::item::entity::ItemBody;
    use crate::domain::item::Item;
    use crate::domain::vo::ContainerLocation;

    fn item(code: &str) -> Item {
        let mut header = ItemHeader::default();
        header.flags = 0x10;
        Item {
            code: code.to_owned(),
            header,
            body: ItemBody {
                code: code.to_owned(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn preserves_skip_and_cube_header_body_rules() {
        let mut unknown = item("nope");
        assert!(ItemPlacement::new(&unknown).should_skip());

        unknown.code = "tsc".to_owned();
        unknown.header.flags = 0;
        assert!(ItemPlacement::new(&unknown).should_skip());

        for dummy_code in ["wyws", "b7ts", "ks d"] {
            assert!(ItemPlacement::new(&item(dummy_code)).should_skip());
        }

        let mut cube_item = item("tsc");
        cube_item.body.page = ContainerLocation::CUBE_PAGE;
        assert!(ItemPlacement::new(&cube_item).is_in_cube());

        let cube = item("box");
        assert!(ItemPlacement::new(&cube).is_cube_container());
    }

    #[test]
    fn preserves_stash_and_inventory_header_body_rules() {
        let mut stash = item("tsc");
        stash.body.mode = 4;
        stash.body.location = 4;
        assert!(ItemPlacement::new(&stash).is_stash());

        let mut inventory = item("tsc");
        inventory.header.mode = 4;
        inventory.body.mode = 4;
        inventory.header.page = 0;
        inventory.body.page = 0;
        inventory.header.location = 0;
        inventory.body.location = 0;
        inventory.header.y = 0;
        inventory.body.y = 0;
        assert!(ItemPlacement::new(&inventory).is_inventory());

        inventory.body.page = 1;
        assert!(!ItemPlacement::new(&inventory).is_inventory());
    }

    // Semantic snapshots, not assertions about the correctness of the Alpha format.
    #[d2r_macros::hypothesis_test]
    fn skip_and_membership_edge_cases() {
        assert!(!ItemPlacement::new(&item("tsc")).should_skip());
        for code in ["", "   ", "\0", "ww", "gcw"] {
            let mut residue = item(code);
            residue.header.save_is_alpha = true;
            assert!(residue.is_residue(), "{code:?}");
            assert!(ItemPlacement::new(&residue).should_skip(), "{code:?}");
        }

        for (header_page, body_page, expected) in
            [(0, 0, false), (3, 0, true), (0, 3, true), (3, 3, true)]
        {
            let mut candidate = item("tsc");
            candidate.header.page = header_page;
            candidate.body.page = body_page;
            assert_eq!(ItemPlacement::new(&candidate).is_in_cube(), expected);
        }

        for (header_mode, body_mode, stored) in
            [(0, 0, false), (4, 0, true), (0, 4, true), (4, 4, true)]
        {
            for (header_page, body_page, header_location, body_location) in [
                (1, 0, 0, 0),
                (0, 1, 0, 0),
                (0, 0, 4, 0),
                (0, 0, 0, 4),
                (0, 0, 12, 0),
            ] {
                let mut candidate = item("tsc");
                candidate.header.mode = header_mode;
                candidate.body.mode = body_mode;
                candidate.header.page = header_page;
                candidate.body.page = body_page;
                candidate.header.location = header_location;
                candidate.body.location = body_location;
                let view = ItemPlacement::new(&candidate);
                assert_eq!(view.is_stash(), stored);
                assert!(!view.is_inventory());
            }
        }

        for (header_mode, body_mode, expected) in [
            (4, 0, true),
            (0, 4, true),
            (4, 4, true),
            (0, 0, false),
            (1, 4, false),
            (4, 1, false),
            (2, 4, false),
            (4, 2, false),
        ] {
            let mut candidate = item("tsc");
            candidate.header.mode = header_mode;
            candidate.body.mode = body_mode;
            candidate.header.page = 0;
            candidate.body.page = 0;
            candidate.header.location = 0;
            candidate.body.location = 0;
            candidate.header.x = 0;
            candidate.header.y = 0;
            assert_eq!(ItemPlacement::new(&candidate).is_inventory(), expected);
        }
    }

    #[d2r_macros::hypothesis_test]
    fn preserves_independent_placements_and_first_error() {
        assert_eq!(InventoryGrid::validate_cube_integrity(&[]), Ok(()));
        assert_eq!(InventoryGrid::validate_stash_integrity(&[]), Ok(()));
        assert_eq!(InventoryGrid::validate_inventory_integrity(&[]), Ok(()));
        assert_eq!(
            InventoryGrid::validate_all_containers_integrity(&[]),
            Ok(())
        );

        let stored = |code: &str, page, x| {
            let mut candidate = item(code);
            candidate.header.mode = 4;
            candidate.body.mode = 4;
            candidate.header.page = page;
            candidate.body.page = page;
            candidate.header.location = 0;
            candidate.body.location = 0;
            candidate.header.x = x;
            candidate.header.y = 0;
            candidate
        };
        let mut items = [
            stored("tsc", 0, 0),
            stored("tsc", 0, 1),
            stored("tsc", 1, 0),
        ];
        assert_eq!(
            InventoryGrid::validate_all_containers_integrity(&items),
            Ok(())
        );
        items[1].header.x = 0;
        let collision = ContainerIntegrityError::Collision {
            container: ContainerKind::Inventory,
            item_a: "tsc".into(),
            item_b: "tsc".into(),
            x: 0,
            y: 0,
        };
        assert_eq!(
            InventoryGrid::validate_inventory_integrity(&items),
            Err(collision)
        );

        // Stash takes precedence over an inventory collision in the unified API.
        items[2].header.x = 10;
        assert_eq!(
            InventoryGrid::validate_all_containers_integrity(&items),
            Err(ContainerIntegrityError::OutOfBounds {
                container: ContainerKind::Stash,
                item_code: "tsc".into(),
                x: 10,
                y: 0,
                w: 1,
                h: 1,
            })
        );
        // A cube dependency failure takes precedence over both grid failures.
        items[0].body.page = ContainerLocation::CUBE_PAGE;
        assert_eq!(
            InventoryGrid::validate_all_containers_integrity(&items),
            Err(ContainerIntegrityError::CubeDependency(
                CubeIntegrityError::CubeNotFoundWithContainedItems
            ))
        );
    }

    #[d2r_macros::hypothesis_test]
    fn parsed_fixture_results_match_pre_adapter_baseline() {
        // Baseline observed before the adapter at 556a6fdeef3c1e8b98fef48f82bc8596053da115.
        // Freeze results rather than copying the validator implementation into tests.
        let root = std::env::var_os("D2R_CORE_PATH")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        let huffman = crate::domain::item::HuffmanTree::new();
        for (fixture, expected_inventory) in [
            (
                "original/amazon_10_scrolls.d2s",
                Err(ContainerIntegrityError::Collision {
                    container: ContainerKind::Inventory,
                    item_a: "tsc".into(),
                    item_b: "tsc".into(),
                    x: 2,
                    y: 0,
                }),
            ),
            (
                "gameplay/normal/act2/TESTDRUID_Q2_CubeFoundNoCainTalk.d2s",
                Ok(()),
            ),
        ] {
            let path = root.join("tests/fixtures/savegames").join(fixture);
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let items =
                Item::read_player_items(&bytes, &huffman, true).expect("Fixture parsing failed");
            assert!(
                !items.is_empty(),
                "{fixture}: empty parse cannot establish parity"
            );
            assert_eq!(
                InventoryGrid::validate_cube_integrity(&items),
                Ok(()),
                "{fixture}"
            );
            assert_eq!(
                InventoryGrid::validate_stash_integrity(&items),
                Ok(()),
                "{fixture}"
            );
            assert_eq!(
                InventoryGrid::validate_inventory_integrity(&items),
                expected_inventory,
                "{fixture}"
            );
            assert_eq!(
                InventoryGrid::validate_all_containers_integrity(&items),
                expected_inventory,
                "{fixture}"
            );
        }
    }
}
