module State.UiState exposing
    ( DragKind(..)
    , FocusedInput
    , MarqueeSelection
    , PaintMode(..)
    , UiState
    , ViewTab(..)
    , beginFocusInput
    , beginMarquee
    , clearFocusInput
    , clearMarquee
    , init
    , setContainerFilter
    , setPaintMode
    , setProgressionDifficulty
    , setSelectedItem
    , setStatusBanner
    , setView
    , togglePaintMode
    , updateFocusInput
    , updateMarquee
    )

import Generated.Protocol exposing (DifficultyMode(..))

{-| Ephemeral UI state owned exclusively by Elm (tabs, progression inspection tab,
paint mode, marquee box coordinates, selected item highlight, container filter).
Domain state and game rules never live here.
-}


type ViewTab
    = ViewA
    | ViewB


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


type alias UiState =
    { activeView : ViewTab
    , progressionDifficulty : DifficultyMode
    , paintMode : Maybe PaintMode
    , marquee : Maybe MarqueeSelection
    , selectedContainerFilter : String
    , selectedItemIndex : Maybe Int
    , statusBanner : Maybe String
    , focusedInput : Maybe FocusedInput
    }


init : UiState
init =
    { activeView = ViewB
    , progressionDifficulty = Normal
    , paintMode = Nothing
    , marquee = Nothing
    , selectedContainerFilter = "all"
    , selectedItemIndex = Nothing
    , statusBanner = Nothing
    , focusedInput = Nothing
    }


setView : ViewTab -> UiState -> UiState
setView tab ui =
    { ui | activeView = tab }


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


clearMarquee : UiState -> UiState
clearMarquee ui =
    { ui | marquee = Nothing }


setContainerFilter : String -> UiState -> UiState
setContainerFilter filterKey ui =
    { ui | selectedContainerFilter = filterKey }


setSelectedItem : Maybe Int -> UiState -> UiState
setSelectedItem maybeIdx ui =
    { ui | selectedItemIndex = maybeIdx }


setStatusBanner : Maybe String -> UiState -> UiState
setStatusBanner maybeBanner ui =
    { ui | statusBanner = maybeBanner }


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
