//! Contract tests for Rust-as-Store projection, `elm_rs` protocol drift, and `UiIntent` application.

use std::fs;
use std::path::Path;

use d2r_core::projection::{
    ContainerKind, DifficultyMode, EditMode, EditorSession, ItemCardView, ItemQualityKind,
    SessionEnvelope, ShopSpawnTarget, UiIntent, generate_elm_protocol_module,
};

fn fixture_bytes(rel: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    fs::read(&path).unwrap_or_else(|e| panic!("Failed to read fixture {}: {e}", path.display()))
}

#[test]
fn generated_protocol_elm_is_in_sync_with_rust_dtos() {
    let generated = generate_elm_protocol_module().expect("elm_rs export");
    let on_disk_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend/src/Generated/Protocol.elm");
    let on_disk = fs::read_to_string(&on_disk_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", on_disk_path.display()));
    assert_eq!(
        on_disk.replace("\r\n", "\n"),
        generated.replace("\r\n", "\n"),
        "frontend/src/Generated/Protocol.elm drifted from Rust projection DTOs; run gen_elm_protocol"
    );
}

#[test]
fn real_fixtures_project_cleanly_and_survive_json_roundtrip() {
    let cases = [
        ("tests/fixtures/savegames/original/amazon_10_scrolls.d2s", "amazon_10_scrolls.d2s"),
        (
            "tests/fixtures/savegames/original/amazon_authority_runeword.d2s",
            "amazon_authority_runeword.d2s",
        ),
        (
            "tests/fixtures/savegames/original/amazon_lvl2_progression_complex.d2s",
            "amazon_lvl2_progression_complex.d2s",
        ),
        (
            "tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s",
            "TESTDRUID_Quest3_AnyaRescued.d2s",
        ),
    ];

    for (rel, label) in cases {
        let bytes = fixture_bytes(rel);
        let session = EditorSession::from_bytes(&bytes, label)
            .unwrap_or_else(|e| panic!("Projection failed on {label}: {e}"));
        let view = session.project();
        assert_eq!(view.fixture_label, label);
        assert!(!view.char_name.is_empty(), "{label} char_name");
        assert_eq!(view.export_supported, view.save_version == 105);
        assert_eq!(
            view.export_unsupported_reason.is_some(),
            !view.export_supported
        );

        let env = SessionEnvelope {
            ok: true,
            session: Some(view.clone()),
            rejection: None,
        };
        let encoded = serde_json::to_string(&env).expect("serialize SessionEnvelope");
        let decoded: SessionEnvelope =
            serde_json::from_str(&encoded).expect("deserialize SessionEnvelope");
        assert_eq!(decoded, env);
    }
}

#[test]
fn intents_mutate_state_and_reject_invalid_merc_level() {
    let bytes =
        fixture_bytes("tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s");
    let mut session = EditorSession::from_bytes(&bytes, "TESTDRUID_Quest3_AnyaRescued.d2s")
        .expect("load fixture");

    let normal_view = session.project();
    let hell_env = session.apply_intent(UiIntent::SelectResDifficulty {
        difficulty: DifficultyMode::Hell,
    });
    assert!(hell_env.ok);
    let hell_view = hell_env.session.expect("session");
    assert_eq!(hell_view.res_difficulty, DifficultyMode::Hell);
    assert_eq!(hell_view.resistances.difficulty_penalty, 100);
    assert!(
        hell_view.resistances.fire <= normal_view.resistances.fire,
        "Hell penalty must reduce effective fire resistance"
    );

    let mode_env = session.apply_intent(UiIntent::SelectSkillMode {
        mode: EditMode::Free,
    });
    assert!(mode_env.ok);
    assert_eq!(mode_env.session.unwrap().skill_mode, EditMode::Free);

    let merc_before = session.project().merc;
    assert!(!merc_before.persistence_supported);
    assert!(merc_before.preview_only_reason.is_some());

    let lvl_locked = session.apply_intent(UiIntent::SetMercLevel { level: 85 });
    assert!(!lvl_locked.ok);
    assert_eq!(
        lvl_locked.rejection.as_ref().map(|r| r.raw_code.as_str()),
        Some("merc_persistence_not_supported")
    );
    let after_lvl_view = lvl_locked.session.expect("session retained");
    assert!(!after_lvl_view.dirty);
    assert_eq!(after_lvl_view.merc.stats.level, merc_before.stats.level);

    let lvl_bad_low = session.apply_intent(UiIntent::SetMercLevel { level: 0 });
    assert!(!lvl_bad_low.ok);
    assert_eq!(
        lvl_bad_low.rejection.as_ref().map(|r| r.raw_code.as_str()),
        Some("merc_level_out_of_range")
    );
    assert_eq!(
        lvl_bad_low.session.unwrap().merc.stats.level,
        merc_before.stats.level
    );

    let lvl_bad_high = session.apply_intent(UiIntent::SetMercLevel { level: 99 });
    assert!(!lvl_bad_high.ok);
    assert_eq!(
        lvl_bad_high.rejection.as_ref().map(|r| r.raw_code.as_str()),
        Some("merc_level_out_of_range")
    );

    let act5_locked = session.apply_intent(UiIntent::SelectMercAct { act_index: 5 });
    assert!(!act5_locked.ok);
    assert_eq!(
        act5_locked.rejection.as_ref().map(|r| r.raw_code.as_str()),
        Some("merc_persistence_not_supported")
    );
    let act5_view = act5_locked.session.expect("session retained");
    assert!(!act5_view.dirty);
    assert_eq!(act5_view.merc.act_index, merc_before.act_index);

    let act4_bad = session.apply_intent(UiIntent::SelectMercAct { act_index: 4 });
    assert!(!act4_bad.ok);
    assert_eq!(
        act4_bad.rejection.as_ref().map(|r| r.raw_code.as_str()),
        Some("invalid_merc_act")
    );

    let all_quests_on = session.apply_intent(UiIntent::SetAllDiffQuests { completed: true });
    assert!(all_quests_on.ok);
    let prog = all_quests_on.session.unwrap().progression;
    assert!(!prog.quest_catalog.is_empty());
    assert!(!prog.waypoint_catalog.is_empty());
    assert_eq!(
        prog.hell_quests_completed.len(),
        prog.quest_catalog.len(),
        "SetAllDiffQuests must complete Hell quests up to catalog count"
    );

    let all_wps_on = session.apply_intent(UiIntent::SetAllDiffWaypoints { active: true });
    assert!(all_wps_on.ok);
    let wp_prog = all_wps_on.session.unwrap().progression;
    assert_eq!(
        wp_prog.hell_waypoints_active.len(),
        wp_prog.waypoint_catalog.len(),
        "SetAllDiffWaypoints must activate Hell waypoints up to catalog count"
    );

    let batch_wp = session.apply_intent(UiIntent::SetWaypointsBatch {
        difficulty: DifficultyMode::Normal,
        bits: vec![0, 1, 2],
        active: true,
    });
    assert!(batch_wp.ok);
    let wps = batch_wp.session.unwrap().progression.normal_waypoints_active;
    assert!(wps.contains(&0) && wps.contains(&1) && wps.contains(&2));

    let batch_quest = session.apply_intent(UiIntent::SetQuestsBatch {
        difficulty: DifficultyMode::Normal,
        keys: vec!["a1q1".to_string(), "a1q2".to_string()],
        completed: true,
    });
    assert!(batch_quest.ok);
    let quests = batch_quest
        .session
        .unwrap()
        .progression
        .normal_quests_completed;
    assert!(quests.contains(&"a1q1".to_string()) && quests.contains(&"a1q2".to_string()));

    session.push_temp_item(ItemCardView {
        index: 0,
        code: "r01".to_string(),
        name_en: "El Rune".to_string(),
        name_ko: "엘 룬".to_string(),
        quality: ItemQualityKind::QualityNormal,
        container: ContainerKind::TempInventory,
        slot_label: None,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        item_level: 1,
        base_attributes: vec![],
        properties: vec![],
        set_name: None,
        set_pieces: vec![],
        piece_set_bonuses: vec![],
        set_bonuses: vec![],
        socketed_names: vec![],
        image_url: None,
        is_belt_consumable: false,
        detailed_properties: vec![],
        defense: None,
        current_durability: None,
        max_durability: None,
        quantity: None,
        is_ethereal: false,
    });
    let projected = session.project();
    assert!(
        projected.items.iter().any(|item| {
            item.image_url
                .as_deref()
                .is_some_and(|url| url.starts_with("/item-vectors/") || url.starts_with("/item-images/"))
        }),
        "expected projected fixture items to resolve at least one local image/vector URL"
    );
    assert!(
        projected.items.iter().any(|item| !item.base_attributes.is_empty()),
        "expected projected fixture items to include formatter base_attributes (defense/damage/durability/req)"
    );
    assert_eq!(session.project().container_counts.temp_inventory, 1);
    let cleared = session.apply_intent(UiIntent::ClearTempInventory);
    assert!(cleared.ok);
    assert_eq!(cleared.session.unwrap().container_counts.temp_inventory, 0);
}

#[test]
fn elm_wire_intent_json_samples_decode_into_rust_ui_intent() {
    let samples = [
        r#"{"SelectResDifficulty":{"difficulty":"Hell"}}"#,
        r#"{"SelectSkillMode":{"mode":"Free"}}"#,
        r#"{"SetMercLevel":{"level":98}}"#,
        r#"{"SelectMercAct":{"act_index":5}}"#,
        r#"{"ToggleQuest":{"difficulty":"Nightmare","key":"a1q1"}}"#,
        r#"{"SetQuestsBatch":{"difficulty":"Normal","keys":["a1q1","a1q2"],"completed":true}}"#,
        r#"{"SetAllDiffQuests":{"completed":true}}"#,
        r#"{"ToggleWaypoint":{"difficulty":"Hell","bit":5}}"#,
        r#"{"SetWaypointsBatch":{"difficulty":"Normal","bits":[0,1,2],"active":false}}"#,
        r#"{"SetAllDiffWaypoints":{"active":true}}"#,
        r#""ClearTempInventory""#,
        r#""ToggleWeaponSwap""#,
        r#""DisposeHeldItem""#,
        r#"{"AdjustSkillRank":{"skill_id":221,"delta":1}}"#,
        r#""ResetSkills""#,
    ];

    for raw in samples {
        let decoded: UiIntent = serde_json::from_str(raw)
            .unwrap_or_else(|e| panic!("Failed to decode Elm UiIntent `{raw}`: {e}"));
        let reencoded = serde_json::to_string(&decoded).expect("reencode UiIntent");
        assert_eq!(reencoded, raw);
    }
}

#[test]
fn dirty_save_writeback_roundtrip_preserves_checksum_and_items() {
    let bytes =
        fixture_bytes("tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s");
    let mut session =
        EditorSession::from_bytes(&bytes, "TESTDRUID_Quest3_AnyaRescued.d2s").expect("load fixture");
    let initial = session.project();
    assert!(!initial.dirty);
    let initial_item_count = initial.items.len();

    let env1 = session.apply_intent(UiIntent::SetQuestsBatch {
        difficulty: DifficultyMode::Nightmare,
        keys: vec!["a1q1".to_string(), "a1q2".to_string()],
        completed: true,
    });
    assert!(env1.ok);
    assert!(env1.session.unwrap().dirty);

    let env2 = session.apply_intent(UiIntent::SetWaypointsBatch {
        difficulty: DifficultyMode::Hell,
        bits: vec![0, 1, 2],
        active: true,
    });
    assert!(env2.ok);

    let exported = session.to_save_bytes().expect("export modified save bytes");
    let reparsed = d2r_core::save::Save::from_bytes(&exported).expect("reparse Save header");
    let recalc = d2r_core::engine::checksum::recalculate_checksum(&exported)
        .expect("recalculate exported checksum");
    assert_eq!(reparsed.header.checksum, recalc);

    let reloaded = EditorSession::from_bytes(&exported, "exported.d2s")
        .expect("reload exported save into EditorSession");
    let reloaded_view = reloaded.project();
    assert!(!reloaded_view.dirty);
    assert_eq!(reloaded_view.items.len(), initial_item_count);
    assert!(
        reloaded_view
            .progression
            .nightmare_quests_completed
            .contains(&"a1q1".to_string())
    );
    assert!(
        reloaded_view
            .progression
            .nightmare_quests_completed
            .contains(&"a1q2".to_string())
    );
    let hell_wps = &reloaded_view.progression.hell_waypoints_active;
    assert!(hell_wps.contains(&0) && hell_wps.contains(&1) && hell_wps.contains(&2));
}

#[test]
fn host_store_command_loopback_handles_load_intent_rejection_and_export() {
    use d2r_core::projection::host::{HostResponse, HostSessionStore};

    let mut store = HostSessionStore::new();
    let empty_resp: HostResponse = store.handle_json_line(r#"{"cmd":"get_session"}"#);
    assert!(!empty_resp.ok);
    assert_eq!(
        empty_resp.error.as_ref().map(|e| e.raw_code.as_str()),
        Some("no_session_loaded")
    );

    let malformed = store.handle_json_line(r#"{"cmd":"non_existent"}"#);
    assert!(!malformed.ok);
    assert_eq!(
        malformed.error.as_ref().map(|e| e.raw_code.as_str()),
        Some("invalid_host_request")
    );

    let bytes =
        fixture_bytes("tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s");
    let load_cmd = serde_json::json!({
        "cmd": "load_save_hex",
        "label": "TESTDRUID_Quest3_AnyaRescued.d2s",
        "hex": hex::encode(&bytes),
    })
    .to_string();
    let loaded = store.handle_json_line(&load_cmd);
    assert!(loaded.ok);
    let initial_session = loaded.envelope.session.expect("loaded session");
    assert_eq!(initial_session.char_name, "TESTDRUID");
    assert!(!initial_session.dirty);

    let bad_merc = store.handle_json_line(
        r#"{"cmd":"dispatch_intent","intent":{"SetMercLevel":{"level":120}}}"#,
    );
    assert!(!bad_merc.ok);
    assert_eq!(
        bad_merc.error.as_ref().map(|e| e.raw_code.as_str()),
        Some("merc_level_out_of_range")
    );
    assert!(
        !bad_merc.envelope.session.expect("session retained").dirty,
        "rejected intent must not mark session dirty"
    );

    let quest_mut = store.handle_json_line(
        r#"{"cmd":"dispatch_intent","intent":{"SetQuestsBatch":{"difficulty":"Hell","keys":["a1q1","a5q3"],"completed":true}}}"#,
    );
    assert!(quest_mut.ok);
    assert!(quest_mut.envelope.session.expect("mutated session").dirty);

    let exported = store.handle_json_line(r#"{"cmd":"export_save_hex"}"#);
    assert!(exported.ok);
    let hex_payload = exported.export_hex.expect("export hex");
    assert_eq!(exported.export_byte_len, Some(hex_payload.len() / 2));

    let mut store2 = HostSessionStore::new();
    let reload_cmd = serde_json::json!({
        "cmd": "load_save_hex",
        "label": "loopback.d2s",
        "hex": hex_payload,
    })
    .to_string();
    let reloaded = store2.handle_json_line(&reload_cmd);
    assert!(reloaded.ok);
    let reloaded_view = reloaded.envelope.session.expect("reloaded session");
    assert!(!reloaded_view.dirty);
    assert!(
        reloaded_view
            .progression
            .hell_quests_completed
            .contains(&"a1q1".to_string())
    );
    assert!(
        reloaded_view
            .progression
            .hell_quests_completed
            .contains(&"a5q3".to_string())
    );
}

#[test]
fn character_profile_and_stat_mutations_enforce_bounds_and_roundtrip_bytes() {
    use d2r_core::projection::CharacterField;

    let bytes =
        fixture_bytes("tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s");
    let mut session = EditorSession::from_bytes(&bytes, "TESTDRUID_Quest3_AnyaRescued.d2s")
        .expect("parse fixture");
    let initial = session.project();
    assert_eq!(initial.schema.level_min, 1);
    assert_eq!(initial.schema.level_max, 99);
    assert_eq!(initial.schema.gold_stash_max, 2_500_000);
    assert_eq!(initial.schema.gold_character_max, initial.char_level * 10_000);

    let bad_level = session.apply_intent(UiIntent::SetCharacterField {
        field: CharacterField::Level,
        value: 120,
    });
    assert!(!bad_level.ok);
    assert_eq!(
        bad_level.rejection.as_ref().map(|r| r.raw_code.as_str()),
        Some("invalid_character_mutation")
    );
    assert!(!bad_level.session.expect("session stays valid").dirty);

    let bad_gold = session.apply_intent(UiIntent::SetCharacterField {
        field: CharacterField::GoldStash,
        value: 3_000_000,
    });
    assert!(!bad_gold.ok);
    assert_eq!(
        bad_gold.rejection.as_ref().map(|r| r.raw_code.as_str()),
        Some("invalid_character_mutation")
    );

    let bad_name = session.apply_intent(UiIntent::SetCharacterName {
        name: "A".to_string(),
    });
    assert!(!bad_name.ok);

    assert!(
        session
            .apply_intent(UiIntent::SetCharacterField {
                field: CharacterField::Level,
                value: 50,
            })
            .ok
    );
    assert!(
        session
            .apply_intent(UiIntent::SetCharacterField {
                field: CharacterField::Strength,
                value: 200,
            })
            .ok
    );
    assert!(
        session
            .apply_intent(UiIntent::SetCharacterField {
                field: CharacterField::GoldStash,
                value: 1_234_567,
            })
            .ok
    );
    let after_name = session.apply_intent(UiIntent::SetCharacterName {
        name: "OrthoDruid".to_string(),
    });
    assert!(after_name.ok);
    let view = after_name.session.expect("mutated view");
    assert!(view.dirty);
    assert_eq!(view.char_level, 50);
    assert_eq!(view.stats.strength_base, 200);
    assert_eq!(view.stats.gold_stash, 1_234_567);
    assert_eq!(view.char_name, "OrthoDruid");
    assert_eq!(view.schema.gold_character_max, 500_000);

    // Character gold exceeding dynamically updated level * 10,000 is rejected cleanly.
    let over_char_gold = session.apply_intent(UiIntent::SetCharacterField {
        field: CharacterField::GoldCharacter,
        value: 600_000,
    });
    assert!(!over_char_gold.ok);

    let exported = session.to_save_bytes().expect("writeback character edits");
    let reloaded = EditorSession::from_bytes(&exported, "reloaded.d2s").expect("re-parse .d2s");
    let reloaded_view = reloaded.project();
    assert!(!reloaded_view.dirty);
    assert_eq!(reloaded_view.char_level, 50);
    assert_eq!(reloaded_view.stats.strength_base, 200);
    assert_eq!(reloaded_view.stats.gold_stash, 1_234_567);
    assert_eq!(reloaded_view.char_name, "OrthoDruid");
    assert_eq!(reloaded_view.items.len(), initial.items.len());
}

#[test]
fn move_item_validates_belt_shop_collisions_and_roundtrips_d2s() {
    let bytes = fixture_bytes("tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s");
    let mut session = EditorSession::from_bytes(&bytes, "TESTDRUID_Quest3_AnyaRescued.d2s")
        .expect("parse fixture");
    let view = session.project();

    // Find a non-consumable 1x1 or larger item in inventory/stash/cube to verify belt rejection.
    let non_consumable = view
        .items
        .iter()
        .find(|item| {
            !item.is_belt_consumable
                && matches!(
                    item.container,
                    ContainerKind::Inventory | ContainerKind::Stash | ContainerKind::Cube
                )
        })
        .expect("fixture should have a non-consumable container item")
        .clone();

    // Dropping on `"belt"` with non-consumable is rejected cleanly.
    let belt_reject = session.apply_intent(UiIntent::MoveItem {
        item_index: non_consumable.index,
        target_container: ContainerKind::Belt,
        x: 0,
        y: 0,
    });
    assert!(!belt_reject.ok);
    assert_eq!(
        belt_reject.rejection.as_ref().map(|r| r.raw_code.as_str()),
        Some("belt_consumable_only")
    );

    // Select item for move: Rust projects allowed_move_targets and belt restriction without Undo pollution.
    let select_env = session.apply_intent(UiIntent::SelectItemForMove {
        item_index: Some(non_consumable.index),
    });
    assert!(select_env.ok);
    let select_view = select_env.session.expect("select view");
    assert_eq!(select_view.selected_move_item_index, Some(non_consumable.index));
    assert!(!select_view.can_undo);
    assert!(select_view.belt_move_restricted_reason.is_some());
    assert!(
        select_view
            .allowed_move_targets
            .iter()
            .any(|t| t.container == ContainerKind::Stash && t.x == 8 && t.y == 8)
    );
    assert!(
        !select_view
            .allowed_move_targets
            .iter()
            .any(|t| t.container == ContainerKind::Belt)
    );

    // Move a compact/stashable item to a free slot in stash (or cube) and verify Undo/Redo + .d2s roundtrip.
    let target_x = 8;
    let target_y = 8;
    let moved = session.apply_intent(UiIntent::MoveItem {
        item_index: non_consumable.index,
        target_container: ContainerKind::Stash,
        x: target_x,
        y: target_y,
    });
    assert!(moved.ok, "unexpected rejection: {:?}", moved.rejection);
    let moved_view = moved.session.expect("moved session");
    assert!(moved_view.can_undo);
    assert!(!moved_view.can_redo);
    assert_eq!(moved_view.selected_move_item_index, None);
    let moved_card = moved_view
        .items
        .iter()
        .find(|item| item.index == non_consumable.index)
        .expect("moved card");
    assert_eq!(moved_card.container, ContainerKind::Stash);
    assert_eq!(moved_card.x, target_x);
    assert_eq!(moved_card.y, target_y);

    // Undo restores the original container and coordinates.
    let undone = session.apply_intent(UiIntent::Undo);
    assert!(undone.ok);
    let undone_view = undone.session.expect("undone view");
    assert!(!undone_view.can_undo);
    assert!(undone_view.can_redo);
    let restored_card = undone_view
        .items
        .iter()
        .find(|item| item.index == non_consumable.index)
        .expect("restored card");
    assert_eq!(restored_card.container, non_consumable.container);
    assert_eq!(restored_card.x, non_consumable.x);
    assert_eq!(restored_card.y, non_consumable.y);

    // Redo re-applies the move to stash (8, 8).
    let redone = session.apply_intent(UiIntent::Redo);
    assert!(redone.ok);
    let redone_view = redone.session.expect("redone view");
    assert!(redone_view.can_undo);
    assert!(!redone_view.can_redo);
    let redone_card = redone_view
        .items
        .iter()
        .find(|item| item.index == non_consumable.index)
        .expect("redone card");
    assert_eq!(redone_card.container, ContainerKind::Stash);
    assert_eq!(redone_card.x, target_x);
    assert_eq!(redone_card.y, target_y);

    // Export and reload from .d2s bytes to verify binary persistence.
    let exported = session.to_save_bytes().expect("export moved item .d2s");
    let reloaded = EditorSession::from_bytes(&exported, "moved.d2s").expect("reload moved .d2s");
    let reloaded_view = reloaded.project();
    assert_eq!(reloaded_view.items.len(), view.items.len());
    let reloaded_card = reloaded_view
        .items
        .iter()
        .find(|item| {
            item.code == non_consumable.code
                && item.container == ContainerKind::Stash
                && item.x == target_x
                && item.y == target_y
        })
        .expect("moved item should persist in stash at (8, 8) after .d2s reload");
    assert_eq!(reloaded_card.name_en, non_consumable.name_en);
}

#[test]
fn cycle_13_shop_dispose_temp_inv_weapon_swap_and_skill_tree_roundtrip_d2s() {
    let bytes =
        fixture_bytes("tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s");
    let mut session = EditorSession::from_bytes(&bytes, "TESTDRUID_Quest3_AnyaRescued.d2s")
        .expect("parse fixture");
    let initial = session.project();
    assert_eq!(initial.active_weapon_set, 1);
    assert_eq!(initial.skills.len(), 30);
    let initial_item_count = initial.items.len();

    // 1. Toggle Weapon Swap I <-> II
    let swap_env = session.apply_intent(UiIntent::ToggleWeaponSwap);
    assert!(swap_env.ok);
    assert_eq!(swap_env.session.unwrap().active_weapon_set, 2);
    let swap_back = session.apply_intent(UiIntent::ToggleWeaponSwap);
    assert!(swap_back.ok);
    assert_eq!(swap_back.session.unwrap().active_weapon_set, 1);

    // 2. Move item to Temp Inventory (9x4), verify export guard, then move back or Undo
    let candidate = initial
        .items
        .iter()
        .find(|it| matches!(it.container, ContainerKind::Inventory | ContainerKind::Stash))
        .expect("find inventory or stash item")
        .clone();
    let to_temp = session.apply_intent(UiIntent::MoveItem {
        item_index: candidate.index,
        target_container: ContainerKind::TempInventory,
        x: 2,
        y: 1,
    });
    assert!(to_temp.ok);
    let temp_view = to_temp.session.unwrap();
    assert_eq!(temp_view.container_counts.temp_inventory, 1);
    assert!(
        session.to_save_bytes().is_err(),
        "export must be blocked while items remain in temp_inventory"
    );
    let undo_temp = session.apply_intent(UiIntent::Undo);
    assert!(undo_temp.ok);
    assert_eq!(undo_temp.session.unwrap().container_counts.temp_inventory, 0);

    // 3. Shop Dispose + Undo recovery
    assert!(
        session
            .apply_intent(UiIntent::SelectItemForMove {
                item_index: Some(candidate.index),
            })
            .ok
    );
    let disposed = session.apply_intent(UiIntent::DisposeHeldItem);
    assert!(disposed.ok);
    let disposed_view = disposed.session.unwrap();
    assert_eq!(disposed_view.items.len(), initial_item_count - 1);
    assert_eq!(disposed_view.selected_move_item_index, None);
    let undo_dispose = session.apply_intent(UiIntent::Undo);
    assert!(undo_dispose.ok);
    assert_eq!(undo_dispose.session.unwrap().items.len(), initial_item_count);

    // 4. Skill Tree Rank Allocation & ResetSkills + .d2s binary roundtrip
    let first_skill_id = initial.skills[0].skill_id;
    let reset_env = session.apply_intent(UiIntent::ResetSkills);
    assert!(reset_env.ok);
    let reset_view = reset_env.session.unwrap();
    assert_eq!(reset_view.stats.active_skill_count, 0);
    assert!(reset_view.stats.unspent_skill_points > 0);
    let unspent_after_reset = reset_view.stats.unspent_skill_points;

    let rank_up = session.apply_intent(UiIntent::AdjustSkillRank {
        skill_id: first_skill_id,
        delta: 1,
    });
    assert!(rank_up.ok);
    let ranked_view = rank_up.session.unwrap();
    assert_eq!(ranked_view.skills[0].level, 1);
    assert_eq!(
        ranked_view.stats.unspent_skill_points,
        unspent_after_reset - 1
    );
    assert_eq!(ranked_view.stats.active_skill_count, 1);

    let exported = session.to_save_bytes().expect("export skill edits .d2s");
    let reloaded = EditorSession::from_bytes(&exported, "skills.d2s").expect("reload skills .d2s");
    let reloaded_view = reloaded.project();
    assert_eq!(reloaded_view.skills[0].level, 1);
    assert_eq!(
        reloaded_view.stats.unspent_skill_points,
        unspent_after_reset - 1
    );
    assert_eq!(reloaded_view.stats.active_skill_count, 1);
}

#[test]
fn cycle_14_shop_catalog_spawn_and_roundtrip_d2s() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s");
    let bytes = fs::read(&path).expect("read TESTDRUID fixture");
    let mut session = EditorSession::from_bytes(&bytes, "TESTDRUID_Quest3_AnyaRescued.d2s")
        .expect("parse fixture");
    let initial = session.project();
    assert!(!initial.shop_catalog.is_empty(), "shop_catalog must be populated from d2r-data");
    let initial_count = initial.items.len();

    // 1. Spawn a Normal compact rune ("base:r33") into inventory and verify Undo/Redo
    let spawned_rune = session.apply_intent(UiIntent::SpawnShopItem {
        catalog_id: "base:r33".to_string(),
        target_container: ShopSpawnTarget::SpawnAuto,
    });
    assert!(spawned_rune.ok, "{}", spawned_rune.rejection.map(|r| r.message).unwrap_or_default());
    let view1 = spawned_rune.session.unwrap();
    assert!(view1.dirty);
    assert!(view1.can_undo);
    assert_eq!(view1.items.len(), initial_count + 1);
    assert_eq!(view1.items.last().unwrap().code.trim(), "r33");

    let undone = session.apply_intent(UiIntent::Undo);
    assert!(undone.ok);
    assert_eq!(undone.session.unwrap().items.len(), initial_count);

    let redone = session.apply_intent(UiIntent::Redo);
    assert!(redone.ok);
    assert_eq!(redone.session.unwrap().items.len(), initial_count + 1);

    // 2. Spawn a Unique item from shop_catalog into temp_inventory, verify export guard, move to stash, and export/reload .d2s
    let unique_entry = initial
        .shop_catalog
        .iter()
        .find(|c| c.quality == ItemQualityKind::QualityUnique)
        .expect("at least one Unique item in shop_catalog")
        .clone();
    let spawned_uniq = session.apply_intent(UiIntent::SpawnShopItem {
        catalog_id: unique_entry.catalog_id.clone(),
        target_container: ShopSpawnTarget::SpawnTempInventory,
    });
    assert!(spawned_uniq.ok, "{}", spawned_uniq.rejection.map(|r| r.message).unwrap_or_default());
    let view2 = spawned_uniq.session.unwrap();
    assert!(!view2.export_supported, "temp_inventory should guard .d2s export");
    let uniq_idx = view2.items.last().unwrap().index;

    assert!(
        session
            .apply_intent(UiIntent::SelectItemForMove {
                item_index: Some(uniq_idx),
            })
            .ok
    );
    let moved_to_stash = session.apply_intent(UiIntent::MoveItem {
        item_index: uniq_idx,
        target_container: ContainerKind::Stash,
        x: 6,
        y: 6,
    });
    assert!(moved_to_stash.ok, "{}", moved_to_stash.rejection.map(|r| r.message).unwrap_or_default());

    let exported = session.to_save_bytes().expect("export spawned shop items .d2s");
    let reloaded = EditorSession::from_bytes(&exported, "shop_spawned.d2s")
        .expect("reload exported save with spawned shop items");
    let reloaded_view = reloaded.project();
    assert_eq!(reloaded_view.items.len(), initial_count + 2);
}

#[test]
fn cycle_15_revision_increments_and_intent_omits_static_shop_catalog() {
    let bytes =
        fixture_bytes("tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s");
    let mut session = EditorSession::from_bytes(&bytes, "TESTDRUID_Quest3_AnyaRescued.d2s")
        .expect("parse fixture");

    let initial = session.project();
    assert_eq!(initial.revision, 1);
    assert!(!initial.shop_catalog.is_empty());

    let env1 = session.apply_intent(UiIntent::SelectResDifficulty {
        difficulty: DifficultyMode::Hell,
    });
    assert!(env1.ok);
    let view1 = env1.session.expect("session after intent");
    assert_eq!(view1.revision, 2);
    assert!(
        view1.shop_catalog.is_empty(),
        "apply_intent must omit static shop_catalog to avoid per-intent payload bloat"
    );

    // Failed intent must NOT increment revision.
    let bad = session.apply_intent(UiIntent::SetMercLevel { level: 0 });
    assert!(!bad.ok);
    assert_eq!(
        bad.session.as_ref().map(|s| s.revision),
        Some(2),
        "Failed intent must preserve current revision"
    );
    assert_eq!(session.project().revision, 2);
}
