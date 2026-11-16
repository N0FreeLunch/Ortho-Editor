module Component.MercenaryPanel exposing (view)

import Generated.Protocol exposing (ItemCardView, MercStatsView, MercSummaryView)
import Html exposing (Html, button, div, h2, h3, input, section, span, text)
import Html.Attributes exposing (attribute, class, classList, disabled, type_, value)
import Html.Events exposing (on, onClick, onInput)
import Json.Decode
import Update exposing (Msg(..))


view : MercSummaryView -> List ItemCardView -> Html Msg
view merc mercItems =
    section [ class "glass-panel merc-summary-card", attribute "data-testid" "merc-summary" ]
        [ div [ class "panel-header" ]
            [ div []
                [ div [ class "brand-title-row" ]
                    [ h2 [] [ text "Mercenary Summary" ]
                    , if merc.persistenceSupported then
                        span [ class "persisted-badge", attribute "data-testid" "merc-persisted-badge" ]
                            [ text "Saved to .d2s" ]

                      else
                        span [ class "readonly-badge", attribute "data-testid" "merc-preview-badge" ]
                            [ text "Read-only · Not saved to .d2s" ]
                    ]
                , div [ class "meta-line", attribute "data-testid" "merc-identity" ]
                    [ text
                        (if merc.hired then
                            merc.className ++ " · " ++ String.fromInt merc.equippedCount ++ " equipped"

                         else if merc.persistenceSupported then
                            "Unhired (Select Act to Hire)"

                         else
                            "Unhired (Preview by Act)"
                        )
                    ]
                ]
            , viewLevelStepper merc.persistenceSupported merc.stats.level
            ]
        , case ( merc.persistenceSupported, merc.previewOnlyReason ) of
            ( False, Just reason ) ->
                div [ class "readonly-note", attribute "data-testid" "merc-preview-note" ]
                    [ text reason ]

            ( False, Nothing ) ->
                div [ class "readonly-note", attribute "data-testid" "merc-preview-note" ]
                    [ text "Read-only: mercenary changes are not saved to .d2s yet." ]

            ( True, _ ) ->
                text ""
        , div [ class "merc-act-bar", attribute "data-testid" "merc-act-selector" ]
            [ actButton merc.persistenceSupported merc.actIndex 1 "Act 1 Rogue"
            , actButton merc.persistenceSupported merc.actIndex 2 "Act 2 Desert"
            , actButton merc.persistenceSupported merc.actIndex 3 "Act 3 Iron Wolf"
            , actButton merc.persistenceSupported merc.actIndex 5 "Act 5 Barbarian"
            ]
        , viewMercStatBreakdown merc.stats
        , viewMercAuras merc.stats mercItems
        , viewMercResGrid1x4 merc.stats
        ]


viewLevelStepper : Bool -> Int -> Html Msg
viewLevelStepper editable lvl =
    div [ class "stepper" ]
        [ input
            [ class "merc-lvl-input"
            , type_ "text"
            , attribute "inputmode" "numeric"
            , attribute "data-testid" "merc-level-input"
            , disabled (not editable)
            , value (String.fromInt lvl)
            , onInput
                (\raw ->
                    raw
                        |> String.toInt
                        |> Maybe.withDefault lvl
                        |> SetMercLevelExact
                )
            ]
            []
        , span [ class "step-val", attribute "data-testid" "merc-level-value" ]
            [ text ("Lv " ++ String.fromInt lvl) ]
        , button
            [ class "step-btn"
            , attribute "data-testid" "merc-lvl-minus"
            , disabled (not editable || lvl <= 1)
            , on "click" (stepClickDecoder False -1 -5)
            ]
            [ text "-1" ]
        , button
            [ class "step-btn"
            , attribute "data-testid" "merc-lvl-plus"
            , disabled (not editable || lvl >= 98)
            , on "click" (stepClickDecoder False 1 5)
            ]
            [ text "+1" ]
        , button
            [ class "step-btn"
            , attribute "data-testid" "merc-lvl-minus5"
            , disabled (not editable || lvl <= 1)
            , onClick (AdjustMercLevel -5)
            ]
            [ text "-5" ]
        , button
            [ class "step-btn"
            , attribute "data-testid" "merc-lvl-plus5"
            , disabled (not editable || lvl >= 98)
            , onClick (AdjustMercLevel 5)
            ]
            [ text "+5" ]
        ]


stepClickDecoder : Bool -> Int -> Int -> Json.Decode.Decoder Msg
stepClickDecoder _ normalDelta shiftDelta =
    Json.Decode.field "shiftKey" Json.Decode.bool
        |> Json.Decode.maybe
        |> Json.Decode.map (Maybe.withDefault False)
        |> Json.Decode.map
            (\isShift ->
                AdjustMercLevel
                    (if isShift then
                        shiftDelta

                     else
                        normalDelta
                    )
            )


actButton : Bool -> Int -> Int -> String -> Html Msg
actButton editable activeAct targetAct label =
    button
        [ classList [ ( "seg-btn", True ), ( "is-active", activeAct == targetAct ) ]
        , attribute "data-testid" ("merc-act-" ++ String.fromInt targetAct)
        , disabled (not editable)
        , onClick (SelectMercAct targetAct)
        ]
        [ text label ]


viewMercStatBreakdown : MercStatsView -> Html Msg
viewMercStatBreakdown stats =
    div [ class "merc-stats-grid", attribute "data-testid" "merc-stat-breakdown" ]
        [ mercStatChip "STR" stats.baseStr stats.bonusStr
        , mercStatChip "DEX" stats.baseDex stats.bonusDex
        , mercStatChip "LIFE" stats.baseLife stats.bonusLife
        , mercStatChip "DEF" stats.baseDef stats.bonusDef
        ]


mercStatChip : String -> Int -> Int -> Html Msg
mercStatChip label baseVal bonusVal =
    div [ class "stat-chip" ]
        [ span [ class "stat-label" ] [ text label ]
        , span [ class "stat-val" ] [ text (String.fromInt (baseVal + bonusVal)) ]
        , span [ class "stat-sub" ] [ text (String.fromInt baseVal ++ "+" ++ String.fromInt bonusVal) ]
        ]


viewMercAuras : MercStatsView -> List ItemCardView -> Html Msg
viewMercAuras stats mercItems =
    div [ class "merc-aura-box", attribute "data-testid" "merc-auras" ]
        [ div [ class "aura-row" ]
            [ span [ class "aura-tag tag-innate" ] [ text "[Innate]" ]
            , span [ class "aura-name" ] [ text stats.innateAura ]
            ]
        , div [ class "aura-row" ]
            [ span [ class "aura-tag tag-gear" ] [ text "[Gear]" ]
            , span [ class "aura-name" ]
                [ text
                    (if List.isEmpty stats.equipmentAuras then
                        "None"

                     else
                        String.join ", " stats.equipmentAuras
                    )
                ]
            ]
        , if List.isEmpty mercItems then
            text ""

          else
            div [ class "gear-tag-list" ]
                (List.map
                    (\item ->
                        span [ class "gear-chip" ]
                            [ text
                                ((Maybe.withDefault "Slot" item.slotLabel)
                                    ++ ": "
                                    ++ item.nameEn
                                )
                            ]
                    )
                    mercItems
                )
        ]


viewMercResGrid1x4 : MercStatsView -> Html Msg
viewMercResGrid1x4 stats =
    div []
        [ h3 [ class "subhead" ] [ text "Mercenary Resistances" ]
        , div [ class "merc-res-grid merc-res-grid-1x4", attribute "data-testid" "merc-res-grid" ]
            [ mercResBox "fire" "Fire" stats.fireRes
            , mercResBox "cold" "Cold" stats.coldRes
            , mercResBox "light" "Light" stats.lightRes
            , mercResBox "poison" "Poison" stats.poisonRes
            ]
        ]


mercResBox : String -> String -> Int -> Html Msg
mercResBox key label value =
    div
        [ classList [ ( "res-card", True ), ( "merc-res-card", True ), ( "is-gold-res", value >= 75 ) ]
        , attribute "data-testid" ("merc-res-" ++ key)
        ]
        [ span [ class "res-label" ] [ text label ]
        , span [ class "res-value" ] [ text (String.fromInt value ++ "%") ]
        ]
