//! Contract tests for Rust-as-Store projection, `elm_rs` protocol drift, and `UiIntent` application.

use std::fs;
use std::path::Path;

use d2r_core::projection::{
    CharacterField, ContainerKind, DifficultyMode, EditMode, EditorSession, ItemCardView,
    ItemQualityKind, RejectionCode, SessionEnvelope, ShopSpawnTarget, SuggestedRecovery, UiIntent,
    generate_elm_protocol_module,
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
    assert!(merc_before.persistence_supported);
    assert!(merc_before.preview_only_reason.is_none());

    let lvl_ok = session.apply_intent(UiIntent::SetMercLevel { level: 85 });
    assert!(lvl_ok.ok);
    let after_lvl_view = lvl_ok.session.expect("session retained");
    assert!(after_lvl_view.dirty);
    assert_eq!(after_lvl_view.merc.stats.level, 85);

    let lvl_bad_low = session.apply_intent(UiIntent::SetMercLevel { level: 0 });
    assert!(!lvl_bad_low.ok);
    assert_eq!(
        lvl_bad_low.rejection.as_ref().map(|r| r.raw_code.as_str()),
        Some("merc_level_out_of_range")
    );
    assert_eq!(lvl_bad_low.session.unwrap().merc.stats.level, 85);

    let lvl_bad_high = session.apply_intent(UiIntent::SetMercLevel { level: 99 });
    assert!(!lvl_bad_high.ok);
    assert_eq!(
        lvl_bad_high.rejection.as_ref().map(|r| r.raw_code.as_str()),
        Some("merc_level_out_of_range")
    );

    let act5_ok = session.apply_intent(UiIntent::SelectMercAct { act_index: 5 });
    assert!(act5_ok.ok);
    let act5_view = act5_ok.session.expect("session retained");
    assert!(act5_view.dirty);
    assert_eq!(act5_view.merc.act_index, 5);

    let act4_bad = session.apply_intent(UiIntent::SelectMercAct { act_index: 4 });
    assert!(!act4_bad.ok);
    assert_eq!(
        act4_bad.rejection.as_ref().map(|r| r.raw_code.as_str()),
        Some("invalid_merc_act")
    );

    // Undo back to clean state before progression assertions
    assert!(session.apply_intent(UiIntent::Undo).ok);
    assert!(session.apply_intent(UiIntent::Undo).ok);

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
        socket_count: 0,
        socketed_names: vec![],
        socketed_items: vec![],
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
            .apply_intent(UiIntent::SelectStatMode {
                mode: EditMode::Free,
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

#[test]
fn cycle_17_item_mutation_enforces_bit_width_and_roundtrip_bounds() {
    let bytes =
        fixture_bytes("tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s");
    let mut session = EditorSession::from_bytes(&bytes, "TESTDRUID_Quest3_AnyaRescued.d2s")
        .expect("parse fixture");

    let view = session.project();

    // 1. Property bounds must come from STAT_COSTS (never hardcoded -10000..=10000) and exclude 511.
    let prop_item = view
        .items
        .iter()
        .find(|it| !it.detailed_properties.is_empty())
        .expect("at least one item with detailed_properties");
    assert!(
        prop_item.detailed_properties.iter().all(|p| p.stat_id != 511),
        "terminator stat 511 must not be exposed"
    );
    let target_prop = &prop_item.detailed_properties[0];
    let overflow_delta = (target_prop.max_value - target_prop.value).saturating_add(1);
    let prop_oob = session.apply_intent(UiIntent::MutateItemProperty {
        item_index: prop_item.index,
        stat_id: target_prop.stat_id,
        delta: overflow_delta,
    });
    assert!(!prop_oob.ok, "out-of-range property delta must be rejected");
    let prop_rej = prop_oob.rejection.expect("rejection details");
    assert_eq!(prop_rej.code, RejectionCode::RejectionStatOutOfRange);
    assert_eq!(
        prop_rej.suggested_action,
        Some(SuggestedRecovery::SuggestAdjustValue)
    );

    // 2. Base fields: defense bit-width, current_durability > max_durability, and missing quantity.
    if let Some(armor) = view
        .items
        .iter()
        .find(|it| it.defense.is_some() && it.max_durability.is_some() && it.quantity.is_none())
    {
        let def_oob = session.apply_intent(UiIntent::MutateItemBase {
            item_index: armor.index,
            defense: Some(99_999),
            max_durability: None,
            current_durability: None,
            quantity: None,
            is_ethereal: None,
        });
        assert!(!def_oob.ok, "defense above bit-width must be rejected");
        assert_eq!(
            def_oob.rejection.unwrap().code,
            RejectionCode::RejectionStatOutOfRange
        );

        let max_dur = armor.max_durability.unwrap();
        let dur_oob = session.apply_intent(UiIntent::MutateItemBase {
            item_index: armor.index,
            defense: None,
            max_durability: None,
            current_durability: Some(max_dur + 1),
            quantity: None,
            is_ethereal: None,
        });
        assert!(!dur_oob.ok, "current_durability > max_durability must be rejected");
        assert_eq!(
            dur_oob.rejection.unwrap().code,
            RejectionCode::RejectionStatOutOfRange
        );

        let qty_na = session.apply_intent(UiIntent::MutateItemBase {
            item_index: armor.index,
            defense: None,
            max_durability: None,
            current_durability: None,
            quantity: Some(5),
            is_ethereal: None,
        });
        assert!(!qty_na.ok, "mutating quantity on non-stackable item must be rejected");
    }

    // 3. Stackable item quantity bounds + .d2s roundtrip sync (item.body.quantity).
    if let Some(stack) = view.items.iter().find(|it| it.quantity.is_some()) {
        let qty_oob = session.apply_intent(UiIntent::MutateItemBase {
            item_index: stack.index,
            defense: None,
            max_durability: None,
            current_durability: None,
            quantity: Some(999),
            is_ethereal: None,
        });
        assert!(!qty_oob.ok, "quantity above 9-bit limit must be rejected");
        assert_eq!(
            qty_oob.rejection.unwrap().code,
            RejectionCode::RejectionStatOutOfRange
        );

        let qty_ok = session.apply_intent(UiIntent::MutateItemBase {
            item_index: stack.index,
            defense: None,
            max_durability: None,
            current_durability: None,
            quantity: Some(42),
            is_ethereal: None,
        });
        assert!(qty_ok.ok, "valid quantity mutation should succeed");

        let exported = session.to_save_bytes().expect("export .d2s");
        let reloaded =
            EditorSession::from_bytes(&exported, "qty_roundtrip.d2s").expect("reload .d2s");
        let reloaded_stack = reloaded
            .project()
            .items
            .into_iter()
            .find(|it| it.index == stack.index)
            .expect("reloaded stack item");
        assert_eq!(reloaded_stack.quantity, Some(42));
    }
}

#[test]
fn cycle_18_merc_item_move_act_swap_replaced_staging_and_d2s_roundtrip() {
    let bytes = fixture_bytes(
        "tests/fixtures/savegames/gameplay/normal/act3/TESTASSASSIN_Act3_ColdMercLvl21.d2s",
    );
    let mut session =
        EditorSession::from_bytes(&bytes, "TESTASSASSIN_Act3_ColdMercLvl21.d2s").expect("parse");

    let initial = session.project();
    assert!(initial.merc.hired);
    assert_eq!(initial.merc.act_index, 3);
    assert_eq!(initial.merc_items.len(), 2);

    // 1. Move one mercenary item (helm) to stash (free 10x10 space) and back to merc slot 1.
    let helm_idx = initial
        .merc_items
        .iter()
        .find(|it| it.x == 1)
        .expect("merc helm")
        .index;
    assert!(
        session
            .apply_intent(UiIntent::SelectItemForMove {
                item_index: Some(helm_idx),
            })
            .ok
    );
    let moved_to_stash = session.apply_intent(UiIntent::MoveItem {
        item_index: helm_idx,
        target_container: ContainerKind::Stash,
        x: 7,
        y: 7,
    });
    assert!(
        moved_to_stash.ok,
        "{}",
        moved_to_stash.rejection.map(|r| r.message).unwrap_or_default()
    );
    assert_eq!(moved_to_stash.session.unwrap().merc_items.len(), 1);

    let moved_back = session.apply_intent(UiIntent::MoveItem {
        item_index: helm_idx,
        target_container: ContainerKind::MercEquipment,
        x: 1,
        y: 0,
    });
    assert!(moved_back.ok);
    assert_eq!(moved_back.session.unwrap().merc_items.len(), 2);

    // 2. Switch Act 3 -> Act 2: old merc & 2 items move to Replaced Staging, export guarded.
    let switch_act2 = session.apply_intent(UiIntent::SelectMercAct { act_index: 2 });
    assert!(switch_act2.ok);
    let act2_view = switch_act2.session.unwrap();
    assert_eq!(act2_view.merc.act_index, 2);
    assert_eq!(act2_view.merc_items.len(), 0);
    assert_eq!(act2_view.merc_replaced_items.len(), 2);
    assert_eq!(act2_view.merc.dismissed.as_ref().map(|d| d.act_index), Some(3));
    assert!(!act2_view.export_supported);

    // Staged items in MercReplaced can be picked up and placed back into MercReplaced,
    // while external items (e.g. Inventory) cannot be dropped into MercReplaced.
    let drop_replaced = session.apply_intent(UiIntent::MoveItem {
        item_index: helm_idx,
        target_container: ContainerKind::MercReplaced,
        x: 1,
        y: 0,
    });
    assert!(drop_replaced.ok);
    if let Some(inv_item) = act2_view
        .items
        .iter()
        .find(|it| it.container == ContainerKind::Inventory)
    {
        let drop_external = session.apply_intent(UiIntent::MoveItem {
            item_index: inv_item.index,
            target_container: ContainerKind::MercReplaced,
            x: 1,
            y: 0,
        });
        assert!(!drop_external.ok);
    }

    // 3. Restore Replaced Mercenary -> swaps Act 3 and its 2 items back.
    let restored = session.apply_intent(UiIntent::RestoreReplacedMerc);
    assert!(restored.ok);
    let restored_view = restored.session.unwrap();
    assert_eq!(restored_view.merc.act_index, 3);
    assert_eq!(restored_view.merc_items.len(), 2);
    assert_eq!(restored_view.merc_replaced_items.len(), 0);

    // 4. Switch Act 3 -> Act 1, then Reclaim & Clear replaced staging, change level, and roundtrip .d2s!
    assert!(session.apply_intent(UiIntent::SelectMercAct { act_index: 1 }).ok);
    let cleared = session.apply_intent(UiIntent::ClearReplacedMerc {
        reclaim_to_storage: true,
    });
    assert!(cleared.ok);
    let cleared_view = cleared.session.unwrap();
    assert!(cleared_view.export_supported);
    assert!(cleared_view.merc.dismissed.is_none());
    assert!(cleared_view.merc_replaced_items.is_empty());

    assert!(session.apply_intent(UiIntent::SetMercLevel { level: 55 }).ok);

    let exported = session.to_save_bytes().expect("export merc changes .d2s");
    let reloaded = EditorSession::from_bytes(&exported, "merc_roundtrip.d2s").expect("reload .d2s");
    let reloaded_view = reloaded.project();
    assert_eq!(reloaded_view.merc.act_index, 1);
    assert_eq!(reloaded_view.merc.stats.level, 55);
}

#[test]
fn cycle_19_quick_move_duplicate_delete_and_d2s_roundtrip() {
    let bytes =
        fixture_bytes("tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s");
    let mut session =
        EditorSession::from_bytes(&bytes, "TESTDRUID_Quest3_AnyaRescued.d2s").expect("parse");

    let initial = session.project();
    let initial_count = initial.items.len();
    let charm = initial
        .items
        .iter()
        .find(|it| it.container == ContainerKind::Inventory && it.width == 1 && it.height == 1)
        .expect("1x1 inventory charm");

    // 1. QuickMoveItem to Stash finds first free slot and moves item cleanly.
    let qm = session.apply_intent(UiIntent::QuickMoveItem {
        item_index: charm.index,
        target_container: ContainerKind::Stash,
    });
    assert!(qm.ok, "{:?}", qm.rejection);
    let qm_view = qm.session.unwrap();
    let moved_card = qm_view
        .items
        .iter()
        .find(|it| it.index == charm.index)
        .expect("moved card");
    assert_eq!(moved_card.container, ContainerKind::Stash);

    // 2. DuplicateItem clones item into free Inventory/Stash/TempInventory slot.
    let dup = session.apply_intent(UiIntent::DuplicateItem {
        item_index: charm.index,
    });
    assert!(dup.ok, "{:?}", dup.rejection);
    let dup_view = dup.session.unwrap();
    assert_eq!(dup_view.items.len(), initial_count + 1);

    // 3. DeleteItem removes the cloned item and Undo restores it.
    let cloned_idx = dup_view.items.last().expect("cloned item").index;
    let del = session.apply_intent(UiIntent::DeleteItem {
        item_index: cloned_idx,
    });
    assert!(del.ok, "{:?}", del.rejection);
    assert_eq!(del.session.unwrap().items.len(), initial_count);

    let undone = session.apply_intent(UiIntent::Undo);
    assert!(undone.ok);
    assert_eq!(undone.session.unwrap().items.len(), initial_count + 1);

    // 4. Export .d2s and reload to verify duplicated item + quick-moved item persist.
    let exported = session.to_save_bytes().expect("export cycle 19 .d2s");
    let reloaded = EditorSession::from_bytes(&exported, "cycle19.d2s").expect("reload .d2s");
    assert_eq!(reloaded.project().items.len(), initial_count + 1);
}

#[test]
fn cycle_20_add_remove_item_property_and_d2s_roundtrip() {
    let bytes = fixture_bytes(
        "tests/fixtures/savegames/gameplay/normal/act3/TESTASSASSIN_Act3_ColdMercLvl21.d2s",
    );
    let mut session =
        EditorSession::from_bytes(&bytes, "TESTASSASSIN_Act3_ColdMercLvl21.d2s").expect("parse");
    let initial = session.project();

    // Verify stat_catalog is populated from d2r-data STAT_COSTS and contains strength (#0).
    assert!(
        !initial.stat_catalog.is_empty(),
        "stat_catalog should be populated on initial project()"
    );
    assert!(initial.stat_catalog.iter().any(|s| s.stat_id == 0 && s.name == "strength"));

    // Pick a non-compact item with properties.
    let target_item = initial
        .items
        .iter()
        .find(|it| !it.detailed_properties.is_empty())
        .expect("non-compact item with detailed_properties");
    let chosen_stat = initial
        .stat_catalog
        .iter()
        .find(|s| {
            s.param_bits == 0
                && !target_item
                    .detailed_properties
                    .iter()
                    .any(|p| p.stat_id == s.stat_id)
                && s.max_value >= 15
        })
        .expect("unused writable stat");

    // 1. AddItemProperty injects the new stat and omits static catalogs in delta projection.
    let add_env = session.apply_intent(UiIntent::AddItemProperty {
        item_index: target_item.index,
        stat_id: chosen_stat.stat_id,
        value: 15,
        param: 0,
    });
    assert!(add_env.ok, "{:?}", add_env.rejection);
    let add_view = add_env.session.unwrap();
    assert!(
        add_view.stat_catalog.is_empty(),
        "project_after_intent should omit static stat_catalog"
    );
    let updated = add_view
        .items
        .iter()
        .find(|it| it.index == target_item.index)
        .expect("updated item");
    assert!(
        updated
            .detailed_properties
            .iter()
            .any(|p| p.stat_id == chosen_stat.stat_id && p.value == 15)
    );

    // 2. Duplicate stat add is rejected cleanly.
    let dup_err = session.apply_intent(UiIntent::AddItemProperty {
        item_index: target_item.index,
        stat_id: chosen_stat.stat_id,
        value: 10,
        param: 0,
    });
    assert!(!dup_err.ok);

    // 3. RemoveItemProperty removes it and Undo restores it.
    let rem_env = session.apply_intent(UiIntent::RemoveItemProperty {
        item_index: target_item.index,
        stat_id: chosen_stat.stat_id,
    });
    assert!(rem_env.ok, "{:?}", rem_env.rejection);
    assert!(
        !rem_env
            .session
            .unwrap()
            .items
            .iter()
            .find(|it| it.index == target_item.index)
            .unwrap()
            .detailed_properties
            .iter()
            .any(|p| p.stat_id == chosen_stat.stat_id)
    );

    let undo_env = session.apply_intent(UiIntent::Undo);
    assert!(undo_env.ok);
    assert!(
        undo_env
            .session
            .unwrap()
            .items
            .iter()
            .find(|it| it.index == target_item.index)
            .unwrap()
            .detailed_properties
            .iter()
            .any(|p| p.stat_id == chosen_stat.stat_id && p.value == 15)
    );

    // 4. .d2s export and reload preserves the injected property.
    let exported = session.to_save_bytes().expect("export cycle 20 .d2s");
    let reloaded = EditorSession::from_bytes(&exported, "cycle20.d2s").expect("reload .d2s");
    let reloaded_item = reloaded
        .project()
        .items
        .into_iter()
        .find(|it| it.index == target_item.index)
        .expect("reloaded item");
    assert!(
        reloaded_item
            .detailed_properties
            .iter()
            .any(|p| p.stat_id == chosen_stat.stat_id && p.value == 15)
    );

    // 5. Cycle 22: parameterized stat injection (param_bits > 0) + out-of-range param rejection + .d2s roundtrip.
    let param_stat = initial
        .stat_catalog
        .iter()
        .find(|s| {
            s.param_bits >= 4
                && !reloaded_item
                    .detailed_properties
                    .iter()
                    .any(|p| p.stat_id == s.stat_id)
                && s.max_value >= 2
                && s.min_value <= 2
        })
        .expect("parameterized writable stat in catalog");

    let bad_param_err = session.apply_intent(UiIntent::AddItemProperty {
        item_index: target_item.index,
        stat_id: param_stat.stat_id,
        value: 2,
        param: -1,
    });
    assert!(!bad_param_err.ok, "negative param must be rejected");

    let add_param_env = session.apply_intent(UiIntent::AddItemProperty {
        item_index: target_item.index,
        stat_id: param_stat.stat_id,
        value: 2,
        param: 7,
    });
    assert!(add_param_env.ok, "{:?}", add_param_env.rejection);

    let set_param_env = session.apply_intent(UiIntent::SetItemPropertyParam {
        item_index: target_item.index,
        stat_id: param_stat.stat_id,
        param: 12,
    });
    assert!(set_param_env.ok, "{:?}", set_param_env.rejection);

    let exported_param = session.to_save_bytes().expect("export cycle 22 param .d2s");
    let reloaded_param = EditorSession::from_bytes(&exported_param, "cycle22_param.d2s")
        .expect("reload cycle 22 param .d2s");
    let reloaded_param_item = reloaded_param
        .project()
        .items
        .into_iter()
        .find(|it| it.index == target_item.index)
        .expect("reloaded param item");
    assert!(
        reloaded_param_item
            .detailed_properties
            .iter()
            .any(|p| p.stat_id == param_stat.stat_id && p.value == 2 && p.param == 12),
        "expected stat {} with value=2, param=12 after .d2s roundtrip",
        param_stat.stat_id
    );
}

#[test]
fn cycle21_socketed_item_view_and_gold_roundtrip() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s");
    let bytes = std::fs::read(&fixture).expect("read TESTDRUID fixture");
    let mut session = EditorSession::from_bytes(&bytes, "TESTDRUID_Quest3_AnyaRescued.d2s")
        .expect("parse fixture");

    let initial_view = session.project();
    assert!(initial_view.merc.persistence_supported);
    for item in initial_view.items.iter().chain(initial_view.merc_items.iter()) {
        assert_eq!(item.socketed_items.len(), item.socketed_names.len());
        assert!(item.socket_count >= item.socketed_items.len() as i32);
    }

    let stash_env = session.apply_intent(UiIntent::SetCharacterField {
        field: d2r_core::projection::CharacterField::GoldStash,
        value: 777_777,
    });
    assert!(stash_env.ok);
    let inv_env = session.apply_intent(UiIntent::SetCharacterField {
        field: d2r_core::projection::CharacterField::GoldCharacter,
        value: 55_555,
    });
    assert!(inv_env.ok);

    let exported = session.to_save_bytes().expect("export cycle 21 .d2s");
    let reloaded = EditorSession::from_bytes(&exported, "cycle21.d2s").expect("reload .d2s");
    let reloaded_view = reloaded.project();
    assert_eq!(reloaded_view.stats.gold_stash, 777_777);
    assert_eq!(reloaded_view.stats.gold_inventory, 55_555);
}

#[test]
fn cycle_23_item_forge_draft_staging_apply_discard_and_undo_roundtrip() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/savegames/gameplay/normal/act3/TESTASSASSIN_Act3_ColdMercLvl21.d2s");
    let bytes = std::fs::read(&fixture).expect("read fixture");
    let mut session = EditorSession::from_bytes(&bytes, "TESTASSASSIN_Act3_ColdMercLvl21.d2s")
        .expect("load session");
    let initial = session.project();
    let target_item = initial
        .items
        .iter()
        .find(|it| !it.detailed_properties.is_empty())
        .expect("non-compact item");
    let chosen_stat = initial
        .stat_catalog
        .iter()
        .find(|s| {
            s.param_bits == 0
                && !target_item
                    .detailed_properties
                    .iter()
                    .any(|p| p.stat_id == s.stat_id)
                && s.max_value >= 25
        })
        .expect("unused stat");

    // 1. Open draft and mutate; main session & undo_stack remain untouched and export is guarded while draft is dirty.
    let open_env = session.apply_intent(UiIntent::OpenItemForgeDraft {
        item_index: target_item.index,
    });
    assert!(open_env.ok);
    let open_view = open_env.session.unwrap();
    assert!(!open_view.active_item_draft.as_ref().unwrap().dirty);
    assert!(open_view.export_supported);

    let add_draft_env = session.apply_intent(UiIntent::AddItemProperty {
        item_index: target_item.index,
        stat_id: chosen_stat.stat_id,
        value: 20,
        param: 0,
    });
    assert!(add_draft_env.ok);
    let draft_view = add_draft_env.session.unwrap();
    assert!(!draft_view.dirty, "main session must stay clean while editing draft");
    assert!(!draft_view.can_undo, "draft edits must not pollute main undo_stack");
    assert!(!draft_view.export_supported, "dirty draft must block .d2s export");
    let active_draft = draft_view.active_item_draft.as_ref().unwrap();
    assert!(active_draft.dirty);
    assert!(
        active_draft
            .card
            .detailed_properties
            .iter()
            .any(|p| p.stat_id == chosen_stat.stat_id && p.value == 20)
    );
    assert!(
        !draft_view
            .items
            .iter()
            .find(|it| it.index == target_item.index)
            .unwrap()
            .detailed_properties
            .iter()
            .any(|p| p.stat_id == chosen_stat.stat_id),
        "main items must not reflect draft edits before ApplyItemForgeDraft"
    );

    // 2. Discard draft drops staged changes cleanly.
    let discard_env = session.apply_intent(UiIntent::DiscardItemForgeDraft);
    assert!(discard_env.ok);
    let discarded = discard_env.session.unwrap();
    assert!(discarded.active_item_draft.is_none());
    assert!(!discarded.dirty);
    assert!(discarded.export_supported);

    // 3. Open again, make two edits, Apply commits atomically in 1 Undo step.
    assert!(
        session
            .apply_intent(UiIntent::OpenItemForgeDraft {
                item_index: target_item.index,
            })
            .ok
    );
    assert!(
        session
            .apply_intent(UiIntent::AddItemProperty {
                item_index: target_item.index,
                stat_id: chosen_stat.stat_id,
                value: 20,
                param: 0,
            })
            .ok
    );
    assert!(
        session
            .apply_intent(UiIntent::MutateItemProperty {
                item_index: target_item.index,
                stat_id: chosen_stat.stat_id,
                delta: 5,
            })
            .ok
    );
    let apply_env = session.apply_intent(UiIntent::ApplyItemForgeDraft);
    assert!(apply_env.ok, "{:?}", apply_env.rejection);
    let applied = apply_env.session.unwrap();
    assert!(applied.active_item_draft.is_none());
    assert!(applied.dirty);
    assert!(applied.can_undo);
    assert!(applied.export_supported);
    assert!(
        applied
            .items
            .iter()
            .find(|it| it.index == target_item.index)
            .unwrap()
            .detailed_properties
            .iter()
            .any(|p| p.stat_id == chosen_stat.stat_id && p.value == 25)
    );

    // 4. Single Undo reverts the entire draft transaction; Redo restores it for .d2s roundtrip.
    let undo_env = session.apply_intent(UiIntent::Undo);
    assert!(undo_env.ok);
    let undone = undo_env.session.unwrap();
    assert!(!undone.can_undo, "all draft edits must collapse into 1 undo step");
    assert!(
        !undone
            .items
            .iter()
            .find(|it| it.index == target_item.index)
            .unwrap()
            .detailed_properties
            .iter()
            .any(|p| p.stat_id == chosen_stat.stat_id)
    );

    assert!(session.apply_intent(UiIntent::Redo).ok);
    let exported = session.to_save_bytes().expect("export cycle 23 .d2s");
    let reloaded = EditorSession::from_bytes(&exported, "cycle23.d2s").expect("reload .d2s");
    assert!(
        reloaded
            .project()
            .items
            .iter()
            .find(|it| it.index == target_item.index)
            .unwrap()
            .detailed_properties
            .iter()
            .any(|p| p.stat_id == chosen_stat.stat_id && p.value == 25)
    );
}

#[test]
fn cycle_25_stat_mode_legit_free_reset_sync_and_vitals_breakdown() {
    let bytes = fixture_bytes("tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s");
    let mut session = EditorSession::from_bytes(&bytes, "testdruid_cycle25.d2s").expect("load session");
    let v0 = session.project();
    assert_eq!(v0.stat_mode, EditMode::Legit);
    assert!(v0.stats.life <= v0.stats.max_life);
    assert!(v0.stats.mana <= v0.stats.max_mana);
    assert!(v0.stats.stamina <= v0.stats.max_stamina);
    assert_eq!(v0.stats.max_life, v0.stats.life_pure + v0.stats.life_bonus);

    // 1. SyncStatPointsToRules aligns unspent_stat_points to rule_cap - invested
    let sync_env = session.apply_intent(UiIntent::SyncStatPointsToRules);
    assert!(sync_env.ok);
    let synced = sync_env.session.unwrap();
    let expected_unspent = (synced.stats.stat_points_rule_cap - synced.stats.stat_points_invested).max(0);
    assert_eq!(synced.stats.unspent_stat_points, expected_unspent);

    // 2. Legit mode debits unspent_stat_points on Strength increase and rejects over-spend
    let cur_str = synced.stats.strength_base;
    let inc_env = session.apply_intent(UiIntent::SetCharacterField {
        field: CharacterField::Strength,
        value: cur_str + 5,
    });
    assert!(inc_env.ok);
    let after_inc = inc_env.session.unwrap();
    assert_eq!(after_inc.stats.strength_base, cur_str + 5);
    assert_eq!(after_inc.stats.unspent_stat_points, expected_unspent - 5);

    let over_env = session.apply_intent(UiIntent::SetCharacterField {
        field: CharacterField::Strength,
        value: cur_str + 5 + expected_unspent + 10,
    });
    assert!(!over_env.ok);

    // 3. ResetStats returns attributes to class base and refunds invested points
    let reset_env = session.apply_intent(UiIntent::ResetStats);
    assert!(reset_env.ok);
    let after_reset = reset_env.session.unwrap();
    assert_eq!(after_reset.stats.strength_base, after_reset.schema.class_base_str);
    assert_eq!(after_reset.stats.stat_points_invested, 0);

    // 4. Free mode allows direct StatPoints and arbitrary Strength within 1..=1023
    assert!(session.apply_intent(UiIntent::SelectStatMode { mode: EditMode::Free }).ok);
    let free_env = session.apply_intent(UiIntent::SetCharacterField {
        field: CharacterField::StatPoints,
        value: 321,
    });
    assert!(free_env.ok);
    assert_eq!(free_env.session.unwrap().stats.unspent_stat_points, 321);
}
