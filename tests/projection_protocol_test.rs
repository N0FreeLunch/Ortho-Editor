//! Contract tests for Rust-as-Store projection, `elm_rs` protocol drift, and `UiIntent` application.

use std::fs;
use std::path::Path;

use d2r_core::projection::{
    DifficultyMode, EditMode, EditorSession, ItemCardView, SessionEnvelope, UiIntent,
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

    let lvl_ok = session.apply_intent(UiIntent::SetMercLevel { level: 85 });
    assert!(lvl_ok.ok);
    assert_eq!(lvl_ok.session.unwrap().merc.stats.level, 85);

    let lvl_bad_low = session.apply_intent(UiIntent::SetMercLevel { level: 0 });
    assert!(!lvl_bad_low.ok);
    assert_eq!(
        lvl_bad_low.rejection.as_ref().map(|r| r.code.as_str()),
        Some("merc_level_out_of_range")
    );
    assert_eq!(lvl_bad_low.session.unwrap().merc.stats.level, 85);

    let lvl_bad_high = session.apply_intent(UiIntent::SetMercLevel { level: 99 });
    assert!(!lvl_bad_high.ok);
    assert_eq!(
        lvl_bad_high.rejection.as_ref().map(|r| r.code.as_str()),
        Some("merc_level_out_of_range")
    );

    let act5_ok = session.apply_intent(UiIntent::SelectMercAct { act_index: 5 });
    assert!(act5_ok.ok);
    let act5_merc = act5_ok.session.unwrap().merc;
    assert_eq!(act5_merc.act_index, 5);
    assert_eq!(act5_merc.class_name, "Barbarian (Act 5)");

    let act4_bad = session.apply_intent(UiIntent::SelectMercAct { act_index: 4 });
    assert!(!act4_bad.ok);
    assert_eq!(
        act4_bad.rejection.as_ref().map(|r| r.code.as_str()),
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
        quality: "Normal".to_string(),
        container: "temp_inventory".to_string(),
        slot_label: None,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        properties: vec![],
    });
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
        empty_resp.error.as_ref().map(|e| e.code.as_str()),
        Some("no_session_loaded")
    );

    let malformed = store.handle_json_line(r#"{"cmd":"non_existent"}"#);
    assert!(!malformed.ok);
    assert_eq!(
        malformed.error.as_ref().map(|e| e.code.as_str()),
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
        bad_merc.error.as_ref().map(|e| e.code.as_str()),
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
        bad_level.rejection.as_ref().map(|r| r.code.as_str()),
        Some("invalid_character_mutation")
    );
    assert!(!bad_level.session.expect("session stays valid").dirty);

    let bad_gold = session.apply_intent(UiIntent::SetCharacterField {
        field: CharacterField::GoldStash,
        value: 3_000_000,
    });
    assert!(!bad_gold.ok);
    assert_eq!(
        bad_gold.rejection.as_ref().map(|r| r.code.as_str()),
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
