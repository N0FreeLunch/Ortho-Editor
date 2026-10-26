//! Source-bound, single-pass property ID mutation with an explicit lazy mapper.
use crate::data::generated::layout_policy::LEGACY_ALPHA_PROPERTY_NORMALIZATION_CODES;
use crate::domain::stats::entity::ItemProperty;

pub(crate) fn normalize_legacy_properties_using<F: FnMut(u32) -> u32>(
    props: &mut [ItemProperty],
    code: &str,
    alpha_admitted: bool,
    mut map_id: F,
) {
    if !alpha_admitted {
        return;
    }
    let trimmed = code.trim();
    if LEGACY_ALPHA_PROPERTY_NORMALIZATION_CODES.contains(&trimmed) {
        for prop in props.iter_mut() {
            let mapped = map_id(prop.stat_id);
            if mapped != prop.stat_id {
                prop.stat_id = mapped;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::item::ItemBitRange;
    use crate::domain::stats::StatMaturityTier;
    use d2r_macros::hypothesis_test;

    fn properties() -> Vec<ItemProperty> {
        [10, 140, 10, 9999]
            .into_iter()
            .enumerate()
            .map(|(i, id)| {
                let mut p = ItemProperty::new(
                    id,
                    format!("keep-{i}"),
                    777,
                    -11,
                    888,
                    ItemBitRange { start: 17, end: 29 },
                );
                p.is_opaque = true;
                p.opaque_bits = Some(vec![true, false]);
                p.maturity = StatMaturityTier::Speculative;
                p
            })
            .collect()
    }

    #[hypothesis_test]
    fn mapper_is_lazy_for_admission_code_and_empty_slice() {
        for (code, admitted, empty) in [
            ("xrs", false, false),
            ("c8xr", false, false),
            ("scs", false, false),
            ("XRS", true, false),
            ("xrs\0", true, false),
            ("\0scs", true, false),
            ("unknown", true, false),
            ("\u{2003}scs\u{2003}", true, true),
        ] {
            let mut props = if empty { vec![] } else { properties() };
            let before = props.clone();
            normalize_legacy_properties_using(&mut props, code, admitted, |_| {
                panic!("mapper must remain lazy")
            });
            assert_eq!(props, before);
        }
    }

    #[hypothesis_test]
    fn each_original_id_is_mapped_once_without_other_field_or_allocation_changes() {
        for code in ["xrs", "c8xr", "scs", "\u{2003}xrs \t"] {
            let mut props = properties();
            let mut expected = props.clone();
            expected[0].stat_id = 140;
            expected[1].stat_id = 320;
            expected[2].stat_id = 140;
            let capacity = props.capacity();
            let pointer = props.as_ptr();
            let mut seen = vec![];
            normalize_legacy_properties_using(&mut props, code, true, |id| {
                seen.push(id);
                match id {
                    10 => 140,
                    140 => 320,
                    _ => id,
                }
            });
            assert_eq!(seen, vec![10, 140, 10, 9999]);
            assert_eq!(props, expected);
            assert_eq!(props.capacity(), capacity);
            assert_eq!(props.as_ptr(), pointer);
        }
    }

    #[hypothesis_test]
    fn mapper_panic_retains_the_original_partial_mutation_order() {
        let mut props = properties();
        let mut expected = props.clone();
        expected[0].stat_id = 140;
        let mut seen = vec![];
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            normalize_legacy_properties_using(&mut props, "xrs", true, |id| {
                seen.push(id);
                if seen.len() == 2 {
                    panic!("controlled mapper panic");
                }
                140
            })
        }));
        assert!(result.is_err());
        assert_eq!(seen, vec![10, 140]);
        assert_eq!(props, expected);
    }
}
