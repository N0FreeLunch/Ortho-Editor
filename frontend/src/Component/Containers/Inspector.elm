module Component.Containers.Inspector exposing (viewSelectedItemInspectorAndBrowser)

import Component.Containers.Shared exposing (..)
import Generated.Protocol exposing (CharacterStatsView, ContainerCountsView, ContainerKind(..), ItemCardView, ItemQualityKind(..), MercSummaryView, PackedSkillSpecView, ParamCatalogView, ParamOptionView, SessionView, SetBonusView, ShopCatalogItemView, SocketedItemView, StatCatalogEntryView, StatParamKind(..))
import Html exposing (Html, button, div, h2, h3, img, input, li, option, section, select, span, text, ul)
import Html.Attributes exposing (alt, attribute, checked, class, classList, disabled, id, placeholder, readonly, selected, src, style, title, type_, value)
import Html.Events exposing (custom, on, onBlur, onClick, onFocus, onInput, onMouseEnter, onMouseLeave)
import Json.Decode as Decode
import State.UiState as UiState exposing (ItemMenuState(..), ModalNotice(..), StashTab(..), UiState)
import Update exposing (Msg(..))


viewSelectedItemInspectorAndBrowser : UiState -> SessionView -> Html Msg
viewSelectedItemInspectorAndBrowser ui session =
    let
        filteredItems =
            case ui.selectedContainerFilter of
                Nothing ->
                    session.items

                Just targetContainer ->
                    List.filter (\it -> it.container == targetContainer) session.items

        selectedItem =
            ui.selectedItemIndex
                |> Maybe.andThen
                    (\idx ->
                        (session.items ++ session.tempItems)
                            |> List.filter (\it -> it.index == idx)
                            |> List.head
                    )
    in
    section [ class "glass-panel" ]
        [ div [ class "panel-header" ]
            [ div [ class "panel-title" ] [ text "Item Inspector & Container Index" ]
            , div [ class "count-grid", attribute "data-testid" "container-counts" ]
                [ countBadge "Equip" session.containerCounts.equipment
                , countBadge "Inv" session.containerCounts.inventory
                , countBadge "Belt" session.containerCounts.belt
                , countBadge "Stash" session.containerCounts.stash
                , countBadge "Cube" session.containerCounts.cube
                , countBadge "Temp" session.containerCounts.tempInventory
                ]
            ]
        , div [ class "item-inspector-dock" ]
            [ div [ class "inspector-card", attribute "data-testid" "selected-item-inspector" ]
                (case selectedItem of
                    Nothing ->
                        [ h3 [ class "inspector-title" ] [ text "No item selected" ]
                        , div [ class "meta-line" ] [ text "Click any item in the 2D Bento grids or index below to inspect its projected properties." ]
                        ]

                    Just item ->
                        [ div [ class "inspector-header-row" ]
                            [ h3 [ class ("inspector-title " ++ qualityClass item.quality) ] [ text item.nameEn ]
                            , button
                                [ class "mini-btn"
                                , attribute "data-testid" "btn-open-item-studio"
                                , onClick (OpenItemEditorModal item.index)
                                ]
                                [ text "⚒️ Forge Studio (F)" ]
                            ]
                        , div [ class "meta-line" ]
                            [ text
                                (UiState.qualityLabel item.quality
                                    ++ " · ilvl "
                                    ++ String.fromInt item.itemLevel
                                    ++ " · "
                                    ++ UiState.containerLabel item.container
                                    ++ " ("
                                    ++ String.fromInt item.x
                                    ++ ","
                                    ++ String.fromInt item.y
                                    ++ ") "
                                    ++ String.fromInt item.width
                                    ++ "x"
                                    ++ String.fromInt item.height
                                )
                            ]
                        , viewItemMutationControls ui item
                        , if List.isEmpty item.baseAttributes then
                            text ""

                          else
                            div [ class "tooltip-basestat" ]
                                (List.map (\line -> div [] [ text line ]) item.baseAttributes)
                        , if List.isEmpty item.properties then
                            div [ class "meta-line" ] [ text "No explicit stat properties projected." ]

                          else
                            ul [ class "item-prop-list" ]
                                (List.map (\line -> li [] [ text line ]) item.properties)
                        ]
                            ++ viewSocketedItemsSection item
                            :: viewSetDetails session.items item
                )
            , div []
                [ div [ class "segmented", attribute "data-testid" "container-filters" ]
                    [ filterButton ui.selectedContainerFilter Nothing "All" "all"
                    , filterButton ui.selectedContainerFilter (Just Equipment) "Equipment" "equipment"
                    , filterButton ui.selectedContainerFilter (Just Inventory) "Inventory" "inventory"
                    , filterButton ui.selectedContainerFilter (Just Belt) "Belt" "belt"
                    , filterButton ui.selectedContainerFilter (Just Stash) "Stash" "stash"
                    , filterButton ui.selectedContainerFilter (Just Cube) "Cube" "cube"
                    , filterButton ui.selectedContainerFilter (Just TempInventory) "Temp" "temp_inventory"
                    ]
                , div [ class "item-card-grid", attribute "data-testid" "container-items" ]
                    (List.map (viewIndexTile ui.selectedItemIndex) filteredItems)
                ]
            ]
        ]


viewItemMutationControls : UiState -> ItemCardView -> Html Msg
viewItemMutationControls ui item =
    let
        baseControls =
            [ case item.defense of
                Just def ->
                    div [ class "stat-chip", attribute "data-testid" "inspector-defense-control" ]
                        [ span [ class "stat-label" ] [ text "Def" ]
                        , viewInlineIntInput ui "item-editor-input item-editor-prop-num" ("item_def:" ++ String.fromInt item.index) def "inspector-defense-input"
                        , button [ class "btn-inline-step", attribute "data-testid" "btn-def-dec", onClick (MutateItemBase item.index (Just (max 0 (def - 10))) Nothing Nothing Nothing Nothing) ] [ text "-10" ]
                        , button [ class "btn-inline-step", attribute "data-testid" "btn-def-inc", onClick (MutateItemBase item.index (Just (def + 10)) Nothing Nothing Nothing Nothing) ] [ text "+10" ]
                        ]

                Nothing ->
                    text ""
            , case ( item.currentDurability, item.maxDurability ) of
                ( Just cur, Just maxD ) ->
                    div [ class "stat-chip", attribute "data-testid" "inspector-durability-control" ]
                        [ span [ class "stat-label" ] [ text "Dur" ]
                        , viewInlineIntInput ui "item-editor-input item-editor-prop-num" ("item_dur:" ++ String.fromInt item.index) cur "inspector-durability-input"
                        , span [ class "meta-line" ] [ text ("/" ++ String.fromInt maxD) ]
                        , button [ class "btn-inline-step", attribute "data-testid" "btn-dur-dec", onClick (MutateItemBase item.index Nothing (Just (max 0 (cur - 5))) Nothing Nothing Nothing) ] [ text "-5" ]
                        , button [ class "btn-inline-step", attribute "data-testid" "btn-dur-inc", onClick (MutateItemBase item.index Nothing (Just (min maxD (cur + 5))) Nothing Nothing Nothing) ] [ text "+5" ]
                        ]

                _ ->
                    text ""
            , case item.quantity of
                Just qty ->
                    div [ class "stat-chip", attribute "data-testid" "inspector-quantity-control" ]
                        [ span [ class "stat-label" ] [ text "Qty" ]
                        , viewInlineIntInput ui "item-editor-input item-editor-prop-num" ("item_qty:" ++ String.fromInt item.index) qty "inspector-quantity-input"
                        , button [ class "btn-inline-step", attribute "data-testid" "btn-qty-dec", onClick (MutateItemBase item.index Nothing Nothing Nothing (Just (max 1 (qty - 10))) Nothing) ] [ text "-10" ]
                        , button [ class "btn-inline-step", attribute "data-testid" "btn-qty-inc", onClick (MutateItemBase item.index Nothing Nothing Nothing (Just (qty + 10)) Nothing) ] [ text "+10" ]
                        ]

                Nothing ->
                    text ""
            , button
                [ classList [ ( "seg-btn", True ), ( "is-active", item.isEthereal ) ]
                , attribute "data-testid" "btn-toggle-ethereal"
                , onClick (MutateItemBase item.index Nothing Nothing Nothing Nothing (Just (not item.isEthereal)))
                ]
                [ text
                    (if item.isEthereal then
                        "Ethereal (Yes)"

                     else
                        "Ethereal (No)"
                    )
                ]
            ]

        propertyControls =
            if List.isEmpty item.detailedProperties then
                []

            else
                [ div [ class "inspector-stat-section", attribute "data-testid" "inspector-detailed-properties" ]
                    [ div [ class "meta-line" ] [ text "Tune item properties:" ]
                    , div [ class "prop-stepper-grid" ]
                        (List.map
                            (\p ->
                                div [ class "prop-stepper-row stepper-stack", attribute "data-testid" ("prop-row-" ++ String.fromInt p.statId) ]
                                    [ div [ class "stepper-input-row" ]
                                        [ span [ class "prop-name" ] [ text p.name ]
                                        , viewInlineIntInput ui
                                            "item-editor-input item-editor-prop-num"
                                            ("item_prop:" ++ String.fromInt item.index ++ ":" ++ String.fromInt p.statId)
                                            p.value
                                            ("inspector-prop-input-" ++ String.fromInt p.statId)
                                        ]
                                    , div [ class "stepper-btn-row" ]
                                        [ button [ class "btn-inline-step", attribute "data-testid" ("btn-prop-dec-" ++ String.fromInt p.statId), onClick (MutateItemProperty item.index p.statId -1) ] [ text "-1" ]
                                        , button [ class "btn-inline-step", attribute "data-testid" ("btn-prop-inc-" ++ String.fromInt p.statId), onClick (MutateItemProperty item.index p.statId 1) ] [ text "+1" ]
                                        , button [ class "btn-inline-step", onClick (MutateItemProperty item.index p.statId -10) ] [ text "-10" ]
                                        , button [ class "btn-inline-step", onClick (MutateItemProperty item.index p.statId 10) ] [ text "+10" ]
                                        ]
                                    ]
                            )
                            item.detailedProperties
                        )
                    ]
                ]
    in
    div [ class "inspector-controls-wrapper", attribute "data-testid" "inspector-controls" ]
        (div [ class "stat-summary-bar", style "margin" "8px 0" ] baseControls :: propertyControls)


