module Component.ContainersPanel exposing (view)

import Component.Containers.ForgeModal exposing (viewItemEditorModal)
import Component.Containers.Inspector exposing (viewSelectedItemInspectorAndBrowser)
import Component.Containers.Shared exposing (..)
import Component.Containers.ShopAndMenus exposing (stashTabBtn, viewItemMenuOverlay, viewShopBody)
import Generated.Protocol exposing (CharacterStatsView, ContainerCountsView, ContainerKind(..), ItemCardView, ItemQualityKind(..), MercSummaryView, PackedSkillSpecView, ParamCatalogView, ParamOptionView, SessionView, SetBonusView, ShopCatalogItemView, SocketedItemView, StatCatalogEntryView, StatParamKind(..))
import Html exposing (Html, button, div, h2, h3, img, input, li, option, section, select, span, text, ul)
import Html.Attributes exposing (alt, attribute, checked, class, classList, disabled, id, placeholder, readonly, selected, src, style, title, type_, value)
import Html.Events exposing (custom, on, onBlur, onClick, onFocus, onInput, onMouseEnter, onMouseLeave)
import Json.Decode as Decode
import State.UiState as UiState exposing (ItemMenuState(..), ModalNotice(..), StashTab(..), UiState)
import Update exposing (Msg(..))


view : UiState -> SessionView -> Html Msg
view ui session =
    let
        stashItems =
            List.filter (\it -> it.container == Stash) session.items

        inventoryItems =
            List.filter (\it -> it.container == Inventory) session.items

        cubeItems =
            List.filter (\it -> it.container == Cube) session.items

        beltItems =
            List.filter (\it -> it.container == Belt) session.items

        equippedItems =
            List.filter (\it -> it.container == Equipment) session.items

        tempInventoryItems =
            List.filter (\it -> it.container == TempInventory) session.items

        selectedMoveItem =
            findHeldItem session
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
            ++ (if ui.itemMenu /= NoItemMenu then
                    [ onClick CloseItemMenu ]

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
        , if isHolding || ui.itemMenu /= NoItemMenu then
            text ""

          else
            viewHoverTooltip ui.hoveredItemIndex ui.hoverMousePos (session.items ++ session.tempItems ++ session.mercItems ++ session.mercReplacedItems)
        , viewItemMenuOverlay ui session
        , viewItemEditorModal ui session
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
                    t.container == Shop

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
                , viewGoldInlineEditor ui "gold_stash" "Gold" session.stats.goldStash session.schema.goldStashMax "viewa-input-gold-stash"
                ]
            , viewSpatialGrid Stash 10 10 "grid-10x10" ui session stashItems
            ]
        , section [ class "glass-panel zone1-shop-panel", attribute "data-testid" "item-shop-preview" ]
            [ div [ class "panel-header" ]
                [ div [ class "panel-title" ] [ text "Item Shop" ]
                , span [ class "shop-live-pill" ] [ text "Live Spawn · Click to Add" ]
                ]
            , viewShopBody ui session isHolding isShopDropHovered
            ]
        ]




viewZone2CenterStack : UiState -> SessionView -> List ItemCardView -> List ItemCardView -> Html Msg
viewZone2CenterStack ui session equippedItems tempInventoryItems =
    let
        allTempCount =
            List.length tempInventoryItems + List.length session.tempItems

        tempGridView =
            if List.isEmpty tempInventoryItems && not (List.isEmpty session.tempItems) then
                viewTempSpatialGrid ui.selectedItemIndex session.tempItems

            else
                viewSpatialGrid TempInventory 9 4 "grid-9x4" ui session tempInventoryItems
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
    div
        ([ class "paperdoll-layout" ]
            ++ (if session.selectedMoveItemIndex /= Nothing then
                    [ onMouseLeave (HoverDropTarget Nothing) ]

                else
                    []
               )
        )
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
                    (\t -> t.container == Equipment && t.x == bodyLoc && t.y == 0)
                    session.allowedMoveTargets

        isHoveredEquipSlot =
            case ui.hoveredDropTarget of
                Just target ->
                    target.container == Equipment && target.x == bodyLoc

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
                [ onMouseEnter (HoverDropTarget (Just { container = Equipment, x = bodyLoc, y = 0 })) ]

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
                            onClick (MoveSelectedItemTo Equipment bodyLoc 0) :: equipHoverAttrs

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

                slotActionAttrs =
                    case session.selectedMoveItemIndex of
                        Just heldIdx ->
                            if heldIdx == item.index then
                                [ onClick (SelectItem Nothing) ]

                            else
                                [ onClick (MoveSelectedItemTo Equipment bodyLoc 0) ]

                        Nothing ->
                            itemMenuTriggerAttrs item.index
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
                 ]
                    ++ slotActionAttrs
                    ++ (if moveActive then
                            equipHoverAttrs

                        else
                            [ onMouseEnter (HoverItem (Just item.index))
                            , onMouseLeave (HoverItem Nothing)
                            ]
                       )
                )
                (span [ class "doll-slot-tag" ] [ text emptyLabel ] :: [ viewSizedEquipGraphic 40 40 item ])


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
                , div [ class "inv-header-meta" ]
                    [ viewGoldInlineEditor ui "gold_character" "Gold" session.stats.goldInventory session.schema.goldCharacterMax "viewa-input-gold-inventory"
                    , span [ class "meta-line" ] [ text ("Items: " ++ String.fromInt (List.length inventoryItems)) ]
                    ]
                ]
            , viewSpatialGrid Inventory 10 4 "grid-10x4" ui session inventoryItems
            ]
        , section [ class "glass-panel zone3-utility-panel" ]
            [ div [ class "panel-header" ]
                [ div [ class "panel-title" ] [ text "Utility Dock" ]
                , span [ class "meta-line" ] [ text "Cube & Belt" ]
                ]
            , div [ class "utility-dock" ]
                [ div [ class "dock-card" ]
                    [ div [ class "dock-card-header" ] [ text "Horadric Cube (3x4)" ]
                    , viewSpatialGrid Cube 3 4 "grid-3x4" ui session cubeItems
                    ]
                , div [ class "dock-card" ]
                    [ div [ class "dock-card-header" ] [ text "Belt (4x4)" ]
                    , viewSpatialGrid Belt 4 4 "grid-4x4" ui session beltItems
                    ]
                ]
            ]
        , viewZone4MercenaryDualStage ui session
        ]


viewZone4MercenaryDualStage : UiState -> SessionView -> Html Msg
viewZone4MercenaryDualStage ui session =
    let
        merc =
            session.merc

        hasReplaced =
            merc.dismissed /= Nothing || not (List.isEmpty session.mercReplacedItems)

        hasReplacedItems =
            not (List.isEmpty session.mercReplacedItems)

        actOption actVal labelText =
            option
                [ value (String.fromInt actVal)
                , selected (merc.actIndex == actVal)
                ]
                [ text labelText ]
    in
    section [ class "glass-panel merc-dual-column", attribute "data-testid" "view-a-merc-panel" ]
        [ div [ class "panel-header" ]
            [ div [ class "panel-title" ] [ text "Mercenary Station (Zone 4)" ]
            , span [ class "merc-stage-badge badge-active-target" ] [ text "Interactive" ]
            ]
        , div [ class "merc-stages-horizontal" ]
            [ div [ class "merc-stage-card stage-active" ]
                [ div [ class "merc-stage-header" ]
                    [ span [ class "merc-stage-title" ] [ text "Active Mercenary" ]
                    , span [ class "merc-stage-badge badge-active-target" ] [ text "Active" ]
                    ]
                , div [ style "display" "flex", style "flex-direction" "column", style "gap" "4px" ]
                    [ select
                        [ id "merc-type-select"
                        , attribute "data-testid" "merc-act-select"
                        , onInput (\raw -> SelectMercAct (Maybe.withDefault 2 (String.toInt raw)))
                        ]
                        ([ if not merc.hired then
                            option [ value "0", selected True, disabled True ] [ text "-- Select Mercenary Act --" ]

                           else
                            text ""
                         , actOption 1 "Act 1 Rogue Scout (Bow)"
                         , actOption 2 "Act 2 Desert Mercenary (Spear/Polearm)"
                         , actOption 3 "Act 3 Iron Wolf (Sword/Shield)"
                         , actOption 5 "Act 5 Barbarian (Sword)"
                         ]
                        )
                    , div [ class "merc-stage-desc" ]
                        [ text
                            (if merc.hired then
                                "Lv " ++ String.fromInt merc.stats.level ++ " " ++ merc.className ++ " · " ++ merc.stats.innateAura

                             else
                                "No mercenary hired — choose an Act above to hire"
                            )
                        ]
                    ]
                , viewMercGridProportional ui session False session.mercItems
                ]
            , div
                [ classList
                    [ ( "merc-stage-card", True )
                    , ( "stage-dismissed", True )
                    , ( "has-items", hasReplacedItems )
                    ]
                , attribute "data-testid" "merc-replaced-stage"
                ]
                [ div [ class "merc-stage-header" ]
                    [ span [ class "merc-stage-title" ] [ text "Replaced Staging" ]
                    , div [ class "merc-stage-actions" ]
                        [ button
                            [ type_ "button"
                            , class "btn-restore-merc"
                            , attribute "data-testid" "btn-restore-merc"
                            , disabled (not hasReplaced)
                            , onClick RestoreReplacedMerc
                            ]
                            [ text "↩️ Restore" ]
                        , button
                            [ type_ "button"
                            , class "btn-restore-merc"
                            , attribute "data-testid" "btn-reclaim-merc"
                            , disabled (not hasReplaced)
                            , onClick (ClearReplacedMerc True)
                            ]
                            [ text "📦 Reclaim" ]
                        , button
                            [ type_ "button"
                            , class "btn-delete-merc"
                            , attribute "data-testid" "btn-delete-merc"
                            , disabled (not hasReplaced)
                            , onClick (ClearReplacedMerc False)
                            ]
                            [ text "🗑️ Discard" ]
                        ]
                    ]
                , div [ class "merc-stage-desc" ]
                    [ text
                        (case merc.dismissed of
                            Just oldMerc ->
                                "Prev: Lv "
                                    ++ String.fromInt oldMerc.level
                                    ++ " "
                                    ++ oldMerc.className
                                    ++ " · Remaining gear: "
                                    ++ String.fromInt (List.length session.mercReplacedItems)

                            Nothing ->
                                "When switching mercenary Acts, previous gear moves here for recovery."
                        )
                    ]
                , if hasReplacedItems then
                    div [ class "merc-discard-warning" ]
                        [ text "⚠️ Unreclaimed gear in Replaced Staging! Move to storage, restore, or clear before .d2s export." ]

                  else
                    text ""
                , viewMercGridProportional ui session True session.mercReplacedItems
                ]
            ]
        ]


viewMercGridProportional : UiState -> SessionView -> Bool -> List ItemCardView -> Html Msg
viewMercGridProportional ui session isReplaced items =
    let
        isHolding =
            session.selectedMoveItemIndex /= Nothing

        targetContainer =
            if isReplaced then
                MercReplaced

            else
                MercEquipment

        mercSlot bodyLoc colSpec rowSpec emptyLabel acceptedLabels =
            let
                matched =
                    items
                        |> List.filter
                            (\it ->
                                it.x == bodyLoc
                                    || (case it.slotLabel of
                                            Just lbl ->
                                                List.member lbl acceptedLabels

                                            Nothing ->
                                                False
                                       )
                            )
                        |> List.head

                isAllowedTarget =
                    List.any
                        (\t -> t.container == targetContainer && t.x == bodyLoc && t.y == 0)
                        session.allowedMoveTargets

                isHovered =
                    case ui.hoveredDropTarget of
                        Just tgt ->
                            tgt.container == targetContainer && tgt.x == bodyLoc && tgt.y == 0

                        Nothing ->
                            False
            in
            case matched of
                Nothing ->
                    div
                        ([ classList
                            [ ( "merc-slot", True )
                            , ( "footprint-valid", isHolding && isAllowedTarget )
                            , ( "footprint-invalid", isHolding && not isAllowedTarget && isHovered )
                            ]
                         , style "grid-column" colSpec
                         , style "grid-row" rowSpec
                         , attribute "data-testid" ((if isReplaced then "merc-replaced-slot-" else "merc-slot-") ++ String.fromInt bodyLoc)
                         ]
                            ++ (if isHolding then
                                    [ onClick (MoveSelectedItemTo targetContainer bodyLoc 0)
                                    , onMouseEnter (HoverDropTarget (Just { container = targetContainer, x = bodyLoc, y = 0 }))
                                    ]

                                else
                                    []
                               )
                        )
                        [ span [ class "doll-slot-tag" ] [ text emptyLabel ] ]

                Just item ->
                    let
                        isSelected =
                            session.selectedMoveItemIndex == Just item.index

                        filledActionAttrs =
                            if isHolding then
                                if isSelected then
                                    [ onClick (SelectItem Nothing) ]

                                else
                                    [ onClick (MoveSelectedItemTo targetContainer bodyLoc 0) ]

                            else
                                itemMenuTriggerAttrs item.index

                        filledHoverAttrs =
                            if isHolding then
                                [ onMouseEnter (HoverDropTarget (Just { container = targetContainer, x = bodyLoc, y = 0 })) ]

                            else
                                [ onMouseEnter (HoverItem (Just item.index))
                                , onMouseLeave (HoverItem Nothing)
                                ]
                    in
                    div
                        ([ classList
                            [ ( "merc-slot", True )
                            , ( "filled", True )
                            , ( qualityClass item.quality, True )
                            , ( "is-held-source", isSelected )
                            , ( "footprint-valid", isHolding && not isSelected && isAllowedTarget )
                            , ( "footprint-invalid", isHolding && not isSelected && not isAllowedTarget && isHovered )
                            ]
                         , style "grid-column" colSpec
                         , style "grid-row" rowSpec
                         , attribute "data-testid" ((if isReplaced then "merc-replaced-slot-" else "merc-slot-") ++ String.fromInt bodyLoc)
                         ]
                            ++ filledActionAttrs
                            ++ filledHoverAttrs
                        )
                        [ viewSizedEquipGraphic 36 28 item ]
    in
    div
        ([ class "merc-grid-proportional" ]
            ++ (if isHolding then
                    [ onMouseLeave (HoverDropTarget Nothing) ]

                else
                    []
               )
        )
        [ mercSlot 1 "1 / span 2" "1 / span 2" "Helm" [ "Helm", "Head" ]
        , mercSlot 4 "1 / span 2" "3 / span 4" "Weapon" [ "Weapon1", "Right Hand" ]
        , mercSlot 3 "3 / span 2" "1 / span 4" "Armor" [ "Armor", "Torso", "Body" ]
        , mercSlot 5 "3 / span 2" "5 / span 4" "Off-Hand" [ "Weapon2", "Left Hand", "Shield" ]
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


viewSpatialGrid : ContainerKind -> Int -> Int -> String -> UiState -> SessionView -> List ItemCardView -> Html Msg
viewSpatialGrid containerTag cols rows gridClass ui session items =
    let
        containerSlug =
            UiState.containerLabel containerTag

        selectedIdx =
            ui.selectedItemIndex

        heldItem =
            findHeldItem session

        moveActive =
            heldItem /= Nothing

        beltRestricted =
            moveActive && containerTag == Belt && session.beltMoveRestrictedReason /= Nothing

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

                            ( gx, gy ) =
                                UiState.resolveGridDropAnchorWithSub session.allowedMoveTargets containerTag w h target.x target.y ui.hoveredDropSub

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
                    if containerTag == Belt then
                        rows - 1 - row

                    else
                        row
            in
            if moveActive then
                let
                    cellTestId =
                        attribute "data-testid" ("grid-cell-" ++ containerSlug ++ "-" ++ String.fromInt col ++ "-" ++ String.fromInt logicalY)

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
                    [ div [ class "grid-quad-overlay" ]
                        [ div
                            [ class "grid-quad-zone"
                            , onMouseEnter (HoverDropTargetSub (Just { container = containerTag, x = col, y = logicalY }) ( 0, 0 ))
                            ]
                            []
                        , div
                            [ class "grid-quad-zone"
                            , onMouseEnter (HoverDropTargetSub (Just { container = containerTag, x = col, y = logicalY }) ( 1, 0 ))
                            ]
                            []
                        , div
                            [ class "grid-quad-zone"
                            , onMouseEnter (HoverDropTargetSub (Just { container = containerTag, x = col, y = logicalY }) ( 0, 1 ))
                            ]
                            []
                        , div
                            [ class "grid-quad-zone"
                            , onMouseEnter (HoverDropTargetSub (Just { container = containerTag, x = col, y = logicalY }) ( 1, 1 ))
                            ]
                            []
                        ]
                    ]

            else
                div [ class "grid-cell" ] []

        footprintPreviewNodes =
            case hoverFootprint of
                Just fp ->
                    if fp.fitsBounds then
                        let
                            visualY =
                                if containerTag == Belt then
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
                            , attribute "data-testid" ("grid-footprint-preview-" ++ containerSlug)
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
                    if containerTag == Belt then
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

                itemActionAttrs =
                    case session.selectedMoveItemIndex of
                        Just heldIdx ->
                            if heldIdx == item.index then
                                [ onClick (SelectItem Nothing) ]

                            else
                                [ onClick (MoveSelectedItemTo containerTag item.x item.y) ]

                        Nothing ->
                            itemMenuTriggerAttrs item.index
            in
            div
                ([ classList
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
                 , onMouseEnter (HoverItem (Just item.index))
                 , onMouseLeave (HoverItem Nothing)
                 ]
                    ++ itemActionAttrs
                )
                (viewItemTileGraphic item)
    in
    div [ class "d2-grid-wrap" ]
        [ div
            [ classList
                [ ( "d2-grid", True )
                , ( gridClass, True )
                , ( "is-restricted", beltRestricted )
                ]
            , attribute "data-testid" ("spatial-grid-" ++ containerSlug)
            , onMouseLeave (HoverDropTarget Nothing)
            ]
            (interactiveCells ++ List.map renderPlaced items ++ footprintPreviewNodes)
        ]


