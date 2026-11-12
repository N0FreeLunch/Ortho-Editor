module Component.ContainersPanel exposing (view)

import Generated.Protocol exposing (CharacterStatsView, ContainerCountsView, ItemCardView, MercSummaryView, SessionView, SetBonusView)
import Html exposing (Html, button, div, h2, h3, img, li, section, span, text, ul)
import Html.Attributes exposing (alt, attribute, class, classList, disabled, src, style, title)
import Html.Events exposing (custom, on, onClick, onMouseEnter, onMouseLeave)
import Json.Decode as Decode
import State.UiState exposing (StashTab(..), UiState)
import Update exposing (Msg(..))


view : UiState -> SessionView -> Html Msg
view ui session =
    let
        stashItems =
            List.filter (\it -> it.container == "stash") session.items

        inventoryItems =
            List.filter (\it -> it.container == "inventory") session.items

        cubeItems =
            List.filter (\it -> it.container == "cube") session.items

        beltItems =
            List.filter (\it -> it.container == "belt") session.items

        equippedItems =
            List.filter (\it -> it.container == "equipment") session.items

        tempInventoryItems =
            List.filter (\it -> it.container == "temp_inventory") session.items

        selectedMoveItem =
            session.selectedMoveItemIndex
                |> Maybe.andThen
                    (\idx ->
                        session.items
                            |> List.filter (\it -> it.index == idx)
                            |> List.head
                    )
    in
    let
        isHolding =
            selectedMoveItem /= Nothing

        trackMouse =
            ui.hoveredItemIndex /= Nothing || isHolding
    in
    div
        ([ classList
            [ ( "viewa-layout", True )
            , ( "is-holding-item", isHolding )
            ]
         , attribute "data-testid" "view-a-containers"
         ]
            ++ (if trackMouse then
                    [ on "mousemove"
                        (Decode.map4
                            (\cx cy maybeVw maybeVh ->
                                MoveTooltipHover
                                    (round cx)
                                    (round cy)
                                    (round (Maybe.withDefault 1440 maybeVw))
                                    (round (Maybe.withDefault 900 maybeVh))
                            )
                            (Decode.field "clientX" Decode.float)
                            (Decode.field "clientY" Decode.float)
                            (Decode.maybe (Decode.at [ "view", "innerWidth" ] Decode.float))
                            (Decode.maybe (Decode.at [ "view", "innerHeight" ] Decode.float))
                        )
                    ]

                else
                    []
               )
            ++ (if isHolding then
                    [ custom "contextmenu"
                        (Decode.succeed
                            { message = SelectItem Nothing
                            , stopPropagation = True
                            , preventDefault = True
                            }
                        )
                    ]

                else
                    []
               )
        )
        [ viewMoveModeBar selectedMoveItem session.beltMoveRestrictedReason
        , div [ class "studio-grid" ]
            [ viewZone1LeftStack ui session stashItems
            , viewZone2CenterStack ui session equippedItems tempInventoryItems
            , viewZone3RightStack ui session inventoryItems cubeItems beltItems
            ]
        , viewSelectedItemInspectorAndBrowser ui session
        , viewHeldItemGhost selectedMoveItem ui.hoverMousePos
        , if isHolding then
            text ""

          else
            viewHoverTooltip ui.hoveredItemIndex ui.hoverMousePos (session.items ++ session.tempItems ++ session.mercItems)
        ]


viewMoveModeBar : Maybe ItemCardView -> Maybe String -> Html Msg
viewMoveModeBar maybeSelected beltRestrictedReason =
    case maybeSelected of
        Nothing ->
            div [ class "move-mode-bar is-idle", attribute "data-testid" "move-mode-bar" ]
                [ div [ class "move-mode-meta" ]
                    [ span [ class "meta-line" ]
                        [ text "Ready · Click any item to pick up, or click a slot to place / swap" ]
                    ]
                ]

        Just sel ->
            div [ class "move-mode-bar", attribute "data-testid" "move-mode-bar" ]
                [ div [ class "move-mode-meta" ]
                    [ Html.strong [] [ text ("Holding: " ++ sel.nameEn) ]
                    , span [ class "meta-line" ]
                        [ text
                            ("("
                                ++ String.fromInt sel.width
                                ++ "x"
                                ++ String.fromInt sel.height
                                ++ ") · Click any slot to drop (Rust validates; Right-click to cancel)"
                            )
                        ]
                    , case beltRestrictedReason of
                        Just _ ->
                            span [ class "move-mode-restricted-pill", attribute "data-testid" "belt-restricted-pill" ]
                                [ text "Belt restricted (consumables only)" ]

                        Nothing ->
                            text ""
                    ]
                , button [ class "btn-ghost", attribute "data-testid" "cancel-move-btn", onClick (SelectItem Nothing) ]
                    [ text "Cancel" ]
                ]


viewZone1LeftStack : UiState -> SessionView -> List ItemCardView -> Html Msg
viewZone1LeftStack ui session stashItems =
    let
        isHolding =
            session.selectedMoveItemIndex /= Nothing

        isShopDropHovered =
            case ui.hoveredDropTarget of
                Just t ->
                    t.container == "shop"

                Nothing ->
                    False
    in
    div [ class "studio-col1-stack" ]
        [ section [ class "glass-panel zone1-stash-panel" ]
            [ div [ class "panel-header" ]
                [ div [ class "panel-title" ] [ text "Stash (Zone 1)" ]
                , div [ class "stash-tabs" ]
                    [ stashTabBtn ui.activeStashTab PersonalStash "Personal" False
                    , stashTabBtn ui.activeStashTab (SharedStash 1) "Shared 1" True
                    , stashTabBtn ui.activeStashTab (SharedStash 2) "Shared 2" True
                    , stashTabBtn ui.activeStashTab (SharedStash 3) "Shared 3" True
                    ]
                ]
            , div [ class "stash-meta-bar" ]
                [ span [] [ text "Personal Stash (10 x 10)" ]
                , span [ class "stash-gold" ] [ text ("Gold: " ++ String.fromInt session.stats.goldStash) ]
                ]
            , viewSpatialGrid "stash" 10 10 "grid-10x10" ui session stashItems
            ]
        , section [ class "glass-panel zone1-shop-panel", attribute "data-testid" "item-shop-preview" ]
            [ div [ class "panel-header" ]
                [ div [ class "panel-title" ] [ text "Item Shop" ]
                , span [ class "preview-only-pill" ] [ text "Preview only · Item spawn pending" ]
                ]
            , div [ class "shop-quality-tabs" ]
                [ button [ class "shop-tab-btn", disabled True ] [ text "Normal" ]
                , button [ class "shop-tab-btn", disabled True ] [ text "Magic" ]
                , button [ class "shop-tab-btn", disabled True ] [ text "Rare" ]
                , button [ class "shop-tab-btn", disabled True ] [ text "Set" ]
                , button [ class "shop-tab-btn", disabled True ] [ text "Unique" ]
                ]
            , div [ class "shop-preview-note" ]
                [ text "Shop catalog spawning requires a dedicated Rust template intent slice." ]
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
                                , onMouseEnter (HoverDropTarget (Just { container = "shop", x = 0, y = 0 }))
                                , onMouseLeave (HoverDropTarget Nothing)
                                ]

                            else
                                []
                           )
                    )
                    (gridBackgroundCells 10 6
                        ++ (if isHolding then
                                [ div [ class "shop-sell-overlay", attribute "data-testid" "shop-sell-overlay" ]
                                    [ text "Drop Held Item Here to Dispose / Sell (Undo Supported)" ]
                                ]

                            else
                                []
                           )
                    )
                ]
            ]
        ]


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


viewZone2CenterStack : UiState -> SessionView -> List ItemCardView -> List ItemCardView -> Html Msg
viewZone2CenterStack ui session equippedItems tempInventoryItems =
    let
        allTempCount =
            List.length tempInventoryItems + List.length session.tempItems

        tempGridView =
            if List.isEmpty tempInventoryItems && not (List.isEmpty session.tempItems) then
                viewTempSpatialGrid ui.selectedItemIndex session.tempItems

            else
                viewSpatialGrid "temp_inventory" 9 4 "grid-9x4" ui session tempInventoryItems
    in
    div [ class "studio-col2-stack" ]
        [ section [ class "glass-panel zone2-paperdoll-panel", attribute "data-testid" "paperdoll-panel" ]
            [ div [ class "panel-header" ]
                [ div [ class "panel-title" ] [ text "Equipped Gear (Zone 2)" ]
                , span [ class "preview-only-pill" ] [ text (String.fromInt (List.length equippedItems) ++ " equipped") ]
                ]
            , viewPaperdollGrid ui session equippedItems
            , viewGearSummaryBar session.stats session.containerCounts
            ]
        , section [ class "glass-panel zone2-temp-inv-panel", attribute "data-testid" "temp-inventory-panel" ]
            [ div [ class "panel-header" ]
                [ div [ class "panel-title" ]
                    [ text "Temp Inventory (9x4)"
                    , span [ class "temp-inv-badge", attribute "data-testid" "temp-capacity-badge" ]
                        [ text (String.fromInt allTempCount ++ " / 36 slots occupied") ]
                    ]
                , button
                    [ class "btn-danger-ghost"
                    , attribute "data-testid" "clear-temp-btn"
                    , disabled (allTempCount == 0)
                    , onClick ClearTempInventory
                    ]
                    [ text "Clear" ]
                ]
            , div [ class "temp-inv-meta-bar" ]
                [ span [] [ text "Drop staging area (9x4). Move items individually or Clear." ] ]
            , tempGridView
            ]
        ]


viewPaperdollGrid : UiState -> SessionView -> List ItemCardView -> Html Msg
viewPaperdollGrid ui session equippedItems =
    let
        slotCard =
            paperdollSlot ui session equippedItems
    in
    div [ class "paperdoll-layout" ]
        [ div
            [ class "doll-slot"
            , style "grid-column" "1 / span 2"
            , style "grid-row" "1 / span 2"
            , style "padding" "0"
            ]
            [ button
                [ classList
                    [ ( "doll-swap-btn", True )
                    , ( "is-swap-active", session.activeWeaponSet == 2 )
                    ]
                , attribute "data-testid" "weapon-swap-preview-btn"
                , onClick ClickToggleWeaponSwap
                , title "Toggle Weapon Set I / II"
                ]
                [ text "Swap I / II"
                , span [ class "meta-line", attribute "data-testid" "active-weapon-set-label" ]
                    [ text
                        (if session.activeWeaponSet == 2 then
                            "Set II"

                         else
                            "Set I"
                        )
                    ]
                ]
            ]
        , slotCard 1 "3 / span 2" "1 / span 2" "Head (2x2)" [ "Head", "Helm" ]
        , div
            [ style "grid-column" "5 / span 2"
            , style "grid-row" "1 / span 1"
            , style "display" "flex"
            , style "align-items" "center"
            , style "justify-content" "center"
            ]
            [ span [ class "preview-only-pill" ] [ text "2D Cards" ] ]
        , slotCard 2 "5 / span 1" "2 / span 1" "Amulet" [ "Amulet", "Neck" ]
        , if session.activeWeaponSet == 2 then
            slotCard 11 "1 / span 2" "3 / span 4" "Weapon II (2x4)" [ "Weapon1Swap" ]

          else
            slotCard 4 "1 / span 2" "3 / span 4" "Weapon (2x4)" [ "Weapon1", "Right Hand" ]
        , slotCard 3 "3 / span 2" "3 / span 4" "Armor (2x4)" [ "Armor", "Torso", "Body" ]
        , if session.activeWeaponSet == 2 then
            slotCard 12 "5 / span 2" "3 / span 4" "Off-Hand II (2x4)" [ "Weapon2Swap" ]

          else
            slotCard 5 "5 / span 2" "3 / span 4" "Off-Hand (2x4)" [ "Weapon2", "Left Hand", "Shield" ]
        , slotCard 10 "1 / span 2" "7 / span 2" "Gloves (2x2)" [ "Gloves", "Hands" ]
        , slotCard 8 "3 / span 2" "7 / span 1" "Belt (2x1)" [ "Belt", "Waist" ]
        , slotCard 6 "3 / span 1" "8 / span 1" "Ring 1" [ "Ring1", "Left Finger" ]
        , slotCard 7 "4 / span 1" "8 / span 1" "Ring 2" [ "Ring2", "Right Finger" ]
        , slotCard 9 "5 / span 2" "7 / span 2" "Boots (2x2)" [ "Boots", "Feet" ]
        ]


paperdollSlot : UiState -> SessionView -> List ItemCardView -> Int -> String -> String -> String -> List String -> Html Msg
paperdollSlot ui session items bodyLoc colSpec rowSpec emptyLabel acceptedLabels =
    let
        selectedIdx =
            ui.selectedItemIndex

        moveActive =
            session.selectedMoveItemIndex /= Nothing

        isValidEquipTarget =
            moveActive
                && List.any
                    (\t -> t.container == "equipment" && t.x == bodyLoc && t.y == 0)
                    session.allowedMoveTargets

        isHoveredEquipSlot =
            case ui.hoveredDropTarget of
                Just target ->
                    target.container == "equipment" && target.x == bodyLoc

                Nothing ->
                    False

        matched =
            items
                |> List.filter
                    (\it ->
                        case it.slotLabel of
                            Just lbl ->
                                List.member lbl acceptedLabels

                            Nothing ->
                                False
                    )
                |> List.head

        equipHoverAttrs =
            if moveActive then
                [ onMouseEnter (HoverDropTarget (Just { container = "equipment", x = bodyLoc, y = 0 }))
                , onMouseLeave (HoverDropTarget Nothing)
                ]

            else
                []
    in
    case matched of
        Nothing ->
            div
                ([ classList
                    [ ( "doll-slot", True )
                    , ( "footprint-valid", isHoveredEquipSlot && isValidEquipTarget )
                    , ( "footprint-invalid", moveActive && isHoveredEquipSlot && not isValidEquipTarget )
                    ]
                 , attribute "data-testid" ("equip-slot-" ++ String.fromInt bodyLoc)
                 , style "grid-column" colSpec
                 , style "grid-row" rowSpec
                 ]
                    ++ (if moveActive then
                            onClick (MoveSelectedItemTo "equipment" bodyLoc 0) :: equipHoverAttrs

                        else
                            []
                       )
                )
                [ span [ class "doll-slot-tag" ] [ text emptyLabel ] ]

        Just item ->
            let
                isSelected =
                    selectedIdx == Just item.index

                isHeldSource =
                    session.selectedMoveItemIndex == Just item.index

                slotClickMsg =
                    case session.selectedMoveItemIndex of
                        Just heldIdx ->
                            if heldIdx == item.index then
                                SelectItem Nothing

                            else
                                MoveSelectedItemTo "equipment" bodyLoc 0

                        Nothing ->
                            SelectItem (Just item.index)
            in
            div
                ([ classList
                    [ ( "doll-slot", True )
                    , ( "filled", True )
                    , ( "is-selected", isSelected )
                    , ( "is-held-source", isHeldSource )
                    , ( "footprint-valid", isHoveredEquipSlot && isValidEquipTarget )
                    , ( "footprint-invalid", moveActive && isHoveredEquipSlot && not isValidEquipTarget )
                    , ( qualityClass item.quality, True )
                    ]
                 , attribute "data-testid" ("equip-slot-" ++ String.fromInt bodyLoc)
                 , style "grid-column" colSpec
                 , style "grid-row" rowSpec
                 , onClick slotClickMsg
                 ]
                    ++ (if moveActive then
                            equipHoverAttrs

                        else
                            [ onMouseEnter (HoverItem (Just item.index))
                            , onMouseLeave (HoverItem Nothing)
                            ]
                       )
                )
                (span [ class "doll-slot-tag" ] [ text emptyLabel ] :: viewItemTileGraphic item)


viewGearSummaryBar : CharacterStatsView -> ContainerCountsView -> Html Msg
viewGearSummaryBar stats counts =
    div [ class "gear-breakpoint-summary-bar", attribute "data-testid" "gear-breakpoint-bar" ]
        [ bpMetricCell "bp-fcr" "FCR" (String.fromInt stats.fcr ++ "%")
        , bpMetricCell "bp-fhr" "FHR" (String.fromInt stats.fhr ++ "%")
        , bpMetricCell "bp-frw" "FRW" (String.fromInt stats.frw ++ "%")
        , bpMetricCell "bp-ias" "IAS" (String.fromInt stats.ias ++ "%")
        , bpMetricCell "bp-mf" "MF" (String.fromInt stats.magicFind ++ "%")
        , bpMetricCell "bp-skills" "+Skills" ("+" ++ String.fromInt stats.allSkillsBonus)
        , summaryCell "Equip" counts.equipment
        , summaryCell "Inv" counts.inventory
        , summaryCell "Belt" counts.belt
        , summaryCell "Stash" counts.stash
        , summaryCell "Cube" counts.cube
        , summaryCell "Temp" counts.tempInventory
        ]


bpMetricCell : String -> String -> String -> Html Msg
bpMetricCell testIdStr label valStr =
    div [ class "bp-item bp-metric", attribute "data-testid" testIdStr ]
        [ span [ class "bp-label" ] [ text label ]
        , span [ class "bp-val" ] [ text valStr ]
        ]


summaryCell : String -> Int -> Html Msg
summaryCell label val =
    div [ class "bp-item" ]
        [ span [ class "bp-label" ] [ text label ]
        , span [ class "bp-val" ] [ text (String.fromInt val) ]
        ]


viewZone3RightStack :
    UiState
    -> SessionView
    -> List ItemCardView
    -> List ItemCardView
    -> List ItemCardView
    -> Html Msg
viewZone3RightStack ui session inventoryItems cubeItems beltItems =
    div [ class "studio-col3-stack" ]
        [ section [ class "glass-panel zone3-inventory-panel" ]
            [ div [ class "panel-header" ]
                [ div [ class "panel-title" ] [ text "Inventory (10 x 4)" ]
                , span [ class "meta-line" ] [ text ("Items: " ++ String.fromInt (List.length inventoryItems)) ]
                ]
            , viewSpatialGrid "inventory" 10 4 "grid-10x4" ui session inventoryItems
            ]
        , section [ class "glass-panel zone3-utility-panel" ]
            [ div [ class "panel-header" ]
                [ div [ class "panel-title" ] [ text "Utility Dock" ]
                , span [ class "meta-line" ] [ text "Cube & Belt" ]
                ]
            , div [ class "utility-dock" ]
                [ div [ class "dock-card" ]
                    [ div [ class "dock-card-header" ] [ text "Horadric Cube (3x4)" ]
                    , viewSpatialGrid "cube" 3 4 "grid-3x4" ui session cubeItems
                    ]
                , div [ class "dock-card" ]
                    [ div [ class "dock-card-header" ] [ text "Belt (4x4)" ]
                    , viewSpatialGrid "belt" 4 4 "grid-4x4" ui session beltItems
                    ]
                ]
            ]
        , viewZone4MercenaryDualStage session.merc session.mercItems
        ]


viewZone4MercenaryDualStage : MercSummaryView -> List ItemCardView -> Html Msg
viewZone4MercenaryDualStage merc mercItems =
    section [ class "glass-panel merc-dual-column", attribute "data-testid" "view-a-merc-panel" ]
        [ div [ class "panel-header" ]
            [ div [ class "panel-title" ] [ text "Mercenary Station (Zone 4)" ]
            , span [ class "readonly-badge" ] [ text "Read-only" ]
            ]
        , div [ class "merc-stages-horizontal" ]
            [ div [ class "merc-stage-card stage-active" ]
                [ div [ class "merc-stage-header" ]
                    [ span [ class "merc-stage-title" ] [ text "Active Mercenary" ]
                    , span [ class "merc-stage-badge badge-active-target" ] [ text "Active" ]
                    ]
                , div [ class "merc-stage-desc" ]
                    [ text
                        (if merc.hired then
                            "Lv " ++ String.fromInt merc.stats.level ++ " " ++ merc.className ++ " · " ++ merc.stats.innateAura

                         else
                            "No mercenary hired"
                        )
                    ]
                , viewMercGridProportional mercItems
                ]
            , div [ class "merc-stage-card stage-dismissed" ]
                [ div [ class "merc-stage-header" ]
                    [ span [ class "merc-stage-title" ] [ text "Replaced Staging" ]
                    , span [ class "merc-stage-badge badge-discard-target" ] [ text "Preview" ]
                    ]
                , div [ class "merc-stage-desc" ]
                    [ text "Mercenary replacement staging requires a dedicated Rust merc-swap slice." ]
                , viewMercGridProportional []
                ]
            ]
        ]


viewMercGridProportional : List ItemCardView -> Html Msg
viewMercGridProportional items =
    let
        mercSlot colSpec rowSpec emptyLabel acceptedLabels =
            let
                matched =
                    items
                        |> List.filter
                            (\it ->
                                case it.slotLabel of
                                    Just lbl ->
                                        List.member lbl acceptedLabels

                                    Nothing ->
                                        False
                            )
                        |> List.head
            in
            case matched of
                Nothing ->
                    div [ class "merc-slot", style "grid-column" colSpec, style "grid-row" rowSpec ]
                        [ span [ class "doll-slot-tag" ] [ text emptyLabel ] ]

                Just item ->
                    div
                        [ classList [ ( "merc-slot", True ), ( "filled", True ), ( qualityClass item.quality, True ) ]
                        , style "grid-column" colSpec
                        , style "grid-row" rowSpec
                        , onMouseEnter (HoverItem (Just item.index))
                        , onMouseLeave (HoverItem Nothing)
                        ]
                        (viewItemTileGraphic item)
    in
    div [ class "merc-grid-proportional" ]
        [ mercSlot "1 / span 2" "1 / span 2" "Helm" [ "Helm", "Head" ]
        , mercSlot "1 / span 2" "3 / span 4" "Weapon" [ "Weapon1", "Right Hand" ]
        , mercSlot "3 / span 2" "1 / span 4" "Armor" [ "Armor", "Torso", "Body" ]
        , mercSlot "3 / span 2" "5 / span 4" "Off-Hand" [ "Weapon2", "Left Hand", "Shield" ]
        ]


viewTempSpatialGrid : Maybe Int -> List ItemCardView -> Html Msg
viewTempSpatialGrid selectedIdx items =
    let
        renderAt i item =
            let
                gx =
                    if item.x >= 0 && item.x < 9 then
                        item.x

                    else
                        modBy 9 i

                gy =
                    if item.y >= 0 && item.y < 4 then
                        item.y

                    else
                        clamp 0 3 (i // 9)

                wCells =
                    clamp 1 (9 - gx) (max 1 item.width)

                hCells =
                    clamp 1 (4 - gy) (max 1 item.height)

                isSelected =
                    selectedIdx == Just item.index
            in
            div
                [ classList
                    [ ( "placed-item", True )
                    , ( "is-selected", isSelected )
                    , ( qualityClass item.quality, True )
                    ]
                , style "left" (String.fromInt (gx * 40 + 2) ++ "px")
                , style "top" (String.fromInt (gy * 40 + 2) ++ "px")
                , style "width" (String.fromInt (wCells * 40 - 4) ++ "px")
                , style "height" (String.fromInt (hCells * 40 - 4) ++ "px")
                , attribute "data-testid" ("temp-item-" ++ String.fromInt item.index)
                , onClick (SelectItem (Just item.index))
                , onMouseEnter (HoverItem (Just item.index))
                , onMouseLeave (HoverItem Nothing)
                ]
                (viewItemTileGraphic item)
    in
    div [ class "d2-grid-wrap" ]
        [ div [ class "d2-grid grid-9x4" ]
            (gridBackgroundCells 9 4
                ++ (if List.isEmpty items then
                        [ div [ class "temp-empty-overlay" ]
                            [ text "Staging buffer empty (9x4)" ]
                        ]

                    else
                        List.indexedMap renderAt items
                   )
            )
        ]


viewSpatialGrid : String -> Int -> Int -> String -> UiState -> SessionView -> List ItemCardView -> Html Msg
viewSpatialGrid containerTag cols rows gridClass ui session items =
    let
        selectedIdx =
            ui.selectedItemIndex

        heldItem =
            session.selectedMoveItemIndex
                |> Maybe.andThen
                    (\idx ->
                        session.items
                            |> List.filter (\it -> it.index == idx)
                            |> List.head
                    )

        moveActive =
            heldItem /= Nothing

        beltRestricted =
            moveActive && containerTag == "belt" && session.beltMoveRestrictedReason /= Nothing

        isAllowedTarget col logicalY =
            List.any
                (\t -> t.container == containerTag && t.x == col && t.y == logicalY)
                session.allowedMoveTargets

        hoverFootprint =
            case ( heldItem, ui.hoveredDropTarget ) of
                ( Just item, Just target ) ->
                    if target.container == containerTag then
                        let
                            w =
                                max 1 item.width

                            h =
                                max 1 item.height

                            gx =
                                clamp 0 (max 0 (cols - w)) target.x

                            gy =
                                clamp 0 (max 0 (rows - h)) target.y

                            fitsBounds =
                                gx + w <= cols && gy + h <= rows

                            valid =
                                fitsBounds && not beltRestricted && isAllowedTarget gx gy
                        in
                        Just { gx = gx, gy = gy, w = w, h = h, fitsBounds = fitsBounds, valid = valid }

                    else
                        Nothing

                _ ->
                    Nothing

        renderCell row col =
            let
                logicalY =
                    if containerTag == "belt" then
                        rows - 1 - row

                    else
                        row
            in
            if moveActive then
                let
                    cellTestId =
                        attribute "data-testid" ("grid-cell-" ++ containerTag ++ "-" ++ String.fromInt col ++ "-" ++ String.fromInt logicalY)

                    dropAnchor =
                        case hoverFootprint of
                            Just fp ->
                                ( fp.gx, fp.gy )

                            Nothing ->
                                ( col, logicalY )

                    dropClick =
                        onClick (MoveSelectedItemTo containerTag (Tuple.first dropAnchor) (Tuple.second dropAnchor))

                    hoverEnter =
                        onMouseEnter (HoverDropTarget (Just { container = containerTag, x = col, y = logicalY }))

                    inFootprint =
                        case hoverFootprint of
                            Just fp ->
                                col >= fp.gx && col < fp.gx + fp.w && logicalY >= fp.gy && logicalY < fp.gy + fp.h

                            Nothing ->
                                False

                    fpValid =
                        Maybe.map .valid hoverFootprint |> Maybe.withDefault False
                in
                div
                    [ classList
                        [ ( "grid-cell", True )
                        , ( "is-restricted-target", beltRestricted )
                        , ( "is-valid-target", not beltRestricted && isAllowedTarget col logicalY )
                        , ( "is-blocked-target", not beltRestricted && not (isAllowedTarget col logicalY) )
                        , ( "cell-footprint-valid", inFootprint && fpValid )
                        , ( "cell-footprint-invalid", inFootprint && not fpValid )
                        ]
                    , cellTestId
                    , dropClick
                    , hoverEnter
                    ]
                    []

            else
                div [ class "grid-cell" ] []

        footprintPreviewNodes =
            case hoverFootprint of
                Just fp ->
                    if fp.fitsBounds then
                        let
                            visualY =
                                if containerTag == "belt" then
                                    rows - 1 - (fp.gy + fp.h - 1)

                                else
                                    fp.gy
                        in
                        [ div
                            [ classList
                                [ ( "grid-footprint-preview", True )
                                , ( "valid", fp.valid )
                                , ( "invalid", not fp.valid )
                                ]
                            , attribute "data-testid" ("grid-footprint-preview-" ++ containerTag)
                            , style "left" (String.fromInt (fp.gx * 40 + 1) ++ "px")
                            , style "top" (String.fromInt (visualY * 40 + 1) ++ "px")
                            , style "width" (String.fromInt (fp.w * 40 - 2) ++ "px")
                            , style "height" (String.fromInt (fp.h * 40 - 2) ++ "px")
                            ]
                            []
                        ]

                    else
                        []

                Nothing ->
                    []

        interactiveCells =
            List.concatMap
                (\row -> List.map (renderCell row) (List.range 0 (cols - 1)))
                (List.range 0 (rows - 1))

        renderPlaced item =
            let
                gx =
                    clamp 0 (cols - 1) item.x

                gy =
                    clamp 0 (rows - 1) item.y

                visualY =
                    if containerTag == "belt" then
                        rows - 1 - gy

                    else
                        gy

                wCells =
                    clamp 1 (cols - gx) (max 1 item.width)

                hCells =
                    clamp 1 (rows - gy) (max 1 item.height)

                isSelected =
                    selectedIdx == Just item.index

                isHeldSource =
                    session.selectedMoveItemIndex == Just item.index

                itemClickMsg =
                    case session.selectedMoveItemIndex of
                        Just heldIdx ->
                            if heldIdx == item.index then
                                SelectItem Nothing

                            else
                                MoveSelectedItemTo containerTag item.x item.y

                        Nothing ->
                            SelectItem (Just item.index)
            in
            div
                [ classList
                    [ ( "placed-item", True )
                    , ( "is-selected", isSelected )
                    , ( "is-held-source", isHeldSource )
                    , ( qualityClass item.quality, True )
                    ]
                , style "left" (String.fromInt (gx * 40 + 2) ++ "px")
                , style "top" (String.fromInt (visualY * 40 + 2) ++ "px")
                , style "width" (String.fromInt (wCells * 40 - 4) ++ "px")
                , style "height" (String.fromInt (hCells * 40 - 4) ++ "px")
                , attribute "data-testid" ("grid-item-" ++ String.fromInt item.index)
                , onClick itemClickMsg
                , onMouseEnter (HoverItem (Just item.index))
                , onMouseLeave (HoverItem Nothing)
                ]
                (viewItemTileGraphic item)
    in
    div [ class "d2-grid-wrap" ]
        [ div
            [ classList
                [ ( "d2-grid", True )
                , ( gridClass, True )
                , ( "is-restricted", beltRestricted )
                ]
            , attribute "data-testid" ("spatial-grid-" ++ containerTag)
            , onMouseLeave (HoverDropTarget Nothing)
            ]
            (interactiveCells ++ List.map renderPlaced items ++ footprintPreviewNodes)
        ]


gridBackgroundCells : Int -> Int -> List (Html Msg)
gridBackgroundCells cols rows =
    List.repeat (cols * rows) (div [ class "grid-cell" ] [])


viewSelectedItemInspectorAndBrowser : UiState -> SessionView -> Html Msg
viewSelectedItemInspectorAndBrowser ui session =
    let
        filteredItems =
            if ui.selectedContainerFilter == "all" then
                session.items

            else
                List.filter (\it -> it.container == ui.selectedContainerFilter) session.items

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
                        [ h3 [ class ("inspector-title " ++ qualityClass item.quality) ] [ text item.nameEn ]
                        , div [ class "meta-line" ]
                            [ text
                                (item.quality
                                    ++ " · ilvl "
                                    ++ String.fromInt item.itemLevel
                                    ++ " · "
                                    ++ item.container
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
                            ++ viewSetDetails session.items item
                )
            , div []
                [ div [ class "segmented", attribute "data-testid" "container-filters" ]
                    [ filterButton ui.selectedContainerFilter "all" "All"
                    , filterButton ui.selectedContainerFilter "equipment" "Equipment"
                    , filterButton ui.selectedContainerFilter "inventory" "Inventory"
                    , filterButton ui.selectedContainerFilter "belt" "Belt"
                    , filterButton ui.selectedContainerFilter "stash" "Stash"
                    , filterButton ui.selectedContainerFilter "cube" "Cube"
                    , filterButton ui.selectedContainerFilter "temp_inventory" "Temp"
                    ]
                , div [ class "item-card-grid", attribute "data-testid" "container-items" ]
                    (List.map (viewIndexTile ui.selectedItemIndex) filteredItems)
                ]
            ]
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


viewIndexTile : Maybe Int -> ItemCardView -> Html Msg
viewIndexTile selectedIdx item =
    let
        isSelected =
            selectedIdx == Just item.index
    in
    div
        [ classList
            [ ( "item-tile", True )
            , ( "is-selected", isSelected )
            , ( qualityClass item.quality, True )
            ]
        , attribute "data-testid" ("item-tile-" ++ String.fromInt item.index)
        , onClick (SelectItem (Just item.index))
        , onMouseEnter (HoverItem (Just item.index))
        , onMouseLeave (HoverItem Nothing)
        ]
        [ div [ class "item-tile-head" ]
            [ case item.imageUrl of
                Just url ->
                    img [ class "item-tile-thumb", src url, alt item.nameEn ] []

                Nothing ->
                    text ""
            , span [ class "item-name" ] [ text item.nameEn ]
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
        ]


qualityClass : String -> String
qualityClass quality =
    case String.toLower quality of
        "magic" ->
            "q-magic"

        "rare" ->
            "q-rare"

        "unique" ->
            "q-unique"

        "set" ->
            "q-set"

        "runeword" ->
            "q-runeword"

        "unknown" ->
            "q-unknown"

        _ ->
            "q-normal"


viewItemTileGraphic : ItemCardView -> List (Html Msg)
viewItemTileGraphic item =
    case item.imageUrl of
        Just url ->
            [ img
                [ class "item-tile-img"
                , src url
                , alt item.nameEn
                , title (item.nameEn ++ " (" ++ item.quality ++ ")")
                ]
                []
            ]

        Nothing ->
            [ span [ class "item-tile-name" ] [ text item.nameEn ]
            , span [ class "item-tile-type" ] [ text item.quality ]
            ]


viewHeldItemGhost : Maybe ItemCardView -> Maybe { x : Int, y : Int, vw : Int, vh : Int } -> Html Msg
viewHeldItemGhost maybeItem maybePos =
    case ( maybeItem, maybePos ) of
        ( Just item, Just pos ) ->
            let
                wPx =
                    max 1 item.width * 40 - 4

                hPx =
                    max 1 item.height * 40 - 4
            in
            div
                [ classList
                    [ ( "held-item-ghost", True )
                    , ( qualityClass item.quality, True )
                    ]
                , attribute "data-testid" "held-item-ghost"
                , style "left" (String.fromInt pos.x ++ "px")
                , style "top" (String.fromInt pos.y ++ "px")
                , style "width" (String.fromInt wPx ++ "px")
                , style "height" (String.fromInt hPx ++ "px")
                ]
                (viewItemTileGraphic item)

        _ ->
            text ""


viewHoverTooltip : Maybe Int -> Maybe { x : Int, y : Int, vw : Int, vh : Int } -> List ItemCardView -> Html Msg
viewHoverTooltip maybeIdx maybePos items =
    case maybeIdx |> Maybe.andThen (\idx -> List.filter (\it -> it.index == idx) items |> List.head) of
        Nothing ->
            text ""

        Just item ->
            let
                posAttrs =
                    case maybePos of
                        Just pos ->
                            let
                                approxWidth =
                                    360

                                bonusSectionHeight bonus =
                                    44 + (List.length bonus.lines * 20)

                                setGroupOverhead =
                                    case ( item.setName, List.isEmpty item.setBonuses, List.isEmpty item.setPieces ) of
                                        ( Nothing, True, True ) ->
                                            0

                                        _ ->
                                            24
                                                + (if item.setName == Nothing then
                                                    0

                                                   else
                                                    24
                                                  )
                                                + List.sum (List.map bonusSectionHeight item.setBonuses)
                                                + (if List.isEmpty item.setPieces then
                                                    0

                                                   else
                                                    24 + (((List.length item.setPieces + 1) // 2) * 24)
                                                  )

                                estimatedHeight =
                                    68
                                        + (List.length item.baseAttributes * 18)
                                        + (if List.isEmpty item.socketedNames then
                                            0

                                           else
                                            24
                                          )
                                        + (List.length item.properties * 18)
                                        + List.sum (List.map bonusSectionHeight item.pieceSetBonuses)
                                        + setGroupOverhead

                                horizontalAttrs =
                                    if pos.x + 16 + approxWidth <= pos.vw - 10 then
                                        [ style "left" (String.fromInt (pos.x + 16) ++ "px") ]

                                    else if pos.x - 16 - approxWidth >= 10 then
                                        [ style "right" (String.fromInt (max 10 (pos.vw - pos.x + 16)) ++ "px") ]

                                    else
                                        [ style "left" (String.fromInt (max 10 (pos.vw - approxWidth - 10)) ++ "px") ]

                                verticalAttrs =
                                    if pos.y + 16 + estimatedHeight <= pos.vh - 10 then
                                        [ style "top" (String.fromInt (max 10 (pos.y + 16)) ++ "px") ]

                                    else if pos.y - 14 - estimatedHeight >= 10 then
                                        [ style "bottom" (String.fromInt (max 10 (pos.vh - pos.y + 14)) ++ "px") ]

                                    else
                                        [ style "bottom" "10px" ]
                            in
                            horizontalAttrs ++ verticalAttrs

                        Nothing ->
                            [ style "right" "22px"
                            , style "bottom" "22px"
                            ]

                baseStatsNodes =
                    if List.isEmpty item.baseAttributes then
                        []

                    else
                        [ div [ class "tooltip-basestat" ]
                            (List.map (\line -> div [] [ text line ]) item.baseAttributes)
                        ]

                socketedNodes =
                    if List.isEmpty item.socketedNames then
                        []

                    else
                        [ div [ class "tooltip-runes" ]
                            [ text ("Socketed: " ++ String.join ", " item.socketedNames) ]
                        ]

                setDetailNodes =
                    viewSetDetails items item
            in
            div ([ class "item-tooltip", attribute "data-testid" "item-hover-tooltip" ] ++ posAttrs)
                ([ div [ class ("tooltip-title " ++ qualityClass item.quality) ] [ text item.nameEn ]
                 , div [ class "tooltip-base" ]
                    [ text
                        (item.quality
                            ++ " · ilvl "
                            ++ String.fromInt item.itemLevel
                            ++ " · "
                            ++ item.code
                            ++ " ("
                            ++ item.container
                            ++ " "
                            ++ String.fromInt item.x
                            ++ ","
                            ++ String.fromInt item.y
                            ++ ")"
                        )
                    ]
                 ]
                    ++ baseStatsNodes
                    ++ socketedNodes
                    ++ List.map (\prop -> div [ class "tooltip-prop" ] [ text prop ]) item.properties
                    ++ setDetailNodes
                )


viewSetDetails : List ItemCardView -> ItemCardView -> List (Html msg)
viewSetDetails allItems item =
    let
        equippedNames =
            allItems
                |> List.filter (\entry -> entry.container == "equipment" || entry.container == "merc_equipment")
                |> List.map .nameEn

        isPieceEquipped pieceName =
            List.any (\eqName -> eqName == pieceName || String.contains pieceName eqName) equippedNames

        renderBonusGroup unitLabel bonus =
            let
                groupClass =
                    if bonus.active then
                        "item-set-bonus is-active"

                    else
                        "item-set-bonus"

                stateLabel =
                    if bonus.active then
                        "Active"

                    else
                        "Inactive"

                optionClass =
                    if bonus.active then
                        "item-detail-set"

                    else
                        "item-detail-set item-detail-set-inactive"
            in
            Html.node "section"
                [ class groupClass ]
                [ div [ class "item-set-bonus-header" ]
                    [ div [ class "item-set-count" ]
                        [ text (String.fromInt bonus.requiredCount ++ " " ++ unitLabel) ]
                    , div [ class "item-detail-set-state" ] [ text stateLabel ]
                    ]
                , div [ class "item-set-options" ]
                    (List.map (\line -> div [ class optionClass ] [ text line ]) bonus.lines)
                ]

        pieceBonusNodes =
            List.map (renderBonusGroup "ITEMS") item.pieceSetBonuses

        setGroupNodes =
            case ( item.setName, List.isEmpty item.setBonuses, List.isEmpty item.setPieces ) of
                ( Nothing, True, True ) ->
                    []

                _ ->
                    let
                        headerNodes =
                            case item.setName of
                                Just name ->
                                    [ div [ class "tooltip-set-name" ] [ text name ] ]

                                Nothing ->
                                    []

                        bonusNodes =
                            List.map (renderBonusGroup "SET") item.setBonuses

                        pieceNodes =
                            if List.isEmpty item.setPieces then
                                []

                            else
                                [ div [ class "item-set-pieces" ]
                                    (List.map
                                        (\piece ->
                                            span
                                                [ classList
                                                    [ ( "item-set-piece-chip", True )
                                                    , ( "is-equipped", isPieceEquipped piece )
                                                    ]
                                                ]
                                                [ text piece ]
                                        )
                                        item.setPieces
                                    )
                                ]
                    in
                    [ div [ class "tooltip-set-group" ] (headerNodes ++ bonusNodes ++ pieceNodes) ]
    in
    pieceBonusNodes ++ setGroupNodes
