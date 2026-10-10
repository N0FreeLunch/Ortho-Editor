//! Consumer-facing player item stream selection and semantic placement.
//! A shared outer save version is not sufficient evidence for an item dialect.

use super::{HuffmanTree, Item};
use crate::verify::alpha_inventory_routing::{
    alpha_inventory_route, is_belt_consumable, AlphaInventoryRoute,
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemStreamDialect {
    D2rV3,
    Alpha,
    Legacy,
}

#[derive(Debug)]
pub struct PlayerItemStream {
    pub dialect: ItemStreamDialect,
    pub items: Vec<Item>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemContainer {
    Equipment,
    Inventory,
    Belt,
    Stash,
    Cube,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StreamItemPlacement {
    pub container: ItemContainer,
    pub x: u8,
    pub y: u8,
    pub equipment_slot: Option<&'static str>,
    pub diagnostic: Option<&'static str>,
}

impl PlayerItemStream {
    /// Prefer a structurally closed, lossless counted v3 list. Rejected candidates
    /// retain the legacy recovery path and an explicit diagnostic for consumers.
    /// This bounded policy does not assert universal dialect detection.
    pub fn read(bytes: &[u8], huffman: &HuffmanTree) -> Result<Self, String> {
        let version = bytes
            .get(4..8)
            .and_then(|raw| <[u8; 4]>::try_from(raw).ok())
            .map(u32::from_le_bytes)
            .ok_or_else(|| "Truncated save version".to_string())?;
        if bytes.get(..4) != Some(crate::save::D2S_MAGIC.to_le_bytes().as_slice()) {
            return Err("Player item streams require a full save header".to_string());
        }
        let mut diagnostics = Vec::new();
        if version == 0x69 {
            let candidate = Item::read_player_items_d2r_v3(bytes, huffman)
                .map_err(|error| error.to_string())
                .and_then(|mut items| {
                    for item in &mut items {
                        retain_source_span(bytes, item)?;
                    }
                    validate_v3_list(bytes, &items)?;
                    Ok(items)
                });
            match candidate {
                Ok(items) => {
                    return Ok(Self {
                        dialect: ItemStreamDialect::D2rV3,
                        items,
                        diagnostics,
                    });
                }
                Err(error) => diagnostics.push(format!(
                    "D2R v3 candidate rejected; using Alpha recovery: {error}"
                )),
            }
        }
        let alpha = version == 105;
        let items = Item::read_player_items(bytes, huffman, alpha)
            .map_err(|error| format!("Player item recovery failed: {error}"))?;
        if items.is_empty() {
            diagnostics.push("The recovery reader returned no player items".to_string());
        }
        Ok(Self {
            dialect: if alpha {
                ItemStreamDialect::Alpha
            } else {
                ItemStreamDialect::Legacy
            },
            items,
            diagnostics,
        })
    }

    pub fn placement(&self, item: &Item) -> StreamItemPlacement {
        self.dialect.placement(item)
    }
}

fn retain_source_span(bytes: &[u8], item: &mut Item) -> Result<(), String> {
    if item.range.start >= item.range.end || item.range.end > bytes.len() as u64 * 8 {
        return Err("Player item source range is invalid".to_string());
    }
    // The primitive reader's socket synchronization invalidates its cache.
    // Preserve the owned source interval here; do not change semantic fields.
    if item.bits.is_empty() {
        item.bits = (item.range.start..item.range.end)
            .map(|offset| super::RecordedBit {
                offset,
                bit: bytes[(offset / 8) as usize] & (1 << (offset % 8)) != 0,
            })
            .collect();
    }
    if item.bits.len() as u64 != item.range.end - item.range.start
        || item.bits.iter().enumerate().any(|(index, recorded)| {
            let offset = item.range.start + index as u64;
            recorded.offset != offset
                || recorded.bit != (bytes[(offset / 8) as usize] & (1 << (offset % 8)) != 0)
        })
    {
        return Err("Retained player item cache differs from its source range".to_string());
    }
    for child in &mut item.socketed_items {
        if child.range.start < item.range.start || child.range.end > item.range.end {
            return Err("Socket child source range exceeds its parent".to_string());
        }
        retain_source_span(bytes, child)?;
    }
    Ok(())
}

fn validate_v3_list(bytes: &[u8], items: &[Item]) -> Result<(), String> {
    let first = items
        .first()
        .ok_or_else(|| "An empty list cannot establish the item dialect".to_string())?;
    let header = first
        .range
        .start
        .checked_div(8)
        .and_then(|start| start.checked_sub(4))
        .and_then(|start| bytes.get(start as usize..start as usize + 4))
        .ok_or_else(|| "Missing counted player list header".to_string())?;
    if &header[..2] != b"JM" || u16::from_le_bytes([header[2], header[3]]) as usize != items.len() {
        return Err("Counted player list header disagrees with parsed items".to_string());
    }
    let mut next = first.range.start;
    for item in items {
        if item.range.start != next
            || item.range.end <= next
            || item.range.end % 8 != 0
            || item.total_bits != item.range.end - next
            || item.bits.len() as u64 != item.total_bits
            || item.is_opaque()
            || item.is_semi_opaque()
            || item.code.trim().is_empty()
            || (item.body.d2r_v3_auxiliary.is_none() && !item.is_compact)
        {
            return Err(format!(
                "Incomplete counted item {}: range={}..{}, expected_start={}, total={}, cached={}, opaque={}, semi_opaque={}, compact={}, auxiliary={}",
                item.code.trim(), item.range.start, item.range.end, next,
                item.total_bits, item.bits.len(), item.is_opaque(), item.is_semi_opaque(),
                item.is_compact, item.body.d2r_v3_auxiliary.is_some()
            ));
        }
        for (index, recorded) in item.bits.iter().enumerate() {
            let offset = next + index as u64;
            let source = bytes
                .get((offset / 8) as usize)
                .map(|byte| byte & (1 << (offset % 8)) != 0);
            if recorded.offset != offset || source != Some(recorded.bit) {
                return Err("Counted player item raw span differs from source".to_string());
            }
        }
        next = item.range.end;
    }
    if bytes.get(next as usize / 8..next as usize / 8 + 2) != Some(b"JM".as_slice()) {
        return Err("Counted player list does not close at the following JM section".to_string());
    }
    Ok(())
}

impl ItemStreamDialect {
    pub fn placement(self, item: &Item) -> StreamItemPlacement {
        let mut result = StreamItemPlacement {
            container: ItemContainer::Unknown,
            x: item.x,
            y: item.y,
            equipment_slot: None,
            diagnostic: None,
        };
        if item.is_residue() || item.code.trim().is_empty() {
            result.diagnostic = Some("Unresolved item fragment");
            return result;
        }
        if self == Self::D2rV3 {
            result.container = match item.mode {
                1 => {
                    result.equipment_slot = body_slot(item.location);
                    if result.equipment_slot.is_some() {
                        ItemContainer::Equipment
                    } else {
                        ItemContainer::Unknown
                    }
                }
                2 if is_belt_consumable(&item.code) && item.x < 16 => {
                    result.x = item.x % 4;
                    result.y = item.x / 4;
                    ItemContainer::Belt
                }
                // Compact items have no extended auxiliary payload; their typed
                // header page already contains stored-in minus one.
                0 => match item
                    .body
                    .d2r_v3_auxiliary
                    .as_ref()
                    .and_then(|aux| aux.stored_in)
                    .or_else(|| item.is_compact.then(|| item.page.saturating_add(1)))
                {
                    Some(1) => ItemContainer::Inventory,
                    Some(4) => ItemContainer::Cube,
                    Some(5) => ItemContainer::Stash,
                    _ => ItemContainer::Unknown,
                },
                _ => ItemContainer::Unknown,
            };
        } else {
            result.container = match alpha_inventory_route(item, self == Self::Alpha) {
                AlphaInventoryRoute::Equipment => ItemContainer::Equipment,
                AlphaInventoryRoute::Inventory => ItemContainer::Inventory,
                AlphaInventoryRoute::Belt => ItemContainer::Belt,
                AlphaInventoryRoute::Stash => ItemContainer::Stash,
                AlphaInventoryRoute::Cube => ItemContainer::Cube,
                AlphaInventoryRoute::Unknown => ItemContainer::Unknown,
            };
            if result.container == ItemContainer::Equipment {
                result.equipment_slot = if self == Self::Alpha {
                    match item.x {
                        0 => Some("Armor"),
                        1 => Some("Weapon1"),
                        2 => Some("Weapon2"),
                        3 => Some("Head"),
                        other => body_slot(other),
                    }
                } else {
                    body_slot(item.x)
                };
            }
        }
        if result.container == ItemContainer::Unknown {
            result.diagnostic = Some("Unsupported or invalid item placement claim");
        }
        result
    }
}

fn body_slot(position: u8) -> Option<&'static str> {
    match position {
        1 => Some("Head"),
        2 => Some("Amulet"),
        3 => Some("Armor"),
        4 | 11 => Some("Weapon1"),
        5 | 12 => Some("Weapon2"),
        6 => Some("Ring1"),
        7 => Some("Ring2"),
        8 => Some("Belt"),
        9 => Some("Boots"),
        10 => Some("Gloves"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2r_macros::settled_contract_test;

    #[settled_contract_test]
    fn player_stream_preserves_recovery_diagnostics_and_owned_source_ranges() {
        let mut bytes = crate::save::D2S_MAGIC.to_le_bytes().to_vec();
        bytes.extend_from_slice(&105_u32.to_le_bytes());
        bytes.extend_from_slice(b"JM\0\0");
        let stream = PlayerItemStream::read(&bytes, &HuffmanTree::new()).unwrap();
        assert_eq!(stream.dialect, ItemStreamDialect::Alpha);
        assert!(stream.items.is_empty());
        assert!(stream.diagnostics[0].contains("D2R v3 candidate rejected"));

        let mut parent = Item::default();
        parent.range.start = 8;
        parent.range.end = 24;
        let mut child = Item::default();
        child.range.start = 16;
        child.range.end = 24;
        parent.socketed_items.push(child);
        retain_source_span(&bytes, &mut parent).unwrap();
        assert_eq!(parent.bits.len(), 16);
        assert_eq!(parent.socketed_items[0].bits.len(), 8);
        parent.socketed_items[0].bits[0].bit ^= true;
        assert!(retain_source_span(&bytes, &mut parent).is_err());
        parent.socketed_items[0].range.end = 25;
        assert!(retain_source_span(&bytes, &mut parent).is_err());
    }

    #[settled_contract_test]
    fn player_stream_selects_closed_v3_lists_and_rejects_broken_closure() {
        for (fixture, count) in [
            ("gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s", 43),
            ("beta/amazon_v105_slice2_equipment.d2s", 16),
        ] {
            let bytes = std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/savegames")
                    .join(fixture),
            )
            .unwrap();
            let mut explicit = Item::read_player_items_d2r_v3(&bytes, &HuffmanTree::new()).unwrap();
            for item in &mut explicit {
                retain_source_span(&bytes, item).unwrap();
            }
            validate_v3_list(&bytes, &explicit).expect(fixture);
            let stream = PlayerItemStream::read(&bytes, &HuffmanTree::new()).unwrap();
            assert_eq!(
                stream.dialect,
                ItemStreamDialect::D2rV3,
                "{fixture}: {:?}",
                stream.diagnostics
            );
            assert_eq!(stream.items.len(), count);
            assert!(stream.diagnostics.is_empty());
            let end = stream.items.last().unwrap().range.end as usize / 8;
            let mut corrupt = bytes.clone();
            corrupt[end] ^= 1;
            assert!(validate_v3_list(&corrupt, &stream.items).is_err());
            corrupt = bytes.clone();
            corrupt[stream.items[0].range.start as usize / 8] ^= 1;
            assert!(validate_v3_list(&corrupt, &stream.items).is_err());
        }
        assert!(PlayerItemStream::read(&[0; 7], &HuffmanTree::new()).is_err());
    }

    #[settled_contract_test]
    fn v3_placement_uses_body_slot_and_belt_index_with_admissibility() {
        let mut item = Item {
            code: "rin".into(),
            ..Item::default()
        };
        item.mode = 1;
        item.location = 6;
        item.x = 0;
        let placed = ItemStreamDialect::D2rV3.placement(&item);
        assert_eq!(placed.equipment_slot, Some("Ring1"));
        item.mode = 2;
        item.x = 9;
        assert_eq!(
            ItemStreamDialect::D2rV3.placement(&item).container,
            ItemContainer::Unknown
        );
        item.code = "tsc".into();
        let placed = ItemStreamDialect::D2rV3.placement(&item);
        assert_eq!(
            (placed.container, placed.x, placed.y),
            (ItemContainer::Belt, 1, 2)
        );
        item.x = 16;
        assert_eq!(
            ItemStreamDialect::D2rV3.placement(&item).container,
            ItemContainer::Unknown
        );
    }
}
