module State.UiState exposing
    ( DragKind(..)
    , FocusedInput
    , MarqueeSelection
    , PaintMode(..)
    , StashTab(..)
    , ThemeMode(..)
    , Toast
    , UiState
    , ViewTab(..)
    , beginFocusInput
    , beginMarquee
    , clearFocusInput
    , clearMarquee
    , clearToast
    , containerLabel
    , extendMarquee
    , init
    , qualityLabel
    , setContainerFilter
    , setHoverMousePos
    , setHoveredDropTarget
    , setHoveredItem
    , setPaintMode
    , setProgressionDifficulty
    , setSelectedItem
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
    , toggleTheme
    , updateFocusInput
    , updateMarquee
    )

import Generated.Protocol exposing (ActionRejection, ContainerKind(..), DifficultyMode(..), ItemQualityKind(..), RejectionCode(..), SuggestedRecovery(..))

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
    , statusBanner : Maybe String
    , toast : Maybe Toast
    , focusedInput : Maybe FocusedInput
    , combatLabOpen : Bool
    , skillTreeTab : String
    , shopQualityTab : ItemQualityKind
    , shopCategoryFilter : String
    , shopPage : Int
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
    , statusBanner = Nothing
    , toast = Nothing
    , focusedInput = Nothing
    , combatLabOpen = False
    , skillTreeTab = "tree0"
    , shopQualityTab = QualityUnique
    , shopCategoryFilter = "all"
    , shopPage = 0
    }


setView : ViewTab -> UiState -> UiState
setView tab ui =
    { ui | activeView = tab, hoveredItemIndex = Nothing, hoverMousePos = Nothing, hoveredDropTarget = Nothing }


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
    { ui | focusedInput = Just { key = key, raw = raw } }


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


setShopQualityTab : ItemQualityKind -> UiState -> UiState
setShopQualityTab tab ui =
    { ui | shopQualityTab = tab, shopCategoryFilter = "all", shopPage = 0 }


setShopCategoryFilter : String -> UiState -> UiState
setShopCategoryFilter cat ui =
    { ui | shopCategoryFilter = cat, shopPage = 0 }


setShopPage : Int -> UiState -> UiState
setShopPage page ui =
    { ui | shopPage = max 0 page }


setHoveredItem : Maybe Int -> UiState -> UiState
setHoveredItem maybeIdx ui =
    { ui
        | hoveredItemIndex = maybeIdx
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
    { ui | hoveredDropTarget = target }


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
