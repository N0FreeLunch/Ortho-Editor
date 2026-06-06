use d2r_core::domain::character::skills::parse_skill_section;
use d2r_core::save::{AttributeSection, Save, class_skill_base_id, map_core_sections};
use d2r_core::domain::progression::Progression;
use d2r_core::domain::progression::waypoint::WaypointSet;
use std::env;
use std::fs;
use serde_json::json;

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

    let get_stat = |id: u32| -> i32 {
        attrs.actual_value(id, is_alpha).unwrap_or(0)
    };

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
        let act_val = attrs.actual_value(entry.stat_id, is_alpha).unwrap_or(entry.raw_value as i32);
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
        wp_set.waypoints()
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
    let header = &bytes[0..d2r_core::domain::progression::axiom::V105_WAYPOINT_OFFSET.min(bytes.len())];
    let w4_bytes = if bytes.len() > d2r_core::domain::progression::axiom::V105_NPC_OFFSET {
        Some(&bytes[d2r_core::domain::progression::axiom::V105_NPC_OFFSET..])
    } else {
        None
    };
    let merc_state = d2r_core::domain::forensic::v105::mercenary::MercenaryState::from_hybrid(header, w4_bytes);

    let merc_json = json!({
        "class_name": merc_state.class_name(),
        "hireling_id": merc_state.hireling_id,
        "subtype_id": merc_state.subtype_id,
        "experience": merc_state.experience,
        "equipped_items": []
    });

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
        "equipment": [],
        "inventory": [],
        "stash": [],
        "mercenary": merc_json
    });

    let out_str = serde_json::to_string_pretty(&payload).expect("Failed to serialize JSON");
    fs::write(&args[2], out_str).expect("Failed to write output JSON");
    println!("Successfully dumped save game JSON to {}", args[2]);
}
