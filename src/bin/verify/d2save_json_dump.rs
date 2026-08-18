use d2r_core::domain::character::skills::parse_skill_section;
use d2r_core::domain::forensic::v105::{MercenaryEquipmentItem, MercenaryFooter};
use d2r_core::domain::progression::Progression;
use d2r_core::domain::progression::waypoint::WaypointSet;
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::save::{AttributeSection, Save, class_skill_base_id, map_core_sections};
use d2r_core::verify::alpha_inventory_routing::{AlphaInventoryRoute, alpha_inventory_route};
use serde_json::json;
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        println!("Usage: d2save_json_dump <file.d2s> <output.json>");
        return;
    }

    let bytes = fs::read(&args[1]).expect("Failed to read d2s file");
    let save = Save::from_bytes(&bytes).expect("Failed to parse save header");
    let map = map_core_sections(&bytes).expect("Failed to map sections");

    let is_alpha = save.header.version == 105;

    // Parse attributes (gf section)
    let attrs = AttributeSection::parse(&bytes, map.gf_pos, map.if_pos)
        .expect("Failed to parse attributes");

    let get_stat = |id: u32| -> i32 { attrs.actual_value(id, is_alpha).unwrap_or(0) };

    // Extract basic character stats (scale life/mana/stamina by 256)
    let strength = get_stat(0);
    let energy = get_stat(1);
    let dexterity = get_stat(2);
    let vitality = get_stat(3);
    let stat_points_left = get_stat(4);
    let skill_points_left = get_stat(5);
    let life = get_stat(6) / 256;
    let max_life = get_stat(7) / 256;
    let mana = get_stat(8) / 256;
    let max_mana = get_stat(9) / 256;
    let stamina = get_stat(10) / 256;
    let max_stamina = get_stat(11) / 256;
    let level = get_stat(12);
    let experience = get_stat(13);
    let gold_inventory = get_stat(14);
    let gold_stash = get_stat(15);

    // Build all_attributes JSON list
    let mut all_attributes_json = Vec::new();
    for entry in &attrs.entries {
        let name = d2r_core::data::stat_costs::STAT_COSTS
            .iter()
            .find(|s| s.id == entry.stat_id)
            .map(|s| s.name.as_ref())
            .unwrap_or("Unknown");
        let act_val = attrs
            .actual_value(entry.stat_id, is_alpha)
            .unwrap_or(entry.raw_value as i32);
        all_attributes_json.push(json!({
            "stat_id": entry.stat_id,
            "name": name,
            "raw_value": entry.raw_value,
            "actual_value": act_val
        }));
    }

    // Build learned skills list
    let mut skills_json = Vec::new();
    let jm0 = map.jm_positions.first().copied();
    if let Ok(skills) = parse_skill_section(&bytes, map.if_pos, jm0) {
        if let Some(base_id) = class_skill_base_id(save.header.char_class) {
            let class_skills = skills.iter_skills(base_id);
            for skill_level in class_skills {
                if skill_level.level > 0 {
                    let skill_name = d2r_core::data::skills::SKILLS
                        .iter()
                        .find(|s| s.id == skill_level.skill_id)
                        .map(|s| s.key)
                        .unwrap_or("Unknown Skill");
                    skills_json.push(json!({
                        "id": skill_level.skill_id,
                        "name": skill_name,
                        "level": skill_level.level
                    }));
                }
            }
        }
    }

    // Resolve expansion flag for waypoints
    let is_expansion = if bytes.len() > d2r_core::domain::header::axiom::EXPANSION_FLAG_OFFSET {
        (bytes[d2r_core::domain::header::axiom::EXPANSION_FLAG_OFFSET] & 0x20) != 0
    } else {
        true
    };

    // progression waypoints (difficulty: 0=normal, 1=nightmare, 2=hell)
    let wp_anchor = d2r_core::domain::progression::axiom::PROG_START_FILE
        + d2r_core::domain::progression::axiom::V105WaypointAxiom::start_offset();
    let wp_bytes = if bytes.len() > d2r_core::domain::progression::axiom::V105_WAYPOINT_OFFSET {
        &bytes[d2r_core::domain::progression::axiom::V105_WAYPOINT_OFFSET..]
    } else {
        &[]
    };

    let normal_wps = WaypointSet::from_bytes(wp_bytes, 0, wp_anchor, is_expansion);
    let nightmare_wps = WaypointSet::from_bytes(wp_bytes, 1, wp_anchor, is_expansion);
    let hell_wps = WaypointSet::from_bytes(wp_bytes, 2, wp_anchor, is_expansion);

    let get_active_wp_ids = |wp_set: &WaypointSet| -> Vec<u8> {
        wp_set
            .waypoints()
            .iter()
            .filter(|w| w.is_active())
            .map(|w| w.ws_bit())
            .collect()
    };

    let waypoints_json = json!({
        "normal": get_active_wp_ids(&normal_wps),
        "nightmare": get_active_wp_ids(&nightmare_wps),
        "hell": get_active_wp_ids(&hell_wps)
    });

    // Parse quests progress
    let mut normal_quests = Vec::new();
    let mut nightmare_quests = Vec::new();
    let mut hell_quests = Vec::new();

    if let Ok(prog_res) = Progression::from_bytes(&bytes, is_alpha).value {
        for quest in prog_res.quests.quests() {
            if quest.is_completed() {
                let key = format!("a{}q{}", quest.act(), quest.index() + 1);
                match quest.difficulty() {
                    0 => normal_quests.push(key),
                    1 => nightmare_quests.push(key),
                    2 => hell_quests.push(key),
                    _ => {}
                }
            }
        }
    }

    let quests_json = json!({
        "normal": normal_quests,
        "nightmare": nightmare_quests,
        "hell": hell_quests
    });

    // Parse mercenary details
    let header =
        &bytes[0..d2r_core::domain::progression::axiom::V105_WAYPOINT_OFFSET.min(bytes.len())];
    let w4_bytes = if bytes.len() > d2r_core::domain::progression::axiom::V105_NPC_OFFSET {
        Some(&bytes[d2r_core::domain::progression::axiom::V105_NPC_OFFSET..])
    } else {
        None
    };
    let merc_state =
        d2r_core::domain::forensic::v105::mercenary::MercenaryState::from_hybrid(header, w4_bytes);
    let huffman = HuffmanTree::new();
    let merc_equipped_items = parse_mercenary_equipped_items(&bytes, &map, &huffman, is_alpha);

    let merc_json = json!({
        "class_name": merc_state.class_name(),
        "hireling_id": merc_state.hireling_id,
        "subtype_id": merc_state.subtype_id,
        "experience": merc_state.experience,
        "equipped_items": merc_equipped_items
    });

    // Parse player items to build equipment, inventory, belt, stash, and cube lists
    let mut equipment_json = Vec::new();
    let mut inventory_json = Vec::new();
    let mut unknown_json = Vec::new();
    let mut belt_json = Vec::new();
    let mut stash_json = Vec::new();
    let mut cube_json = Vec::new();

    if let Ok(items) = Item::read_player_items(&bytes, &huffman, is_alpha) {
        for (i, item) in items.iter().enumerate() {
            // Filter out residue and structural summary items (like ks d, b7ts)
            let trimmed_code = item.code.trim();
            if item.is_residue()
                || trimmed_code.is_empty()
                || trimmed_code == "ks d"
                || trimmed_code == "b7ts"
            {
                continue;
            }

            let (name_en, name_ko) = get_item_localization(&item.code);
            let (width, height) = d2r_core::inventory::get_item_size(&item.code);
            let q_str = if item.header.is_runeword {
                "Runeword"
            } else {
                quality_to_str(item.header.quality)
            };

            let mut socket_json = Vec::new();
            for (si, s_item) in item.socketed_items.iter().enumerate() {
                let (s_name_en, s_name_ko) = get_item_localization(&s_item.code);
                socket_json.push(json!({
                    "index": si,
                    "code": s_item.code.trim(),
                    "name_en": s_name_en,
                    "name_ko": s_name_ko,
                    "quality": quality_to_str(s_item.header.quality),
                    "properties": serialize_item_properties(&s_item.properties),
                }));
            }

            let mut props_json = serialize_item_properties(&item.properties);
            if props_json.is_empty() {
                if let Some(payload) = &item.body.v105_7mgw_payload {
                    let decoded = decode_opaque_bits_properties(payload);
                    if !decoded.is_empty() {
                        props_json = decoded;
                    }
                }
            }

            let item_data = json!({
                "index": i,
                "code": item.code.trim(),
                "name_en": name_en,
                "name_ko": name_ko,
                "type": d2r_core::inventory::get_item_category(&item.code),
                "quality": q_str,
                "x": item.x,
                "y": item.y,
                "width": width,
                "height": height,
                "is_equipped": item.mode == 1,
                "location": item.location,
                "socketed_items": socket_json,
                "properties": props_json,
                "set_attributes": item.set_attributes.iter().map(|set_props| serialize_item_properties(set_props)).collect::<Vec<_>>(),
                "runeword_attributes": serialize_item_properties(&item.runeword_attributes),
                "opaque_info": json!({
                    "is_opaque": item.body.v105_7mgw_payload.is_some(),
                    "bit_count": item.body.v105_7mgw_payload.as_ref().map(|b| b.len()).unwrap_or(0)
                }),
                "defense": item.defense,
                "max_durability": item.max_durability,
                "current_durability": item.current_durability,
                "quantity": item.quantity,
            });

            match alpha_inventory_route(item, is_alpha) {
                AlphaInventoryRoute::Equipment => {
                    let mut eq_data = item_data.clone();
                    // Alpha v105 heuristic: 0=Armor, 1=Weapon1 observed in fixtures
                    let slot_name = if is_alpha {
                        match item.x {
                            0 => "Armor",
                            1 => "Weapon1",
                            2 => "Weapon2",
                            3 => "Head",
                            _ => body_position_to_slot(item.x),
                        }
                    } else {
                        body_position_to_slot(item.x)
                    };
                    eq_data["slot_en"] = json!(slot_name);
                    equipment_json.push(eq_data);
                }
                AlphaInventoryRoute::Belt => {
                    belt_json.push(item_data);
                }
                AlphaInventoryRoute::Inventory => {
                    inventory_json.push(item_data);
                }
                AlphaInventoryRoute::Unknown => {
                    unknown_json.push(item_data);
                }
                AlphaInventoryRoute::Stash => {
                    stash_json.push(item_data);
                }
                AlphaInventoryRoute::Cube => {
                    cube_json.push(item_data);
                }
            }
        }
    }

    // Map entire payload to match index.html expected format
    let payload = json!({
        "character": {
            "name": save.header.char_name,
            "class": d2r_core::save::class_name(save.header.char_class),
            "level": level,
            "experience": experience,
            "stats": {
                "strength": strength,
                "energy": energy,
                "dexterity": dexterity,
                "vitality": vitality,
                "stat_points_left": stat_points_left,
                "skill_points_left": skill_points_left,
                "life": life,
                "max_life": max_life,
                "mana": mana,
                "max_mana": max_mana,
                "stamina": stamina,
                "max_stamina": max_stamina
            },
            "gold": {
                "inventory": gold_inventory,
                "stash": gold_stash
            }
        },
        "all_attributes": all_attributes_json,
        "skills": skills_json,
        "waypoints": waypoints_json,
        "quests": quests_json,
        "equipment": equipment_json,
        "inventory": inventory_json,
        "unknown": unknown_json,
        "belt": belt_json,
        "stash": stash_json,
        "cube": cube_json,
        "mercenary": merc_json
    });

    let out_str = serde_json::to_string_pretty(&payload).expect("Failed to serialize JSON");
    fs::write(&args[2], out_str).expect("Failed to write output JSON");
    println!("Successfully dumped save game JSON to {}", args[2]);
}

fn parse_mercenary_equipped_items(
    bytes: &[u8],
    map: &d2r_core::save::SaveSectionMap,
    huffman: &HuffmanTree,
    is_alpha: bool,
) -> Vec<serde_json::Value> {
    let Some(jf) = map.jf_pos.or_else(|| find_marker(bytes, b"jf")) else {
        return Vec::new();
    };

    let jm_positions = d2r_core::save::find_jm_markers(bytes);
    let Some(merc_jm_idx) = jm_positions
        .iter()
        .enumerate()
        .find_map(|(idx, pos)| (*pos > jf).then_some(idx))
    else {
        return Vec::new();
    };

    let jm_pos = jm_positions[merc_jm_idx];
    if jm_pos + 4 > bytes.len() {
        return Vec::new();
    }

    let item_count = u16::from_le_bytes([bytes[jm_pos + 2], bytes[jm_pos + 3]]);
    let (Some(kf), Some(lf)) = (
        map.kf_pos.or_else(|| find_marker(bytes, b"kf")),
        map.lf_pos.or_else(|| find_marker(bytes, b"lf")),
    ) else {
        return Vec::new();
    };

    let next_pos = jm_positions
        .get(merc_jm_idx + 1)
        .copied()
        .unwrap_or(bytes.len());
    let items_start = jm_pos;
    let items_end = kf.min(next_pos);
    if !MercenaryFooter::from_bytes(&bytes[kf..]).is_standard()
        || lf < kf
        || items_start >= items_end
    {
        return Vec::new();
    }

    let items_data = &bytes[items_start..items_end];
    let Ok(items) = Item::read_section(
        items_data,
        items_start as u64 * 8,
        item_count,
        huffman,
        is_alpha,
        false,
    ) else {
        return Vec::new();
    };

    items
        .into_iter()
        .enumerate()
        .filter_map(|(i, item)| {
            let trimmed_code = item.code.trim();
            if !is_alpha && trimmed_code.is_empty() {
                return None;
            }

            let code_display = if trimmed_code.is_empty() {
                "unk" // Unknown code placeholder
            } else {
                trimmed_code
            };

            let merc_item = MercenaryEquipmentItem {
                code: code_display.to_string(),
                location: item.location,
                mode: item.mode,
                x: item.x,
                y: item.y,
            };
            let (name_en, name_ko) = get_item_localization(code_display);
            let (width, height) = d2r_core::inventory::get_item_size(code_display);
            let quality = if item.header.is_runeword {
                "Runeword"
            } else {
                quality_to_str(item.header.quality)
            };

            let (slot_en, slot_source, candidate_kind) = if is_alpha {
                alpha_mercenary_slot_semantics(&merc_item, item.is_residue())
            } else {
                (
                    merc_item.slot_name(),
                    "shared_mercenary_slot_name",
                    "legacy_candidate",
                )
            };

            Some(json!({
                "index": i,
                "code": code_display,
                "name_en": name_en,
                "name_ko": name_ko,
                "type": d2r_core::inventory::get_item_category(code_display),
                "quality": quality,
                "x": item.x,
                "y": item.y,
                "width": width,
                "height": height,
                "is_equipped": item.mode == 1 || (is_alpha && item.mode != 0),
                "location": item.location,
                "mode": item.mode,
                "slot_en": slot_en,
                "slot_source": slot_source,
                "candidate_kind": candidate_kind,
                "properties": serialize_item_properties(&item.properties),
                "set_attributes": item.set_attributes.iter().map(|set_props| serialize_item_properties(set_props)).collect::<Vec<_>>(),
                "runeword_attributes": serialize_item_properties(&item.runeword_attributes),
                "opaque_info": json!({
                    "is_opaque": item.body.v105_7mgw_payload.is_some(),
                    "bit_count": item.body.v105_7mgw_payload.as_ref().map(|b| b.len()).unwrap_or(0)
                }),
                "defense": item.defense,
                "max_durability": item.max_durability,
                "current_durability": item.current_durability,
                "quantity": item.quantity,
            }))
        })
        .collect()
}

fn alpha_mercenary_slot_semantics(
    item: &MercenaryEquipmentItem,
    is_residue: bool,
) -> (String, &'static str, &'static str) {
    let slot = match (item.location, item.mode, item.x) {
        (3, 1, _) => Some("Armor"),
        (4, 1, _) => Some("Weapon1"),
        (1, 1, 1) => Some("Helm"),
        (1, 1, 3) => Some("Armor"),
        (1, 1, 4) => Some("Weapon1"),
        (1, 1, 5) => Some("Weapon2"),
        (4, 5, _) => Some("Weapon1"),
        (1, 4, _) => Some("Weapon2"),
        (4, 0, _) => Some("Helm"),
        (0, 3, _) => Some("Weapon1"), // Rogue Bow/Weapon candidate (observed mode 3)
        (0, 5, _) => Some("Weapon1"), // Rogue Bow/Weapon candidate (observed mode 5)
        _ => None,
    };

    match slot {
        Some(s) => {
            let kind = if is_residue {
                "parser_residue_in_valid_slot"
            } else if item.code == "unk" {
                "unknown_code_localization_gap"
            } else {
                "equipped_slot_candidate"
            };
            (s.to_string(), "alpha_v105_signature", kind)
        }
        None => (
            "Unknown".to_string(),
            "unclassified_alpha_v105_candidate",
            "parser_residue",
        ),
    }
}

fn find_marker(bytes: &[u8], marker: &[u8; 2]) -> Option<usize> {
    bytes.windows(2).position(|window| window == marker)
}

// Helper: map body slot index to EN slot name string
fn body_position_to_slot(pos: u8) -> &'static str {
    match pos {
        1 => "Head",
        2 => "Amulet",
        3 => "Armor",
        4 => "Weapon1",
        5 => "Weapon2",
        6 => "Ring1",
        7 => "Ring2",
        8 => "Belt",
        9 => "Boots",
        10 => "Gloves",
        _ => "None",
    }
}

// Helper: get standard quality string representation
fn quality_to_str(q: Option<d2r_core::domain::item::quality::ItemQuality>) -> &'static str {
    use d2r_core::domain::item::quality::ItemQuality;
    match q {
        Some(ItemQuality::Low) => "Low",
        Some(ItemQuality::Normal) => "Normal",
        Some(ItemQuality::High) => "High",
        Some(ItemQuality::Magic) => "Magic",
        Some(ItemQuality::Set) => "Set",
        Some(ItemQuality::Rare) => "Rare",
        Some(ItemQuality::Unique) => "Unique",
        Some(ItemQuality::Crafted) => "Crafted",
        None => "None",
    }
}

// Helper: serialize list of ItemProperty into serde_json::Value
fn serialize_item_properties(props: &[d2r_core::item::ItemProperty]) -> Vec<serde_json::Value> {
    props
        .iter()
        .map(|p| {
            let stat_name = if !p.name.trim().is_empty() {
                p.name.clone()
            } else {
                d2r_core::data::stat_costs::STAT_COSTS
                    .iter()
                    .find(|s| s.id == p.stat_id)
                    .map(|s| s.name.to_string())
                    .unwrap_or_else(|| format!("stat_{}", p.stat_id))
            };
            json!({
                "stat_id": p.stat_id,
                "name": stat_name,
                "param": p.param,
                "raw_value": p.raw_value,
                "value": p.value,
            })
        })
        .collect()
}

// Helper: search localization dictionary for item name translation
fn get_item_localization(code: &str) -> (String, String) {
    let trimmed = code.trim().to_lowercase();
    let entry = d2r_core::data::localization::LOCALIZATIONS
        .iter()
        .find(|loc| loc.key.to_lowercase() == trimmed);

    if let Some(e) = entry {
        (e.en.to_string(), e.ko.to_string())
    } else {
        let name_en = d2r_core::data::item_codes::ITEM_TEMPLATES
            .iter()
            .find(|t| t.code == trimmed)
            .map(|t| t.name)
            .unwrap_or(code);
        (name_en.to_string(), name_en.to_string())
    }
}

// Helper: decode properties from opaque bitstreams
fn decode_opaque_bits_properties(bits: &[bool]) -> Vec<serde_json::Value> {
    let mut results = Vec::new();
    if bits.len() < 18 {
        return results;
    }

    let mut bit_pos = 0;
    while bit_pos + 18 <= bits.len() {
        let mut stat_id = 0u32;
        for i in 0..9 {
            if bits[bit_pos + i] {
                stat_id |= 1 << i;
            }
        }
        if stat_id == 0x1FF {
            break;
        }

        let mut raw_val = 0u32;
        for i in 0..9 {
            if bits[bit_pos + 9 + i] {
                raw_val |= 1 << i;
            }
        }

        if let Some(stat) = d2r_core::data::stat_costs::STAT_COSTS.iter().find(|s| s.id == stat_id) {
            results.push(json!({
                "stat_id": stat_id,
                "name": stat.name.to_string(),
                "param": 0,
                "raw_value": raw_val,
                "value": raw_val as i32,
            }));
        } else if stat_id < 511 && raw_val > 0 {
            results.push(json!({
                "stat_id": stat_id,
                "name": format!("stat_{}", stat_id),
                "param": 0,
                "raw_value": raw_val,
                "value": raw_val as i32,
            }));
        }
        bit_pos += 18;
    }
    results
}
