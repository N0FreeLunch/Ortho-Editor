use d2r_core::domain::forensic::registry::get_registry;
use d2r_core::domain::item::serialization::write_property_list;
use d2r_core::domain::item::{
    BitEmitter, HuffmanTree, Item, ItemBitRange, ItemQuality, RecordedBit,
};
use d2r_core::domain::stats::{ItemProperty, StatsAxiom};
use d2r_macros::invariant_test;

const GUARD: [bool; 5] = [false, true, true, false, true];

fn prefix(offset: usize) -> Vec<bool> {
    (0..offset).map(|i| i % 2 == 0).collect()
}

fn field(id: u32) -> Vec<bool> {
    (0..9).map(|i| id & (1 << i) != 0).collect()
}

fn packed(bits: &[bool]) -> Vec<u8> {
    bits.chunks(8)
        .map(|chunk| {
            chunk
                .iter()
                .enumerate()
                .fold(0, |byte, (i, &bit)| byte | (u8::from(bit) << i))
        })
        .collect()
}

fn property(id: u32) -> ItemProperty {
    ItemProperty::new(
        id,
        "nested writer witness".into(),
        0,
        0,
        0,
        ItemBitRange { start: 0, end: 0 },
    )
}

fn axiom(code: &str) -> StatsAxiom {
    let mut a = StatsAxiom::new(0, ItemQuality::Normal, true);
    a.code = code.into();
    a
}

fn assert_budget_inputs() {
    let registry = get_registry();
    assert_eq!(
        registry.item_overrides.as_ref().unwrap()["oesw"]["320"],
        2871
    );
    assert!(!registry.mappings.contains_key("320") && !registry.stats.contains_key("320"));
    for code in ["plain", "wa2"] {
        assert!(
            registry
                .item_overrides
                .as_ref()
                .and_then(|items| items.get(code))
                .and_then(|row| row.get("320"))
                .is_none()
        );
    }
}

fn budget(code: &str, id: u32) -> usize {
    if code == "oesw" && id == 320 { 2873 } else { 0 }
}

fn append_child(expected: &mut Vec<bool>, code: &str, id: u32, bits: &[bool]) {
    expected.extend(field(id));
    expected.extend(bits);
    let padding = budget(code, id).saturating_sub(bits.len());
    expected.extend(std::iter::repeat_n(false, padding));
}

fn emit(code: &str, props: &[ItemProperty], children: &[Item], offset: usize) -> BitEmitter {
    let mut emitter = BitEmitter::new();
    emitter.extend_bits(prefix(offset)).unwrap();
    write_property_list(
        &mut emitter,
        code,
        props,
        children,
        &HuffmanTree::new(),
        0,
        true,
        false,
        false,
        ItemQuality::Normal,
        false,
        &axiom(code),
    )
    .unwrap();
    emitter
}

fn assert_output(mut emitter: BitEmitter, mut expected: Vec<bool>) {
    assert_eq!(emitter.written_bits(), expected.len() as u64);
    emitter.extend_bits(GUARD).unwrap();
    expected.extend(GUARD);
    assert_eq!(emitter.bits(), expected);
    assert_eq!(emitter.into_bytes(), packed(&expected));
}

#[invariant_test]
fn cached_nested_slots_preserve_order_padding_empty_and_oversized_children() {
    assert_budget_inputs();
    let ids = [317, 320, 387];
    let props: Vec<_> = ids.into_iter().map(property).collect();
    let mut cases = 0;
    for code in ["plain", "oesw", "wa2"] {
        for length in [0, 1, 9, 2872, 2873, 2874] {
            let children: Vec<_> = [7, length, 13]
                .into_iter()
                .enumerate()
                .map(|(seed, size)| {
                    let mut child = Item::default();
                    child.code = if code == "wa2" { "@" } else { "\u{00fe}" }.into();
                    child.bits = (0..size)
                        .map(|i| RecordedBit {
                            offset: 100 + i as u64,
                            bit: (i + seed) % 3 == 0,
                        })
                        .collect();
                    // Disable child.to_bits' raw replay: the host must choose its raw-child branch.
                    child.socketed_items.push(Item::default());
                    child
                })
                .collect();
            let before = children.clone();
            for offset in [0, 3, 7] {
                let mut expected = prefix(offset);
                for (&id, child) in ids.iter().zip(&children) {
                    let bits: Vec<_> = child.bits.iter().map(|bit| bit.bit).collect();
                    append_child(&mut expected, code, id, &bits);
                }
                assert_output(emit(code, &props, &children, offset), expected);
                assert_eq!(children, before);
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 54);
}

fn semantic_child(code: &str) -> Item {
    let mut child = Item::default();
    child.code = code.into();
    child.body.code = code.into();
    child.header.version = 0;
    child.header.is_compact = true;
    child.header.save_is_alpha = true;
    child.header.quality = Some(ItemQuality::Normal);
    child
}

#[invariant_test]
fn ascii_nested_child_callback_preserves_cache_free_emission_and_ownership() {
    assert_budget_inputs();
    let child = semantic_child("abc");
    assert!(child.bits.is_empty() && child.modules.is_empty() && child.socketed_items.is_empty());
    let child_bits = child.to_bits(0, &HuffmanTree::new(), true).unwrap();
    assert!(!child_bits.is_empty());
    let before = child.clone();
    let mut cases = 0;
    for code in ["plain", "oesw"] {
        for id in [317, 320, 387] {
            for offset in [0, 3, 7] {
                let mut expected = prefix(offset);
                append_child(&mut expected, code, id, &child_bits);
                assert_output(
                    emit(code, &[property(id)], std::slice::from_ref(&child), offset),
                    expected,
                );
                assert_eq!(child, before);
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 18);
}

#[invariant_test]
fn nested_child_io_failure_preserves_error_and_parent_prefix() {
    let child = semantic_child("@");
    let tree = HuffmanTree::new();
    let direct_error = child.to_bits(0, &tree, true).unwrap_err();
    assert_eq!(direct_error.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(direct_error.to_string(), "Char '@' not in Huffman table");
    let before = child.clone();
    let mut cases = 0;
    for code in ["plain", "oesw"] {
        for id in [317, 320, 387] {
            for offset in [0, 3, 7] {
                let mut emitter = BitEmitter::new();
                emitter.extend_bits(prefix(offset)).unwrap();
                let error = write_property_list(
                    &mut emitter,
                    code,
                    &[property(id)],
                    std::slice::from_ref(&child),
                    &tree,
                    0,
                    true,
                    false,
                    false,
                    ItemQuality::Normal,
                    false,
                    &axiom(code),
                )
                .unwrap_err();
                assert_eq!(error.kind(), direct_error.kind());
                assert_eq!(error.to_string(), direct_error.to_string());
                let expected: Vec<_> = prefix(offset).into_iter().chain(field(id)).collect();
                assert_output(emitter, expected);
                assert_eq!(child, before);
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 18);
}
