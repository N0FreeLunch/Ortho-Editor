module Component.CharacterPanel exposing (view, viewCombatLabAccordion, viewSkillTreePreview)

import Generated.Protocol
    exposing
        ( CharacterSchemaView
        , CharacterStatsView
        , DifficultyMode(..)
        , EditMode(..)
        , ResistanceCardView
        , SessionView
        )
import Html exposing (Html, button, div, h2, input, p, section, span, text)
import Html.Attributes exposing (attribute, class, classList, disabled, style, title, type_, value)
import Html.Events exposing (on, onBlur, onClick, onFocus, onInput)
import Json.Decode
import State.UiState exposing (UiState)
import Update exposing (Msg(..))


view : UiState -> SessionView -> Html Msg
view ui session =
    let
        schema =
            session.schema
    in
    section [ class "glass-panel col1-card", attribute "data-testid" "character-summary" ]
        [ div [ class "panel-header" ]
            [ div [ class "panel-title" ] [ text "Character Profile & Stats" ]
            , span [ class "fixture-chip" ] [ text session.fixtureLabel ]
            ]
        , div [ class "identity-row" ]
            [ div []
                [ h2 [ attribute "data-testid" "char-name" ] [ text session.charName ]
                , div [ class "meta-line" ]
                    [ text (session.charClass ++ " · Lv " ++ String.fromInt session.charLevel)
                    , text (" · v" ++ String.fromInt session.saveVersion ++ " (" ++ session.itemDialect ++ ")")
                    ]
                ]
            ]
        , div [ class "stat-strip" ]
            [ editableFieldChip ui
                "char_name"
                "Name"
                session.charName
                (String.fromInt schema.nameMinLen ++ ".." ++ String.fromInt schema.nameMaxLen ++ " ASCII")
                "input-char-name"
            , editableFieldChip ui
                "level"
                "Level"
                (String.fromInt session.charLevel)
                (String.fromInt schema.levelMin ++ ".." ++ String.fromInt schema.levelMax)
                "input-char-level"
            ]
        , div [ class "stat-breakdown-grid" ]
            [ editableStatChip ui "strength" "STR" session.stats.strengthBase session.stats.strengthBonus schema "input-stat-strength"
            , editableStatChip ui "dexterity" "DEX" session.stats.dexterityBase session.stats.dexterityBonus schema "input-stat-dexterity"
            , editableStatChip ui "vitality" "VIT" session.stats.vitalityBase session.stats.vitalityBonus schema "input-stat-vitality"
            , editableStatChip ui "energy" "ENE" session.stats.energyBase session.stats.energyBonus schema "input-stat-energy"
            ]
        , viewVitalsGauges session.stats
        , div [ class "stat-strip" ]
            [ editableFieldChip ui
                "gold_character"
                "Gold Inv"
                (String.fromInt session.stats.goldInventory)
                ("<= " ++ String.fromInt schema.goldCharacterMax)
                "input-gold-inventory"
            , editableFieldChip ui
                "gold_stash"
                "Gold Stash"
                (String.fromInt session.stats.goldStash)
                ("<= " ++ String.fromInt schema.goldStashMax)
                "input-gold-stash"
            ]
        , viewSkillModeStrip ui schema session.skillMode session.stats
        , viewResistancesSection session.resistances
        ]


viewVitalsGauges : CharacterStatsView -> Html Msg
viewVitalsGauges stats =
    div [ class "vitals-group", attribute "data-testid" "vitals-gauges" ]
        [ vitalBar "Life" "life" stats.life stats.maxLife
        , vitalBar "Mana" "mana" stats.mana stats.maxMana
        , vitalBar "Stamina" "stamina" stats.stamina stats.maxStamina
        ]


vitalBar : String -> String -> Int -> Int -> Html Msg
vitalBar label barClass current maxVal =
    let
        pct =
            clamp 0 100 ((max 0 current * 100) // max 1 maxVal)
    in
    div [ class "vital-gauge-wrapper" ]
        [ div [ class "vital-gauge-label" ]
            [ span [] [ text label ]
            , span [] [ text (String.fromInt current ++ " / " ++ String.fromInt maxVal) ]
            ]
        , div [ class "gauge-bar-outer" ]
            [ div
                [ class ("gauge-bar-inner " ++ barClass)
                , style "width" (String.fromInt pct ++ "%")
                ]
                []
            ]
        ]


viewSkillModeStrip : UiState -> CharacterSchemaView -> EditMode -> CharacterStatsView -> Html Msg
viewSkillModeStrip ui schema activeMode stats =
    div [ class "res-section", attribute "data-testid" "skill-mode-panel" ]
        [ div [ class "res-header" ]
            [ span [] [ text "Allocation Mode" ]
            , div [ class "segmented" ]
                [ modeButton activeMode Legit "Legit"
                , modeButton activeMode Free "Free"
                ]
            ]
        , div [ class "stat-strip" ]
            [ editableFieldChip ui
                "stat_points"
                "Unspent Stats"
                (String.fromInt stats.unspentStatPoints)
                (String.fromInt schema.statPointsMin ++ ".." ++ String.fromInt schema.statPointsMax)
                "input-stat-points"
            , plainChip "Unspent Skills" (String.fromInt stats.unspentSkillPoints)
            , plainChip "Active Skills" (String.fromInt stats.activeSkillCount)
            ]
        ]


viewResistancesSection : ResistanceCardView -> Html Msg
viewResistancesSection res =
    div [ class "res-section", attribute "data-testid" "resistances-panel" ]
        [ div [ class "res-header" ]
            [ span [] [ text "Resistances" ]
            , div [ class "segmented" ]
                [ diffButton res.difficulty Normal "Normal"
                , diffButton res.difficulty Nightmare "Nightmare"
                , diffButton res.difficulty Hell "Hell"
                ]
            ]
        , div [ class "resistance-grid res-grid-2x2" ]
            [ resCard "Fire" res.fire res.fireCap
            , resCard "Cold" res.cold res.coldCap
            , resCard "Lightning" res.light res.lightCap
            , resCard "Poison" res.poison res.poisonCap
            ]
        , div [ class "meta-line" ]
            [ text
                ("Anya +"
                    ++ String.fromInt res.anyaBonus
                    ++ "% · Diff Penalty -"
                    ++ String.fromInt res.difficultyPenalty
                    ++ "%"
                )
            ]
        ]


viewSkillTreePreview : UiState -> SessionView -> Html Msg
viewSkillTreePreview ui session =
    let
        treeTab key label =
            button
                [ classList [ ( "skill-tab-btn", True ), ( "active", ui.skillTreeTab == key ) ]
                , attribute "data-testid" ("skill-tree-tab-" ++ key)
                , onClick (SelectSkillTreeTab key)
                ]
                [ text label ]

        tierRow lvl =
            [ div [ class "skill-tier-badge" ] [ text ("Tier Lv " ++ String.fromInt lvl) ]
            , skillPreviewBtn ("Lv " ++ String.fromInt lvl ++ " A")
            , skillPreviewBtn ("Lv " ++ String.fromInt lvl ++ " B")
            , skillPreviewBtn ("Lv " ++ String.fromInt lvl ++ " C")
            ]

        skillPreviewBtn titleText =
            button
                [ class "skill-node-preview"
                , disabled True
                , title "Preview only · Skill allocation intents require a dedicated Rust skill-tree slice"
                ]
                [ text titleText ]
    in
    section [ class "glass-panel col2-skill-panel", attribute "data-testid" "skill-tree-preview" ]
        [ div [ class "panel-header" ]
            [ div [ class "panel-title" ] [ text ("Skill Tree (" ++ session.charClass ++ ")") ]
            , span [ class "preview-only-pill" ] [ text "Preview only" ]
            ]
        , div [ class "meta-line" ]
            [ text
                ("Unspent Skills: "
                    ++ String.fromInt session.stats.unspentSkillPoints
                    ++ " · Active Skills: "
                    ++ String.fromInt session.stats.activeSkillCount
                )
            ]
        , div [ class "skill-subtabs" ]
            [ treeTab "tree0" "Tree I"
            , treeTab "tree1" "Tree II"
            , treeTab "tree2" "Tree III"
            ]
        , div [ class "skill-tree-preview-grid" ]
            (List.concatMap tierRow [ 1, 6, 12, 18, 24, 30 ])
        , div [ class "readonly-note" ]
            [ text "Skill rank editing is locked (Preview only) until Rust skill-tree intents are wired." ]
        ]


viewCombatLabAccordion : UiState -> SessionView -> Html Msg
viewCombatLabAccordion ui session =
    div [ class "combat-lab-accordion", attribute "data-testid" "combat-lab-accordion" ]
        [ button
            [ class "combat-lab-header"
            , attribute "data-testid" "combat-lab-toggle"
            , onClick ToggleCombatLab
            ]
            [ span [] [ text "Combat Lab (Preview)" ]
            , span []
                [ text
                    (if ui.combatLabOpen then
                        "Hide ▲"

                     else
                        "Expand ▼"
                    )
                ]
            ]
        , if ui.combatLabOpen then
            div [ class "combat-lab-content", attribute "data-testid" "combat-lab-content" ]
                [ p []
                    [ text
                        ("Preview only: Simulation tables will consume Rust combat projection outputs ("
                            ++ session.charClass
                            ++ " Lv "
                            ++ String.fromInt session.charLevel
                            ++ ", "
                            ++ String.fromInt session.containerCounts.equipment
                            ++ " equipped items, EXP "
                            ++ String.fromInt session.experience
                            ++ ")."
                        )
                    ]
                ]

          else
            text ""
        ]


diffButton : DifficultyMode -> DifficultyMode -> String -> Html Msg
diffButton active target label =
    button
        [ classList [ ( "seg-btn", True ), ( "is-active", active == target ) ]
        , attribute "data-testid" ("res-diff-" ++ String.toLower label)
        , onClick (SelectResDifficulty target)
        ]
        [ text label ]


modeButton : EditMode -> EditMode -> String -> Html Msg
modeButton active target label =
    button
        [ classList [ ( "seg-btn", True ), ( "is-active", active == target ) ]
        , attribute "data-testid" ("skill-mode-" ++ String.toLower label)
        , onClick (SelectSkillMode target)
        ]
        [ text label ]


resCard : String -> Int -> Int -> Html Msg
resCard label effective maxCap =
    div
        [ classList [ ( "res-card", True ), ( "is-gold-res", effective >= 75 ) ]
        , attribute "data-testid" ("res-card-" ++ String.toLower label)
        ]
        [ span [ class "res-label" ] [ text label ]
        , span [ class "res-value" ] [ text (String.fromInt effective ++ "%") ]
        , span [ class "res-cap" ] [ text ("Max " ++ String.fromInt maxCap ++ "%") ]
        ]


focusedDraftValue : UiState -> String -> String -> String
focusedDraftValue ui key serverVal =
    case ui.focusedInput of
        Just focused ->
            if focused.key == key then
                focused.raw

            else
                serverVal

        Nothing ->
            serverVal


onEnterOrEscape : Html.Attribute Msg
onEnterOrEscape =
    on "keydown"
        (Json.Decode.field "key" Json.Decode.string
            |> Json.Decode.andThen
                (\k ->
                    case k of
                        "Enter" ->
                            Json.Decode.succeed CommitFocusedInput

                        "Escape" ->
                            Json.Decode.succeed CancelFocusedInput

                        _ ->
                            Json.Decode.fail "ignore key"
                )
        )


draftInputEl : UiState -> String -> String -> String -> Html Msg
draftInputEl ui key serverVal tid =
    input
        [ class "stat-input"
        , type_ "text"
        , value (focusedDraftValue ui key serverVal)
        , attribute "data-testid" tid
        , onFocus (FocusField key serverVal)
        , onInput EditFocusedInput
        , onBlur CommitFocusedInput
        , onEnterOrEscape
        ]
        []


editableFieldChip : UiState -> String -> String -> String -> String -> String -> Html Msg
editableFieldChip ui key label serverVal hint tid =
    div [ class "stat-chip" ]
        [ span [ class "stat-label" ] [ text label ]
        , draftInputEl ui key serverVal tid
        , span [ class "stat-sub" ] [ text hint ]
        ]


editableStatChip : UiState -> String -> String -> Int -> Int -> CharacterSchemaView -> String -> Html Msg
editableStatChip ui key label baseVal bonusVal schema tid =
    div [ class "stat-chip" ]
        [ span [ class "stat-label" ] [ text label ]
        , draftInputEl ui key (String.fromInt baseVal) tid
        , span [ class "stat-sub" ]
            [ text
                ("+"
                    ++ String.fromInt bonusVal
                    ++ " ("
                    ++ String.fromInt schema.statMin
                    ++ ".."
                    ++ String.fromInt schema.statMax
                    ++ ")"
                )
            ]
        ]


plainChip : String -> String -> Html Msg
plainChip label val =
    div [ class "stat-chip" ]
        [ span [ class "stat-label" ] [ text label ]
        , span [ class "stat-val" ] [ text val ]
        ]
