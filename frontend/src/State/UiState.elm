module State.UiState exposing
    ( DragKind(..)
    , FocusedInput
    , ItemMenuState(..)
    , MarqueeSelection
    , ModalNotice(..)
    , PaintMode(..)
    , QuickMoveOrigin
    , StashTab(..)
    , ThemeMode(..)
    , Toast
    , UiState
    , ViewTab(..)
    , beginFocusInput
    , beginMarquee
    , clearFocusInput
    , clearMarquee
    , clampDropAnchor
    , clearQuickMoveOrigin
    , clearToast
    , closeItemMenu
    , closeItemEditorModal
    , closeSkillDropdown
    , containerLabel
    , extendMarquee
    , focusedDraftValue
    , init
    , itemEditorSkillSearchQuery
    , openActionPalette
    , openContextMenu
    , openItemEditorModal
    , qualityLabel
    , resolveGridDropAnchor
    , resolveGridDropAnchorWithSub
    , selectItemEditorInjectStat
    , setContainerFilter
    , setHoverMousePos
    , setHoveredDropTarget
    , setHoveredDropTargetSub
    , setHoveredItem
    , setHoveredSkill
    , setItemEditorInjectParam
    , setItemEditorInjectValue
    , setItemEditorNotice
    , setItemEditorSearch
    , setItemEditorSkillSearch
    , setPaintMode
    , setProgressionDifficulty
    , setQuickMoveOrigin
    , setSelectedItem
    , setSelectedQuestTarget
    , setShopCategoryFilter
    , setShopPage
    , setShopQualityTab
    , setSkillTreeTab
    , setStashTab
    , setStatusBanner
    , setToast
    , setView
    , toggleCombatLab
    , togglePaintMode
    , toggleSkillDropdown
    , toggleTheme
    , updateFocusInput
    , updateMarquee
    )

import Dict exposing (Dict)
import Generated.Protocol exposing (ActionRejection, AllowedMoveTargetView, ContainerKind(..), DifficultyMode(..), ItemQualityKind(..), RejectionCode(..), SuggestedRecovery(..))

{-| Ephemeral UI state owned exclusively by Elm (tabs, theme, stash tab preview,
progression difficulty, paint mode, marquee coordinates, selected item, filter).
Domain state and game rules never live here.
-}


type ViewTab
    = ViewA
    | ViewB


type ThemeMode
    = Dark
    | Light


type StashTab
    = PersonalStash
    | SharedStash Int


type PaintMode
    = Activate
    | Deactivate


type DragKind
    = DragQuests
    | DragWaypoints


type alias MarqueeSelection =
    { kind : DragKind
    , mode : PaintMode
    , anchorX : Float
    , anchorY : Float
    , currentX : Float
    , currentY : Float
    , candidateIds : List String
    }


type alias FocusedInput =
    { key : String
    , raw : String
    }


type ItemMenuState
    = NoItemMenu
    | ActionPaletteMenu { itemIndex : Int, x : Int, y : Int }
    | ContextItemMenu { itemIndex : Int, x : Int, y : Int }


type ModalNotice
    = NoticeInfo String
    | NoticeWarning String
    | NoticeError String


type alias Toast =
    { code : RejectionCode
    , rawCode : String
    , message : String
    , targetId : Maybe String
    , suggestedAction : Maybe SuggestedRecovery
    }


type alias UiState =
    { activeView : ViewTab
    , theme : ThemeMode
    , activeStashTab : StashTab
    , progressionDifficulty : DifficultyMode
    , paintMode : Maybe PaintMode
    , marquee : Maybe MarqueeSelection
    , selectedContainerFilter : Maybe ContainerKind
    , selectedItemIndex : Maybe Int
    , hoveredItemIndex : Maybe Int
    , hoverMousePos : Maybe { x : Int, y : Int, vw : Int, vh : Int }
    , hoveredDropTarget : Maybe { container : ContainerKind, x : Int, y : Int }
    , hoveredDropSub : ( Int, Int )
    , quickMoveOrigin : Maybe QuickMoveOrigin
    , statusBanner : Maybe String
    , toast : Maybe Toast
    , focusedInput : Maybe FocusedInput
    , combatLabOpen : Bool
    , skillTreeTab : String
    , hoveredSkillId : Maybe Int
    , shopQualityTab : ItemQualityKind
    , shopCategoryFilter : String
    , shopPage : Int
    , itemMenu : ItemMenuState
    , itemEditorModalIndex : Maybe Int
    , itemEditorSearch : String
    , itemEditorSkillQueries : Dict String String
    , openSkillDropdown : Maybe String
    , itemEditorInjectStatId : Maybe Int
    , itemEditorInjectValue : Int
    , itemEditorInjectParam : Int
    , itemEditorNotice : Maybe ModalNotice
    , selectedQuestTarget : Maybe { difficulty : DifficultyMode, key : String }
    }


init : UiState
init =
    { activeView = ViewB
    , theme = Dark
    , activeStashTab = PersonalStash
    , progressionDifficulty = Normal
    , paintMode = Nothing
    , marquee = Nothing
    , selectedContainerFilter = Nothing
    , selectedItemIndex = Nothing
    , hoveredItemIndex = Nothing
    , hoverMousePos = Nothing
    , hoveredDropTarget = Nothing
    , hoveredDropSub = ( 1, 1 )
    , quickMoveOrigin = Nothing
    , statusBanner = Nothing
    , toast = Nothing
    , focusedInput = Nothing
    , combatLabOpen = False
    , skillTreeTab = "tree0"
    , hoveredSkillId = Nothing
    , shopQualityTab = QualityUnique
    , shopCategoryFilter = "all"
    , shopPage = 0
    , itemMenu = NoItemMenu
    , itemEditorModalIndex = Nothing
    , itemEditorSearch = ""
    , itemEditorSkillQueries = Dict.empty
    , openSkillDropdown = Nothing
    , itemEditorInjectStatId = Nothing
    , itemEditorInjectValue = 10
    , itemEditorInjectParam = 0
    , itemEditorNotice = Nothing
    , selectedQuestTarget = Nothing
    }


setView : ViewTab -> UiState -> UiState
setView tab ui =
    { ui | activeView = tab, hoveredItemIndex = Nothing, hoveredSkillId = Nothing, hoverMousePos = Nothing, hoveredDropTarget = Nothing, hoveredDropSub = ( 1, 1 ), quickMoveOrigin = Nothing, itemMenu = NoItemMenu }


setSelectedQuestTarget : Maybe { difficulty : DifficultyMode, key : String } -> UiState -> UiState
setSelectedQuestTarget target ui =
    { ui | selectedQuestTarget = target }


openActionPalette : Int -> Int -> Int -> UiState -> UiState
openActionPalette idx x y ui =
    { ui | selectedItemIndex = Just idx, itemMenu = ActionPaletteMenu { itemIndex = idx, x = x, y = y } }


openContextMenu : Int -> Int -> Int -> UiState -> UiState
openContextMenu idx x y ui =
    { ui | selectedItemIndex = Just idx, itemMenu = ContextItemMenu { itemIndex = idx, x = x, y = y } }


closeItemMenu : UiState -> UiState
closeItemMenu ui =
    { ui | itemMenu = NoItemMenu }


openItemEditorModal : Int -> UiState -> UiState
openItemEditorModal idx ui =
    { ui
        | selectedItemIndex = Just idx
        , itemMenu = NoItemMenu
        , itemEditorModalIndex = Just idx
        , itemEditorSearch = ""
        , itemEditorSkillQueries = Dict.empty
        , openSkillDropdown = Nothing
        , itemEditorInjectStatId = Nothing
        , itemEditorInjectValue = 10
        , itemEditorInjectParam = 0
        , itemEditorNotice = Nothing
    , selectedQuestTarget = Nothing
    }


closeItemEditorModal : UiState -> UiState
closeItemEditorModal ui =
    { ui | itemEditorModalIndex = Nothing, itemEditorNotice = Nothing, itemEditorSkillQueries = Dict.empty, openSkillDropdown = Nothing }


setItemEditorNotice : Maybe ModalNotice -> UiState -> UiState
setItemEditorNotice notice ui =
    { ui | itemEditorNotice = notice }


setItemEditorSearch : String -> UiState -> UiState
setItemEditorSearch query ui =
    { ui | itemEditorSearch = query }


itemEditorSkillSearchQuery : String -> UiState -> String
itemEditorSkillSearchQuery tid ui =
    Dict.get tid ui.itemEditorSkillQueries |> Maybe.withDefault ""


setItemEditorSkillSearch : String -> String -> UiState -> UiState
setItemEditorSkillSearch tid query ui =
    { ui | itemEditorSkillQueries = Dict.insert tid query ui.itemEditorSkillQueries }


toggleSkillDropdown : String -> UiState -> UiState
toggleSkillDropdown tid ui =
    { ui
        | openSkillDropdown =
            if ui.openSkillDropdown == Just tid then
                Nothing

            else
                Just tid
    }


closeSkillDropdown : UiState -> UiState
closeSkillDropdown ui =
    { ui | openSkillDropdown = Nothing }


selectItemEditorInjectStat : Int -> Int -> UiState -> UiState
selectItemEditorInjectStat statId defaultVal ui =
    { ui
        | itemEditorInjectStatId = Just statId
        , itemEditorInjectValue = defaultVal
        , itemEditorInjectParam = 0
    }


setItemEditorInjectValue : Int -> UiState -> UiState
setItemEditorInjectValue val ui =
    { ui | itemEditorInjectValue = val }


setItemEditorInjectParam : Int -> UiState -> UiState
setItemEditorInjectParam param ui =
    { ui | itemEditorInjectParam = param }


setProgressionDifficulty : DifficultyMode -> UiState -> UiState
setProgressionDifficulty diff ui =
    { ui | progressionDifficulty = diff, marquee = Nothing }


togglePaintMode : PaintMode -> UiState -> UiState
togglePaintMode target ui =
    if ui.paintMode == Just target then
        { ui | paintMode = Nothing, marquee = Nothing }

    else
        { ui | paintMode = Just target, marquee = Nothing }


setPaintMode : Maybe PaintMode -> UiState -> UiState
setPaintMode mode ui =
    { ui | paintMode = mode, marquee = Nothing }


beginMarquee : DragKind -> Float -> Float -> List String -> UiState -> UiState
beginMarquee kind x y initialIds ui =
    case ui.paintMode of
        Nothing ->
            ui

        Just mode ->
            { ui
                | marquee =
                    Just
                        { kind = kind
                        , mode = mode
                        , anchorX = x
                        , anchorY = y
                        , currentX = x
                        , currentY = y
                        , candidateIds = initialIds
                        }
            }


updateMarquee : Float -> Float -> List String -> UiState -> UiState
updateMarquee x y ids ui =
    case ui.marquee of
        Nothing ->
            ui

        Just box ->
            { ui
                | marquee =
                    Just
                        { box
                            | currentX = x
                            , currentY = y
                            , candidateIds = ids
                        }
            }


extendMarquee : DragKind -> String -> UiState -> UiState
extendMarquee kind candidateId ui =
    case ui.marquee of
        Nothing ->
            ui

        Just box ->
            if box.kind /= kind then
                ui

            else
                let
                    nextIds =
                        if List.member candidateId box.candidateIds then
                            box.candidateIds

                        else
                            box.candidateIds ++ [ candidateId ]
                in
                { ui | marquee = Just { box | candidateIds = nextIds } }


clearMarquee : UiState -> UiState
clearMarquee ui =
    { ui | marquee = Nothing }


setContainerFilter : Maybe ContainerKind -> UiState -> UiState
setContainerFilter filterKey ui =
    { ui | selectedContainerFilter = filterKey }


setSelectedItem : Maybe Int -> UiState -> UiState
setSelectedItem maybeIdx ui =
    { ui | selectedItemIndex = maybeIdx }


setStatusBanner : Maybe String -> UiState -> UiState
setStatusBanner maybeBanner ui =
    { ui | statusBanner = maybeBanner }


setToast : ActionRejection -> UiState -> UiState
setToast rej ui =
    { ui
        | toast =
            Just
                { code = rej.code
                , rawCode = rej.rawCode
                , message = rej.message
                , targetId = rej.targetId
                , suggestedAction = rej.suggestedAction
                }
    }


clearToast : UiState -> UiState
clearToast ui =
    { ui | toast = Nothing }


beginFocusInput : String -> String -> UiState -> UiState
beginFocusInput key raw ui =
    { ui | focusedInput = Just { key = key, raw = raw }, itemEditorNotice = Nothing }


updateFocusInput : String -> UiState -> UiState
updateFocusInput raw ui =
    case ui.focusedInput of
        Nothing ->
            ui

        Just focused ->
            { ui | focusedInput = Just { focused | raw = raw } }


clearFocusInput : UiState -> UiState
clearFocusInput ui =
    { ui | focusedInput = Nothing }


toggleTheme : UiState -> UiState
toggleTheme ui =
    { ui
        | theme =
            case ui.theme of
                Dark ->
                    Light

                Light ->
                    Dark
    }


setStashTab : StashTab -> UiState -> UiState
setStashTab tab ui =
    { ui | activeStashTab = tab }


toggleCombatLab : UiState -> UiState
toggleCombatLab ui =
    { ui | combatLabOpen = not ui.combatLabOpen }


setSkillTreeTab : String -> UiState -> UiState
setSkillTreeTab treeId ui =
    { ui | skillTreeTab = treeId }


setHoveredSkill : Maybe Int -> UiState -> UiState
setHoveredSkill maybeSkillId ui =
    { ui | hoveredSkillId = maybeSkillId }


setShopQualityTab : ItemQualityKind -> UiState -> UiState
setShopQualityTab tab ui =
    { ui | shopQualityTab = tab, shopCategoryFilter = "all", shopPage = 0 }


setShopCategoryFilter : String -> UiState -> UiState
setShopCategoryFilter cat ui =
    { ui | shopCategoryFilter = cat, shopPage = 0 }


setShopPage : Int -> UiState -> UiState
setShopPage page ui =
    { ui | shopPage = max 0 page }


type alias QuickMoveOrigin =
    { itemIndex : Int
    , destContainer : ContainerKind
    , container : ContainerKind
    , x : Int
    , y : Int
    , width : Int
    , height : Int
    , sentRevision : Int
    }


setQuickMoveOrigin : QuickMoveOrigin -> UiState -> UiState
setQuickMoveOrigin origin ui =
    { ui | quickMoveOrigin = Just origin }


clearQuickMoveOrigin : UiState -> UiState
clearQuickMoveOrigin ui =
    { ui | quickMoveOrigin = Nothing }


setHoveredItem : Maybe Int -> UiState -> UiState
setHoveredItem maybeIdx ui =
    let
        nextQuickOrigin =
            case ( maybeIdx, ui.quickMoveOrigin ) of
                ( Just idx, Just origin ) ->
                    if origin.itemIndex == idx then
                        Just origin

                    else
                        Nothing

                ( Nothing, existing ) ->
                    existing

                _ ->
                    Nothing
    in
    { ui
        | hoveredItemIndex = maybeIdx
        , quickMoveOrigin = nextQuickOrigin
        , hoverMousePos =
            case maybeIdx of
                Nothing ->
                    Nothing

                Just _ ->
                    ui.hoverMousePos
    }


setHoverMousePos : Int -> Int -> Int -> Int -> UiState -> UiState
setHoverMousePos x y vw vh ui =
    { ui | hoverMousePos = Just { x = x, y = y, vw = vw, vh = vh } }


setHoveredDropTarget : Maybe { container : ContainerKind, x : Int, y : Int } -> UiState -> UiState
setHoveredDropTarget target ui =
    setHoveredDropTargetSub target ( 1, 1 ) ui


setHoveredDropTargetSub : Maybe { container : ContainerKind, x : Int, y : Int } -> ( Int, Int ) -> UiState -> UiState
setHoveredDropTargetSub target sub ui =
    let
        nextQuickOrigin =
            case ( target, ui.quickMoveOrigin ) of
                ( Just t, Just origin ) ->
                    if t.container /= origin.container then
                        Nothing

                    else
                        case origin.container of
                            Equipment ->
                                Just origin

                            MercEquipment ->
                                Just origin

                            MercReplaced ->
                                Just origin

                            _ ->
                                if
                                    t.x >= origin.x
                                        && t.x < origin.x + max 1 origin.width
                                        && t.y >= origin.y
                                        && t.y < origin.y + max 1 origin.height
                                then
                                    Just origin

                                else
                                    Nothing

                _ ->
                    Nothing
    in
    { ui | hoveredDropTarget = target, hoveredDropSub = sub, quickMoveOrigin = nextQuickOrigin }


containerGridDims : ContainerKind -> Maybe ( Int, Int )
containerGridDims kind =
    case kind of
        Inventory ->
            Just ( 10, 4 )

        Stash ->
            Just ( 10, 10 )

        Cube ->
            Just ( 3, 4 )

        Belt ->
            Just ( 4, 4 )

        TempInventory ->
            Just ( 9, 4 )

        _ ->
            Nothing


clampDropAnchor : ContainerKind -> Int -> Int -> Int -> Int -> ( Int, Int )
clampDropAnchor kind itemW itemH targetX targetY =
    case containerGridDims kind of
        Just ( cols, rows ) ->
            let
                w =
                    max 1 itemW

                h =
                    max 1 itemH
            in
            ( clamp 0 (max 0 (cols - w)) targetX
            , clamp 0 (max 0 (rows - h)) targetY
            )

        Nothing ->
            ( targetX, targetY )


resolveGridDropAnchor : List AllowedMoveTargetView -> ContainerKind -> Int -> Int -> Int -> Int -> ( Int, Int )
resolveGridDropAnchor allowedTargets kind itemW itemH targetX targetY =
    resolveGridDropAnchorWithSub allowedTargets kind itemW itemH targetX targetY ( 1, 1 )


resolveGridDropAnchorWithSub : List AllowedMoveTargetView -> ContainerKind -> Int -> Int -> Int -> Int -> ( Int, Int ) -> ( Int, Int )
resolveGridDropAnchorWithSub _ kind itemW itemH targetX targetY ( qx, qy ) =
    case containerGridDims kind of
        Nothing ->
            ( targetX, targetY )

        Just _ ->
            let
                w =
                    max 1 itemW

                h =
                    max 1 itemH

                preferredX =
                    if modBy 2 w == 0 then
                        targetX - (w // 2) + clamp 0 1 qx

                    else
                        targetX - ((w - 1) // 2)

                preferredY =
                    if modBy 2 h == 0 then
                        targetY - (h // 2) + clamp 0 1 qy

                    else
                        targetY - ((h - 1) // 2)
            in
            clampDropAnchor kind w h preferredX preferredY


containerLabel : ContainerKind -> String
containerLabel kind =
    case kind of
        Stash ->
            "stash"

        Inventory ->
            "inventory"

        TempInventory ->
            "temp_inventory"

        Cube ->
            "cube"

        Belt ->
            "belt"

        Equipment ->
            "equipment"

        MercEquipment ->
            "merc_equipment"

        MercReplaced ->
            "merc_replaced"

        Cursor ->
            "cursor"

        Shop ->
            "shop"

        UnknownContainer ->
            "unknown"


qualityLabel : ItemQualityKind -> String
qualityLabel quality =
    case quality of
        QualityLow ->
            "Low"

        QualityNormal ->
            "Normal"

        QualitySuperior ->
            "Superior"

        QualityMagic ->
            "Magic"

        QualitySet ->
            "Set"

        QualityRare ->
            "Rare"

        QualityUnique ->
            "Unique"

        QualityCrafted ->
            "Crafted"

        QualityRuneword ->
            "Runeword"

        QualityUnknown ->
            "Unknown"


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
