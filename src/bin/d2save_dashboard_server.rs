use d2r_core::data::item_specs::get_base_item_spec;
use d2r_core::data::set_items::SET_ITEMS;
use d2r_core::domain::character::skills::parse_skill_section;
use d2r_core::domain::item::quality::ItemQuality;
use d2r_core::domain::item::stream::{ItemContainer, PlayerItemStream};
use d2r_core::domain::item::ItemModule;
use d2r_core::domain::progression::waypoint::WaypointSet;
use d2r_core::domain::progression::Progression;
use d2r_core::engine::formatter::{format_item, FormattedSetBonus};
use d2r_core::item::{HuffmanTree, Item, ItemProperty};
use d2r_core::save::{class_skill_base_id, map_core_sections, AttributeSection, Save};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

const MAX_SAVE_BYTES: usize = 16 * 1024 * 1024;

fn main() -> std::io::Result<()> {
    let port = std::env::args()
        .skip(1)
        .find_map(|arg| arg.strip_prefix("--port=").and_then(|v| v.parse().ok()))
        .unwrap_or(8765);
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    eprintln!("dashboard server listening on http://127.0.0.1:{port}");
    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            let _ = handle(stream);
        }
    }
    Ok(())
}

fn handle(mut stream: TcpStream) -> std::io::Result<()> {
    let request = read_request(&mut stream)?;
    let Some(split) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
        return reply(&mut stream, 400, "text/plain", b"Malformed request");
    };
    let (head, body) = request.split_at(split + 4);
    let head = String::from_utf8_lossy(head);
    let mut words = head.lines().next().unwrap_or_default().split_whitespace();
    let method = words.next().unwrap_or_default();
    let path = words.next().unwrap_or_default();
    match (method, path) {
        ("POST", "/api/parse-save") => {
            if body.len() > MAX_SAVE_BYTES {
                return reply(
                    &mut stream,
                    413,
                    "application/json",
                    br#"{"error":"save too large"}"#,
                );
            }
            match dashboard_payload(body) {
                Ok(payload) => reply(
                    &mut stream,
                    200,
                    "application/json; charset=utf-8",
                    serde_json::to_string(&payload).unwrap().as_bytes(),
                ),
                Err(error) => reply(
                    &mut stream,
                    422,
                    "application/json; charset=utf-8",
                    json!({"error": error}).to_string().as_bytes(),
                ),
            }
        }
        ("GET", "/") | ("GET", "/index.html") => serve_file(
            &mut stream,
            frontend_root().join("index.html"),
            "text/html; charset=utf-8",
        ),
        ("GET", "/item-model.mjs") => serve_file(
            &mut stream,
            frontend_root().join("item-model.mjs"),
            "text/javascript; charset=utf-8",
        ),
        ("GET", path) if path.starts_with("/fixtures/") && !path.contains("..") => serve_file(
            &mut stream,
            fixture_root().join(&path[10..]),
            "application/octet-stream",
        ),
        _ => reply(&mut stream, 404, "text/plain", b"Not found"),
    }
}

fn serve_file(stream: &mut TcpStream, path: PathBuf, content_type: &str) -> std::io::Result<()> {
    match fs::read(path) {
        Ok(bytes) => reply(stream, 200, content_type, &bytes),
        Err(_) => reply(stream, 404, "text/plain", b"Not found"),
    }
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/savegames/original")
}

fn frontend_root() -> PathBuf {
    std::env::var_os("D2R_SPEC_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../d2r-spec"))
        .join("examples/dashboard")
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut request = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Ok(request);
        }
        request.extend_from_slice(&buffer[..read]);
        let Some(split) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..split + 4]);
        let length = headers
            .lines()
            .find_map(|line| {
                line.strip_prefix("Content-Length:")
                    .or_else(|| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse::<usize>().ok())
            })
            .unwrap_or(0);
        if request.len() >= split + 4 + length {
            return Ok(request);
        }
    }
}

fn reply(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let text = match status {
        200 => "OK",
        400 => "Bad Request",
        413 => "Payload Too Large",
        422 => "Unprocessable Content",
        _ => "Not Found",
    };
    write!(stream, "HTTP/1.1 {status} {text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len())?;
    stream.write_all(body)
}

fn dashboard_payload(bytes: &[u8]) -> Result<Value, String> {
    let save = Save::from_bytes(bytes).map_err(|error| error.to_string())?;
    let map = map_core_sections(bytes).map_err(|error| error.to_string())?;
    let alpha = save.header.version == 105;
    let huffman = HuffmanTree::new();
    let attrs = AttributeSection::parse(bytes, map.gf_pos, map.if_pos)
        .map_err(|error| error.to_string())?;
    let get_stat = |id: u32| attrs.actual_value(id, alpha).unwrap_or(0);
    let level = get_stat(12).try_into().unwrap_or(0_u8);
    let mut all_attributes = Vec::new();
    for entry in &attrs.entries {
        let name = d2r_core::data::stat_costs::STAT_COSTS
            .iter()
            .find(|stat| stat.id == entry.stat_id)
            .map(|stat| stat.name.as_ref())
            .unwrap_or("Unknown");
        all_attributes.push(json!({
            "stat_id": entry.stat_id,
            "name": name,
            "raw_value": entry.raw_value,
            "actual_value": attrs.actual_value(entry.stat_id, alpha).unwrap_or(entry.raw_value as i32),
        }));
    }
    let mut skills = Vec::new();
    if let Ok(parsed_skills) =
        parse_skill_section(bytes, map.if_pos, map.jm_positions.first().copied())
    {
        if let Some(base_id) = class_skill_base_id(save.header.char_class) {
            for skill in parsed_skills
                .iter_skills(base_id)
                .filter(|skill| skill.level > 0)
            {
                let name = d2r_core::data::skills::SKILLS
                    .iter()
                    .find(|entry| entry.id == skill.skill_id)
                    .map(|entry| entry.key)
                    .unwrap_or("Unknown Skill");
                skills.push(json!({"id": skill.skill_id, "name": name, "level": skill.level}));
            }
        }
    }
    let is_expansion = bytes
        .get(d2r_core::domain::header::axiom::EXPANSION_FLAG_OFFSET)
        .map(|byte| byte & 0x20 != 0)
        .unwrap_or(true);
    let waypoint_bytes = bytes
        .get(d2r_core::domain::progression::axiom::V105_WAYPOINT_OFFSET..)
        .unwrap_or_default();
    let waypoint_anchor = d2r_core::domain::progression::axiom::PROG_START_FILE
        + d2r_core::domain::progression::axiom::V105WaypointAxiom::start_offset();
    let active_waypoints = |difficulty| {
        WaypointSet::from_bytes(waypoint_bytes, difficulty, waypoint_anchor, is_expansion)
            .waypoints()
            .iter()
            .filter(|waypoint| waypoint.is_active())
            .map(|waypoint| waypoint.ws_bit())
            .collect::<Vec<_>>()
    };
    let mut quests = [Vec::new(), Vec::new(), Vec::new()];
    if let Ok(progression) = Progression::from_bytes(bytes, alpha).value {
        for quest in progression
            .quests
            .quests()
            .iter()
            .filter(|quest| quest.is_completed())
        {
            if let Some(bucket) = quests.get_mut(quest.difficulty() as usize) {
                bucket.push(format!("a{}q{}", quest.act(), quest.index() + 1));
            }
        }
    }
    let mut equipment = Vec::new();
    let mut inventory = Vec::new();
    let mut belt = Vec::new();
    let mut stash = Vec::new();
    let mut cube = Vec::new();
    let mut unknown = Vec::new();
    let stream = PlayerItemStream::read(bytes, &huffman)?;
    {
        // Aggregate set piece counts from equipped items to accurately resolve active set bonuses.
        let mut set_piece_counts: HashMap<&'static str, usize> = HashMap::new();
        for item in &stream.items {
            if stream.placement(item).container == ItemContainer::Equipment
                && item.quality == Some(ItemQuality::Set)
                && item.is_identified
            {
                if let Some(id) = item.unique_id {
                    if let Some(set_item) = SET_ITEMS.iter().find(|s| s.id == id as u32) {
                        *set_piece_counts.entry(set_item.set_id).or_insert(0) += 1;
                    }
                }
            }
        }

        for (index, item) in stream.items.iter().enumerate() {
            let active_set_count = if item.quality == Some(ItemQuality::Set) {
                item.unique_id
                    .and_then(|id| SET_ITEMS.iter().find(|s| s.id == id as u32))
                    .and_then(|s| set_piece_counts.get(s.set_id).copied())
                    .unwrap_or(1)
            } else {
                0
            };
            let mut data = item_json(index, item, level, active_set_count);
            let placement = stream.placement(item);
            data["placement"] = json!(placement);
            data["x"] = json!(placement.x);
            data["y"] = json!(placement.y);
            data["is_equipped"] = json!(placement.container == ItemContainer::Equipment);
            if let Some(slot) = placement.equipment_slot {
                data["slot_en"] = json!(slot);
            }
            match placement.container {
                ItemContainer::Equipment => equipment.push(data),
                ItemContainer::Belt => belt.push(data),
                ItemContainer::Inventory => inventory.push(data),
                ItemContainer::Cube => cube.push(data),
                ItemContainer::Stash => stash.push(data),
                ItemContainer::Unknown => unknown.push(data),
            }
        }
    }
    Ok(json!({
        "parser": {
            "item_dialect": stream.dialect,
            "player_item_count": stream.items.len(),
            "diagnostics": stream.diagnostics,
        },
        "character": {
            "name": save.header.char_name,
            "class": d2r_core::save::class_name(save.header.char_class),
            "level": level,
            "experience": get_stat(13),
            "stats": {
                "strength": get_stat(0), "energy": get_stat(1), "dexterity": get_stat(2), "vitality": get_stat(3),
                "stat_points_left": get_stat(4), "skill_points_left": get_stat(5),
                "life": get_stat(6) / 256, "max_life": get_stat(7) / 256,
                "mana": get_stat(8) / 256, "max_mana": get_stat(9) / 256,
                "stamina": get_stat(10) / 256, "max_stamina": get_stat(11) / 256,
            },
            "gold": {"inventory": get_stat(14), "stash": get_stat(15)},
        },
        "all_attributes": all_attributes,
        "skills": skills,
        "waypoints": {"normal": active_waypoints(0), "nightmare": active_waypoints(1), "hell": active_waypoints(2)},
        "quests": {"normal": quests[0], "nightmare": quests[1], "hell": quests[2]},
        "equipment": equipment, "inventory": inventory, "unknown": unknown, "belt": belt, "stash": stash, "cube": cube,
        "mercenary": Value::Null,
    }))
}

fn item_json(index: usize, item: &Item, level: u8, active_set_count: usize) -> Value {
    let ko = format_item(item, "ko", active_set_count, level);
    let en = format_item(item, "en", active_set_count, level);
    let socketed_items = item
        .socketed_items
        .iter()
        .enumerate()
        .map(|(child_index, child)| item_json(child_index, child, level, 0))
        .collect::<Vec<_>>();
    let mut opaque_bits = item.body.v105_7mgw_payload.as_ref().map_or(0, Vec::len);
    let mut opaque_reasons = Vec::new();
    for module in &item.modules {
        match module {
            ItemModule::Opaque(bits) | ItemModule::Residue(bits) => opaque_bits += bits.len(),
            ItemModule::SemiOpaque { body_bits, reason } => {
                opaque_bits += body_bits.len();
                opaque_reasons.push(reason.as_str());
            }
            _ => {}
        }
    }
    let has_opaque_properties = item
        .properties
        .iter()
        .chain(item.set_attributes.iter().flatten())
        .chain(item.runeword_attributes.iter())
        .any(|property| property.is_opaque);
    let is_opaque = item.is_opaque()
        || item.is_semi_opaque()
        || item.body.v105_7mgw_payload.is_some()
        || has_opaque_properties;
    let mut data = json!({
        "index": index,
        "code": item.code.trim(),
        "name_en": en.name,
        "name_ko": ko.name,
        "type": d2r_core::inventory::get_item_category(&item.code),
        "quality": if item.header.is_runeword { "Runeword" } else { quality(item.header.quality) },
        "x": item.x,
        "y": item.y,
        "width": d2r_core::inventory::get_item_size(&item.code).0,
        "height": d2r_core::inventory::get_item_size(&item.code).1,
        "is_equipped": item.mode == 1,
        "location": item.location,
        "properties": properties(&item.properties),
        "formatted_lines_ko": ko.properties,
        "formatted_lines_en": en.properties,
        "formatted_base_ko": ko.base_attributes,
        "formatted_base_en": en.base_attributes,
        "set_bonuses_ko": format_set_bonuses(&ko.set_bonuses),
        "set_bonuses_en": format_set_bonuses(&en.set_bonuses),
        "warnings_ko": ko.warnings,
        "warnings_en": en.warnings,
        "detailed_warnings_ko": ko.detailed_warnings,
        "detailed_warnings_en": en.detailed_warnings,
        "set_attributes": item.set_attributes.iter().map(|set| properties(set)).collect::<Vec<_>>(),
        "runeword_attributes": properties(&item.runeword_attributes),
        "socketed_items": socketed_items,
        "base_damage": base_damage(&item.code),
        "defense": item.defense,
        "max_durability": item.max_durability,
        "current_durability": item.current_durability,
        "quantity": item.quantity,
        "opaque_info": {
            "is_opaque": is_opaque,
            "is_semi_opaque": item.is_semi_opaque(),
            "bit_count": opaque_bits,
            "reasons": opaque_reasons,
        }
    });
    data.as_object_mut().expect("item projection object").extend(
        json!({
            "mode": item.mode,
            "page": item.page,
            "raw_x": item.x,
            "raw_y": item.y,
            "is_identified": item.is_identified,
            "unique_id": item.unique_id,
            "raw_range": item.range,
            "parse_status": if item.is_residue() { "residue" } else if is_opaque { "partial" } else { "parsed" },
        }).as_object().expect("item provenance object").clone()
    );
    data
}

fn format_set_bonuses(bonuses: &[FormattedSetBonus]) -> Value {
    json!(bonuses
        .iter()
        .map(|bonus| {
            json!({
                "active": bonus.active,
                "required_count": bonus.required_count,
                "lines": bonus.lines,
            })
        })
        .collect::<Vec<_>>())
}

fn base_damage(code: &str) -> Value {
    let Some(spec) = get_base_item_spec(code.trim()) else {
        return Value::Null;
    };
    let one_hand = (spec.max_dmg > 0).then(|| json!({"min": spec.min_dmg, "max": spec.max_dmg}));
    let two_hand =
        (spec.max_dmg2h > 0).then(|| json!({"min": spec.min_dmg2h, "max": spec.max_dmg2h}));
    if one_hand.is_none() && two_hand.is_none() {
        Value::Null
    } else {
        json!({"one_hand": one_hand, "two_hand": two_hand})
    }
}

fn properties(items: &[ItemProperty]) -> Vec<Value> {
    items
        .iter()
        .map(|item| {
            // Serialize the parser's complete property contract, including provenance
            // and opaque state. Future fields no longer require a second allowlist.
            let mut value = serde_json::to_value(item).expect("ItemProperty serialization");
            if item.name.trim().is_empty() {
                value["name"] = json!(d2r_core::data::stat_costs::STAT_COSTS
                    .iter()
                    .find(|stat| stat.id == item.stat_id)
                    .map(|stat| stat.name.to_string())
                    .unwrap_or_else(|| format!("stat_{}", item.stat_id)));
            }
            value
        })
        .collect()
}

fn quality(quality: Option<d2r_core::domain::item::quality::ItemQuality>) -> &'static str {
    use d2r_core::domain::item::quality::ItemQuality;
    match quality {
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

#[cfg(test)]
mod tests {
    use super::*;
    use d2r_macros::settled_contract_test;

    fn assert_projection(data: &Value, item: &Item, level: u8, set_count: usize) {
        assert_eq!(data["code"], item.code.trim());
        for (field, source) in [
            ("properties", &item.properties),
            ("runeword_attributes", &item.runeword_attributes),
        ] {
            for (actual, property) in data[field].as_array().unwrap().iter().zip(source) {
                let expected = serde_json::to_value(property).unwrap();
                for (key, value) in expected.as_object().unwrap() {
                    if key != "name" || !property.name.trim().is_empty() {
                        assert_eq!(&actual[key], value, "property field {key}");
                    }
                }
            }
            assert_eq!(data[field].as_array().unwrap().len(), source.len());
        }
        assert_eq!(
            data["set_attributes"],
            json!(item
                .set_attributes
                .iter()
                .map(|p| properties(p))
                .collect::<Vec<_>>())
        );
        for language in ["ko", "en"] {
            let formatted = format_item(item, language, set_count, level);
            assert_eq!(data[format!("name_{language}")], formatted.name);
            assert_eq!(
                data[format!("formatted_lines_{language}")],
                json!(formatted.properties)
            );
            assert_eq!(
                data[format!("formatted_base_{language}")],
                json!(formatted.base_attributes)
            );
            assert_eq!(
                data[format!("warnings_{language}")],
                json!(formatted.warnings)
            );
        }
        assert_eq!(
            data["socketed_items"].as_array().unwrap().len(),
            item.socketed_items.len()
        );
        for (child_data, child) in data["socketed_items"]
            .as_array()
            .unwrap()
            .iter()
            .zip(&item.socketed_items)
        {
            assert_projection(child_data, child, level, 0);
        }
    }

    #[settled_contract_test]
    fn dashboard_projection_preserves_target_stream_and_formatter_contract() {
        let bytes = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s",
        ))
        .unwrap();
        let stream = PlayerItemStream::read(&bytes, &HuffmanTree::new()).unwrap();
        let payload = dashboard_payload(&bytes).unwrap();
        assert_eq!(payload["parser"]["item_dialect"], "d2r-v3");
        assert_eq!(payload["parser"]["player_item_count"], 43);
        assert_eq!(payload["equipment"].as_array().unwrap().len(), 10);
        assert!(payload["unknown"].as_array().unwrap().is_empty());
        let all = ["equipment", "inventory", "belt", "stash", "cube", "unknown"]
            .into_iter()
            .flat_map(|key| payload[key].as_array().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(all.len(), stream.items.len());
        let level = payload["character"]["level"].as_u64().unwrap() as u8;
        for (index, item) in stream.items.iter().enumerate() {
            let data = all.iter().find(|data| data["index"] == index).unwrap();
            let set_count = if item.quality == Some(ItemQuality::Set) {
                1
            } else {
                0
            };
            assert_projection(data, item, level, set_count);
            assert_eq!(data["placement"], json!(stream.placement(item)));
        }
        for item in payload["belt"].as_array().unwrap() {
            assert!(
                d2r_core::verify::alpha_inventory_routing::is_belt_consumable(
                    item["code"].as_str().unwrap()
                )
            );
            assert!(item["x"].as_u64().unwrap() < 4 && item["y"].as_u64().unwrap() < 4);
        }
        assert!(dashboard_payload(&bytes[..7]).is_err());
    }

    #[settled_contract_test]
    fn dashboard_projection_exposes_partial_state_in_socket_children() {
        let mut child = Item {
            code: "rin".into(),
            ..Item::default()
        };
        child.modules.push(ItemModule::SemiOpaque {
            body_bits: vec![true, false],
            reason: "Unresolved tail".into(),
        });
        let parent = Item {
            code: "cap".into(),
            socketed_items: vec![child],
            ..Item::default()
        };
        let data = item_json(0, &parent, 1, 0);
        let child = &data["socketed_items"][0];
        assert_eq!(child["parse_status"], "partial");
        assert_eq!(child["opaque_info"]["is_opaque"], true);
        assert_eq!(child["opaque_info"]["bit_count"], 2);
        assert_eq!(child["opaque_info"]["reasons"][0], "Unresolved tail");
        assert!(child["runeword_attributes"].is_array());
        assert!(child["warnings_ko"].is_array());
    }
}
