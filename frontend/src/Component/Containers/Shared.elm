module Component.Containers.Shared exposing (..)

import Generated.Protocol exposing (CharacterStatsView, ContainerCountsView, ContainerKind(..), ItemCardView, ItemQualityKind(..), MercSummaryView, PackedSkillSpecView, ParamCatalogView, ParamOptionView, SessionView, SetBonusView, ShopCatalogItemView, SocketedItemView, StatCatalogEntryView, StatParamKind(..))
import Html exposing (Html, button, div, h2, h3, img, input, li, option, section, select, span, text, ul)
import Html.Attributes exposing (alt, attribute, checked, class, classList, disabled, id, placeholder, readonly, selected, src, style, title, type_, value)
import Html.Events exposing (custom, on, onBlur, onClick, onFocus, onInput, onMouseEnter, onMouseLeave)
import Json.Decode as Decode
import State.UiState as UiState exposing (ItemMenuState(..), ModalNotice(..), StashTab(..), UiState)
import Update exposing (Msg(..))


findHeldItem : SessionView -> Maybe ItemCardView
findHeldItem session =
    session.selectedMoveItemIndex
        |> Maybe.andThen
            (\idx ->
                (session.items ++ session.mercItems ++ session.mercReplacedItems)
                    |> List.filter (\it -> it.index == idx)
                    |> List.head
            )




itemMenuTriggerAttrs : Int -> List (Html.Attribute Msg)
itemMenuTriggerAttrs itemIndex =
    let
        coordDecoder toMsg =
            Decode.map2
                (\maybeX maybeY ->
                    { message =
                        toMsg
                            itemIndex
                            (round (Maybe.withDefault 240 maybeX))
                            (round (Maybe.withDefault 180 maybeY))
                    , stopPropagation = True
                    , preventDefault = False
                    }
                )
                (Decode.maybe (Decode.field "clientX" Decode.float))
                (Decode.maybe (Decode.field "clientY" Decode.float))

        contextDecoder =
            Decode.map2
                (\maybeX maybeY ->
                    { message =
                        OpenContextMenu
                            itemIndex
                            (round (Maybe.withDefault 240 maybeX))
                            (round (Maybe.withDefault 180 maybeY))
                    , stopPropagation = True
                    , preventDefault = True
                    }
                )
                (Decode.maybe (Decode.field "clientX" Decode.float))
                (Decode.maybe (Decode.field "clientY" Decode.float))
    in
    [ custom "click" (coordDecoder OpenActionPalette)
    , custom "contextmenu" contextDecoder
    ]




countBadge : String -> Int -> Html Msg
countBadge label count =
    div [ class "stat-chip" ]
        [ span [ class "stat-label" ] [ text label ]
        , span [ class "stat-val" ] [ text (String.fromInt count) ]
        ]


filterButton : Maybe ContainerKind -> Maybe ContainerKind -> String -> String -> Html Msg
filterButton active target label testSlug =
    button
        [ classList [ ( "seg-btn", True ), ( "is-active", active == target ) ]
        , attribute "data-testid" ("container-filter-" ++ testSlug)
        , onClick (SelectContainerFilter target)
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
            , span [ class "item-quality-pill" ] [ text (UiState.qualityLabel item.quality) ]
            ]
        , div [ class "meta-line" ]
            [ text
                (UiState.containerLabel item.container
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


qualityClass : ItemQualityKind -> String
qualityClass quality =
    case quality of
        QualityMagic ->
            "q-magic"

        QualityRare ->
            "q-rare"

        QualityUnique ->
            "q-unique"

        QualitySet ->
            "q-set"

        QualityRuneword ->
            "q-runeword"

        QualityUnknown ->
            "q-unknown"

        _ ->
            "q-normal"


viewSizedEquipGraphic : Int -> Int -> ItemCardView -> Html Msg
viewSizedEquipGraphic cellW cellH item =
    let
        wPx =
            max 1 item.width * cellW - 4

        hPx =
            max 1 item.height * cellH - 4
    in
    div
        [ class "equip-item-box"
        , style "width" (String.fromInt wPx ++ "px")
        , style "height" (String.fromInt hPx ++ "px")
        ]
        (viewItemTileGraphic item)


viewItemTileGraphic : ItemCardView -> List (Html Msg)
viewItemTileGraphic item =
    case item.imageUrl of
        Just url ->
            [ img
                [ class "item-tile-img"
                , src url
                , alt item.nameEn
                , title (item.nameEn ++ " (" ++ UiState.qualityLabel item.quality ++ ")")
                ]
                []
            ]

        Nothing ->
            [ span [ class "item-tile-name" ] [ text item.nameEn ]
            , span [ class "item-tile-type" ] [ text (UiState.qualityLabel item.quality) ]
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
                        (UiState.qualityLabel item.quality
                            ++ " · ilvl "
                            ++ String.fromInt item.itemLevel
                            ++ " · "
                            ++ item.code
                            ++ " ("
                            ++ UiState.containerLabel item.container
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
                |> List.filter (\entry -> entry.container == Equipment || entry.container == MercEquipment)
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


viewInlineIntInput : UiState -> String -> String -> Int -> String -> Html Msg
viewInlineIntInput ui className fieldKey currentVal tid =
    let
        serverVal =
            String.fromInt currentVal

        onEnterOrEscape =
            on "keydown"
                (Decode.field "key" Decode.string
                    |> Decode.andThen
                        (\k ->
                            case k of
                                "Enter" ->
                                    Decode.succeed CommitFocusedInput

                                "Escape" ->
                                    Decode.succeed CancelFocusedInput

                                _ ->
                                    Decode.fail "ignore key"
                        )
                )
    in
    input
        [ class className
        , type_ "text"
        , value (UiState.focusedDraftValue ui fieldKey serverVal)
        , attribute "data-testid" tid
        , onFocus (FocusField fieldKey serverVal)
        , onInput EditFocusedInput
        , onBlur CommitFocusedInput
        , onEnterOrEscape
        ]
        []


viewGoldInlineEditor : UiState -> String -> String -> Int -> Int -> String -> Html Msg
viewGoldInlineEditor ui fieldKey labelText currentVal maxVal tid =
    div [ class "gold-inline-editor" ]
        [ span [] [ text (labelText ++ ":") ]
        , viewInlineIntInput ui "gold-inline-input" fieldKey currentVal tid
        , span [ class "gold-inline-max" ] [ text ("/ " ++ String.fromInt maxVal) ]
        ]


viewSocketedItemsSection : ItemCardView -> Html Msg
viewSocketedItemsSection item =
    let
        filledCount =
            List.length item.socketedItems

        totalCount =
            max item.socketCount filledCount

        emptyCount =
            max 0 (totalCount - filledCount)
    in
    if totalCount <= 0 then
        text ""

    else
        div [ class "socketed-viewer", attribute "data-testid" "socketed-items-viewer" ]
            [ div [ class "socketed-viewer-header" ]
                [ span [] [ text "Socketed Gems / Runes" ]
                , span [] [ text (String.fromInt filledCount ++ " / " ++ String.fromInt totalCount) ]
                ]
            , div [ class "socketed-slot-list" ]
                (List.map viewSocketedSlotCard item.socketedItems
                    ++ List.repeat emptyCount
                        (div [ class "socketed-empty-slot" ] [ text "Empty Socket" ])
                )
            ]


viewSocketedSlotCard : SocketedItemView -> Html Msg
viewSocketedSlotCard child =
    div [ class "socketed-slot-card" ]
        [ case child.imageUrl of
            Just url ->
                img [ class "socketed-slot-thumb", src url, alt child.nameEn ] []

            Nothing ->
                span [ class "item-editor-stat-id-badge" ] [ text child.code ]
        , div [ class "socketed-slot-body" ]
            [ div [ class ("socketed-slot-name " ++ qualityClass child.quality) ] [ text child.nameEn ]
            , if List.isEmpty child.properties then
                text ""

              else
                ul [ class "socketed-slot-props" ]
                    (List.map (\line -> li [] [ text line ]) child.properties)
            ]
        ]


gridBackgroundCells : Int -> Int -> List (Html Msg)
gridBackgroundCells cols rows =
    List.repeat (cols * rows) (div [ class "grid-cell" ] [])

