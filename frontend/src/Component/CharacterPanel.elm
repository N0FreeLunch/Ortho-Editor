module Component.CharacterPanel exposing (view)

import Generated.Protocol
    exposing
        ( CharacterSchemaView
        , CharacterStatsView
        , DifficultyMode(..)
        , EditMode(..)
        , ResistanceCardView
        , SessionView
        )
import Html exposing (Html, button, div, h2, input, section, span, text)
import Html.Attributes exposing (attribute, class, classList, type_, value)
import Html.Events exposing (on, onBlur, onClick, onFocus, onInput)
import Json.Decode
import State.UiState exposing (UiState)
import Update exposing (Msg(..))


view : UiState -> SessionView -> Html Msg
view ui session =
    div [ class "viewb-left-col" ]
        [ viewIdentityAndStats ui session
        , viewResistancesCard session.resistances
        , viewSkillModeCard ui session.schema session.skillMode session.stats
        ]


viewIdentityAndStats : UiState -> SessionView -> Html Msg
viewIdentityAndStats ui session =
    let
        schema =
            session.schema
    in
    section [ class "panel hero-panel", attribute "data-testid" "character-summary" ]
        [ div [ class "identity-row" ]
            [ div []
                [ h2 [ attribute "data-testid" "char-name" ] [ text session.charName ]
                , div [ class "meta-line" ]
                    [ text (session.charClass ++ " · Lv " ++ String.fromInt session.charLevel)
                    , text (" · v" ++ String.fromInt session.saveVersion ++ " (" ++ session.itemDialect ++ ")")
                    ]
                ]
            , span [ class "fixture-chip" ] [ text session.fixtureLabel ]
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
        , div [ class "stat-strip" ]
            [ plainChip "Life" (String.fromInt session.stats.life ++ "/" ++ String.fromInt session.stats.maxLife)
            , plainChip "Mana" (String.fromInt session.stats.mana ++ "/" ++ String.fromInt session.stats.maxMana)
            , editableFieldChip ui
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
        ]


viewResistancesCard : ResistanceCardView -> Html Msg
viewResistancesCard res =
    section [ class "panel res-panel", attribute "data-testid" "resistances-panel" ]
        [ div [ class "panel-header" ]
            [ h2 [] [ text "Resistances" ]
            , div [ class "segmented" ]
                [ diffButton res.difficulty Normal "Normal"
                , diffButton res.difficulty Nightmare "Nightmare"
                , diffButton res.difficulty Hell "Hell"
                ]
            ]
        , div [ class "res-grid-2x2" ]
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


viewSkillModeCard : UiState -> CharacterSchemaView -> EditMode -> CharacterStatsView -> Html Msg
viewSkillModeCard ui schema activeMode stats =
    section [ class "panel mode-panel", attribute "data-testid" "skill-mode-panel" ]
        [ div [ class "panel-header" ]
            [ h2 [] [ text "Skill Allocation Mode" ]
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
