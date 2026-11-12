module Component.MercenaryPanel exposing (view)

import Generated.Protocol exposing (ItemCardView, MercStatsView, MercSummaryView)
import Html exposing (Html, button, div, h2, h3, input, section, span, text)
import Html.Attributes exposing (attribute, class, classList, disabled, type_, value)
import Html.Events exposing (on, onClick, onInput)
import Json.Decode
import Update exposing (Msg(..))


view : MercSummaryView -> List ItemCardView -> Html Msg
view merc mercItems =
    section [ class "panel merc-panel", attribute "data-testid" "merc-summary" ]
        [ div [ class "panel-header" ]
            [ div []
                [ h2 [] [ text "Mercenary Summary" ]
                , div [ class "meta-line", attribute "data-testid" "merc-identity" ]
                    [ text
                        (if merc.hired then
                            merc.className ++ " · " ++ String.fromInt merc.equippedCount ++ " equipped"

                         else
                            "Unhired (Preview by Act)"
                        )
                    ]
                ]
            , viewLevelStepper merc.stats.level
            ]
        , div [ class "merc-act-bar", attribute "data-testid" "merc-act-selector" ]
            [ actButton merc.actIndex 1 "Act 1 Rogue"
            , actButton merc.actIndex 2 "Act 2 Desert"
            , actButton merc.actIndex 3 "Act 3 Iron Wolf"
            , actButton merc.actIndex 5 "Act 5 Barbarian"
            ]
        , viewMercStatBreakdown merc.stats
        , viewMercAuras merc.stats mercItems
        , viewMercResGrid1x4 merc.stats
        ]


viewLevelStepper : Int -> Html Msg
viewLevelStepper lvl =
    div [ class "stepper" ]
        [ button
            [ class "step-btn"
            , attribute "data-testid" "merc-lvl-minus5"
            , disabled (lvl <= 1)
            , onClick (AdjustMercLevel -5)
            ]
            [ text "-5" ]
        , button
            [ class "step-btn"
            , attribute "data-testid" "merc-lvl-minus"
            , disabled (lvl <= 1)
            , on "click" (stepClickDecoder False -1 -5)
            ]
            [ text "-" ]
        , input
            [ class "merc-lvl-input"
            , type_ "number"
            , attribute "data-testid" "merc-level-input"
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
            , attribute "data-testid" "merc-lvl-plus"
            , disabled (lvl >= 98)
            , on "click" (stepClickDecoder False 1 5)
            ]
            [ text "+" ]
        , button
            [ class "step-btn"
            , attribute "data-testid" "merc-lvl-plus5"
            , disabled (lvl >= 98)
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


actButton : Int -> Int -> String -> Html Msg
actButton activeAct targetAct label =
    button
        [ classList [ ( "seg-btn", True ), ( "is-active", activeAct == targetAct ) ]
        , attribute "data-testid" ("merc-act-" ++ String.fromInt targetAct)
        , onClick (SelectMercAct targetAct)
        ]
        [ text label ]


viewMercStatBreakdown : MercStatsView -> Html Msg
viewMercStatBreakdown stats =
    div [ class "stat-breakdown-grid", attribute "data-testid" "merc-stat-breakdown" ]
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
        , div [ class "merc-res-grid-1x4", attribute "data-testid" "merc-res-grid" ]
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
