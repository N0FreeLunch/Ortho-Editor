module FrontendTests exposing (suite)

import Component.Workspace as Workspace
import Expect
import Generated.Protocol as Protocol
    exposing
        ( DifficultyMode(..)
        , EditMode(..)
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
        "item_dialect": "d2r-v3",
        "char_name": "TESTDRUID",
        "char_class": "Druid",
        "char_level": 31,
        "experience": 6105692,
        "is_expansion": true,
        "res_difficulty": "Normal",
        "skill_mode": "Legit",
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
          "active_skill_count": 11
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
          "equipped_count": 3
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
            "quality": "Unique", "container": "equipment", "slot_label": "Left Finger",
            "x": 0, "y": 0, "width": 1, "height": 1, "properties": ["+30% Better Chance of Getting Magic Items"]
          }
        ],
        "merc_items": [
          {
            "index": 0, "code": "vou", "name_en": "Insight", "name_ko": "통찰",
            "quality": "Runeword", "container": "mercenary", "slot_label": "Right Hand",
            "x": 0, "y": 0, "width": 2, "height": 4, "properties": ["Level 15 Meditation Aura When Equipped"]
          }
        ],
        "temp_items": [
          {
            "index": 0, "code": "r01", "name_en": "El Rune", "name_ko": "엘 룬",
            "quality": "Normal", "container": "temp_inventory", "slot_label": null,
            "x": 0, "y": 0, "width": 1, "height": 1, "properties": []
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
        , test "Mercenary Card renders 1x4 res grid, auras, act selector, and 1..98 bounds" <|
            \_ ->
                Expect.all
                    [ \_ ->
                        Workspace.view loadedModel
                            |> Query.fromHtml
                            |> Expect.all
                                [ Query.find [ testId "merc-res-fire" ]
                                    >> Query.has [ Selector.classes [ "res-card", "merc-res-card", "is-gold-res" ] ]
                                , Query.find [ testId "merc-res-cold" ]
                                    >> Query.hasNot [ Selector.class "is-gold-res" ]
                                , Query.find [ testId "merc-auras" ]
                                    >> Query.has [ Selector.text "Paladin Aura (Act 2)", Selector.text "Aura #120 Lv.15", Selector.text "Insight" ]
                                , Query.find [ testId "merc-act-5" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (SelectMercAct 5)
                                , Query.find [ testId "merc-lvl-plus5" ]
                                    >> Event.simulate Event.click
                                    >> Event.expect (AdjustMercLevel 5)
                                ]
                    , \_ ->
                        Workspace.view (withMercLevel 1 loadedModel)
                            |> Query.fromHtml
                            |> Query.find [ testId "merc-lvl-minus" ]
                            >> Query.has [ Selector.disabled True ]
                    , \_ ->
                        Workspace.view (withMercLevel 98 loadedModel)
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
                        Workspace.view loadedModel
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
        ]
