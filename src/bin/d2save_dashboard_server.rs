use d2r_core::domain::character::skills::parse_skill_section;
use d2r_core::domain::progression::waypoint::WaypointSet;
use d2r_core::domain::progression::Progression;
use d2r_core::engine::formatter::format_item;
use d2r_core::item::{HuffmanTree, Item, ItemProperty};
use d2r_core::save::{class_skill_base_id, map_core_sections, AttributeSection, Save};
use d2r_core::verify::alpha_inventory_routing::{alpha_inventory_route, AlphaInventoryRoute};
use serde_json::{json, Value};
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
    if let Ok(items) = Item::read_player_items(bytes, &huffman, alpha) {
        for (index, item) in items.iter().enumerate() {
            if item.is_residue() || item.code.trim().is_empty() {
                continue;
            }
            let mut data = item_json(index, item, level);
            let route = alpha_inventory_route(item, alpha);
            if route == AlphaInventoryRoute::Equipment {
                data["slot_en"] = json!(equipment_slot(item.x, alpha));
            }
            match route {
                AlphaInventoryRoute::Equipment => equipment.push(data),
                AlphaInventoryRoute::Belt => belt.push(data),
                AlphaInventoryRoute::Inventory => inventory.push(data),
                AlphaInventoryRoute::Cube => cube.push(data),
                AlphaInventoryRoute::Stash => stash.push(data),
                AlphaInventoryRoute::Unknown => unknown.push(data),
            }
        }
    }
    Ok(json!({
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

fn item_json(index: usize, item: &Item, level: u8) -> Value {
    let ko = format_item(item, "ko", 0, level);
    let en = format_item(item, "en", 0, level);
    let (name_en, name_ko) = item_localization(&item.code);
    let socketed_items = item.socketed_items.iter().enumerate().map(|(child_index, child)| {
        let child_ko = format_item(child, "ko", 0, level);
        let child_en = format_item(child, "en", 0, level);
        let (child_name_en, child_name_ko) = item_localization(&child.code);
        json!({"index":child_index,"code":child.code.trim(),"name_en":child_name_en,"name_ko":child_name_ko,"quality":quality(child.header.quality),"properties":properties(&child.properties),"formatted_lines_ko":child_ko.properties,"formatted_lines_en":child_en.properties})
    }).collect::<Vec<_>>();
    json!({"index":index,"code":item.code.trim(),"name_en":name_en,"name_ko":name_ko,"type":d2r_core::inventory::get_item_category(&item.code),"quality":if item.header.is_runeword { "Runeword" } else { quality(item.header.quality) },"x":item.x,"y":item.y,"width":d2r_core::inventory::get_item_size(&item.code).0,"height":d2r_core::inventory::get_item_size(&item.code).1,"is_equipped":item.mode==1,"location":item.location,"properties":properties(&item.properties),"formatted_lines_ko":ko.properties,"formatted_lines_en":en.properties,"formatted_base_ko":ko.base_attributes,"formatted_base_en":en.base_attributes,"set_attributes":item.set_attributes.iter().map(|set| properties(set)).collect::<Vec<_>>(),"runeword_attributes":properties(&item.runeword_attributes),"socketed_items":socketed_items,"defense":item.defense,"max_durability":item.max_durability,"current_durability":item.current_durability,"quantity":item.quantity,"opaque_info":{"is_opaque":item.body.v105_7mgw_payload.is_some(),"bit_count":item.body.v105_7mgw_payload.as_ref().map(|bits|bits.len()).unwrap_or(0)}})
}

fn properties(items: &[ItemProperty]) -> Vec<Value> {
    items.iter().map(|item| json!({"stat_id":item.stat_id,"name":if item.name.trim().is_empty() { d2r_core::data::stat_costs::STAT_COSTS.iter().find(|stat| stat.id == item.stat_id).map(|stat| stat.name.to_string()).unwrap_or_else(|| format!("stat_{}", item.stat_id)) } else { item.name.clone() },"param":item.param,"raw_value":item.raw_value,"value":item.value})).collect()
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

fn item_localization(code: &str) -> (String, String) {
    let code = code.trim().to_lowercase();
    if let Some(entry) = d2r_core::data::localization::LOCALIZATIONS
        .iter()
        .find(|entry| entry.key.to_lowercase() == code)
    {
        return (entry.en.to_string(), entry.ko.to_string());
    }
    let name = d2r_core::data::item_codes::ITEM_TEMPLATES
        .iter()
        .find(|entry| entry.code == code)
        .map(|entry| entry.name)
        .unwrap_or(code.as_str());
    (name.to_string(), name.to_string())
}

fn equipment_slot(x: u8, alpha: bool) -> &'static str {
    if alpha {
        match x {
            0 => "Armor",
            1 => "Weapon1",
            2 => "Weapon2",
            3 => "Head",
            _ => body_position_to_slot(x),
        }
    } else {
        body_position_to_slot(x)
    }
}

fn body_position_to_slot(position: u8) -> &'static str {
    match position {
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
