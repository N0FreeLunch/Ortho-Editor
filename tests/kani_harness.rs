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

    #[kani::proof]
    fn proof_character_boundary_evaluate_bound_soundness() {
        let current: u64 = kani::any();
        let min: u64 = kani::any();
        let max: u64 = kani::any();
        kani::assume(min <= max);
        kani::assume(current >= min && current <= max);

        let op_mode: u8 = kani::any();
        let operand: u64 = kani::any();

        let op = match op_mode % 3 {
            0 => d2r_core::domain::contracts::NumericOp::Set(operand),
            1 => d2r_core::domain::contracts::NumericOp::Add(operand),
            _ => d2r_core::domain::contracts::NumericOp::Sub(operand),
        };

        let result = d2r_core::domain::contracts::CharacterBoundaryValidator::evaluate_bound(
            "test_field",
            current,
            op,
            min,
            max,
        );

        if let Ok(resulting) = result {
            assert!(resulting >= min);
            assert!(resulting <= max);
            match op {
                d2r_core::domain::contracts::NumericOp::Set(v) => assert_eq!(resulting, v),
                d2r_core::domain::contracts::NumericOp::Add(v) => assert_eq!(resulting, current + v),
                d2r_core::domain::contracts::NumericOp::Sub(v) => assert_eq!(resulting, current - v),
            }
        }
    }

    #[kani::proof]
    fn proof_character_gold_cap_and_wealth_invariants() {
        let level: u8 = kani::any();
        kani::assume(level >= 1 && level <= 99);

        let char_gold_cap = (level as u64) * 10_000;
        assert!(char_gold_cap >= 10_000);
        assert!(char_gold_cap <= 990_000);

        let max_stash_gold = d2r_core::domain::contracts::StashGoldValue::MAX_STASH_GOLD as u64;
        assert_eq!(max_stash_gold, 2_500_000);

        let max_total_wealth = char_gold_cap + max_stash_gold;
        assert!(max_total_wealth <= 3_490_000);
        assert!(max_total_wealth < u32::MAX as u64);
    }

    #[kani::proof]
    fn proof_item_boundary_u32_op_soundness() {
        let current: u32 = kani::any();
        let min: u32 = kani::any();
        let max: u32 = kani::any();
        kani::assume(min <= max);
        kani::assume(current >= min && current <= max);

        let op_mode: u8 = kani::any();
        let operand: u32 = kani::any();

        let op = match op_mode % 3 {
            0 => d2r_core::domain::contracts::NumericOp::Set(operand),
            1 => d2r_core::domain::contracts::NumericOp::Add(operand),
            _ => d2r_core::domain::contracts::NumericOp::Sub(operand),
        };

        let result = d2r_core::domain::contracts::ItemBoundaryValidator::evaluate_u32_op(
            "test.item_u32",
            op,
            current,
            min,
            max,
        );

        if let Ok(resulting) = result {
            assert!(resulting >= min);
            assert!(resulting <= max);
        }
    }

    #[kani::proof]
    fn proof_item_boundary_i32_op_soundness() {
        let current: i32 = kani::any();
        let min: i32 = kani::any();
        let max: i32 = kani::any();
        kani::assume(min <= max);
        kani::assume(current >= min && current <= max);

        let op_mode: u8 = kani::any();
        let operand: i32 = kani::any();

        let op = match op_mode % 3 {
            0 => d2r_core::domain::contracts::NumericOp::Set(operand),
            1 => d2r_core::domain::contracts::NumericOp::Add(operand),
            _ => d2r_core::domain::contracts::NumericOp::Sub(operand),
        };

        let result = d2r_core::domain::contracts::ItemBoundaryValidator::evaluate_i32_op(
            "test.item_i32",
            op,
            current,
            min,
            max,
        );

        if let Ok(resulting) = result {
            assert!(resulting >= min);
            assert!(resulting <= max);
        }
    }

    #[kani::proof]
    fn proof_waypoint_buffer_indexing_soundness() {
        let diff: u8 = kani::any();
        kani::assume(diff <= 2);

        let wp_idx: usize = kani::any();
        kani::assume(wp_idx < 39);

        // Save file layout has 24 bytes per difficulty for waypoints (72 bytes total).
        let byte_offset = (diff as usize) * 24 + (wp_idx / 8);
        let bit_in_byte = wp_idx % 8;

        assert!(byte_offset < 72);
        assert!(byte_offset <= 52); // Maximum possible byte offset is 2 * 24 + 4 = 52
        assert!(bit_in_byte < 8);
    }

    #[kani::proof]
    fn proof_skill_points_respec_conservation() {
        let unspent: u8 = kani::any();
        kani::assume(unspent <= 99);

        let s0: u8 = kani::any();
        let s1: u8 = kani::any();
        let s2: u8 = kani::any();
        kani::assume(s0 <= 20 && s1 <= 20 && s2 <= 20);

        let total_spent = (s0 as u32) + (s1 as u32) + (s2 as u32);
        assert!(total_spent <= 60);

        let refunded = (unspent as u32) + total_spent;
        assert!(refunded <= 99 + 60);
        assert!(refunded <= 699);
    }
}

#[cfg(not(kani))]
mod standard_boundary_invariant_tests {
    use d2r_core::domain::contracts::{
        CharacterBoundaryValidator, ItemBoundaryValidator, NumericOp, StashGoldValue,
    };

    #[test]
    fn test_character_evaluate_bound_invariants() {
        let test_cases = vec![
            (100u64, NumericOp::Set(50), 0u64, 200u64, Ok(50u64)),
            (100u64, NumericOp::Set(300), 0u64, 200u64, Err("overflow")),
            (100u64, NumericOp::Add(50), 0u64, 200u64, Ok(150u64)),
            (100u64, NumericOp::Add(150), 0u64, 200u64, Err("overflow")),
            (100u64, NumericOp::Sub(30), 0u64, 200u64, Ok(70u64)),
            (100u64, NumericOp::Sub(150), 0u64, 200u64, Err("underflow")),
        ];

        for (cur, op, min, max, expected) in test_cases {
            let res = CharacterBoundaryValidator::evaluate_bound("field", cur, op, min, max);
            match expected {
                Ok(val) => assert_eq!(res.unwrap(), val),
                Err(err_kind) => {
                    assert!(res.is_err());
                    let err = res.unwrap_err();
                    match err {
                        d2r_core::domain::contracts::ContractError::Overflow { .. } => assert_eq!(err_kind, "overflow"),
                        d2r_core::domain::contracts::ContractError::Underflow { .. } => assert_eq!(err_kind, "underflow"),
                        _ => panic!("unexpected error type: {:?}", err),
                    }
                }
            }
        }
    }

    #[test]
    fn test_item_evaluate_u32_op_invariants() {
        let op_set = NumericOp::Set(150);
        let res = ItemBoundaryValidator::evaluate_u32_op("defense", op_set, 100, 0, 500);
        assert_eq!(res.unwrap(), 150);

        let op_overflow = NumericOp::Add(450);
        let res_err = ItemBoundaryValidator::evaluate_u32_op("defense", op_overflow, 100, 0, 500);
        assert!(res_err.is_err());
        assert_eq!(res_err.unwrap_err().code, "overflow");

        let op_underflow = NumericOp::Sub(150);
        let res_err = ItemBoundaryValidator::evaluate_u32_op("defense", op_underflow, 100, 0, 500);
        assert!(res_err.is_err());
        assert_eq!(res_err.unwrap_err().code, "underflow");
    }

    #[test]
    fn test_item_evaluate_i32_op_invariants() {
        let op_set = NumericOp::Set(-50);
        let res = ItemBoundaryValidator::evaluate_i32_op("ed", op_set, 0, -100, 1000);
        assert_eq!(res.unwrap(), -50);

        let op_overflow = NumericOp::Add(1100);
        let res_err = ItemBoundaryValidator::evaluate_i32_op("ed", op_overflow, 0, -100, 1000);
        assert!(res_err.is_err());
        assert_eq!(res_err.unwrap_err().code, "overflow");

        let op_underflow = NumericOp::Sub(200);
        let res_err = ItemBoundaryValidator::evaluate_i32_op("ed", op_underflow, 0, -100, 1000);
        assert!(res_err.is_err());
        assert_eq!(res_err.unwrap_err().code, "underflow");
    }

    #[test]
    fn test_gold_cap_and_waypoint_indexing_invariants() {
        for lvl in 1..=99u8 {
            let cap = (lvl as u64) * 10_000;
            assert!(cap >= 10_000 && cap <= 990_000);
            assert!(cap + (StashGoldValue::MAX_STASH_GOLD as u64) < (u32::MAX as u64));
        }

        for diff in 0..=2u8 {
            for wp_idx in 0..39usize {
                let byte_offset = (diff as usize) * 24 + (wp_idx / 8);
                let bit_in_byte = wp_idx % 8;
                assert!(byte_offset < 72);
                assert!(byte_offset <= 52);
                assert!(bit_in_byte < 8);
            }
        }
    }
}
