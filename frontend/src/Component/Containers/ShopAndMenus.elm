module Component.Containers.ShopAndMenus exposing (stashTabBtn, viewItemMenuOverlay, viewShopBody)

import Component.Containers.Shared exposing (..)
import Generated.Protocol exposing (CharacterStatsView, ContainerCountsView, ContainerKind(..), ItemCardView, ItemQualityKind(..), MercSummaryView, PackedSkillSpecView, ParamCatalogView, ParamOptionView, SessionView, SetBonusView, ShopCatalogItemView, SocketedItemView, StatCatalogEntryView, StatParamKind(..))
import Html exposing (Html, button, div, h2, h3, img, input, li, option, section, select, span, text, ul)
import Html.Attributes exposing (alt, attribute, checked, class, classList, disabled, id, placeholder, readonly, selected, src, style, title, type_, value)
import Html.Events exposing (custom, on, onBlur, onClick, onFocus, onInput, onMouseEnter, onMouseLeave)
import Json.Decode as Decode
import State.UiState as UiState exposing (ItemMenuState(..), ModalNotice(..), StashTab(..), UiState)
import Update exposing (Msg(..))


viewItemMenuOverlay : UiState -> SessionView -> Html Msg
viewItemMenuOverlay ui session =
    let
        allItems =
            session.items ++ session.tempItems ++ session.mercItems ++ session.mercReplacedItems

        findItem idx =
            allItems |> List.filter (\it -> it.index == idx) |> List.head

        stopClick =
            custom "click" (Decode.succeed { message = CloseItemMenu, stopPropagation = True, preventDefault = False })
    in
    case ui.itemMenu of
        NoItemMenu ->
            text ""

        ActionPaletteMenu { itemIndex, x, y } ->
            case findItem itemIndex of
                Nothing ->
                    text ""

                Just item ->
                    let
                        posLeft =
                            String.fromInt (max 10 (min x (Maybe.map (\m -> m.vw - 235) ui.hoverMousePos |> Maybe.withDefault 1100))) ++ "px"

                        posTop =
                            String.fromInt (max 10 (min y (Maybe.map (\m -> m.vh - 280) ui.hoverMousePos |> Maybe.withDefault 620))) ++ "px"

                        actionRow testSlug icon labelText kbdText clickMsg =
                            div
                                [ class "action-palette-item"
                                , attribute "data-testid" ("palette-action-" ++ testSlug)
                                , onClick clickMsg
                                ]
                                [ div [ class "action-palette-left" ]
                                    [ span [] [ text icon ]
                                    , span [] [ text labelText ]
                                    ]
                                , span [ class "action-kbd" ] [ text kbdText ]
                                ]

                        destActions =
                            case item.container of
                                Inventory ->
                                    [ actionRow "stash" "📦" "Move to Stash" "S" (QuickMoveItem item.index Stash)
                                    , actionRow "temp" "📥" "Move to Temp Inv" "T" (QuickMoveItem item.index TempInventory)
                                    , actionRow "equip" "🥋" "Equip to Character" "E" (QuickMoveItem item.index Equipment)
                                    , actionRow "merc" "🛡️" "Equip to Mercenary" "R" (QuickMoveItem item.index MercEquipment)
                                    , actionRow "cube" "🔮" "Move to Cube" "Q" (QuickMoveItem item.index Cube)
                                    ]

                                Stash ->
                                    [ actionRow "inventory" "🎒" "Move to Inventory" "A / S" (QuickMoveItem item.index Inventory)
                                    , actionRow "temp" "📥" "Move to Temp Inv" "T" (QuickMoveItem item.index TempInventory)
                                    , actionRow "equip" "🥋" "Equip to Character" "E" (QuickMoveItem item.index Equipment)
                                    , actionRow "merc" "🛡️" "Equip to Mercenary" "R" (QuickMoveItem item.index MercEquipment)
                                    , actionRow "cube" "🔮" "Move to Cube" "Q" (QuickMoveItem item.index Cube)
                                    ]

                                Equipment ->
                                    [ actionRow "inventory" "🎒" "Unequip to Inventory" "A / E" (QuickMoveItem item.index Inventory)
                                    , actionRow "stash" "📦" "Move to Stash" "S" (QuickMoveItem item.index Stash)
                                    , actionRow "temp" "📥" "Move to Temp Inv" "T" (QuickMoveItem item.index TempInventory)
                                    , actionRow "merc" "🛡️" "Give to Mercenary" "R" (QuickMoveItem item.index MercEquipment)
                                    ]

                                MercEquipment ->
                                    [ actionRow "inventory" "🎒" "Reclaim to Inventory" "A / R" (QuickMoveItem item.index Inventory)
                                    , actionRow "stash" "📦" "Move to Stash" "S" (QuickMoveItem item.index Stash)
                                    , actionRow "temp" "📥" "Move to Temp Inv" "T" (QuickMoveItem item.index TempInventory)
                                    , actionRow "equip" "🥋" "Equip to Character" "E" (QuickMoveItem item.index Equipment)
                                    ]

                                MercReplaced ->
                                    [ actionRow "inventory" "🎒" "Reclaim to Inventory" "A / R" (QuickMoveItem item.index Inventory)
                                    , actionRow "stash" "📦" "Move to Stash" "S" (QuickMoveItem item.index Stash)
                                    , actionRow "temp" "📥" "Move to Temp Inv" "T" (QuickMoveItem item.index TempInventory)
                                    , actionRow "equip" "🥋" "Equip to Character" "E" (QuickMoveItem item.index Equipment)
                                    ]

                                Cube ->
                                    [ actionRow "inventory" "🎒" "Take to Inventory" "A / Q" (QuickMoveItem item.index Inventory)
                                    , actionRow "stash" "📦" "Move to Stash" "S" (QuickMoveItem item.index Stash)
                                    , actionRow "temp" "📥" "Move to Temp Inv" "T" (QuickMoveItem item.index TempInventory)
                                    ]

                                Belt ->
                                    [ actionRow "inventory" "🎒" "Move to Inventory" "A" (QuickMoveItem item.index Inventory)
                                    , actionRow "stash" "📦" "Move to Stash" "S" (QuickMoveItem item.index Stash)
                                    , actionRow "temp" "📥" "Move to Temp Inv" "T" (QuickMoveItem item.index TempInventory)
                                    ]

                                TempInventory ->
                                    [ actionRow "stash" "📦" "Move to Stash" "S" (QuickMoveItem item.index Stash)
                                    , actionRow "inventory" "🎒" "Move to Inventory" "A / T" (QuickMoveItem item.index Inventory)
                                    , actionRow "equip" "🥋" "Equip to Character" "E" (QuickMoveItem item.index Equipment)
                                    , actionRow "merc" "🛡️" "Equip to Mercenary" "R" (QuickMoveItem item.index MercEquipment)
                                    , actionRow "cube" "🔮" "Move to Cube" "Q" (QuickMoveItem item.index Cube)
                                    ]

                                _ ->
                                    [ actionRow "inventory" "🎒" "Move to Inventory" "A" (QuickMoveItem item.index Inventory) ]
                    in
                    div
                        [ class "action-palette"
                        , attribute "data-testid" "item-action-palette"
                        , style "left" posLeft
                        , style "top" posTop
                        , custom "click" (Decode.succeed { message = CloseItemMenu, stopPropagation = True, preventDefault = False })
                        ]
                        (div [ class "action-palette-header" ]
                            [ span [] [ text "🎯 Quick Actions" ]
                            , span [ style "font-size" "10px", style "color" "var(--text-muted)", style "font-family" "monospace" ]
                                [ text (String.toUpper (UiState.containerLabel item.container)) ]
                            ]
                            :: actionRow "pick" "🖐️" "Pick Up Item" "Space" (SelectItem (Just item.index))
                            :: destActions
                        )

        ContextItemMenu { itemIndex, x, y } ->
            let
                posLeft =
                    String.fromInt (max 10 (min x (Maybe.map (\m -> m.vw - 175) ui.hoverMousePos |> Maybe.withDefault 1200))) ++ "px"

                posTop =
                    String.fromInt (max 10 (min y (Maybe.map (\m -> m.vh - 140) ui.hoverMousePos |> Maybe.withDefault 720))) ++ "px"

                _ =
                    stopClick
            in
            div
                [ class "context-menu"
                , attribute "data-testid" "item-context-menu"
                , style "left" posLeft
                , style "top" posTop
                , custom "click" (Decode.succeed { message = CloseItemMenu, stopPropagation = True, preventDefault = False })
                ]
                [ div
                    [ class "context-menu-item"
                    , attribute "data-testid" "context-action-edit"
                    , onClick (ContextEditItem itemIndex)
                    ]
                    [ text "⚒️ Forge Item (F)" ]
                , div
                    [ class "context-menu-item"
                    , attribute "data-testid" "context-action-duplicate"
                    , onClick (ContextDuplicateItem itemIndex)
                    ]
                    [ text "📋 Duplicate Item" ]
                , div
                    [ class "context-menu-item danger"
                    , attribute "data-testid" "context-action-delete"
                    , onClick (ContextDeleteItem itemIndex)
                    ]
                    [ text "🗑️ Delete Item" ]
                ]




viewShopBody : UiState -> SessionView -> Bool -> Bool -> Html Msg
viewShopBody ui session isHolding isShopDropHovered =
    let
        filteredByQuality =
            List.filter (\it -> it.quality == ui.shopQualityTab) session.shopCatalog

        filteredCatalog =
            if ui.shopCategoryFilter == "all" then
                filteredByQuality

            else
                List.filter (\it -> it.category == ui.shopCategoryFilter) filteredByQuality

        pages =
            packShopPages filteredCatalog

        totalPages =
            max 1 (List.length pages)

        clampedPage =
            clamp 0 (totalPages - 1) ui.shopPage

        currentPageItems =
            pages
                |> List.drop clampedPage
                |> List.head
                |> Maybe.withDefault []

        qualityTabs =
            [ ( QualityNormal, "Normal" )
            , ( QualityMagic, "Magic" )
            , ( QualityRare, "Rare" )
            , ( QualitySet, "Set" )
            , ( QualityUnique, "Unique" )
            ]

        categoryPills =
            [ ( "all", "All" )
            , ( "consumables", "Consumables" )
            , ( "runes_gems", "Runes & Gems" )
            , ( "weapons", "Weapons" )
            , ( "armor", "Armor" )
            , ( "jewelry_charms", "Jewelry & Charms" )
            ]
    in
    div [ class "shop-interactive-body" ]
        [ div [ class "shop-quality-tabs" ]
            (List.map
                (\( qKey, qLabel ) ->
                    button
                        [ classList
                            [ ( "shop-tab-btn", True )
                            , ( "active", ui.shopQualityTab == qKey )
                            ]
                        , attribute "data-testid" ("shop-tab-" ++ String.toLower qLabel)
                        , onClick (SelectShopQualityTab qKey)
                        ]
                        [ text qLabel ]
                )
                qualityTabs
            )
        , div [ class "shop-controls-bar" ]
            [ div [ class "shop-tier-pills" ]
                (List.map
                    (\( cKey, cLabel ) ->
                        button
                            [ classList
                                [ ( "pill-btn", True )
                                , ( "active", ui.shopCategoryFilter == cKey )
                                ]
                            , attribute "data-testid" ("shop-cat-" ++ cKey)
                            , onClick (SelectShopCategory cKey)
                            ]
                            [ text cLabel ]
                    )
                    categoryPills
                )
            ]
        , div [ class "d2-grid-wrap" ]
            [ div
                ([ attribute "data-testid" "shop-dispose-grid"
                 , classList
                    [ ( "d2-grid", True )
                    , ( "grid-10x6", True )
                    , ( "shop-sell-drop-active", isHolding && isShopDropHovered )
                    ]
                 ]
                    ++ (if isHolding then
                            [ onClick ClickDisposeHeldItem
                            , onMouseEnter (HoverDropTarget (Just { container = Shop, x = 0, y = 0 }))
                            , onMouseLeave (HoverDropTarget Nothing)
                            ]

                        else
                            []
                       )
                )
                (gridBackgroundCells 10 6
                    ++ List.map (viewShopCatalogTile isHolding) currentPageItems
                    ++ (if isHolding then
                            [ div [ class "shop-sell-overlay", attribute "data-testid" "shop-sell-overlay" ]
                                [ text "Drop Held Item Here to Dispose / Sell (Undo Supported)" ]
                            ]

                        else
                            []
                       )
                )
            ]
        , div [ class "shop-pagination-bar" ]
            [ span [ class "meta-line", attribute "data-testid" "shop-page-indicator" ]
                [ text
                    ("Page "
                        ++ String.fromInt (clampedPage + 1)
                        ++ " / "
                        ++ String.fromInt totalPages
                        ++ " ("
                        ++ String.fromInt (List.length filteredCatalog)
                        ++ " items)"
                    )
                ]
            , div [ class "shop-page-actions" ]
                [ button
                    [ class "btn-ghost btn-tiny"
                    , attribute "data-testid" "shop-prev-btn"
                    , disabled (clampedPage <= 0)
                    , onClick (ChangeShopPage -1)
                    ]
                    [ text "Prev" ]
                , button
                    [ class "btn-ghost btn-tiny"
                    , attribute "data-testid" "shop-next-btn"
                    , disabled (clampedPage >= totalPages - 1)
                    , onClick (ChangeShopPage 1)
                    ]
                    [ text "Next" ]
                ]
            ]
        ]


type alias PlacedShopTile =
    { item : ShopCatalogItemView
    , x : Int
    , y : Int
    }


packShopPages : List ShopCatalogItemView -> List (List PlacedShopTile)
packShopPages items =
    packShopPagesStep items [] []


packShopPagesStep : List ShopCatalogItemView -> List PlacedShopTile -> List (List PlacedShopTile) -> List (List PlacedShopTile)
packShopPagesStep remaining currentPage accPages =
    case remaining of
        [] ->
            if List.isEmpty currentPage && not (List.isEmpty accPages) then
                List.reverse accPages

            else
                List.reverse (List.reverse currentPage :: accPages)

        item :: rest ->
            case findShopFreeSlot item.width item.height currentPage of
                Just ( px, py ) ->
                    packShopPagesStep rest ({ item = item, x = px, y = py } :: currentPage) accPages

                Nothing ->
                    if List.isEmpty currentPage then
                        -- Oversized fallback: drop item to avoid infinite recursion
                        packShopPagesStep rest [] accPages

                    else
                        packShopPagesStep remaining [] (List.reverse currentPage :: accPages)


findShopFreeSlot : Int -> Int -> List PlacedShopTile -> Maybe ( Int, Int )
findShopFreeSlot w h placed =
    let
        coords =
            List.concatMap (\y -> List.map (\x -> ( x, y )) (List.range 0 (10 - w))) (List.range 0 (6 - h))

        fits ( x, y ) =
            not
                (List.any
                    (\p ->
                        x < p.x + p.item.width
                            && x + w > p.x
                            && y < p.y + p.item.height
                            && y + h > p.y
                    )
                    placed
                )
    in
    coords |> List.filter fits |> List.head


viewShopCatalogTile : Bool -> PlacedShopTile -> Html Msg
viewShopCatalogTile isHolding { item, x, y } =
    let
        cellPx =
            26

        tileContent =
            case item.imageUrl of
                Just url ->
                    [ img [ class "item-tile-img", src url, alt item.nameEn ] []
                    , span [ class "item-tile-code-badge" ] [ text item.code ]
                    ]

                Nothing ->
                    [ div [ class "item-tile-title" ] [ text item.nameEn ]
                    , div [ class "item-tile-code" ] [ text item.code ]
                    ]
    in
    div
        ([ classList
            [ ( "placed-item", True )
            , ( "shop-catalog-tile", True )
            , ( qualityClass item.quality, True )
            ]
         , attribute "data-testid" ("shop-item-" ++ item.catalogId)
         , title (item.nameEn ++ " (" ++ item.summary ++ ") - Click to add to Inventory")
         , style "left" (String.fromInt (x * cellPx + 2) ++ "px")
         , style "top" (String.fromInt (y * cellPx + 2) ++ "px")
         , style "width" (String.fromInt (item.width * cellPx - 4) ++ "px")
         , style "height" (String.fromInt (item.height * cellPx - 4) ++ "px")
         ]
            ++ (if isHolding then
                    []

                else
                    [ onClick (ClickSpawnShopItem item.catalogId) ]
               )
        )
        tileContent


stashTabBtn : StashTab -> StashTab -> String -> Bool -> Html Msg
stashTabBtn activeTab targetTab label isShared =
    button
        [ classList
            [ ( "stash-tab-btn", True )
            , ( "tab-personal", targetTab == PersonalStash )
            , ( "tab-shared", isShared )
            , ( "is-unloaded", isShared )
            , ( "active", activeTab == targetTab )
            ]
        , disabled isShared
        , title
            (if isShared then
                "Preview only · Shared stash files require multi-file host slice"

             else
                "Active save personal stash"
            )
        , onClick (SelectStashTab targetTab)
        ]
        [ text label ]


