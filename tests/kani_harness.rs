#[cfg(kani)]
mod kani_tests {
    use d2r_core::domain::vo::*;

    #[kani::proof]
    fn proof_item_stat_value_invariants() {
        let val: i32 = kani::any();
        let result = ItemStatValue::new(val);

        if val >= ItemStatValue::MIN_SAFE && val <= ItemStatValue::MAX_SAFE {
            assert!(result.is_ok());
            assert_eq!(result.unwrap().value(), val);
        } else {
            assert!(result.is_err());
        }
    }

    #[kani::proof]
    fn proof_inventory_coordinate_invariants() {
        let x: u8 = kani::any();
        let y: u8 = kani::any();
        let result = InventoryCoordinate::new(x, y);

        if x <= InventoryCoordinate::MAX_X && y <= InventoryCoordinate::MAX_Y {
            assert!(result.is_ok());
            let vo = result.unwrap();
            assert_eq!(vo.x(), x);
            assert_eq!(vo.y(), y);
        } else {
            assert!(result.is_err());
        }
    }

    #[kani::proof]
    fn proof_item_quality_mapping_is_total() {
        let val: u8 = kani::any();
        // Since ItemQuality::from(v) uses a match with a wildcard _,
        // it is a total function and should never panic.
        let _quality = d2r_core::item::map_item_quality(val);
    }

    #[kani::proof]
    fn proof_calculate_stat_value_is_safe() {
        let raw: i32 = kani::any();
        let save_add: i32 = kani::any();
        // wrapping_sub is defined for all i32 combinations.
        let _ = d2r_core::item::calculate_stat_value(raw, save_add);
    }

    #[kani::proof]
    fn proof_item_size_validity() {
        let w: u8 = kani::any();
        let h: u8 = kani::any();
        let result = ItemSize::new(w, h);
        if w > 0 && w <= ItemSize::MAX_DIM && h > 0 && h <= ItemSize::MAX_DIM {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
        }
    }

    #[kani::proof]
    fn proof_inventory_placement_grid_safety() {
        let x: u8 = kani::any();
        let y: u8 = kani::any();
        let w: u8 = kani::any();
        let h: u8 = kani::any();

        let coord = InventoryCoordinate::new(x, y);
        let size = ItemSize::new(w, h);

        if coord.is_ok() && size.is_ok() {
            let placement = InventoryPlacement::new(coord.unwrap(), size.unwrap());
            if placement.is_ok() {
                let p = placement.unwrap();
                // Mathematical proof that the item fits in the 10x10 grid
                assert!(p.coordinate().x() + p.size().width() <= InventoryCoordinate::MAX_X);
                assert!(p.coordinate().y() + p.size().height() <= InventoryCoordinate::MAX_Y);
            }
        }
    }

    #[kani::proof]
    fn proof_align_to_byte_correctness() {
        let pos: u64 = kani::any();
        // Skip extreme boundary to avoid overflow in (pos + 7) during proof
        // though u64 is huge enough for normal bit positions.
        kani::assume(pos < u64::MAX - 8);

        let aligned = d2r_core::domain::vo::align_to_byte(pos);

        assert!(aligned % 8 == 0);
        assert!(aligned >= pos);
        assert!(aligned < pos + 8);
    }

    #[kani::proof]
    fn proof_descfunc_11_reciprocal_safety() {
        let value: i32 = kani::any();
        if value > 0 && value < 100 {
            // Guarded division: value is strictly between 1 and 99, so 100 / value never divides by zero
            let secs = 100 / value;
            assert!(secs >= 1);
            assert!(secs <= 100);
        } else if value >= 100 {
            let rate = value / 100;
            assert!(rate >= 1);
        }
    }

    #[kani::proof]
    fn proof_weapon_damage_and_ed_synthesis_safety() {
        let base_min: u16 = kani::any();
        let base_max: u16 = kani::any();
        kani::assume(base_min > 0 && base_min <= 500);
        kani::assume(base_max >= base_min && base_max <= 1000);

        let is_ethereal: bool = kani::any();
        let ed_pct: i32 = kani::any();
        kani::assume(ed_pct >= -100 && ed_pct <= 1000);

        let add_min: i32 = kani::any();
        let add_max: i32 = kani::any();
        kani::assume(add_min >= 0 && add_min <= 500);
        kani::assume(add_max >= add_min && add_max <= 500);

        let mut b_min = base_min as i32;
        let mut b_max = base_max as i32;
        if is_ethereal {
            b_min = (b_min * 150) / 100;
            b_max = (b_max * 150) / 100;
        }

        let f_min = ((b_min * (100 + ed_pct)) / 100 + add_min).max(1);
        let f_max = ((b_max * (100 + ed_pct)) / 100 + add_max).max(f_min);

        assert!(f_min >= 1);
        assert!(f_max >= f_min);
    }

    #[kani::proof]
    fn proof_equipment_requirements_reduction_safety() {
        let base_req: u16 = kani::any();
        kani::assume(base_req <= 255);

        let is_ethereal: bool = kani::any();
        let req_pct: i32 = kani::any();
        kani::assume(req_pct >= -100 && req_pct <= 100);

        let mut base = base_req as i32;
        if is_ethereal {
            base = (base - 10).max(0);
        }

        let final_req = ((base * (100 + req_pct)) / 100).max(0);

        assert!(final_req >= 0);
        if req_pct <= -100 {
            assert_eq!(final_req, 0);
        }
        if is_ethereal && base_req <= 10 {
            assert_eq!(final_req, 0);
        }
    }
}
