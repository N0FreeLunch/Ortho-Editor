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
          "gold_stash_max": 2500000
        },
        "stats": {
          "strength_base": 60, "strength_bonus": 18,
          "dexterity_base": 20, "dexterity_bonus": 2,
          "vitality_base": 60, "vitality_bonus": 0,
          "energy_base": 20, "energy_bonus": 18,
          "life": 230, "max_life": 190,
          "mana": 126, "max_mana": 80,
          "stamina": 190, "max_stamina": 149,
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
          "all_skills_bonus": 2
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
            { "key": "a1q1", "act": 1, "index": 0, "name": "Den of Evil" },
            { "key": "a1q2", "act": 1, "index": 1, "name": "Sisters' Burial Grounds" },
            { "key": "a5q3", "act": 5, "index": 2, "name": "Prison of Ice" }
          ],
          "waypoint_catalog": [
            { "bit": 0, "act": 1, "index": 0, "name": "Act 1 - Town" },
            { "bit": 1, "act": 1, "index": 1, "name": "Act 1 - Wilderness 2" }
          ],
          "normal_quests_completed": ["a1q1"],
          "nightmare_quests_completed": [],
          "hell_quests_completed": [],
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
            "socketed_names": [],
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
            "socketed_names": ["Ral Rune", "Tir Rune", "Tal Rune", "Sol Rune"],
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
            "socketed_names": [],
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
            "bonus_level": 2
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
            "bonus_level": 0
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
            "bonus_level": 2
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
        ]
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
                        , { tid = "input-gold-inventory", fieldKey = "gold_character", rawVal = "229492", nextRaw = "150000", expectedOut = EmitIntent (Protocol.SetCharacterField { field = Protocol.GoldCharacter, value = 150000 }) }
                        , { tid = "input-gold-stash", fieldKey = "gold_stash", rawVal = "281974", nextRaw = "500000", expectedOut = EmitIntent (Protocol.SetCharacterField { field = Protocol.GoldStash, value = 500000 }) }
                        , { tid = "input-stat-points", fieldKey = "stat_points", rawVal = "75", nextRaw = "10", expectedOut = EmitIntent (Protocol.SetCharacterField { field = Protocol.StatPoints, value = 10 }) }
                        ]

                    checkCharInput { tid, fieldKey, rawVal, nextRaw, expectedOut } _ =
                        let
                            ( focused, _ ) =
                                Update.update (FocusField fieldKey rawVal) loadedModel

                            ( edited, _ ) =
                                Update.update (EditFocusedInput nextRaw) focused

                            ( _, commitOut ) =
                                Update.update CommitFocusedInput edited

                            ( cancelled, _ ) =
                                Update.update CancelFocusedInput edited
                        in
                        Expect.all
                            [ \_ ->
                                Workspace.view loadedModel
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
                        , socketedNames = [ "Perfect Diamond" ]
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
                                , socketedNames = []
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
                                    >> Query.has [ Selector.text "Raven", Selector.text "5 (7)" ]
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
                        , socketedNames = []
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
                                    >> Event.expect (SelectItem (Just 0))
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
                                            String.left idx s ++ "\"shop_catalog\": [] }, \"rejection\": null }"

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
                                    >> Event.expect (SelectItem (Just 0))
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
        ]
