module Component.ContainersPanel exposing (view)

import Generated.Protocol exposing (ContainerCountsView, ItemCardView, SessionView)
import Html exposing (Html, button, div, h2, h3, li, section, span, text, ul)
import Html.Attributes exposing (attribute, class, classList, disabled)
import Html.Events exposing (onClick)
import State.UiState exposing (UiState)
import Update exposing (Msg(..))


view : UiState -> SessionView -> Html Msg
view ui session =
    let
        filteredItems =
            if ui.selectedContainerFilter == "all" then
                session.items

            else
                List.filter (\it -> it.container == ui.selectedContainerFilter) session.items
    in
    div [ class "viewa-layout", attribute "data-testid" "view-a-containers" ]
        [ section [ class "panel" ]
            [ div [ class "panel-header" ]
                [ h2 [] [ text "Containers" ]
                , viewCountStrip session.containerCounts
                ]
            , div [ class "segmented", attribute "data-testid" "container-filters" ]
                [ filterButton ui.selectedContainerFilter "all" "All"
                , filterButton ui.selectedContainerFilter "equipment" "Equipment"
                , filterButton ui.selectedContainerFilter "inventory" "Inventory"
                , filterButton ui.selectedContainerFilter "belt" "Belt"
                , filterButton ui.selectedContainerFilter "stash" "Stash"
                , filterButton ui.selectedContainerFilter "cube" "Cube"
                ]
            , div [ class "item-card-grid", attribute "data-testid" "container-items" ]
                (List.map (viewItemTile ui.selectedItemIndex) filteredItems)
            ]
        , section [ class "panel temp-inventory-panel", attribute "data-testid" "temp-inventory-panel" ]
            [ div [ class "panel-header" ]
                [ div []
                    [ h2 [] [ text "Temp Inventory (9x4)" ]
                    , span [ class "meta-line", attribute "data-testid" "temp-capacity-badge" ]
                        [ text (String.fromInt (List.length session.tempItems) ++ " / 36 slots occupied") ]
                    ]
                , button
                    [ class "btn-danger-ghost"
                    , attribute "data-testid" "clear-temp-btn"
                    , disabled (List.isEmpty session.tempItems)
                    , onClick ClearTempInventory
                    ]
                    [ text "Clear" ]
                ]
            , div [ class "temp-grid-9x4" ]
                (if List.isEmpty session.tempItems then
                    [ div [ class "temp-empty-note" ] [ text "Drop staging area (9x4). Move items individually or Clear." ] ]

                 else
                    List.map (viewItemTile ui.selectedItemIndex) session.tempItems
                )
            ]
        ]


viewCountStrip : ContainerCountsView -> Html Msg
viewCountStrip counts =
    div [ class "count-grid", attribute "data-testid" "container-counts" ]
        [ countBadge "Equip" counts.equipment
        , countBadge "Inv" counts.inventory
        , countBadge "Belt" counts.belt
        , countBadge "Stash" counts.stash
        , countBadge "Cube" counts.cube
        , countBadge "Temp" counts.tempInventory
        ]


countBadge : String -> Int -> Html Msg
countBadge label count =
    div [ class "stat-chip" ]
        [ span [ class "stat-label" ] [ text label ]
        , span [ class "stat-val" ] [ text (String.fromInt count) ]
        ]


filterButton : String -> String -> String -> Html Msg
filterButton active key label =
    button
        [ classList [ ( "seg-btn", True ), ( "is-active", active == key ) ]
        , attribute "data-testid" ("container-filter-" ++ key)
        , onClick (SelectContainerFilter key)
        ]
        [ text label ]


viewItemTile : Maybe Int -> ItemCardView -> Html Msg
viewItemTile selectedIdx item =
    let
        isSelected =
            selectedIdx == Just item.index
    in
    div
        [ classList
            [ ( "item-tile", True )
            , ( "is-selected", isSelected )
            ]
        , attribute "data-testid" ("item-tile-" ++ String.fromInt item.index)
        , onClick (SelectItem (Just item.index))
        ]
        [ div [ class "item-tile-head" ]
            [ span [ class "item-name" ] [ text item.nameEn ]
            , span [ class "item-quality-pill" ] [ text item.quality ]
            ]
        , div [ class "meta-line" ]
            [ text
                (item.container
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
        , if isSelected && not (List.isEmpty item.properties) then
            ul [ class "item-prop-list" ]
                (List.map (\line -> li [] [ text line ]) item.properties)

          else
            text ""
        ]
