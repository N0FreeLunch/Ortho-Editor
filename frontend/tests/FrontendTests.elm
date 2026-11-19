module FrontendTests exposing (suite)

import Component.Workspace as Workspace
import Expect
import Generated.Protocol as Protocol
    exposing
        ( ContainerKind(..)
        , DifficultyMode(..)
        , EditMode(..)
        , ItemQualityKind(..)
        , uiIntentEncoder
        )
import Html.Attributes
import Json.Decode
import Json.Encode
import State.UiState exposing (DragKind(..), PaintMode(..), ViewTab(..))
import Test exposing (Test, describe, test)
import Test.Html.Event as Event
import Test.Html.Query as Query
import Test.Html.Selector as Selector
import Update exposing (Model, Msg(..), OutMsg(..))


sampleSessionJson : String
sampleSessionJson =
    """
    {
      "ok": true,
      "session": {
        "fixture_label": "TESTDRUID_Quest3_AnyaRescued.d2s",
        "save_version": 105,
        "export_supported": true,
        "export_unsupported_reason": null,
        "item_dialect": "d2r-v3",
        "char_name": "TESTDRUID",
        "char_class": "Druid",
        "char_level": 31,
        "experience": 6105692,
        "is_expansion": true,
        "res_difficulty": "Normal",
        "skill_mode": "Legit",
        "stat_mode": "Legit",
        "revision": 1,
        "dirty": false,
        "schema": {
          "name_min_len": 2,
          "name_max_len": 15,
          "level_min": 1,
          "level_max": 99,
          "stat_min": 1,
          "stat_max": 1023,
          "stat_points_min": 0,
          "stat_points_max": 1023,
          "gold_character_max": 310000,
          "gold_stash_max": 2500000,
          "class_base_str": 15,
          "class_base_dex": 20,
          "class_base_vit": 25,
          "class_base_ene": 20,
          "item_defense_max": 2037,
          "item_quantity_max": 511,
          "skill_legit_max": 20,
          "skill_free_max": 20
        },
        "stats": {
          "strength_base": 60, "strength_bonus": 18,
          "dexterity_base": 20, "dexterity_bonus": 2,
          "vitality_base": 60, "vitality_bonus": 0,
          "energy_base": 20, "energy_bonus": 18,
          "life": 190, "max_life": 230,
          "mana": 80, "max_mana": 126,
          "stamina": 149, "max_stamina": 190,
          "unspent_stat_points": 75,
          "unspent_skill_points": 1,
          "gold_inventory": 229492,
          "gold_stash": 281974,
          "active_skill_count": 11,
          "fcr": 20,
          "fhr": 30,
          "frw": 25,
          "ias": 15,
          "magic_find": 55,
          "all_skills_bonus": 2,
          "stat_points_rule_cap": 155,
          "stat_points_invested": 80,
          "life_pure": 170,
          "life_bonus": 60,
          "mana_pure": 80,
          "mana_bonus": 46,
          "stamina_pure": 149,
          "stamina_bonus": 41
        },
        "resistances": {
          "difficulty": "Normal",
          "fire": 75, "cold": 42, "light": 65, "poison": 90,
          "fire_cap": 75, "cold_cap": 75, "light_cap": 75, "poison_cap": 90,
          "anya_bonus": 0, "difficulty_penalty": 0
        },
        "container_counts": {
          "equipment": 10, "inventory": 17, "belt": 11,
          "stash": 5, "cube": 0, "temp_inventory": 1,
          "unknown": 0, "mercenary": 3
        },
        "merc": {
          "hired": true,
          "act_index": 2,
          "class_name": "Desert Mercenary (Act 2)",
          "subtype_name": "N/A",
          "experience": 3357936,
          "stats": {
            "level": 28,
            "base_str": 90, "bonus_str": 12,
            "base_dex": 80, "bonus_dex": 4,
            "base_life": 330, "bonus_life": 40,
            "base_def": 214, "bonus_def": 93,
            "fire_res": 75, "cold_res": 63, "light_res": 63, "poison_res": 63,
            "innate_aura": "Paladin Aura (Act 2)",
            "equipment_auras": ["Aura #120 Lv.15"]
          },
          "equipped_count": 3,
          "persistence_supported": true,
          "preview_only_reason": null,
          "dismissed": null
        },
        "progression": {
          "is_expansion": true,
          "total_quests_per_difficulty": 27,
          "total_waypoints_per_difficulty": 39,
          "quest_catalog": [
            { "key": "a1q1", "act": 1, "index": 0, "name": "Den of Evil", "name_ko": "덴 오브 이블", "desc_en": "Look for the Den in the wilderness outside the Rogues' Camp.", "desc_ko": "로그 캠프 밖에 있는 덴 오브 이블을 찾으시오.", "has_npc_reward": true },
            { "key": "a1q2", "act": 1, "index": 1, "name": "Sisters' Burial Grounds", "name_ko": "베리얼 그라운드", "desc_en": "Look for Blood Raven in the Burial Grounds next to the Cold Plains.", "desc_ko": "콜드 플레인 부근 베리얼 그라운드에서 블러드 레이븐을 찾으시오.", "has_npc_reward": false },
            { "key": "a5q3", "act": 5, "index": 2, "name": "Prison of Ice", "name_ko": "얼음 감옥", "desc_en": "Look for Anya under the Crystalline Passage by the Frozen River.", "desc_ko": "크리스탈라인을 지나 프로즌리버에서 앤야를 찾으시오", "has_npc_reward": true }
          ],
          "waypoint_catalog": [
            { "bit": 0, "act": 1, "index": 0, "name": "Rogue Encampment", "name_ko": "로그 캠프" },
            { "bit": 1, "act": 1, "index": 1, "name": "Cold Plains", "name_ko": "콜드 플레인" }
          ],
          "normal_quests_completed": ["a1q1"],
          "nightmare_quests_completed": [],
          "hell_quests_completed": [],
          "normal_quest_states": [
            { "key": "a1q1", "status": "QuestCompleted", "raw_state": 4097 },
            { "key": "a1q2", "status": "QuestRewardPending", "raw_state": 2 },
            { "key": "a5q3", "status": "QuestInProgress", "raw_state": 4 }
          ],
          "nightmare_quest_states": [],
          "hell_quest_states": [],
          "normal_waypoints_active": [0],
          "nightmare_waypoints_active": [],
          "hell_waypoints_active": []
        },
        "items": [
          {
            "index": 0, "code": "rin", "name_en": "Nagelring", "name_ko": "나겔링",
            "quality": "QualityUnique", "container": "Equipment", "slot_label": "Left Finger",
            "x": 0, "y": 0, "width": 1, "height": 1,
            "item_level": 49,
            "base_attributes": ["Required Level: 7"],
            "properties": ["+30% Better Chance of Getting Magic Items"],
            "set_name": null, "set_pieces": [], "piece_set_bonuses": [], "set_bonuses": [],
            "socket_count": 0,
            "socketed_names": [],
            "socketed_items": [],
            "image_url": "/item-vectors/misc/ring/ring.svg",
            "is_belt_consumable": false,
            "detailed_properties": [],
            "defense": null,
            "current_durability": null,
            "max_durability": null,
            "quantity": null,
            "is_ethereal": false
          }
        ],
        "merc_items": [
          {
            "index": 0, "code": "vou", "name_en": "Insight", "name_ko": "통찰",
            "quality": "QualityRuneword", "container": "MercEquipment", "slot_label": "Right Hand",
            "x": 0, "y": 0, "width": 2, "height": 4,
            "item_level": 45,
            "base_attributes": ["Two-Hand Damage: 24 to 105", "Durability: 25 of 50"],
            "properties": ["Level 15 Meditation Aura When Equipped"],
            "set_name": null, "set_pieces": [], "piece_set_bonuses": [], "set_bonuses": [],
            "socket_count": 4,
            "socketed_names": ["Ral Rune", "Tir Rune", "Tal Rune", "Sol Rune"],
            "socketed_items": [
              { "code": "r08", "name_en": "Ral Rune", "name_ko": "랄 룬", "quality": "QualityNormal", "image_url": null, "properties": ["+30% Fire Resist"] },
              { "code": "r03", "name_en": "Tir Rune", "name_ko": "티르 룬", "quality": "QualityNormal", "image_url": null, "properties": ["+2 to Mana After Each Kill"] },
              { "code": "r07", "name_en": "Tal Rune", "name_ko": "탈 룬", "quality": "QualityNormal", "image_url": null, "properties": ["+30% Poison Resist"] },
              { "code": "r12", "name_en": "Sol Rune", "name_ko": "솔 룬", "quality": "QualityNormal", "image_url": null, "properties": ["+9 to Minimum Damage"] }
            ],
            "image_url": null,
            "is_belt_consumable": false,
            "detailed_properties": [],
            "defense": null,
            "current_durability": null,
            "max_durability": null,
            "quantity": null,
            "is_ethereal": false
          }
        ],
        "merc_replaced_items": [],
        "temp_items": [
          {
            "index": 0, "code": "r01", "name_en": "El Rune", "name_ko": "엘 룬",
            "quality": "QualityNormal", "container": "TempInventory", "slot_label": null,
            "x": 0, "y": 0, "width": 1, "height": 1,
            "item_level": 1,
            "base_attributes": [],
            "properties": [],
            "set_name": null, "set_pieces": [], "piece_set_bonuses": [], "set_bonuses": [],
            "socket_count": 0,
            "socketed_names": [],
            "socketed_items": [],
            "image_url": null,
            "is_belt_consumable": false,
            "detailed_properties": [],
            "defense": null,
            "current_durability": null,
            "max_durability": null,
            "quantity": null,
            "is_ethereal": false
          }
        ],
        "selected_move_item_index": null,
        "allowed_move_targets": [],
        "belt_move_restricted_reason": null,
        "can_undo": false,
        "can_redo": false,
        "active_weapon_set": 1,
        "skills": [
          {
            "skill_id": 221,
            "slot_index": 0,
            "tree_index": 0,
            "row": 1,
            "col": 1,
            "name_en": "Raven",
            "name_ko": "큰까마귀",
            "req_level": 1,
            "level": 5,
            "bonus_level": 2,
            "desc_en": "Summon ravens to peck at your enemies",
            "desc_ko": "큰까마귀를 소환합니다",
            "prereq_ids": [],
            "synergy_ids": [],
            "unlock_ids": [225],
            "damage": null
          },
          {
            "skill_id": 225,
            "slot_index": 4,
            "tree_index": 0,
            "row": 2,
            "col": 2,
            "name_en": "Firestorm",
            "name_ko": "화염 폭풍",
            "req_level": 1,
            "level": 0,
            "bonus_level": 0,
            "desc_en": "Unleash fiery waves",
            "desc_ko": "불타는 파도",
            "prereq_ids": [221],
            "synergy_ids": [221],
            "unlock_ids": [],
            "damage": {
              "elem_type": "Fire",
              "pure_min": 4,
              "pure_max": 9,
              "synergy_pct": 115,
              "synergy_bonus_min": 4,
              "synergy_bonus_max": 10,
              "gear_bonus_min": 0,
              "gear_bonus_max": 0,
              "total_min": 8,
              "total_max": 19,
              "preview_only": true
            }
          },
          {
            "skill_id": 231,
            "slot_index": 10,
            "tree_index": 1,
            "row": 1,
            "col": 1,
            "name_en": "Werewolf",
            "name_ko": "늑대인간",
            "req_level": 1,
            "level": 3,
            "bonus_level": 2,
            "desc_en": "Transform into a werewolf",
            "desc_ko": "늑대인간으로 변신합니다",
            "prereq_ids": [],
            "synergy_ids": [],
            "unlock_ids": [],
            "damage": null
          }
        ],
        "shop_catalog": [
          {
            "catalog_id": "uniq:345",
            "code": "usk",
            "name_en": "Andariel's Visage",
            "name_ko": "안다리엘의 두개골",
            "quality": "QualityUnique",
            "category": "armor",
            "width": 2,
            "height": 2,
            "image_url": null,
            "summary": "Unique · qlvl 85"
          },
          {
            "catalog_id": "base:r33",
            "code": "r33",
            "name_en": "Zod Rune",
            "name_ko": "조드 룬",
            "quality": "QualityNormal",
            "category": "runes_gems",
            "width": 1,
            "height": 1,
            "image_url": null,
            "summary": "Normal · r33"
          }
        ],
        "stat_catalog": [
          {
            "stat_id": 0,
            "name": "strength",
            "min_value": -32,
            "max_value": 223,
            "param_bits": 0,
            "value_label": "Value",
            "param_label": "Param",
            "param_kind": "ParamNone"
          },
          {
            "stat_id": 105,
            "name": "item_fastercastrate",
            "min_value": -20,
            "max_value": 235,
            "param_bits": 0,
            "value_label": "Value",
            "param_label": "Param",
            "param_kind": "ParamNone"
          }
        ],
        "param_catalog": {
          "classes": [ { "value": 1, "label": "#1 Sorceress" } ],
          "elements": [ { "value": 0, "label": "#0 Fire" } ],
          "skill_tabs": [ { "value": 8, "label": "#8 Fire Skills (Sorceress Only)" } ],
          "skills": [ { "value": 54, "label": "#54 Teleport" } ],
          "packed_skill": { "stride": 64, "min_level": 1, "max_level": 63 }
        },
        "active_item_draft": null
      },
      "rejection": null
    }
    """


loadedModel : Model
loadedModel =
    case Json.Decode.decodeString Json.Decode.value sampleSessionJson of
        Ok rawVal ->
            let
                ( nextModel, _ ) =
                    Update.update (ReceiveSessionEnvelope rawVal) Update.init
            in
            nextModel

        Err _ ->
            Update.init


withMercLevel : Int -> Model -> Model
withMercLevel lvl model =
    case model.session of
        Nothing ->
            model

        Just s ->
            let
                merc =
                    s.merc

                stats =
                    merc.stats

                nextStats =
                    { stats | level = lvl }

                nextMerc =
                    { merc | stats = nextStats }
            in
            { model | session = Just { s | merc = nextMerc } }


withMercEditable : Bool -> Model -> Model
withMercEditable editable model =
    case model.session of
        Nothing ->
            model

        Just s ->
            let
                merc =
                    s.merc

                nextMerc =
                    { merc | persistenceSupported = editable }
            in
            { model | session = Just { s | merc = nextMerc } }


withUnsupportedSaveVersion : Int -> String -> Model -> Model
withUnsupportedSaveVersion ver reason model =
    case model.session of
        Nothing ->
            model

        Just s ->
            { model
                | session =
                    Just
                        { s
                            | saveVersion = ver
                            , exportSupported = False
                            , exportUnsupportedReason = Just reason
                        }
            }


withView : ViewTab -> Model -> Model
withView tab model =
    let
        ( nextModel, _ ) =
            Update.update (SwitchView tab) model
    in
    nextModel


testId : String -> Selector.Selector
testId id =
    Selector.attribute (Html.Attributes.attribute "data-testid" id)


suite : Test
suite =
    describe "Ortho Editor Cycle 1 Studio UX & Pure Elm Bridge Suite"
        [ test "decodes SessionEnvelope including Rust-projected quest/waypoint catalogs" <|
            \_ ->
                Expect.all
                    [ \_ ->
                        loadedModel.session
                            |> Maybe.map .charName
                            |> Expect.equal (Just "TESTDRUID")
                    , \_ ->
                        loadedModel.session
                            |> Maybe.map (.progression >> .questCatalog >> List.length)
                            |> Expect.equal (Just 3)
                    , \_ ->
                        loadedModel.session
                            |> Maybe.map (.progression >> .waypointCatalog >> List.length)
                            |> Expect.equal (Just 2)
                    ]
                    ()
        , test "View B Character Resistances obey >=75 gold rule and emit SelectResDifficulty" <|
            \_ ->
                Workspace.view loadedModel
                    |> Query.fromHtml
                    |> Expect.all
                        [ Query.find [ testId "res-card-fire" ]
                            >> Query.has [ Selector.classes [ "res-card", "is-gold-res" ] ]
                        , Query.find [ testId "res-card-cold" ]
                            >> Query.hasNot [ Selector.class "is-gold-res" ]
                        , Query.find [ testId "res-diff-hell" ]
                            >> Event.simulate Event.click
                            >> Event.expect (SelectResDifficulty Hell)
                        ]
        , test "Mercenary Card guards unpersisted controls as read-only and honors 1..98 bounds when enabled" <|
            \_ ->
                Expect.all
                    [ \_ ->
                        Workspace.view (withMercEditable False loadedModel)
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "merc-res-fire" ]
                                    >> Query.has [ Selector.classes [ "res-card", "merc-res-card", "is-gold-res" ] ]
                                , Query.find [ testId "merc-res-cold" ]
                                    >> Query.hasNot [ Selector.class "is-gold-res" ]
                                , Query.find [ testId "merc-auras" ]
                                    >> Query.has [ Selector.text "Paladin Aura (Act 2)", Selector.text "Aura #120 Lv.15", Selector.text "Insight" ]
                                , Query.find [ testId "merc-preview-badge" ]
                                    >> Query.has [ Selector.text "Read-only · Not saved to .d2s" ]
                                , Query.find [ testId "merc-preview-note" ]
                                    >> Query.has [ Selector.text "Read-only: mercenary changes are not saved to .d2s yet." ]
                                , Query.find [ testId "merc-act-5" ]
                                    >> Query.has [ Selector.disabled True ]
                                , Query.find [ testId "merc-lvl-plus5" ]
                                    >> Query.has [ Selector.disabled True ]
                                , Query.find [ testId "merc-level-input" ]
                                    >> Query.has [ Selector.disabled True ]
                                ]
                    , \_ ->
                        Workspace.view (withMercEditable True loadedModel)
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.findAll [ testId "merc-preview-badge" ]
                                    >> Query.count (Expect.equal 0)
                                , Query.find [ testId "merc-persisted-badge" ]
                                    >> Query.has [ Selector.text "Saved to .d2s" ]
                                , Query.find [ testId "merc-act-5" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (SelectMercAct 5)
                                , Query.find [ testId "merc-lvl-plus5" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (AdjustMercLevel 5)
                                ]
                    , \_ ->
                        Workspace.view (loadedModel |> withMercEditable True |> withMercLevel 1)
                            |> Query.fromHtml
                            |> Query.find [ testId "merc-lvl-minus" ]
                            >> Query.has [ Selector.disabled True ]
                    , \_ ->
                        Workspace.view (loadedModel |> withMercEditable True |> withMercLevel 98)
                            |> Query.fromHtml
                            |> Query.find [ testId "merc-lvl-plus" ]
                            >> Query.has [ Selector.disabled True ]
                    ]
                    ()
        , test "Progression PaintMode FSM switches click <-> green/red marquee and resets on re-click" <|
            \_ ->
                let
                    ( mAct, oAct ) =
                        Update.update (TogglePaintMode Activate) loadedModel

                    ( mDeact, _ ) =
                        Update.update (TogglePaintMode Deactivate) mAct

                    ( mCancel, _ ) =
                        Update.update (TogglePaintMode Deactivate) mDeact

                    ( _, clickPlain ) =
                        Update.update (ClickQuestTile Normal "a1q1") mCancel

                    ( _, clickPaint ) =
                        Update.update (ClickQuestTile Normal "a1q2") mAct
                in
                Expect.all
                    [ \_ -> Expect.equal (Just Activate) mAct.ui.paintMode
                    , \_ -> Expect.equal NoOut oAct
                    , \_ -> Expect.equal (Just Deactivate) mDeact.ui.paintMode
                    , \_ -> Expect.equal Nothing mCancel.ui.paintMode
                    , \_ -> Expect.equal (EmitIntent (Protocol.ToggleQuest { difficulty = Normal, key = "a1q1" })) clickPlain
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.SetQuestsBatch { difficulty = Normal, keys = [ "a1q2" ], completed = True }))
                            clickPaint
                    , \_ ->
                        Workspace.view mAct
                            |> Query.fromHtml
                            |> Query.find [ testId "progression-panel" ]
                            |> Query.has [ Selector.classes [ "is-marquee-mode", "mode-activate" ] ]
                    , \_ ->
                        Workspace.view mDeact
                            |> Query.fromHtml
                            |> Query.find [ testId "progression-panel" ]
                            |> Query.has [ Selector.classes [ "is-marquee-mode", "mode-deactivate" ] ]
                    ]
                    ()
        , test "Marquee drag selection emits SetWaypointsBatch on CommitMarquee and excludes All Clear button" <|
            \_ ->
                let
                    ( mPaint, _ ) =
                        Update.update (TogglePaintMode Deactivate) loadedModel

                    ( mDragStart, _ ) =
                        Update.update (BeginMarquee DragWaypoints 10 10 [ "0" ]) mPaint

                    ( mDragMove, _ ) =
                        Update.update (MoveMarquee 120 80 [ "0", "1" ]) mDragStart

                    ( mCommitted, outIntent ) =
                        Update.update (CommitMarquee Nightmare) mDragMove

                    ( _, outAllDiff ) =
                        Update.update ToggleAllDiffQuests loadedModel
                in
                Expect.all
                    [ \_ -> Expect.equal Nothing mCommitted.ui.marquee
                    , \_ -> Expect.equal (Just Deactivate) mCommitted.ui.paintMode
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.SetWaypointsBatch { difficulty = Nightmare, bits = [ 0, 1 ], active = False }))
                            outIntent
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.SetAllDiffQuests { completed = True }))
                            outAllDiff
                    , \_ ->
                        Workspace.view loadedModel
                            |> Query.fromHtml
                            |> Query.findAll [ Selector.text "All Clear" ]
                            |> Query.count (Expect.equal 0)
                    ]
                    ()
        , test "Marquee selection box renders fixed rect coordinates, hit classes, and multi-difficulty quest batch" <|
            \_ ->
                let
                    ( mActivate, _ ) =
                        Update.update (TogglePaintMode Activate) loadedModel

                    ( mQuestStart, _ ) =
                        Update.update (BeginMarquee DragQuests 120 90 []) mActivate

                    ( mQuestMove, _ ) =
                        Update.update (MoveMarquee 40 30 [ "normal:a1q1", "hell:a1q2" ]) mQuestStart

                    ( mQuestCommitted, questOut ) =
                        Update.update (CommitMarquee Normal) mQuestMove

                    ( mDeactivate, _ ) =
                        Update.update (TogglePaintMode Deactivate) loadedModel

                    ( mWpStart, _ ) =
                        Update.update (BeginMarquee DragWaypoints 10 20 []) mDeactivate

                    ( mWpMove, _ ) =
                        Update.update (MoveMarquee 60 80 [ "0", "1" ]) mWpStart
                in
                Expect.all
                    [ \_ ->
                        Workspace.view mQuestMove
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "drag-marquee-box" ]
                                    >> Query.has
                                        [ Selector.class "marquee-selection-box"
                                        , Selector.style "display" "block"
                                        , Selector.style "left" "40px"
                                        , Selector.style "top" "30px"
                                        , Selector.style "width" "80px"
                                        , Selector.style "height" "60px"
                                        ]
                                , Query.find [ testId "quest-tile-a1q1" ]
                                    >> Query.has [ Selector.class "marquee-hit-activate" ]
                                , Query.find [ testId "quest-tile-hell-a1q2" ]
                                    >> Query.has [ Selector.class "marquee-hit-activate" ]
                                ]
                    , \_ -> Expect.equal Nothing mQuestCommitted.ui.marquee
                    , \_ ->
                        Expect.equal
                            (EmitIntents
                                [ Protocol.SetQuestsBatch { difficulty = Normal, keys = [ "a1q1" ], completed = True }
                                , Protocol.SetQuestsBatch { difficulty = Hell, keys = [ "a1q2" ], completed = True }
                                ]
                            )
                            questOut
                    , \_ ->
                        Workspace.view mWpMove
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "drag-marquee-box" ]
                                    >> Query.has
                                        [ Selector.class "marquee-selection-box"
                                        , Selector.class "is-deactivate"
                                        , Selector.style "display" "block"
                                        , Selector.style "left" "10px"
                                        , Selector.style "top" "20px"
                                        , Selector.style "width" "50px"
                                        , Selector.style "height" "60px"
                                        ]
                                , Query.find [ testId "wp-tile-0" ]
                                    >> Query.has [ Selector.class "marquee-hit-deactivate" ]
                                , Query.find [ testId "wp-tile-1" ]
                                    >> Query.has [ Selector.class "marquee-hit-deactivate" ]
                                ]
                    ]
                    ()
        , test "View A Temp Inventory provides Clear only and no Move All to Stash button" <|
            \_ ->
                Workspace.view (withView ViewA loadedModel)
                    |> Query.fromHtml
                    |> Expect.all
                        [ Query.find [ testId "temp-inventory-panel" ]
                            >> Query.has [ Selector.text "El Rune", Selector.text "1 / 36 slots occupied" ]
                        , Query.find [ testId "clear-temp-btn" ]
                            >> Event.simulate Event.click
                            >> Event.expect ClearTempInventory
                        , Query.findAll [ Selector.text "Move All to Stash" ]
                            >> Query.count (Expect.equal 0)
                        ]
        , test "Encoded Cycle 1 intents match serde wire JSON" <|
            \_ ->
                Expect.all
                    [ \_ ->
                        uiIntentEncoder Protocol.ClearTempInventory
                            |> Json.Encode.encode 0
                            |> Expect.equal "\"ClearTempInventory\""
                    , \_ ->
                        uiIntentEncoder (Protocol.SelectMercAct { actIndex = 5 })
                            |> Json.Encode.encode 0
                            |> Expect.equal "{\"SelectMercAct\":{\"act_index\":5}}"
                    , \_ ->
                        uiIntentEncoder (Protocol.SetAllDiffQuests { completed = True })
                            |> Json.Encode.encode 0
                            |> Expect.equal "{\"SetAllDiffQuests\":{\"completed\":true}}"
                    ]
                    ()
        , test "Workspace shows Unsaved badge only when session.dirty is True" <|
            \_ ->
                let
                    dirtyModel =
                        case loadedModel.session of
                            Just s ->
                                { loadedModel | session = Just { s | dirty = True } }

                            Nothing ->
                                loadedModel
                in
                Expect.all
                    [ \_ ->
                        Workspace.view (withMercEditable False loadedModel)
                            |> Query.fromHtml
                            |> Query.findAll [ testId "unsaved-badge" ]
                            |> Query.count (Expect.equal 0)
                    , \_ ->
                        Workspace.view dirtyModel
                            |> Query.fromHtml
                            |> Query.find [ testId "unsaved-badge" ]
                            |> Query.has [ Selector.text "Unsaved changes" ]
                    ]
                    ()
        , test "Workspace surfaces v105 export readiness badge and non-v105 warning notice" <|
            \_ ->
                let
                    unsupportedModel =
                        withUnsupportedSaveVersion 96 "Binary .d2s export supports v105 saves only (loaded v96)." loadedModel
                in
                Expect.all
                    [ \_ ->
                        Workspace.view (withMercEditable False loadedModel)
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "export-support-badge" ]
                                    >> Query.has [ Selector.text "v105 · .d2s Export Ready" ]
                                , Query.findAll [ testId "export-unsupported-notice" ]
                                    >> Query.count (Expect.equal 0)
                                ]
                    , \_ ->
                        Workspace.view unsupportedModel
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "export-support-badge" ]
                                    >> Query.has [ Selector.text "Read-only v96 · Export requires v105" ]
                                , Query.find [ testId "export-unsupported-notice" ]
                                    >> Query.has [ Selector.text "Binary .d2s export supports v105 saves only (loaded v96)." ]
                                ]
                    ]
                    ()
        , test "View B focusedInput isolates keystroke drafts as NoOut and commits typed UiIntent" <|
            \_ ->
                let
                    ( focusedModel, focusOut ) =
                        Update.update (FocusField "level" "31") loadedModel

                    ( editingModel, editOut ) =
                        Update.update (EditFocusedInput "50") focusedModel

                    ( cancelledModel, cancelOut ) =
                        Update.update CancelFocusedInput editingModel

                    ( committedModel, commitOut ) =
                        Update.update CommitFocusedInput editingModel

                    ( _, nameCommitOut ) =
                        Update.init
                            |> Update.update (FocusField "char_name" "TESTDRUID")
                            |> Tuple.first
                            |> Update.update (EditFocusedInput "OrthoDruid")
                            |> Tuple.first
                            |> Update.update CommitFocusedInput

                    ( invalidModel, invalidOut ) =
                        Update.update (FocusField "gold_stash" "150000") loadedModel
                            |> Tuple.first
                            |> Update.update (EditFocusedInput "not-a-number")
                            |> Tuple.first
                            |> Update.update CommitFocusedInput
                in
                Expect.all
                    [ \_ -> Expect.equal NoOut focusOut
                    , \_ -> Expect.equal NoOut editOut
                    , \_ -> Expect.equal (Just { key = "level", raw = "50" }) editingModel.ui.focusedInput
                    , \_ -> Expect.equal NoOut cancelOut
                    , \_ -> Expect.equal Nothing cancelledModel.ui.focusedInput
                    , \_ -> Expect.equal Nothing committedModel.ui.focusedInput
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.SetCharacterField { field = Protocol.Level, value = 50 }))
                            commitOut
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.SetCharacterName { name = "OrthoDruid" }))
                            nameCommitOut
                    , \_ -> Expect.equal NoOut invalidOut
                    , \_ ->
                        invalidModel.ui.statusBanner
                            |> Maybe.map (String.contains "Invalid integer input")
                            |> Expect.equal (Just True)
                    , \_ ->
                        Workspace.view loadedModel
                            |> Query.fromHtml
                            |> Query.find [ testId "input-char-level" ]
                            |> Event.simulate (Event.input "65")
                            |> Event.expect (EditFocusedInput "65")
                    ]
                    ()
        , test "ProgressionPanel wires real DOM click and mousedown->mouseenter->mouseup pointer drag parity" <|
            \_ ->
                let
                    ( progModel, _ ) =
                        Update.update (SwitchView ViewB) loadedModel

                    ( paintModel, _ ) =
                        Update.update (TogglePaintMode Activate) progModel

                    questDownMsg =
                        BeginMarquee DragQuests 0 0 [ "a1q1" ]

                    questEnterMsg =
                        ExtendMarquee DragQuests "a1q2"

                    questUpMsg =
                        CommitMarquee Normal

                    ( draggingQuest1, _ ) =
                        Update.update questDownMsg paintModel

                    ( draggingQuest2, _ ) =
                        Update.update questEnterMsg draggingQuest1

                    ( committedQuestModel, committedQuestOut ) =
                        Update.update questUpMsg draggingQuest2

                    wpDownMsg =
                        BeginMarquee DragWaypoints 0 0 [ "0" ]

                    wpEnterMsg =
                        ExtendMarquee DragWaypoints "1"

                    wpUpMsg =
                        CommitMarquee Normal

                    ( draggingWp1, _ ) =
                        Update.update wpDownMsg paintModel

                    ( draggingWp2, _ ) =
                        Update.update wpEnterMsg draggingWp1

                    ( committedWpModel, committedWpOut ) =
                        Update.update wpUpMsg draggingWp2
                in
                Expect.all
                    [ \_ ->
                        Workspace.view progModel
                            |> Query.fromHtml
                            |> Query.find [ testId "quest-tile-a1q1" ]
                            |> Event.simulate Event.click
                            |> Event.expect (ClickQuestTile Normal "a1q1")
                    , \_ ->
                        Workspace.view progModel
                            |> Query.fromHtml
                            |> Query.find [ testId "wp-tile-0" ]
                            |> Event.simulate Event.click
                            |> Event.expect (ClickWaypointTile Normal 0)
                    , \_ ->
                        Workspace.view paintModel
                            |> Query.fromHtml
                            |> Query.find [ testId "quest-tile-a1q1" ]
                            |> Event.simulate (Event.custom "mousedown" (Json.Encode.object []))
                            |> Event.expect questDownMsg
                    , \_ ->
                        Workspace.view draggingQuest1
                            |> Query.fromHtml
                            |> Query.find [ testId "quest-tile-a1q2" ]
                            |> Event.simulate (Event.custom "mouseenter" (Json.Encode.object []))
                            |> Event.expect questEnterMsg
                    , \_ ->
                        Workspace.view draggingQuest2
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "quest-tile-a1q1" ]
                                    >> Query.has [ Selector.class "is-marquee-target" ]
                                , Query.find [ testId "quest-tile-a1q2" ]
                                    >> Query.has [ Selector.class "is-marquee-target" ]
                                , Query.find [ testId "quest-tile-a1q2" ]
                                    >> Event.simulate (Event.custom "mouseup" (Json.Encode.object []))
                                    >> Event.expect questUpMsg
                                , Query.find [ testId "progression-panel" ]
                                    >> Event.simulate (Event.custom "mouseup" (Json.Encode.object []))
                                    >> Event.expect questUpMsg
                                , Query.find [ testId "progression-panel" ]
                                    >> Event.simulate (Event.custom "mouseleave" (Json.Encode.object []))
                                    >> Event.expect CancelMarquee
                                , Query.find [ testId "cancel-marquee-btn" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect CancelMarquee
                                ]
                    , \_ -> Expect.equal Nothing committedQuestModel.ui.marquee
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.SetQuestsBatch { difficulty = Normal, keys = [ "a1q1", "a1q2" ], completed = True }))
                            committedQuestOut
                    , \_ ->
                        Workspace.view paintModel
                            |> Query.fromHtml
                            |> Query.find [ testId "wp-tile-0" ]
                            |> Event.simulate (Event.custom "mousedown" (Json.Encode.object []))
                            |> Event.expect wpDownMsg
                    , \_ ->
                        Workspace.view draggingWp1
                            |> Query.fromHtml
                            |> Query.find [ testId "wp-tile-1" ]
                            |> Event.simulate (Event.custom "mouseenter" (Json.Encode.object []))
                            |> Event.expect wpEnterMsg
                    , \_ ->
                        Workspace.view draggingWp2
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "wp-tile-0" ]
                                    >> Query.has [ Selector.class "is-marquee-target" ]
                                , Query.find [ testId "wp-tile-1" ]
                                    >> Query.has [ Selector.class "is-marquee-target" ]
                                , Query.find [ testId "wp-tile-1" ]
                                    >> Event.simulate (Event.custom "mouseup" (Json.Encode.object []))
                                    >> Event.expect wpUpMsg
                                ]
                    , \_ -> Expect.equal Nothing committedWpModel.ui.marquee
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.SetWaypointsBatch { difficulty = Normal, bits = [ 0, 1 ], active = True }))
                            committedWpOut
                    ]
                    ()
        , test "Exhaustive DOM parity across all 9 CharacterPanel inputs, controls, and ContainersPanel" <|
            \_ ->
                let
                    charInputs =
                        [ { tid = "input-char-name", fieldKey = "char_name", rawVal = "TESTDRUID", nextRaw = "OrthoDruid", expectedOut = EmitIntent (Protocol.SetCharacterName { name = "OrthoDruid" }) }
                        , { tid = "input-char-level", fieldKey = "level", rawVal = "31", nextRaw = "65", expectedOut = EmitIntent (Protocol.SetCharacterField { field = Protocol.Level, value = 65 }) }
                        , { tid = "input-stat-strength", fieldKey = "strength", rawVal = "60", nextRaw = "95", expectedOut = EmitIntent (Protocol.SetCharacterField { field = Protocol.Strength, value = 95 }) }
                        , { tid = "input-stat-dexterity", fieldKey = "dexterity", rawVal = "20", nextRaw = "45", expectedOut = EmitIntent (Protocol.SetCharacterField { field = Protocol.Dexterity, value = 45 }) }
                        , { tid = "input-stat-vitality", fieldKey = "vitality", rawVal = "60", nextRaw = "85", expectedOut = EmitIntent (Protocol.SetCharacterField { field = Protocol.Vitality, value = 85 }) }
                        , { tid = "input-stat-energy", fieldKey = "energy", rawVal = "20", nextRaw = "35", expectedOut = EmitIntent (Protocol.SetCharacterField { field = Protocol.Energy, value = 35 }) }
                        , { tid = "input-stat-points", fieldKey = "stat_points", rawVal = "75", nextRaw = "10", expectedOut = EmitIntent (Protocol.SetCharacterField { field = Protocol.StatPoints, value = 10 }) }
                        ]

                    freeStatModel =
                        { loadedModel
                            | session =
                                Maybe.map (\s -> { s | statMode = Protocol.Free }) loadedModel.session
                        }

                    checkCharInput { tid, fieldKey, rawVal, nextRaw, expectedOut } _ =
                        let
                            ( focused, _ ) =
                                Update.update (FocusField fieldKey rawVal) freeStatModel

                            ( edited, _ ) =
                                Update.update (EditFocusedInput nextRaw) focused

                            ( _, commitOut ) =
                                Update.update CommitFocusedInput edited

                            ( cancelled, _ ) =
                                Update.update CancelFocusedInput edited
                        in
                        Expect.all
                            [ \_ ->
                                Workspace.view freeStatModel
                                    |> Query.fromHtml
                                    |> Query.find [ testId tid ]
                                    |> Event.simulate Event.focus
                                    |> Event.expect (FocusField fieldKey rawVal)
                            , \_ ->
                                Workspace.view focused
                                    |> Query.fromHtml
                                    |> Query.find [ testId tid ]
                                    |> Event.simulate (Event.input nextRaw)
                                    |> Event.expect (EditFocusedInput nextRaw)
                            , \_ ->
                                Workspace.view edited
                                    |> Query.fromHtml
                                    |> Query.find [ testId tid ]
                                    |> Event.simulate (Event.custom "keydown" (Json.Encode.object [ ( "key", Json.Encode.string "Enter" ) ]))
                                    |> Event.expect CommitFocusedInput
                            , \_ ->
                                Workspace.view edited
                                    |> Query.fromHtml
                                    |> Query.find [ testId tid ]
                                    |> Event.simulate Event.blur
                                    |> Event.expect CommitFocusedInput
                            , \_ ->
                                Workspace.view edited
                                    |> Query.fromHtml
                                    |> Query.find [ testId tid ]
                                    |> Event.simulate (Event.custom "keydown" (Json.Encode.object [ ( "key", Json.Encode.string "Escape" ) ]))
                                    |> Event.expect CancelFocusedInput
                            , \_ -> Expect.equal expectedOut commitOut
                            , \_ -> Expect.equal Nothing cancelled.ui.focusedInput
                            ]
                            ()

                    ( viewAModel, _ ) =
                        Update.update (SwitchView ViewA) loadedModel
                in
                Expect.all
                    (List.map checkCharInput charInputs
                        ++ [ \_ ->
                                Workspace.view loadedModel
                                    |> Query.fromHtml
                                    |> Query.find [ testId "res-diff-nightmare" ]
                                    |> Event.simulate Event.click
                                    |> Event.expect (SelectResDifficulty Nightmare)
                           , \_ ->
                                Workspace.view loadedModel
                                    |> Query.fromHtml
                                    |> Query.find [ testId "res-diff-hell" ]
                                    |> Event.simulate Event.click
                                    |> Event.expect (SelectResDifficulty Hell)
                           , \_ ->
                                Workspace.view loadedModel
                                    |> Query.fromHtml
                                    |> Query.find [ testId "skill-mode-free" ]
                                    |> Event.simulate Event.click
                                    |> Event.expect (SelectSkillMode Free)
                           , \_ ->
                                Workspace.view viewAModel
                                    |> Query.fromHtml
                                    |> Query.find [ testId "container-filter-equipment" ]
                                    |> Event.simulate Event.click
                                    |> Event.expect (SelectContainerFilter (Just Equipment))
                           , \_ ->
                                Workspace.view viewAModel
                                    |> Query.fromHtml
                                    |> Query.find [ testId "clear-temp-btn" ]
                                    |> Event.simulate Event.click
                                    |> Event.expect ClearTempInventory
                           , \_ ->
                                Workspace.view viewAModel
                                    |> Query.fromHtml
                                    |> Query.find [ testId "theme-toggle-btn" ]
                                    |> Event.simulate Event.click
                                    |> Event.expect ToggleTheme
                           ]
                    )
                    ()
        , test "Cycle 9 Top-Down Header theme toggle and View A 3-Zone Bento honor preview locks" <|
            \_ ->
                let
                    ( lightViewAModel, _ ) =
                        loadedModel
                            |> Update.update (SwitchView ViewA)
                            |> Tuple.first
                            |> Update.update ToggleTheme
                            |> Tuple.first
                            |> Update.update (SelectItem (Just 0))
                in
                Workspace.view lightViewAModel
                    |> Query.fromHtml
                    |> Expect.all
                        [ Query.has [ Selector.class "ortho-shell", Selector.class "theme-light" ]
                        , Query.find [ testId "undo-btn" ]
                            >> Query.has [ Selector.attribute (Html.Attributes.disabled True) ]
                        , Query.find [ testId "weapon-swap-preview-btn" ]
                            >> Query.has [ Selector.text "Swap I / II", Selector.text "Set I" ]
                        , Query.find [ testId "item-shop-preview" ]
                            >> Query.has [ Selector.text "Live Spawn" ]
                        , Query.find [ testId "paperdoll-panel" ]
                            >> Query.has [ Selector.text "Equipped Gear (Zone 2)" ]
                        , Query.find [ testId "gear-breakpoint-bar" ]
                            >> Query.has [ Selector.text "Equip" ]
                        , Query.find [ testId "view-a-merc-panel" ]
                            >> Query.has [ Selector.text "Mercenary Station (Zone 4)" ]
                        , Query.find [ testId "temp-inventory-panel" ]
                            >> Query.has [ Selector.text "El Rune" ]
                        , Query.find [ testId "selected-item-inspector" ]
                            >> Query.has [ Selector.text "Nagelring", Selector.text "Better Chance" ]
                        ]
        , test "Cycle 10 View B 4-Col .spec-grid renders vitals gauges, skill tree, 3-diff compact quest dots, 5-act waypoints, and combat lab accordion" <|
            \_ ->
                let
                    ( labOpened, _ ) =
                        loadedModel
                            |> Update.update (SelectSkillTreeTab "tree1")
                            |> Tuple.first
                            |> Update.update ToggleCombatLab
                in
                Expect.all
                    [ \_ ->
                        Workspace.view labOpened
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "view-b-workspace" ]
                                    >> Query.has [ Selector.class "spec-grid" ]
                                , Query.find [ testId "vitals-gauges" ]
                                    >> Query.has [ Selector.text "Life", Selector.text "Mana", Selector.text "Stamina" ]
                                , Query.find [ testId "skill-tree-preview" ]
                                    >> Query.has [ Selector.text "Reset Skills", Selector.text "Werewolf" ]
                                , Query.find [ testId "skill-tree-tab-tree1" ]
                                    >> Query.has [ Selector.class "active" ]
                                , Query.find [ testId "quests-panel" ]
                                    >> Query.has [ Selector.text "Normal", Selector.text "Nightmare", Selector.text "Hell" ]
                                , Query.find [ testId "quest-tile-hell-a1q1" ]
                                    >> Query.has [ Selector.class "quest-cell-dot" ]
                                , Query.find [ testId "waypoint-matrix" ]
                                    >> Query.has [ Selector.text "Act 1", Selector.text "Act 5" ]
                                , Query.find [ testId "combat-lab-content" ]
                                    >> Query.has [ Selector.text "Combat Sheet Projection" ]
                                ]
                    , \_ ->
                        Workspace.view loadedModel
                            |> Query.fromHtml
                            |> Query.find [ testId "combat-lab-toggle" ]
                            |> Event.simulate Event.click
                            |> Event.expect ToggleCombatLab
                    , \_ ->
                        Workspace.view loadedModel
                            |> Query.fromHtml
                            |> Query.find [ testId "skill-tree-tab-tree2" ]
                            |> Event.simulate Event.click
                            |> Event.expect (SelectSkillTreeTab "tree2")
                    , \_ ->
                        Workspace.view loadedModel
                            |> Query.fromHtml
                            |> Query.find [ testId "quest-tile-nightmare-a1q1" ]
                            |> Event.simulate Event.click
                            |> Event.expect (ClickQuestTile Nightmare "a1q1")
                    ]
                    ()
        , test "Cycle 11 View A renders item images, bottom-up belt coordinates (y=0 -> top 122px), and hover item tooltip" <|
            \_ ->
                let
                    beltPotion =
                        { index = 900
                        , code = "hp1"
                        , nameEn = "Lesser Healing Potion"
                        , nameKo = "하급 치유 물약"
                        , quality = QualityNormal
                        , container = Belt
                        , slotLabel = Nothing
                        , x = 0
                        , y = 0
                        , width = 1
                        , height = 1
                        , itemLevel = 12
                        , baseAttributes = [ "Defense: 185", "Chance to Block: 42%", "Durability: 20 of 20" ]
                        , properties = [ "Restores 45 Life" ]
                        , setName = Just "Sigon's Complete Steel"
                        , setPieces = [ "Sigon's Guard" ]
                        , pieceSetBonuses = [ { active = True, requiredCount = 2, lines = [ "+20% Increased Chance of Blocking" ] } ]
                        , setBonuses = [ { active = False, requiredCount = 2, lines = [ "10% Life stolen per hit" ] } ]
                        , socketCount = 1
                        , socketedNames = [ "Perfect Diamond" ]
                        , socketedItems = []
                        , imageUrl = Just "/item-vectors/misc/potion/lesser_healing_potion.svg"
                        , isBeltConsumable = True
                        , detailedProperties = []
                        , defense = Nothing
                        , currentDurability = Nothing
                        , maxDurability = Nothing
                        , quantity = Nothing
                        , isEthereal = False
                        }

                    viewAModel =
                        let
                            base =
                                loadedModel
                                    |> Update.update (SwitchView ViewA)
                                    |> Tuple.first
                        in
                        { base
                            | session =
                                Maybe.map (\s -> { s | items = beltPotion :: s.items }) base.session
                        }

                    hoveredModel =
                        viewAModel
                            |> Update.update (HoverItem (Just 900))
                            |> Tuple.first
                            |> Update.update (MoveTooltipHover 320 240 1440 900)
                            |> Tuple.first
                in
                Expect.all
                    [ \_ ->
                        Workspace.view viewAModel
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "grid-item-900" ]
                                    >> Query.has
                                        [ Selector.style "top" "122px"
                                        , Selector.tag "img"
                                        , Selector.attribute (Html.Attributes.src "/item-vectors/misc/potion/lesser_healing_potion.svg")
                                        ]
                                , Query.find [ testId "grid-item-900" ]
                                    >> Event.simulate Event.mouseEnter
                                    >> Event.expect (HoverItem (Just 900))
                                , Query.find [ testId "grid-item-900" ]
                                    >> Event.simulate Event.mouseLeave
                                    >> Event.expect (HoverItem Nothing)
                                ]
                    , \_ ->
                        Workspace.view hoveredModel
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "view-a-containers" ]
                                    >> Event.simulate
                                        (Event.custom "mousemove"
                                            (Json.Encode.object
                                                [ ( "clientX", Json.Encode.float 320 )
                                                , ( "clientY", Json.Encode.float 240 )
                                                ]
                                            )
                                        )
                                    >> Event.expect (MoveTooltipHover 320 240 1440 900)
                                , Query.find [ testId "item-hover-tooltip" ]
                                    >> Query.has
                                        [ Selector.style "left" "336px"
                                        , Selector.style "top" "256px"
                                        , Selector.text "Lesser Healing Potion"
                                        , Selector.text "Defense: 185"
                                        , Selector.text "Chance to Block: 42%"
                                        , Selector.text "Durability: 20 of 20"
                                        , Selector.text "Restores 45 Life"
                                        , Selector.text "Sigon's Complete Steel"
                                        , Selector.text "2 ITEMS"
                                        , Selector.text "Active"
                                        , Selector.text "+20% Increased Chance of Blocking"
                                        , Selector.text "2 SET"
                                        , Selector.text "Inactive"
                                        , Selector.text "10% Life stolen per hit"
                                        , Selector.text "Sigon's Guard"
                                        , Selector.text "Socketed: Perfect Diamond"
                                        ]
                                , \_ ->
                                    viewAModel
                                        |> Update.update (HoverItem (Just 900))
                                        |> Tuple.first
                                        |> Update.update (MoveTooltipHover 1380 860 1440 900)
                                        |> Tuple.first
                                        |> Workspace.view
                                        |> Query.fromHtml
                                        |> Query.find [ testId "item-hover-tooltip" ]
                                        |> Query.has
                                            [ Selector.style "right" "76px"
                                            , Selector.style "bottom" "54px"
                                            ]
                                ]
                    ]
                    ()
        , test "Cycle 12 View A renders Rust-projected allowedMoveTargets, belt restriction, MoveItem, and Undo/Redo" <|
            \_ ->
                let
                    nonConsumableModel =
                        let
                            base =
                                loadedModel
                                    |> Update.update (SwitchView ViewA)
                                    |> Tuple.first
                        in
                        { base
                            | session =
                                Maybe.map
                                    (\s ->
                                        { s
                                            | selectedMoveItemIndex = Just 0
                                            , allowedMoveTargets = [ { container = Stash, x = 5, y = 6 } ]
                                            , beltMoveRestrictedReason = Just "Only 1x1 belt consumables can be placed in the belt."
                                            , canUndo = True
                                            , canRedo = True
                                        }
                                    )
                                    base.session
                        }

                    potionModel =
                        let
                            base =
                                loadedModel
                                    |> Update.update (SwitchView ViewA)
                                    |> Tuple.first

                            potion =
                                { index = 901
                                , code = "hp1"
                                , nameEn = "Minor Healing Potion"
                                , nameKo = "하급 치유 물약"
                                , quality = QualityNormal
                                , container = Inventory
                                , slotLabel = Nothing
                                , x = 0
                                , y = 0
                                , width = 1
                                , height = 1
                                , itemLevel = 1
                                , baseAttributes = []
                                , properties = []
                                , setName = Nothing
                                , setPieces = []
                                , pieceSetBonuses = []
                                , setBonuses = []
                                , socketCount = 0
                                , socketedNames = []
                                , socketedItems = []
                                , imageUrl = Nothing
                                , isBeltConsumable = True
                                , detailedProperties = []
                                , defense = Nothing
                                , currentDurability = Nothing
                                , maxDurability = Nothing
                                , quantity = Nothing
                                , isEthereal = False
                                }
                        in
                        { base
                            | session =
                                Maybe.map
                                    (\s ->
                                        { s
                                            | items = potion :: s.items
                                            , selectedMoveItemIndex = Just 901
                                            , allowedMoveTargets = [ { container = Belt, x = 2, y = 0 } ]
                                            , beltMoveRestrictedReason = Nothing
                                        }
                                    )
                                    base.session
                        }
                in
                Expect.all
                    [ \_ ->
                        Workspace.view nonConsumableModel
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "belt-restricted-pill" ]
                                    >> Query.has [ Selector.text "Belt restricted (consumables only)" ]
                                , Query.find [ testId "spatial-grid-belt" ]
                                    >> Query.has [ Selector.class "is-restricted" ]
                                , Query.find [ testId "grid-cell-stash-5-6" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (MoveSelectedItemTo Stash 5 6)
                                , Query.find [ testId "grid-cell-belt-0-0" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (MoveSelectedItemTo Belt 0 0)
                                , Query.find [ testId "undo-btn" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect ClickUndo
                                , Query.find [ testId "redo-btn" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect ClickRedo
                                ]
                    , \_ ->
                        nonConsumableModel
                            |> Update.update (MoveTooltipHover 320 240 1440 900)
                            |> Tuple.first
                            |> Workspace.view
                            |> Query.fromHtml
                            |> Query.find [ testId "held-item-ghost" ]
                            |> Query.has
                                [ Selector.style "left" "320px"
                                , Selector.style "top" "240px"
                                ]
                    , \_ ->
                        nonConsumableModel
                            |> Update.update (HoverDropTarget (Just { container = Stash, x = 5, y = 6 }))
                            |> Tuple.first
                            |> Workspace.view
                            |> Query.fromHtml
                            |> Query.find [ testId "grid-footprint-preview-stash" ]
                            |> Query.has [ Selector.class "valid" ]
                    , \_ ->
                        nonConsumableModel
                            |> Update.update (HoverDropTarget (Just { container = Belt, x = 0, y = 0 }))
                            |> Tuple.first
                            |> Workspace.view
                            |> Query.fromHtml
                            |> Query.find [ testId "grid-footprint-preview-belt" ]
                            |> Query.has [ Selector.class "invalid" ]
                    , \_ ->
                        Update.update (SelectItem (Just 0)) nonConsumableModel
                            |> Tuple.second
                            |> Expect.equal (EmitIntent (Protocol.SelectItemForMove { itemIndex = Just 0 }))
                    , \_ ->
                        Update.update (MoveSelectedItemTo Stash 5 6) nonConsumableModel
                            |> Tuple.second
                            |> Expect.equal
                                (EmitIntent
                                    (Protocol.MoveItem
                                        { itemIndex = 0
                                        , targetContainer = Stash
                                        , x = 5
                                        , y = 6
                                        }
                                    )
                                )
                    , \_ ->
                        Update.update ClickUndo nonConsumableModel
                            |> Tuple.second
                            |> Expect.equal (EmitIntent Protocol.Undo)
                    , \_ ->
                        Update.update ClickRedo nonConsumableModel
                            |> Tuple.second
                            |> Expect.equal (EmitIntent Protocol.Redo)
                    , \_ ->
                        Workspace.view potionModel
                            |> Query.fromHtml
                            |> Query.find [ testId "grid-cell-belt-2-0" ]
                            >> Event.simulate Event.click
                            >> Event.expect (MoveSelectedItemTo Belt 2 0)
                    ]
                    ()
        , test "Cycle 13 View A & B render shop sell/dispose, weapon swap I/II, gear breakpoints, and interactive 30-skill tree" <|
            \_ ->
                let
                    holdingViewA =
                        let
                            base =
                                loadedModel
                                    |> Update.update (SwitchView ViewA)
                                    |> Tuple.first
                        in
                        { base
                            | session =
                                Maybe.map
                                    (\s ->
                                        { s
                                            | selectedMoveItemIndex = Just 0
                                            , allowedMoveTargets = [ { container = TempInventory, x = 2, y = 1 } ]
                                        }
                                    )
                                    base.session
                        }
                            |> Update.update (HoverDropTarget (Just { container = Shop, x = 0, y = 0 }))
                            |> Tuple.first
                in
                Expect.all
                    [ \_ ->
                        Workspace.view holdingViewA
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "shop-dispose-grid" ]
                                    >> Query.has [ Selector.class "shop-sell-drop-active" ]
                                , Query.find [ testId "shop-dispose-grid" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect ClickDisposeHeldItem
                                , Query.find [ testId "weapon-swap-preview-btn" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect ClickToggleWeaponSwap
                                , Query.find [ testId "bp-fcr" ]
                                    >> Query.has [ Selector.text "20%" ]
                                , Query.find [ testId "bp-mf" ]
                                    >> Query.has [ Selector.text "55%" ]
                                , Query.find [ testId "bp-skills" ]
                                    >> Query.has [ Selector.text "+2" ]
                                ]
                    , \_ ->
                        Workspace.view loadedModel
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "skill-node-221" ]
                                    >> Query.has [ Selector.text "Raven", Selector.text "5/20 (+2)" ]
                                , Query.find [ testId "skill-plus-221" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (ClickAdjustSkillRank 221 1)
                                , Query.find [ testId "skill-minus-221" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (ClickAdjustSkillRank 221 -1)
                                , Query.find [ testId "reset-skills-btn" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect ClickResetSkills
                                ]
                    ]
                    ()
        , test "Equipment item can be picked up and placed back onto the same or another equipment slot" <|
            \_ ->
                let
                    ringItem =
                        { index = 0
                        , code = "rin"
                        , nameEn = "Nagelring"
                        , nameKo = "나겔링"
                        , quality = QualityUnique
                        , container = Equipment
                        , slotLabel = Just "Ring1"
                        , x = 0
                        , y = 0
                        , width = 1
                        , height = 1
                        , itemLevel = 10
                        , baseAttributes = []
                        , properties = []
                        , setName = Nothing
                        , setPieces = []
                        , pieceSetBonuses = []
                        , setBonuses = []
                        , socketCount = 0
                        , socketedNames = []
                        , socketedItems = []
                        , imageUrl = Nothing
                        , isBeltConsumable = False
                        , detailedProperties = []
                        , defense = Nothing
                        , currentDurability = Nothing
                        , maxDurability = Nothing
                        , quantity = Nothing
                        , isEthereal = False
                        }

                    baseViewA =
                        loadedModel
                            |> Update.update (SwitchView ViewA)
                            |> Tuple.first

                    withEquip =
                        { baseViewA
                            | session =
                                Maybe.map
                                    (\s ->
                                        { s
                                            | items = [ ringItem ]
                                            , selectedMoveItemIndex = Nothing
                                            , allowedMoveTargets = []
                                        }
                                    )
                                    baseViewA.session
                        }

                    holdingEquip =
                        { withEquip
                            | session =
                                Maybe.map
                                    (\s ->
                                        { s
                                            | selectedMoveItemIndex = Just 0
                                            , allowedMoveTargets =
                                                [ { container = Equipment, x = 6, y = 0 }
                                                , { container = Equipment, x = 7, y = 0 }
                                                ]
                                        }
                                    )
                                    withEquip.session
                        }
                in
                Expect.all
                    [ \_ ->
                        Workspace.view withEquip
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "move-mode-bar" ]
                                    >> Query.has [ Selector.class "is-idle" ]
                                , Query.find [ testId "equip-slot-6" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (OpenActionPalette 0 240 180)
                                ]
                    , \_ ->
                        let
                            hoveredEquip =
                                holdingEquip
                                    |> Update.update (HoverDropTarget (Just { container = Equipment, x = 6, y = 0 }))
                                    |> Tuple.first
                        in
                        Expect.all
                            [ \_ ->
                                Workspace.view holdingEquip
                                    |> Query.fromHtml
                                    |> Expect.all
                                        [ Query.find [ testId "move-mode-bar" ]
                                            >> Query.hasNot [ Selector.class "is-idle" ]
                                        , Query.find [ testId "equip-slot-6" ]
                                            >> Query.has [ Selector.class "is-held-source" ]
                                        , Query.find [ testId "equip-slot-6" ]
                                            >> Query.hasNot [ Selector.class "footprint-valid" ]
                                        , Query.find [ testId "equip-slot-6" ]
                                            >> Event.simulate Event.click
                                            >> Event.expect (SelectItem Nothing)
                                        , Query.find [ testId "equip-slot-7" ]
                                            >> Event.simulate Event.click
                                            >> Event.expect (MoveSelectedItemTo Equipment 7 0)
                                        ]
                            , \_ ->
                                Workspace.view hoveredEquip
                                    |> Query.fromHtml
                                    |> Query.find [ testId "equip-slot-6" ]
                                    >> Query.has [ Selector.class "is-held-source", Selector.class "footprint-valid" ]
                            ]
                            ()
                    ]
                    ()
        , test "Cycle 14 Item Shop switches quality tabs and emits ClickSpawnShopItem on catalog tile click" <|
            \_ ->
                let
                    viewAModel =
                        loadedModel
                            |> Update.update (SwitchView ViewA)
                            |> Tuple.first

                    normalTabModel =
                        viewAModel
                            |> Update.update (SelectShopQualityTab QualityNormal)
                            |> Tuple.first
                in
                Expect.all
                    [ \_ ->
                        Workspace.view viewAModel
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "shop-item-uniq:345" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (ClickSpawnShopItem "uniq:345")
                                , Query.find [ testId "shop-tab-normal" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (SelectShopQualityTab QualityNormal)
                                ]
                    , \_ ->
                        Workspace.view normalTabModel
                            |> Query.fromHtml
                            |> Query.find [ testId "shop-item-base:r33" ]
                            >> Event.simulate Event.click
                            >> Event.expect (ClickSpawnShopItem "base:r33")
                    ]
                    ()
        , test "Cycle 15 revision guard ignores stale responses and preserves cached shopCatalog across intents" <|
            \_ ->
                let
                    decodeEnvelopeJson raw =
                        case Json.Decode.decodeString Json.Decode.value raw of
                            Ok val ->
                                val

                            Err _ ->
                                Json.Encode.null

                    newerEmptyCatalogJson =
                        sampleSessionJson
                            |> String.replace "\"revision\": 1," "\"revision\": 2,"
                            |> String.replace "\"char_level\": 31," "\"char_level\": 99,"
                            |> String.replace "\"catalog_id\": \"uniq:345\"," "\"__unused_marker\": \"x\","

                    newerMinimalJson =
                        sampleSessionJson
                            |> String.replace "\"revision\": 1," "\"revision\": 2,"
                            |> String.replace "\"dirty\": false," "\"dirty\": true,"
                            |> String.replace "\"char_level\": 31," "\"char_level\": 99,"
                            |> (\s ->
                                    case String.indexes "\"shop_catalog\": [" s of
                                        idx :: _ ->
                                            String.left idx s ++ "\"shop_catalog\": [], \"stat_catalog\": [], \"param_catalog\": { \"classes\": [], \"elements\": [], \"skill_tabs\": [], \"skills\": [], \"packed_skill\": { \"stride\": 64, \"min_level\": 1, \"max_level\": 63 } }, \"active_item_draft\": null }, \"rejection\": null }"

                                        _ ->
                                            s
                               )

                    staleJson =
                        sampleSessionJson
                            |> String.replace "\"revision\": 1," "\"revision\": 1,"
                            |> String.replace "\"dirty\": false," "\"dirty\": true,"
                            |> String.replace "\"char_level\": 31," "\"char_level\": 12,"

                    afterNewer =
                        loadedModel
                            |> Update.update (ReceiveSessionEnvelope (decodeEnvelopeJson newerMinimalJson))
                            |> Tuple.first

                    afterStale =
                        afterNewer
                            |> Update.update (ReceiveSessionEnvelope (decodeEnvelopeJson staleJson))
                            |> Tuple.first

                    _ =
                        newerEmptyCatalogJson
                in
                Expect.all
                    [ \_ ->
                        afterNewer.session
                            |> Maybe.map .charLevel
                            |> Expect.equal (Just 99)
                    , \_ ->
                        afterNewer.session
                            |> Maybe.map (.shopCatalog >> List.length)
                            |> Expect.equal (Just 2)
                    , \_ ->
                        afterStale.session
                            |> Maybe.map .charLevel
                            |> Expect.equal (Just 99)
                    ]
                    ()
        , test "Cycle 18 View A Zone 4 Mercenary Dual Stage emits SelectMercAct, item pickup, and Replaced Staging actions" <|
            \_ ->
                let
                    viewAModel =
                        loadedModel
                            |> Update.update (SwitchView State.UiState.ViewA)
                            |> Tuple.first

                    withDismissedModel =
                        { viewAModel
                            | session =
                                viewAModel.session
                                    |> Maybe.map
                                        (\s ->
                                            let
                                                oldMerc =
                                                    s.merc
                                            in
                                            { s
                                                | merc =
                                                    { oldMerc
                                                        | dismissed =
                                                            Just
                                                                { actIndex = 2
                                                                , className = "Desert Mercenary (Act 2)"
                                                                , subtypeName = "N/A"
                                                                , level = 28
                                                                , remainingItems = 0
                                                                }
                                                    }
                                            }
                                        )
                        }

                    withHoldingMercModel =
                        { viewAModel
                            | session =
                                viewAModel.session
                                    |> Maybe.map
                                        (\s ->
                                            let
                                                mercItem =
                                                    List.head s.mercItems
                                                        |> Maybe.map (\it -> { it | index = 50 })
                                            in
                                            { s
                                                | items = []
                                                , mercItems =
                                                    case mercItem of
                                                        Just it ->
                                                            [ it ]

                                                        Nothing ->
                                                            []
                                                , selectedMoveItemIndex = Just 50
                                                , allowedMoveTargets = [ { container = Stash, x = 0, y = 0 } ]
                                            }
                                        )
                        }
                in
                Expect.all
                    [ \_ ->
                        Workspace.view viewAModel
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "merc-act-select" ]
                                    >> Event.simulate (Event.input "1")
                                    >> Event.expect (SelectMercAct 1)
                                , Query.find [ testId "merc-slot-4" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (OpenActionPalette 0 240 180)
                                , Query.find [ testId "btn-restore-merc" ]
                                    >> Query.has [ Selector.disabled True ]
                                ]
                    , \_ ->
                        Workspace.view withDismissedModel
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "btn-restore-merc" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect RestoreReplacedMerc
                                , Query.find [ testId "btn-reclaim-merc" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (ClearReplacedMerc True)
                                , Query.find [ testId "btn-delete-merc" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (ClearReplacedMerc False)
                                ]
                    , \_ ->
                        Workspace.view withHoldingMercModel
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "grid-cell-stash-0-0" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (MoveSelectedItemTo Stash 0 0)
                                , Query.find [ testId "merc-slot-4" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (SelectItem Nothing)
                                ]
                    ]
                    ()
        , test "Cycle 19 Action Palette, Edit/Duplicate/Delete Context Menu, and Hover Keyboard Shortcuts" <|
            \_ ->
                let
                    viewAModel =
                        loadedModel
                            |> Update.update (SwitchView State.UiState.ViewA)
                            |> Tuple.first

                    withInvItem =
                        { viewAModel
                            | session =
                                viewAModel.session
                                    |> Maybe.map
                                        (\s ->
                                            let
                                                invItem =
                                                    { index = 0
                                                    , code = "cm1"
                                                    , nameEn = "Small Charm"
                                                    , nameKo = "작은 부적"
                                                    , quality = QualityMagic
                                                    , container = Inventory
                                                    , slotLabel = Nothing
                                                    , x = 0
                                                    , y = 0
                                                    , width = 1
                                                    , height = 1
                                                    , itemLevel = 10
                                                    , baseAttributes = []
                                                    , properties = []
                                                    , setName = Nothing
                                                    , setPieces = []
                                                    , pieceSetBonuses = []
                                                    , setBonuses = []
                                                    , socketCount = 0
                                                    , socketedNames = []
                                                    , socketedItems = []
                                                    , imageUrl = Nothing
                                                    , isBeltConsumable = False
                                                    , defense = Nothing
                                                    , currentDurability = Nothing
                                                    , maxDurability = Nothing
                                                    , quantity = Nothing
                                                    , isEthereal = False
                                                    , detailedProperties = []
                                                    }
                                            in
                                            { s | items = [ invItem ] }
                                        )
                        }

                    paletteOpened =
                        withInvItem
                            |> Update.update (OpenActionPalette 0 120 140)
                            |> Tuple.first

                    contextOpened =
                        withInvItem
                            |> Update.update (OpenContextMenu 0 160 180)
                            |> Tuple.first

                    hoveredModel =
                        withInvItem
                            |> Update.update (HoverItem (Just 0))
                            |> Tuple.first

                    ( _, spaceOut ) =
                        Update.update
                            (HandleKeyShortcut { key = " ", code = "Space", ctrlKey = False, altKey = False, metaKey = False, isInput = False })
                            hoveredModel

                    ( _, quickStashOut ) =
                        Update.update
                            (HandleKeyShortcut { key = "s", code = "KeyS", ctrlKey = False, altKey = False, metaKey = False, isInput = False })
                            hoveredModel
                in
                Expect.all
                    [ \_ ->
                        Workspace.view withInvItem
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "grid-item-0" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (OpenActionPalette 0 240 180)
                                , Query.find [ testId "grid-item-0" ]
                                    >> Event.simulate (Event.custom "contextmenu" (Json.Encode.object []))
                                    >> Event.expect (OpenContextMenu 0 240 180)
                                ]
                    , \_ ->
                        Workspace.view paletteOpened
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "item-action-palette" ]
                                    >> Query.has [ Selector.text "Quick Actions" ]
                                , Query.find [ testId "palette-action-pick" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (SelectItem (Just 0))
                                , Query.find [ testId "palette-action-stash" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (QuickMoveItem 0 Stash)
                                ]
                    , \_ ->
                        Workspace.view contextOpened
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "item-context-menu" ]
                                    >> Query.has [ Selector.text "Forge Item" ]
                                , Query.find [ testId "context-action-edit" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (ContextEditItem 0)
                                , Query.find [ testId "context-action-duplicate" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (ContextDuplicateItem 0)
                                , Query.find [ testId "context-action-delete" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (ContextDeleteItem 0)
                                ]
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.SelectItemForMove { itemIndex = Just 0 }))
                            spaceOut
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.QuickMoveItem { itemIndex = 0, targetContainer = Stash }))
                            quickStashOut
                    ]
                    ()
        , test "Cycle 20 3-Column Item Option Editor Modal opens, filters stat_catalog, emits Add/RemoveItemProperty, and closes on Esc" <|
            \_ ->
                let
                    viewAModel =
                        loadedModel
                            |> Update.update (SwitchView ViewA)
                            |> Tuple.first

                    modalOpened =
                        viewAModel
                            |> Update.update (ContextEditItem 0)
                            |> Tuple.first
                            |> Update.update (SetItemEditorSearch "faster")
                            |> Tuple.first

                    statSelected =
                        modalOpened
                            |> Update.update (SelectItemEditorInjectStat 105 20)
                            |> Tuple.first

                    ( closedByEsc, _ ) =
                        statSelected
                            |> Update.update
                                (HandleKeyShortcut
                                    { key = "Escape"
                                    , code = "Escape"
                                    , ctrlKey = False
                                    , altKey = False
                                    , metaKey = False
                                    , isInput = False
                                    }
                                )
                in
                Expect.all
                    [ \_ ->
                        Workspace.view statSelected
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "item-editor-modal" ]
                                    >> Query.has [ Selector.text "Item Forge Studio" ]
                                , Query.find [ testId "modal-catalog-stat-105" ]
                                    >> Query.has [ Selector.text "item_fastercastrate" ]
                                , Query.find [ testId "btn-inject-stat" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (AddItemProperty 0 105 20 0)
                                , Query.find [ testId "item-editor-close-btn" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect CloseItemEditorModal
                                ]
                    , \_ ->
                        Expect.equal Nothing closedByEsc.ui.itemEditorModalIndex
                    ]
                    ()
        , test "Space drop clamps multi-cell held item to green footprint anchor at grid edge" <|
            \_ ->
                let
                    holdingModel =
                        { loadedModel
                            | session =
                                loadedModel.session
                                    |> Maybe.map
                                        (\baseSession ->
                                            { baseSession
                                                | items =
                                                    List.map
                                                        (\it ->
                                                            if it.index == 0 then
                                                                { it | width = 2, height = 2 }

                                                            else
                                                                it
                                                        )
                                                        baseSession.items
                                                , selectedMoveItemIndex = Just 0
                                            }
                                        )
                        }
                            |> Update.update (HoverDropTarget (Just { container = Inventory, x = 9, y = 3 }))
                            |> Tuple.first

                    ( _, spaceDropOut ) =
                        holdingModel
                            |> Update.update
                                (HandleKeyShortcut
                                    { key = " "
                                    , code = "Space"
                                    , ctrlKey = False
                                    , altKey = False
                                    , metaKey = False
                                    , isInput = False
                                    }
                                )
                in
                Expect.equal
                    (EmitIntent (Protocol.MoveItem { itemIndex = 0, targetContainer = Inventory, x = 8, y = 2 }))
                    spaceDropOut
        , test "Multi-cell 2x2 pocket snaps to valid anchor when hovering middle, right, or bottom cells" <|
            \_ ->
                let
                    pocketModel =
                        { loadedModel
                            | session =
                                loadedModel.session
                                    |> Maybe.map
                                        (\baseSession ->
                                            { baseSession
                                                | items =
                                                    List.map
                                                        (\it ->
                                                            if it.index == 0 then
                                                                { it | width = 2, height = 2 }

                                                            else
                                                                it
                                                        )
                                                        baseSession.items
                                                , selectedMoveItemIndex = Just 0
                                                , allowedMoveTargets = [ { container = Inventory, x = 0, y = 0 } ]
                                            }
                                        )
                        }
                            |> Update.update (SwitchView ViewA)
                            |> Tuple.first

                    checkPocketCell ( hx, hy, sub ) _ =
                        let
                            hovered =
                                pocketModel
                                    |> Update.update (HoverDropTargetSub (Just { container = Inventory, x = hx, y = hy }) sub)
                                    |> Tuple.first

                            ( _, spaceOut ) =
                                hovered
                                    |> Update.update
                                        (HandleKeyShortcut
                                            { key = " "
                                            , code = "Space"
                                            , ctrlKey = False
                                            , altKey = False
                                            , metaKey = False
                                            , isInput = False
                                            }
                                        )
                        in
                        Expect.all
                            [ \_ ->
                                Workspace.view hovered
                                    |> Query.fromHtml
                                    |> Query.find [ testId "grid-footprint-preview-inventory" ]
                                    |> Query.has [ Selector.class "valid" ]
                            , \_ ->
                                Expect.equal
                                    (EmitIntent (Protocol.MoveItem { itemIndex = 0, targetContainer = Inventory, x = 0, y = 0 }))
                                    spaceOut
                            ]
                            ()
                in
                Expect.all
                    (List.map checkPocketCell
                        [ ( 0, 0, ( 1, 1 ) )
                        , ( 1, 0, ( 0, 1 ) )
                        , ( 0, 1, ( 1, 0 ) )
                        , ( 1, 1, ( 0, 0 ) )
                        ]
                    )
                    ()
        , test "Centered drop anchor aligns 2x3 held ghost item vertically around cursor" <|
            \_ ->
                let
                    openStashTargets =
                        List.concatMap
                            (\y -> List.map (\x -> { container = Stash, x = x, y = y }) (List.range 0 8))
                            (List.range 0 7)
                in
                Expect.equal
                    ( 4, 4 )
                    (State.UiState.resolveGridDropAnchor openStashTargets Stash 2 3 4 5)
        , test "Equipment and MercEquipment body slots keep exact (bodyLoc, 0) without centering y=-1" <|
            \_ ->
                Expect.all
                    [ \_ -> Expect.equal ( 5, 0 ) (State.UiState.resolveGridDropAnchor [] Equipment 2 3 5 0)
                    , \_ -> Expect.equal ( 3, 0 ) (State.UiState.resolveGridDropAnchor [] MercEquipment 2 3 3 0)
                    ]
                    ()
        , test "Space pickup initializes hoveredDropTarget so immediate second Space drops Equipment item back" <|
            \_ ->
                let
                    hoveredEquip =
                        { loadedModel
                            | session =
                                loadedModel.session
                                    |> Maybe.map
                                        (\baseSession ->
                                            { baseSession
                                                | items =
                                                    List.map
                                                        (\it ->
                                                            if it.index == 0 then
                                                                { it | container = Equipment, slotLabel = Just "Weapon2", width = 2, height = 3 }

                                                            else
                                                                it
                                                        )
                                                        baseSession.items
                                            }
                                        )
                        }
                            |> Update.update (HoverItem (Just 0))
                            |> Tuple.first

                    spaceKey =
                        HandleKeyShortcut
                            { key = " "
                            , code = "Space"
                            , ctrlKey = False
                            , altKey = False
                            , metaKey = False
                            , isInput = False
                            }

                    ( pickedModel, pickupOut ) =
                        Update.update spaceKey hoveredEquip

                    holdingModel =
                        { pickedModel
                            | session =
                                pickedModel.session
                                    |> Maybe.map (\s -> { s | selectedMoveItemIndex = Just 0 })
                        }

                    ( _, dropOut ) =
                        Update.update spaceKey holdingModel
                in
                Expect.all
                    [ \_ -> Expect.equal (EmitIntent (Protocol.SelectItemForMove { itemIndex = Just 0 })) pickupOut
                    , \_ -> Expect.equal (Just { container = Equipment, x = 5, y = 0 }) pickedModel.ui.hoveredDropTarget
                    , \_ -> Expect.equal (EmitIntent (Protocol.MoveItem { itemIndex = 0, targetContainer = Equipment, x = 5, y = 0 })) dropOut
                    ]
                    ()
        , test "Cycle 21: View A inline gold editors emit FocusField/EditFocusedInput/CommitFocusedInput and socketed viewer renders child gems/runes" <|
            \_ ->
                let
                    viewAModel =
                        loadedModel
                            |> Update.update (SwitchView ViewA)
                            |> Tuple.first
                            |> Update.update (SelectItem (Just 0))
                            |> Tuple.first

                    -- Promote merc_items[0] (Insight, 4 sockets) into session.items with index 99 for inspector socketed-viewer check
                    socketedViewAModel =
                        { viewAModel
                            | session =
                                viewAModel.session
                                    |> Maybe.map
                                        (\s ->
                                            let
                                                insight =
                                                    s.mercItems
                                                        |> List.head
                                                        |> Maybe.map (\it -> { it | index = 99 })
                                            in
                                            case insight of
                                                Just it ->
                                                    { s | items = s.items ++ [ it ] }

                                                Nothing ->
                                                    s
                                        )
                            , ui = State.UiState.setSelectedItem (Just 99) viewAModel.ui
                        }
                in
                Workspace.view socketedViewAModel
                    |> Query.fromHtml
                    |> Expect.all
                        [ Query.find [ testId "viewa-input-gold-stash" ]
                            >> Event.simulate (Event.input "500000")
                            >> Event.expect (EditFocusedInput "500000")
                        , Query.find [ testId "viewa-input-gold-inventory" ]
                            >> Event.simulate (Event.input "12345")
                            >> Event.expect (EditFocusedInput "12345")
                        , Query.find [ testId "socketed-items-viewer" ]
                            >> Query.has
                                [ Selector.text "Socketed Gems / Runes"
                                , Selector.text "4 / 4"
                                , Selector.text "Ral Rune"
                                , Selector.text "+30% Fire Resist"
                                , Selector.text "Sol Rune"
                                ]
                        ]
        , test "F key shortcut opens item editor modal for hovered or selected item" <|
            \_ ->
                let
                    hoveredModel =
                        loadedModel
                            |> Update.update (HoverItem (Just 0))
                            |> Tuple.first

                    ( afterHoverF, _ ) =
                        hoveredModel
                            |> Update.update
                                (HandleKeyShortcut
                                    { key = "f"
                                    , code = "KeyF"
                                    , ctrlKey = False
                                    , altKey = False
                                    , metaKey = False
                                    , isInput = False
                                    }
                                )

                    selectedModel =
                        loadedModel
                            |> Update.update (SelectItem (Just 0))
                            |> Tuple.first

                    ( afterSelectF, _ ) =
                        selectedModel
                            |> Update.update
                                (HandleKeyShortcut
                                    { key = "f"
                                    , code = "KeyF"
                                    , ctrlKey = False
                                    , altKey = False
                                    , metaKey = False
                                    , isInput = False
                                    }
                                )

                    ( afterEsc, _ ) =
                        afterSelectF
                            |> Update.update
                                (HandleKeyShortcut
                                    { key = "Escape"
                                    , code = "Escape"
                                    , ctrlKey = False
                                    , altKey = False
                                    , metaKey = False
                                    , isInput = False
                                    }
                                )
                in
                Expect.all
                    [ \_ -> Expect.equal (Just 0) afterHoverF.ui.itemEditorModalIndex
                    , \_ -> Expect.equal (Just 0) afterSelectF.ui.itemEditorModalIndex
                    , \_ -> Expect.equal Nothing afterEsc.ui.itemEditorModalIndex
                    ]
                    ()
        , test "Direct integer input commits item_def and item_prop delta while rejecting invalid input" <|
            \_ ->
                let
                    propModel =
                        { loadedModel
                            | session =
                                loadedModel.session
                                    |> Maybe.map
                                        (\s ->
                                            { s
                                                | items =
                                                    List.map
                                                        (\it ->
                                                            if it.index == 0 then
                                                                { it
                                                                    | detailedProperties =
                                                                        [ { statId = 0
                                                                          , name = "Strength"
                                                                          , value = 10
                                                                          , minValue = 0
                                                                          , maxValue = 50
                                                                          , param = 0
                                                                          , paramBits = 0
                                                                          , valueLabel = "Value"
                                                                          , paramLabel = "Param"
                                                                          , paramKind = Protocol.ParamNone
                                                                          , paramPreview = Nothing
                                                                          }
                                                                        ]
                                                                }

                                                            else
                                                                it
                                                        )
                                                        s.items
                                            }
                                        )
                        }

                    ( defModel, defOut ) =
                        propModel
                            |> Update.update (FocusField "item_def:0" "10")
                            |> Tuple.first
                            |> Update.update (EditFocusedInput "250")
                            |> Tuple.first
                            |> Update.update CommitFocusedInput

                    ( badModel, badOut ) =
                        propModel
                            |> Update.update (FocusField "item_qty:0" "1")
                            |> Tuple.first
                            |> Update.update (EditFocusedInput "xyz")
                            |> Tuple.first
                            |> Update.update CommitFocusedInput

                    ( _, propOut ) =
                        propModel
                            |> Update.update (FocusField "item_prop:0:0" "10")
                            |> Tuple.first
                            |> Update.update (EditFocusedInput "17")
                            |> Tuple.first
                            |> Update.update CommitFocusedInput
                in
                Expect.all
                    [ \_ ->
                        Expect.equal
                            (EmitIntent
                                (Protocol.MutateItemBase
                                    { itemIndex = 0
                                    , defense = Just 250
                                    , currentDurability = Nothing
                                    , maxDurability = Nothing
                                    , quantity = Nothing
                                    , isEthereal = Nothing
                                    }
                                )
                            )
                            defOut
                    , \_ -> Expect.equal Nothing defModel.ui.focusedInput
                    , \_ -> Expect.equal NoOut badOut
                    , \_ ->
                        Expect.equal
                            True
                            (badModel.ui.statusBanner
                                |> Maybe.map (String.contains "Invalid integer")
                                |> Maybe.withDefault False
                            )
                    , \_ ->
                        Expect.equal
                            (EmitIntent
                                (Protocol.MutateItemProperty
                                    { itemIndex = 0
                                    , statId = 0
                                    , delta = 7
                                    }
                                )
                            )
                            propOut
                    ]
                    ()
        , test "Item Forge Studio notice bar shows focused range, clamps out-of-bounds values, and surfaces errors" <|
            \_ ->
                let
                    modalWithPropModel =
                        { loadedModel
                            | session =
                                loadedModel.session
                                    |> Maybe.map
                                        (\s ->
                                            { s
                                                | items =
                                                    List.map
                                                        (\it ->
                                                            if it.index == 0 then
                                                                { it
                                                                    | detailedProperties =
                                                                        [ { statId = 0
                                                                          , name = "Strength"
                                                                          , value = 10
                                                                          , minValue = 0
                                                                          , maxValue = 50
                                                                          , param = 0
                                                                          , paramBits = 0
                                                                          , valueLabel = "Value"
                                                                          , paramLabel = "Param"
                                                                          , paramKind = Protocol.ParamNone
                                                                          , paramPreview = Nothing
                                                                          }
                                                                        ]
                                                                }

                                                            else
                                                                it
                                                        )
                                                        s.items
                                            }
                                        )
                        }
                            |> Update.update (SwitchView ViewA)
                            |> Tuple.first
                            |> Update.update (OpenItemEditorModal 0)
                            |> Tuple.first

                    focusedModel =
                        modalWithPropModel
                            |> Update.update (FocusField "item_prop:0:0" "10")
                            |> Tuple.first

                    ( clampedHighModel, clampedHighOut ) =
                        focusedModel
                            |> Update.update (EditFocusedInput "999")
                            |> Tuple.first
                            |> Update.update CommitFocusedInput

                    ( clampedLowModel, clampedLowOut ) =
                        focusedModel
                            |> Update.update (EditFocusedInput "-5")
                            |> Tuple.first
                            |> Update.update CommitFocusedInput

                    ( invalidModalModel, invalidModalOut ) =
                        focusedModel
                            |> Update.update (EditFocusedInput "abc")
                            |> Tuple.first
                            |> Update.update CommitFocusedInput
                in
                Expect.all
                    [ \_ ->
                        Workspace.view focusedModel
                            |> Query.fromHtml
                            |> Query.find [ testId "item-editor-notice" ]
                            |> Query.has [ Selector.text "allowed range 0 .. 50" ]
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.MutateItemProperty { itemIndex = 0, statId = 0, delta = 40 }))
                            clampedHighOut
                    , \_ ->
                        Workspace.view clampedHighModel
                            |> Query.fromHtml
                            |> Query.find [ testId "item-editor-notice" ]
                            |> Query.has [ Selector.text "clamped from 999 to 50" ]
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.MutateItemProperty { itemIndex = 0, statId = 0, delta = -10 }))
                            clampedLowOut
                    , \_ ->
                        Workspace.view clampedLowModel
                            |> Query.fromHtml
                            |> Query.find [ testId "item-editor-notice" ]
                            |> Query.has [ Selector.text "clamped from -5 to 0" ]
                    , \_ -> Expect.equal NoOut invalidModalOut
                    , \_ ->
                        Workspace.view invalidModalModel
                            |> Query.fromHtml
                            |> Query.find [ testId "item-editor-notice" ]
                            |> Query.has [ Selector.text "Invalid integer" ]
                    ]
                    ()
        , test "Space picks item back up immediately after dropping into Equipment slot without mouse move" <|
            \_ ->
                let
                    movingModel =
                        { loadedModel
                            | session =
                                loadedModel.session
                                    |> Maybe.map (\s -> { s | selectedMoveItemIndex = Just 0 })
                            , ui =
                                loadedModel.ui
                                    |> State.UiState.setHoveredItem Nothing
                                    |> State.UiState.setHoveredDropTarget (Just { container = Equipment, x = 1, y = 0 })
                        }

                    placedSession =
                        loadedModel.session
                            |> Maybe.map
                                (\s ->
                                    { s
                                        | selectedMoveItemIndex = Nothing
                                        , items =
                                            List.map
                                                (\it ->
                                                    if it.index == 0 then
                                                        { it | container = Equipment, slotLabel = Just "Head", x = 0, y = 0 }

                                                    else
                                                        it
                                                )
                                                s.items
                                    }
                                )

                    afterDropModel =
                        Update.applyEnvelope
                            { ok = True
                            , session = placedSession
                            , rejection = Nothing
                            }
                            movingModel

                    ( _, spaceOut ) =
                        afterDropModel
                            |> Update.update
                                (HandleKeyShortcut
                                    { key = " "
                                    , code = "Space"
                                    , ctrlKey = False
                                    , altKey = False
                                    , metaKey = False
                                    , isInput = False
                                    }
                                )
                in
                Expect.all
                    [ \_ -> Expect.equal (Just 0) afterDropModel.ui.hoveredItemIndex
                    , \_ -> Expect.equal Nothing afterDropModel.ui.hoveredDropTarget
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.SelectItemForMove { itemIndex = Just 0 }))
                            spaceOut
                    ]
                    ()
        , test "Cycle 22 renders full stat_catalog beyond 40 entries and supports param injection" <|
            \_ ->
                let
                    largeCatalog =
                        List.range 0 59
                            |> List.map
                                (\i ->
                                    { statId = i
                                    , name = "stat_" ++ String.fromInt i
                                    , minValue = 0
                                    , maxValue = 100
                                    , paramBits =
                                        if i == 59 then
                                            9

                                        else
                                            0
                                    , valueLabel =
                                        if i == 59 then
                                            "Skill Level"

                                        else
                                            "Value"
                                    , paramLabel =
                                        if i == 59 then
                                            "Skill"

                                        else
                                            "Param"
                                    , paramKind =
                                        if i == 59 then
                                            Protocol.ParamSkill

                                        else
                                            Protocol.ParamNone
                                    }
                                )

                    modelWithLargeCatalog =
                        { loadedModel
                            | session =
                                loadedModel.session
                                    |> Maybe.map (\s -> { s | statCatalog = largeCatalog })
                        }
                            |> Update.update (SwitchView ViewA)
                            |> Tuple.first
                            |> Update.update (OpenItemEditorModal 0)
                            |> Tuple.first
                            |> Update.update (SelectItemEditorInjectStat 59 3)
                            |> Tuple.first
                            |> Update.update (SetItemEditorInjectParam "54")
                            |> Tuple.first
                in
                Workspace.view modelWithLargeCatalog
                    |> Query.fromHtml
                    |> Expect.all
                        [ Query.find [ testId "modal-catalog-stat-59" ]
                            >> Query.has [ Selector.text "#59 stat_59" ]
                        , Query.find [ testId "item-editor-inject-param" ]
                            >> Query.has [ Selector.text "#54 Teleport" ]
                        , Query.find [ testId "btn-inject-stat" ]
                            >> Event.simulate Event.click
                            >> Event.expect (AddItemProperty 0 59 3 54)
                        ]
        , test "Existing Properties edits both value and param with semantic labels and selector choices" <|
            \_ ->
                let
                    paramPropModel =
                        { loadedModel
                            | session =
                                loadedModel.session
                                    |> Maybe.map
                                        (\s ->
                                            { s
                                                | items =
                                                    List.map
                                                        (\it ->
                                                            if it.index == 0 then
                                                                { it
                                                                    | detailedProperties =
                                                                        [ { statId = 59
                                                                          , name = "item_singleskill"
                                                                          , value = 2
                                                                          , minValue = 0
                                                                          , maxValue = 30
                                                                          , param = 54
                                                                          , paramBits = 9
                                                                          , valueLabel = "Skill Level"
                                                                          , paramLabel = "Skill"
                                                                          , paramKind = Protocol.ParamSkill
                                                                          , paramPreview = Just "Teleport"
                                                                          }
                                                                        , { statId = 22
                                                                          , name = "monster_test"
                                                                          , value = 10
                                                                          , minValue = 0
                                                                          , maxValue = 100
                                                                          , param = 7
                                                                          , paramBits = 9
                                                                          , valueLabel = "Stat Value"
                                                                          , paramLabel = "Monster ID"
                                                                          , paramKind = Protocol.ParamRaw
                                                                          , paramPreview = Just "Fallen"
                                                                          }
                                                                        ]
                                                                }

                                                            else
                                                                it
                                                        )
                                                        s.items
                                            }
                                        )
                        }
                            |> Update.update (SwitchView ViewA)
                            |> Tuple.first
                            |> Update.update (OpenItemEditorModal 0)
                            |> Tuple.first

                    focusedRawParam =
                        paramPropModel
                            |> Update.update (FocusField "item_param:0:22" "7")
                            |> Tuple.first

                    ( _, commitRawParamOut ) =
                        focusedRawParam
                            |> Update.update (EditFocusedInput "12")
                            |> Tuple.first
                            |> Update.update CommitFocusedInput
                in
                Expect.all
                    [ \_ ->
                        Workspace.view focusedRawParam
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "modal-prop-row-59" ]
                                    >> Query.has [ Selector.text "Skill Level", Selector.text "Skill", Selector.text "Teleport" ]
                                , Query.find [ testId "modal-prop-param-input-59-toggle" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (ToggleItemEditorSkillDropdown "modal-prop-param-input-59")
                                , Query.find [ testId "modal-prop-param-input-22" ]
                                    >> Query.has [ Selector.attribute (Html.Attributes.value "7") ]
                                , Query.find [ testId "item-editor-notice" ]
                                    >> Query.has [ Selector.text "Monster ID", Selector.text "allowed range 0 .. 511", Selector.text "Fallen" ]
                                ]
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.SetItemPropertyParam { itemIndex = 0, statId = 22, param = 12 }))
                            commitRawParamOut
                    ]
                    ()
        , test "Item Forge Studio validates and guides 2nd/3rd parameters (inject_param & Skill Level 1..63)" <|
            \_ ->
                let
                    packedModel =
                        { loadedModel
                            | session =
                                loadedModel.session
                                    |> Maybe.map
                                        (\s ->
                                            { s
                                                | statCatalog =
                                                    [ { statId = 198
                                                      , name = "item_skillonhit"
                                                      , minValue = 1
                                                      , maxValue = 100
                                                      , paramBits = 16
                                                      , valueLabel = "Chance (%)"
                                                      , paramLabel = "Skill"
                                                      , paramKind = Protocol.ParamPackedSkill
                                                      }
                                                    ]
                                                , items =
                                                    List.map
                                                        (\it ->
                                                            if it.index == 0 then
                                                                { it
                                                                    | detailedProperties =
                                                                        [ { statId = 198
                                                                          , name = "item_skillonhit"
                                                                          , value = 10
                                                                          , minValue = 1
                                                                          , maxValue = 100
                                                                          , param = 54 * 64 + 5
                                                                          , paramBits = 16
                                                                          , valueLabel = "Chance (%)"
                                                                          , paramLabel = "Skill"
                                                                          , paramKind = Protocol.ParamPackedSkill
                                                                          , paramPreview = Just "Lv.5 Teleport (#54)"
                                                                          }
                                                                        ]
                                                                }

                                                            else
                                                                it
                                                        )
                                                        s.items
                                            }
                                        )
                        }
                            |> Update.update (SwitchView ViewA)
                            |> Tuple.first
                            |> Update.update (OpenItemEditorModal 0)
                            |> Tuple.first
                            |> Update.update (SelectItemEditorInjectStat 198 10)
                            |> Tuple.first

                    focusedLvl =
                        packedModel
                            |> Update.update (FocusField "item_param_lvl:0:198" "5")
                            |> Tuple.first

                    ( clampedLvlModel, clampedLvlOut ) =
                        focusedLvl
                            |> Update.update (EditFocusedInput "99")
                            |> Tuple.first
                            |> Update.update CommitFocusedInput

                    focusedInjectLvl =
                        packedModel
                            |> Update.update (FocusField "inject_param_lvl:198" "1")
                            |> Tuple.first

                    ( clampedInjectModel, _ ) =
                        focusedInjectLvl
                            |> Update.update (EditFocusedInput "99")
                            |> Tuple.first
                            |> Update.update CommitFocusedInput
                in
                Expect.all
                    [ \_ ->
                        Workspace.view focusedLvl
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "modal-prop-row-198" ]
                                    >> Query.has [ Selector.text "Chance (%) (1..100)", Selector.text "Skill", Selector.text "Skill Lv (1..63)", Selector.text "-10", Selector.text "+10" ]
                                , Query.find [ testId "item-editor-notice" ]
                                    >> Query.has [ Selector.text "Skill Level", Selector.text "allowed range 1 .. 63" ]
                                ]
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.SetItemPropertyParam { itemIndex = 0, statId = 198, param = 54 * 64 + 63 }))
                            clampedLvlOut
                    , \_ ->
                        Workspace.view clampedLvlModel
                            |> Query.fromHtml
                            |> Query.find [ testId "item-editor-notice" ]
                            |> Query.has [ Selector.text "clamped from 99 to 63" ]
                    , \_ ->
                        Workspace.view focusedInjectLvl
                            |> Query.fromHtml
                            |> Query.find [ testId "item-editor-notice" ]
                            |> Query.has [ Selector.text "Inject #198", Selector.text "allowed range 1 .. 63" ]
                    , \_ -> Expect.equal 63 (modBy 64 clampedInjectModel.ui.itemEditorInjectParam)
                    ]
                    ()
        , test "Stepper +/- buttons surface allowed range and boundary warnings without input focus" <|
            \_ ->
                let
                    modalModel =
                        { loadedModel
                            | session =
                                loadedModel.session
                                    |> Maybe.map
                                        (\s ->
                                            { s
                                                | statCatalog =
                                                    [ { statId = 198
                                                      , name = "item_skillonhit"
                                                      , minValue = 1
                                                      , maxValue = 100
                                                      , paramBits = 16
                                                      , valueLabel = "Chance (%)"
                                                      , paramLabel = "Skill"
                                                      , paramKind = Protocol.ParamPackedSkill
                                                      }
                                                    ]
                                                , items =
                                                    List.map
                                                        (\it ->
                                                            if it.index == 0 then
                                                                { it
                                                                    | detailedProperties =
                                                                        [ { statId = 198
                                                                          , name = "item_skillonhit"
                                                                          , value = 10
                                                                          , minValue = 1
                                                                          , maxValue = 100
                                                                          , param = 54 * 64 + 5
                                                                          , paramBits = 16
                                                                          , valueLabel = "Chance (%)"
                                                                          , paramLabel = "Skill"
                                                                          , paramKind = Protocol.ParamPackedSkill
                                                                          , paramPreview = Just "Lv.5 Teleport (#54)"
                                                                          }
                                                                        ]
                                                                }

                                                            else
                                                                it
                                                        )
                                                        s.items
                                            }
                                        )
                        }
                            |> Update.update (SwitchView ViewA)
                            |> Tuple.first
                            |> Update.update (OpenItemEditorModal 0)
                            |> Tuple.first
                            |> Update.update (SelectItemEditorInjectStat 198 1)
                            |> Tuple.first

                    ( stepUpModel, stepUpOut ) =
                        modalModel
                            |> Update.update (MutateItemProperty 0 198 1)

                    ( clampedStepModel, clampedStepOut ) =
                        modalModel
                            |> Update.update (MutateItemProperty 0 198 999)

                    ( injectMinModel, _ ) =
                        modalModel
                            |> Update.update (SetItemEditorInjectValue "0")
                in
                Expect.all
                    [ \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.MutateItemProperty { itemIndex = 0, statId = 198, delta = 1 }))
                            stepUpOut
                    , \_ ->
                        Workspace.view stepUpModel
                            |> Query.fromHtml
                            |> Query.find [ testId "item-editor-notice" ]
                            |> Query.has [ Selector.text "allowed 1..100" ]
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.MutateItemProperty { itemIndex = 0, statId = 198, delta = 90 }))
                            clampedStepOut
                    , \_ ->
                        Workspace.view clampedStepModel
                            |> Query.fromHtml
                            |> Query.find [ testId "item-editor-notice" ]
                            |> Query.has [ Selector.text "at boundary 100 (allowed 1..100)" ]
                    , \_ -> Expect.equal 1 injectMinModel.ui.itemEditorInjectValue
                    , \_ ->
                        Workspace.view injectMinModel
                            |> Query.fromHtml
                            |> Query.find [ testId "item-editor-notice" ]
                            |> Query.has [ Selector.text "at boundary 1 (allowed 1..100)" ]
                    ]
                    ()
        , test "Cycle 23 Item Forge Draft staging emits OpenItemForgeDraft, renders draft card, and wires Save/Discard" <|
            \_ ->
                let
                    viewAModel =
                        loadedModel
                            |> Update.update (SwitchView ViewA)
                            |> Tuple.first

                    ( openedModel, openOut ) =
                        viewAModel
                            |> Update.update (OpenItemEditorModal 0)

                    stageDraft : Protocol.SessionView -> Protocol.SessionView
                    stageDraft oldSession =
                        case List.head oldSession.items of
                            Just firstItem ->
                                { oldSession
                                    | activeItemDraft =
                                        Just
                                            { itemIndex = 0
                                            , card = { firstItem | nameEn = "Staged Forge Sword" }
                                            , dirty = True
                                            }
                                }

                            Nothing ->
                                oldSession

                    draftSession =
                        Maybe.map stageDraft openedModel.session

                    stagedModel =
                        { openedModel | session = draftSession }

                    ( appliedModel, applyOut ) =
                        stagedModel
                            |> Update.update ApplyItemForgeDraft

                    ( discardedModel, discardOut ) =
                        stagedModel
                            |> Update.update DiscardItemForgeDraft
                in
                Expect.all
                    [ \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.OpenItemForgeDraft { itemIndex = 0 }))
                            openOut
                    , \_ ->
                        Expect.equal
                            (EmitIntent Protocol.ApplyItemForgeDraft)
                            applyOut
                    , \_ ->
                        Expect.equal Nothing appliedModel.ui.itemEditorModalIndex
                    , \_ ->
                        Expect.equal
                            (EmitIntent Protocol.DiscardItemForgeDraft)
                            discardOut
                    , \_ ->
                        Expect.equal Nothing discardedModel.ui.itemEditorModalIndex
                    , \_ ->
                        Workspace.view stagedModel
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "item-editor-modal" ]
                                    >> Query.has [ Selector.text "Staged Forge Sword", Selector.text "Unsaved Draft" ]
                                , Query.find [ testId "btn-forge-apply" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect ApplyItemForgeDraft
                                , Query.find [ testId "btn-forge-discard" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect DiscardItemForgeDraft
                                ]
                    ]
                    ()
        , test "QuickMove hotkey toggles back via Undo when cursor stays inside origin item bounds, and clears when leaving" <|
            \_ ->
                let
                    viewAModel =
                        loadedModel
                            |> Update.update (SwitchView ViewA)
                            |> Tuple.first

                    firstItem =
                        viewAModel.session
                            |> Maybe.andThen (.items >> List.head)

                    ( afterMove, moveOut ) =
                        case firstItem of
                            Just it ->
                                viewAModel
                                    |> Update.update (HoverItem (Just it.index))
                                    |> Tuple.first
                                    |> Update.update
                                        (HandleKeyShortcut
                                            { key = "s"
                                            , code = "KeyS"
                                            , ctrlKey = False
                                            , altKey = False
                                            , metaKey = False
                                            , isInput = False
                                            }
                                        )

                            Nothing ->
                                ( viewAModel, NoOut )

                    simulatedAfterMove =
                        let
                            nextSession =
                                afterMove.session
                                    |> Maybe.map (\s -> { s | revision = s.revision + 1, canUndo = True })
                        in
                        { afterMove | session = nextSession }
                            |> Update.update (HoverItem Nothing)
                            |> Tuple.first

                    sameSpotModel =
                        case firstItem of
                            Just it ->
                                simulatedAfterMove
                                    |> Update.update (HoverDropTarget (Just { container = it.container, x = it.x, y = it.y }))
                                    |> Tuple.first

                            Nothing ->
                                simulatedAfterMove

                    ( afterUndoShortcut, undoOut ) =
                        sameSpotModel
                            |> Update.update
                                (HandleKeyShortcut
                                    { key = "s"
                                    , code = "KeyS"
                                    , ctrlKey = False
                                    , altKey = False
                                    , metaKey = False
                                    , isInput = False
                                    }
                                )

                    leftAreaModel =
                        simulatedAfterMove
                            |> Update.update (HoverDropTarget Nothing)
                            |> Tuple.first

                    ( _, leftShortcutOut ) =
                        leftAreaModel
                            |> Update.update
                                (HandleKeyShortcut
                                    { key = "s"
                                    , code = "KeyS"
                                    , ctrlKey = False
                                    , altKey = False
                                    , metaKey = False
                                    , isInput = False
                                    }
                                )
                in
                Expect.all
                    [ \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.QuickMoveItem { itemIndex = 0, targetContainer = Stash }))
                            moveOut
                    , \_ -> Expect.equal (EmitIntent Protocol.Undo) undoOut
                    , \_ -> Expect.equal (Just 0) afterUndoShortcut.ui.hoveredItemIndex
                    , \_ -> Expect.equal NoOut leftShortcutOut
                    ]
                    ()
        , test "Cycle 24 CharacterPanel Level and STR/DEX/VIT/ENE render -1/+1/-10/+10 steppers and clamp boundaries" <|
            \_ ->
                let
                    ( stepLvlOutModel, stepLvlOut ) =
                        loadedModel
                            |> Update.update (StepCharacterField "level" 10)

                    ( clampedLvlModel, clampedLvlOut ) =
                        loadedModel
                            |> Update.update (StepCharacterField "level" 999)

                    ( _, stepStrOut ) =
                        loadedModel
                            |> Update.update (StepCharacterField "strength" -10)
                in
                Expect.all
                    [ \_ ->
                        Workspace.view loadedModel
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "step-level-10" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (StepCharacterField "level" 10)
                                , Query.find [ testId "step-level--1" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (StepCharacterField "level" -1)
                                , Query.find [ testId "step-strength-10" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (StepCharacterField "strength" 10)
                                , Query.find [ testId "step-dexterity--10" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (StepCharacterField "dexterity" -10)
                                , Query.find [ testId "step-vitality-1" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (StepCharacterField "vitality" 1)
                                , Query.find [ testId "step-energy--1" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (StepCharacterField "energy" -1)
                                ]
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.SetCharacterField { field = Protocol.Level, value = 41 }))
                            stepLvlOut
                    , \_ ->
                        Expect.equal Nothing stepLvlOutModel.ui.statusBanner
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.SetCharacterField { field = Protocol.Level, value = 99 }))
                            clampedLvlOut
                    , \_ ->
                        Expect.equal
                            (Just "Level at boundary 99 (allowed 1..99).")
                            clampedLvlModel.ui.statusBanner
                    , \_ ->
                        Expect.equal
                            (EmitIntent (Protocol.SetCharacterField { field = Protocol.Strength, value = 50 }))
                            stepStrOut
                    ]
                    ()
        , test "Cycle 25 StatMode Legit/Free, Reset/Sync buttons, and Vitals Pure+Gear breakdown render cleanly" <|
            \_ ->
                let
                    ( _, modeOut ) =
                        loadedModel
                            |> Update.update (SelectStatMode Protocol.Free)

                    ( _, resetOut ) =
                        loadedModel
                            |> Update.update ClickResetStats

                    ( _, syncOut ) =
                        loadedModel
                            |> Update.update ClickSyncStatPointsToRules

                    ( clampedLegitModel, _ ) =
                        loadedModel
                            |> Update.update (StepCharacterField "strength" 500)
                in
                Expect.all
                    [ \_ ->
                        Workspace.view loadedModel
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "stat-mode-panel" ]
                                    >> Query.has [ Selector.text "75 / 155", Selector.text "80 / 155" ]
                                , Query.find [ testId "stat-mode-free" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (SelectStatMode Protocol.Free)
                                , Query.find [ testId "reset-stats-btn" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect ClickResetStats
                                , Query.find [ testId "sync-stat-points-btn" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect ClickSyncStatPointsToRules
                                , Query.find [ testId "vital-breakdown-life" ]
                                    >> Query.has [ Selector.text "Pure 170 + Gear +60" ]
                                , Query.find [ testId "vital-breakdown-mana" ]
                                    >> Query.has [ Selector.text "Pure 80 + Gear +46" ]
                                , Query.find [ testId "vital-breakdown-stamina" ]
                                    >> Query.has [ Selector.text "Pure 149 + Gear +41" ]
                                ]
                    , \_ -> Expect.equal (EmitIntent (Protocol.SelectStatMode { mode = Protocol.Free })) modeOut
                    , \_ -> Expect.equal (EmitIntent Protocol.ResetStats) resetOut
                    , \_ -> Expect.equal (EmitIntent Protocol.SyncStatPointsToRules) syncOut
                    , \_ ->
                        Expect.equal
                            (Just "STR at boundary 135 (allowed 15..135).")
                            clampedLegitModel.ui.statusBanner
                    ]
                    ()
        , test "Cycle 27 Skill Tree split-click zones, hover dependency classes, and detail panel" <|
            \_ ->
                let
                    hoveredModel =
                        loadedModel
                            |> Update.update (HoverSkill (Just 225))
                            |> Tuple.first

                    htmlQuery =
                        Workspace.view hoveredModel
                            |> Query.fromHtml
                in
                Expect.all
                    [ \_ ->
                        Expect.equal (Just 225) hoveredModel.ui.hoveredSkillId
                    , \_ ->
                        htmlQuery
                            |> Query.find [ testId "skill-minus-221" ]
                            |> Event.simulate Event.click
                            |> Event.expect (ClickAdjustSkillRank 221 -1)
                    , \_ ->
                        htmlQuery
                            |> Query.find [ testId "skill-plus-221" ]
                            |> Event.simulate Event.click
                            |> Event.expect (ClickAdjustSkillRank 221 1)
                    , \_ ->
                        htmlQuery
                            |> Query.find [ testId "skill-node-225" ]
                            |> Query.has [ Selector.class "active-hover" ]
                    , \_ ->
                        htmlQuery
                            |> Query.find [ testId "skill-node-221" ]
                            |> Query.has [ Selector.class "dep-prereq" ]
                    , \_ ->
                        htmlQuery
                            |> Query.find [ testId "skill-deps-panel" ]
                            |> Query.has [ Selector.text "Raven" ]
                    , \_ ->
                        htmlQuery
                            |> Query.find [ testId "skill-detail-panel" ]
                            |> Query.has [ Selector.text "Firestorm", Selector.text "Unleash fiery waves", Selector.text "순수 스킬 (Pure)", Selector.text "시너지 보정 (+115%)", Selector.text "최종 데미지 (Total)" ]
                    ]
                    ()
        , test "Item Forge searchable skill parameter filters catalog.skills by name or #id while keeping selected value" <|
            \_ ->
                let
                    skillJson =
                        sampleSessionJson
                            |> String.replace
                                "\"stat_catalog\": ["
                                "\"stat_catalog\": [{ \"stat_id\": 97, \"name\": \"item_nonclassskill\", \"min_value\": 1, \"max_value\": 63, \"param_bits\": 9, \"value_label\": \"Value\", \"param_label\": \"Skill\", \"param_kind\": \"ParamSkill\" },"
                            |> String.replace
                                "\"skills\": [ { \"value\": 54, \"label\": \"#54 Teleport\" } ],"
                                "\"skills\": [ { \"value\": 54, \"label\": \"#54 Teleport\" }, { \"value\": 59, \"label\": \"#59 Blizzard\" } ],"

                    skillModel =
                        case Json.Decode.decodeString Json.Decode.value skillJson of
                            Ok rawVal ->
                                Update.init
                                    |> Update.update (ReceiveSessionEnvelope rawVal)
                                    |> Tuple.first
                                    |> Update.update (SwitchView ViewA)
                                    |> Tuple.first
                                    |> Update.update (ContextEditItem 0)
                                    |> Tuple.first
                                    |> Update.update (SelectItemEditorInjectStat 97 1)
                                    |> Tuple.first
                                    |> Update.update (ToggleItemEditorSkillDropdown "item-editor-inject-param")
                                    |> Tuple.first
                                    |> Update.update (SetItemEditorSkillSearch "item-editor-inject-param" "Blizz")
                                    |> Tuple.first

                            Err _ ->
                                Update.init

                    htmlQuery =
                        Workspace.view skillModel
                            |> Query.fromHtml
                in
                Expect.all
                    [ \_ ->
                        htmlQuery
                            |> Query.find [ testId "item-editor-inject-param-search" ]
                            |> Event.simulate (Event.input "Teleport")
                            |> Event.expect (SetItemEditorSkillSearch "item-editor-inject-param" "Teleport")
                    , \_ ->
                        htmlQuery
                            |> Query.find [ testId "item-editor-inject-param-opt-59" ]
                            |> Event.simulate Event.click
                            |> Event.expect (SetItemEditorInjectParam "59")
                    ]
                    ()
        , test "2x2 grid drop anchor uses 4-quadrant sub-cell snapping to hold center anchor until crossing outer half" <|
            \_ ->
                Expect.all
                    [ \_ -> Expect.equal ( 2, 2 ) (State.UiState.resolveGridDropAnchorWithSub [] Stash 2 2 2 2 ( 1, 1 ))
                    , \_ -> Expect.equal ( 2, 2 ) (State.UiState.resolveGridDropAnchorWithSub [] Stash 2 2 3 2 ( 0, 1 ))
                    , \_ -> Expect.equal ( 2, 2 ) (State.UiState.resolveGridDropAnchorWithSub [] Stash 2 2 2 3 ( 1, 0 ))
                    , \_ -> Expect.equal ( 2, 2 ) (State.UiState.resolveGridDropAnchorWithSub [] Stash 2 2 3 3 ( 0, 0 ))
                    , \_ -> Expect.equal ( 1, 3 ) (State.UiState.resolveGridDropAnchorWithSub [] Stash 2 2 2 3 ( 0, 1 ))
                    ]
                    ()
        , test "Unified progression header controls shared paint/difficulty actions and Waypoints show localized names" <|
            \_ ->
                let
                    htmlQuery =
                        Workspace.view loadedModel
                            |> Query.fromHtml

                    headerQuery =
                        htmlQuery
                            |> Query.find [ testId "progression-header" ]
                in
                Expect.all
                    [ \_ ->
                        headerQuery
                            |> Query.find [ testId "paint-mode-activate" ]
                            |> Event.simulate Event.click
                            |> Event.expect (TogglePaintMode State.UiState.Activate)
                    , \_ ->
                        headerQuery
                            |> Query.find [ testId "paint-mode-deactivate" ]
                            |> Event.simulate Event.click
                            |> Event.expect (TogglePaintMode State.UiState.Deactivate)
                    , \_ ->
                        htmlQuery
                            |> Query.find [ testId "quests-panel" ]
                            |> Query.find [ testId "all-diff-quests-btn" ]
                            |> Event.simulate Event.click
                            |> Event.expect ToggleCurrentDiffQuests
                    , \_ ->
                        htmlQuery
                            |> Query.find [ testId "waypoint-matrix" ]
                            |> Query.find [ testId "all-diff-waypoints-btn" ]
                            |> Event.simulate Event.click
                            |> Event.expect ToggleCurrentDiffWaypoints
                    , \_ ->
                        headerQuery
                            |> Query.find [ testId "prog-diff-nightmare" ]
                            |> Event.simulate Event.click
                            |> Event.expect (SelectProgressionDifficulty Nightmare)
                    , \_ ->
                        htmlQuery
                            |> Query.findAll [ testId "wp-paint-toolbar" ]
                            |> Query.count (Expect.equal 0)
                    , \_ ->
                        htmlQuery
                            |> Query.find [ testId "wp-tile-0" ]
                            |> Query.has [ Selector.text "Rogue Encampment" ]
                    , \_ ->
                        htmlQuery
                            |> Query.find [ testId "wp-tile-1" ]
                            |> Query.has [ Selector.text "Cold Plains" ]
                    ]
                    ()
        , test "ToggleCurrentDiffQuests and ToggleCurrentDiffWaypoints target only selected difficulty" <|
            \_ ->
                let
                    ( _, outNormalQuests ) =
                        Update.update ToggleCurrentDiffQuests loadedModel

                    ( nmModel, _ ) =
                        Update.update (SelectProgressionDifficulty Nightmare) loadedModel

                    ( _, outNmWps ) =
                        Update.update ToggleCurrentDiffWaypoints nmModel

                    fullNormalWpModel =
                        { loadedModel
                            | session =
                                Maybe.map
                                    (\s ->
                                        let
                                            prog =
                                                s.progression
                                        in
                                        { s | progression = { prog | normalWaypointsActive = [ 0, 1 ] } }
                                    )
                                    loadedModel.session
                        }

                    ( _, outNormalWpsOff ) =
                        Update.update ToggleCurrentDiffWaypoints fullNormalWpModel
                in
                Expect.all
                    [ \_ ->
                        Expect.equal
                            (EmitIntent
                                (Protocol.SetQuestsBatch
                                    { difficulty = Normal
                                    , keys = [ "a1q1", "a1q2", "a5q3" ]
                                    , completed = True
                                    }
                                )
                            )
                            outNormalQuests
                    , \_ ->
                        Expect.equal
                            (EmitIntent
                                (Protocol.SetWaypointsBatch
                                    { difficulty = Nightmare
                                    , bits = [ 0, 1 ]
                                    , active = True
                                    }
                                )
                            )
                            outNmWps
                    , \_ ->
                        Expect.equal
                            (EmitIntent
                                (Protocol.SetWaypointsBatch
                                    { difficulty = Normal
                                    , bits = [ 0, 1 ]
                                    , active = False
                                    }
                                )
                            )
                            outNormalWpsOff
                    ]
                    ()
        , test "Quest tiles render localized game-data tooltips and semantic sub-quest status classes" <|
            \_ ->
                let
                    htmlQuery =
                        Workspace.view loadedModel
                            |> Query.fromHtml
                in
                Expect.all
                    [ \_ ->
                        htmlQuery
                            |> Query.find [ testId "quest-tile-a1q1" ]
                            |> Query.has
                                [ Selector.class "is-completed"
                                , Selector.attribute
                                    (Html.Attributes.title
                                        "a1q1 · Den of Evil (덴 오브 이블)\nStatus: Completed (0x1001)\nLook for the Den in the wilderness outside the Rogues' Camp.\n로그 캠프 밖에 있는 덴 오브 이블을 찾으시오."
                                    )
                                ]
                    , \_ ->
                        htmlQuery
                            |> Query.find [ testId "quest-tile-a1q2" ]
                            |> Query.has
                                [ Selector.class "is-reward-pending"
                                , Selector.attribute
                                    (Html.Attributes.title
                                        "a1q2 · Sisters' Burial Grounds (베리얼 그라운드)\nStatus: Reward Pending (0x0002)\nLook for Blood Raven in the Burial Grounds next to the Cold Plains.\n콜드 플레인 부근 베리얼 그라운드에서 블러드 레이븐을 찾으시오."
                                    )
                                ]
                    , \_ ->
                        htmlQuery
                            |> Query.find [ testId "quest-tile-a5q3" ]
                            |> Query.has
                                [ Selector.class "is-in-progress"
                                , Selector.attribute
                                    (Html.Attributes.title
                                        "a5q3 · Prison of Ice (얼음 감옥)\nStatus: In Progress (0x0004)\nLook for Anya under the Crystalline Passage by the Frozen River.\n크리스탈라인을 지나 프로즌리버에서 앤야를 찾으시오"
                                    )
                                ]
                    ]
                    ()
        , test "Right-clicking a quest opens detail bar and gates Reward Pending by hasNpcReward" <|
            \_ ->
                let
                    ( mSelectedReward, _ ) =
                        Update.update (SelectQuestTarget (Just { difficulty = Normal, key = "a1q1" })) loadedModel

                    ( mSelectedNoReward, _ ) =
                        Update.update (SelectQuestTarget (Just { difficulty = Normal, key = "a1q2" })) loadedModel
                in
                Expect.all
                    [ \_ ->
                        Workspace.view loadedModel
                            |> Query.fromHtml
                            |> Query.find [ testId "quest-tile-a1q1" ]
                            |> Event.simulate (Event.custom "contextmenu" (Json.Encode.object []))
                            |> Event.expect (SelectQuestTarget (Just { difficulty = Normal, key = "a1q1" }))
                    , \_ ->
                        Workspace.view mSelectedReward
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "quest-detail-bar" ]
                                    >> Query.has [ Selector.text "Den of Evil", Selector.text "[0x1001]" ]
                                , Query.find [ testId "quest-status-reward-pending" ]
                                    >> Query.has [ Selector.attribute (Html.Attributes.disabled False) ]
                                , Query.find [ testId "quest-status-reward-pending" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (SetQuestSemanticStatus Normal "a1q1" Protocol.QuestRewardPending)
                                ]
                    , \_ ->
                        Workspace.view mSelectedNoReward
                            |> Query.fromHtml
                            |> Query.find [ testId "quest-status-reward-pending" ]
                            |> Query.has [ Selector.attribute (Html.Attributes.disabled True) ]
                    ]
                    ()
        ]
