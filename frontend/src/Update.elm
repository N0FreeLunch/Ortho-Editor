module Update exposing
    ( Model
    , Msg(..)
    , OutMsg(..)
    , allQuestsCompletedAllDiffs
    , allWaypointsActiveAllDiffs
    , init
    , paintModeToBool
    , update
    )

import Generated.Protocol as Protocol
    exposing
        ( ActionRejection
        , CharacterField(..)
        , DifficultyMode(..)
        , EditMode(..)
        , SessionEnvelope
        , SessionView
        , UiIntent
        , sessionEnvelopeDecoder
        )
import Json.Decode
import State.UiState as UiState exposing (DragKind(..), PaintMode(..), UiState, ViewTab)


type alias Model =
    { session : Maybe SessionView
    , ui : UiState
    , lastRejection : Maybe ActionRejection
    , lastDecodeError : Maybe String
    }


type Msg
    = ReceiveSessionEnvelope Json.Decode.Value
    | SwitchView ViewTab
    | SelectResDifficulty DifficultyMode
    | SelectSkillMode EditMode
    | AdjustMercLevel Int
    | SetMercLevelExact Int
    | SelectMercAct Int
    | SelectProgressionDifficulty DifficultyMode
    | TogglePaintMode PaintMode
    | ClickQuestTile DifficultyMode String
    | ClickWaypointTile DifficultyMode Int
    | BeginMarquee DragKind Float Float (List String)
    | MoveMarquee Float Float (List String)
    | CommitMarquee DifficultyMode
    | CancelMarquee
    | SetQuestsBatch DifficultyMode (List String) Bool
    | SetWaypointsBatch DifficultyMode (List Int) Bool
    | ToggleAllDiffQuests
    | ToggleAllDiffWaypoints
    | SelectContainerFilter String
    | ClearTempInventory
    | SelectItem (Maybe Int)
    | FocusField String String
    | EditFocusedInput String
    | CommitFocusedInput
    | CancelFocusedInput
    | ClearBanner


type OutMsg
    = NoOut
    | EmitIntent UiIntent
    | ReportDecodeError String


init : Model
init =
    { session = Nothing
    , ui = UiState.init
    , lastRejection = Nothing
    , lastDecodeError = Nothing
    }


paintModeToBool : PaintMode -> Bool
paintModeToBool mode =
    case mode of
        Activate ->
            True

        Deactivate ->
            False


allQuestsCompletedAllDiffs : SessionView -> Bool
allQuestsCompletedAllDiffs session =
    let
        total =
            List.length session.progression.questCatalog
    in
    total
        > 0
        && (List.length session.progression.normalQuestsCompleted >= total)
        && (List.length session.progression.nightmareQuestsCompleted >= total)
        && (List.length session.progression.hellQuestsCompleted >= total)


allWaypointsActiveAllDiffs : SessionView -> Bool
allWaypointsActiveAllDiffs session =
    let
        total =
            List.length session.progression.waypointCatalog
    in
    total
        > 0
        && (List.length session.progression.normalWaypointsActive >= total)
        && (List.length session.progression.nightmareWaypointsActive >= total)
        && (List.length session.progression.hellWaypointsActive >= total)


update : Msg -> Model -> ( Model, OutMsg )
update msg model =
    case msg of
        ReceiveSessionEnvelope rawValue ->
            case Json.Decode.decodeValue sessionEnvelopeDecoder rawValue of
                Ok envelope ->
                    ( applyEnvelope envelope model, NoOut )

                Err err ->
                    let
                        detail =
                            Json.Decode.errorToString err
                    in
                    ( { model
                        | lastDecodeError = Just detail
                        , ui = UiState.setStatusBanner (Just ("Protocol decode error: " ++ detail)) model.ui
                      }
                    , ReportDecodeError detail
                    )

        SwitchView tab ->
            ( { model | ui = UiState.setView tab model.ui }, NoOut )

        SelectResDifficulty difficulty ->
            ( model, EmitIntent (Protocol.SelectResDifficulty { difficulty = difficulty }) )

        SelectSkillMode mode ->
            ( model, EmitIntent (Protocol.SelectSkillMode { mode = mode }) )

        AdjustMercLevel delta ->
            let
                currentLevel =
                    model.session
                        |> Maybe.map (\s -> s.merc.stats.level)
                        |> Maybe.withDefault 1

                nextLevel =
                    clamp 1 98 (currentLevel + delta)
            in
            ( model, EmitIntent (Protocol.SetMercLevel { level = nextLevel }) )

        SetMercLevelExact level ->
            ( model, EmitIntent (Protocol.SetMercLevel { level = clamp 1 98 level }) )

        SelectMercAct actIndex ->
            ( model, EmitIntent (Protocol.SelectMercAct { actIndex = actIndex }) )

        SelectProgressionDifficulty diff ->
            ( { model | ui = UiState.setProgressionDifficulty diff model.ui }, NoOut )

        TogglePaintMode mode ->
            ( { model | ui = UiState.togglePaintMode mode model.ui }, NoOut )

        ClickQuestTile difficulty key ->
            case model.ui.paintMode of
                Nothing ->
                    ( model, EmitIntent (Protocol.ToggleQuest { difficulty = difficulty, key = key }) )

                Just mode ->
                    ( model
                    , EmitIntent
                        (Protocol.SetQuestsBatch
                            { difficulty = difficulty
                            , keys = [ key ]
                            , completed = paintModeToBool mode
                            }
                        )
                    )

        ClickWaypointTile difficulty bit ->
            case model.ui.paintMode of
                Nothing ->
                    ( model, EmitIntent (Protocol.ToggleWaypoint { difficulty = difficulty, bit = bit }) )

                Just mode ->
                    ( model
                    , EmitIntent
                        (Protocol.SetWaypointsBatch
                            { difficulty = difficulty
                            , bits = [ bit ]
                            , active = paintModeToBool mode
                            }
                        )
                    )

        BeginMarquee kind x y initialIds ->
            ( { model | ui = UiState.beginMarquee kind x y initialIds model.ui }, NoOut )

        MoveMarquee x y ids ->
            ( { model | ui = UiState.updateMarquee x y ids model.ui }, NoOut )

        CommitMarquee difficulty ->
            case model.ui.marquee of
                Nothing ->
                    ( model, NoOut )

                Just box ->
                    let
                        nextModel =
                            { model | ui = UiState.clearMarquee model.ui }

                        targetState =
                            paintModeToBool box.mode
                    in
                    case box.kind of
                        DragQuests ->
                            if List.isEmpty box.candidateIds then
                                ( nextModel, NoOut )

                            else
                                ( nextModel
                                , EmitIntent
                                    (Protocol.SetQuestsBatch
                                        { difficulty = difficulty
                                        , keys = box.candidateIds
                                        , completed = targetState
                                        }
                                    )
                                )

                        DragWaypoints ->
                            let
                                bits =
                                    List.filterMap String.toInt box.candidateIds
                            in
                            if List.isEmpty bits then
                                ( nextModel, NoOut )

                            else
                                ( nextModel
                                , EmitIntent
                                    (Protocol.SetWaypointsBatch
                                        { difficulty = difficulty
                                        , bits = bits
                                        , active = targetState
                                        }
                                    )
                                )

        CancelMarquee ->
            ( { model | ui = UiState.clearMarquee model.ui }, NoOut )

        SetQuestsBatch difficulty keys completed ->
            ( { model | ui = UiState.clearMarquee model.ui }
            , EmitIntent
                (Protocol.SetQuestsBatch
                    { difficulty = difficulty
                    , keys = keys
                    , completed = completed
                    }
                )
            )

        SetWaypointsBatch difficulty bits active ->
            ( { model | ui = UiState.clearMarquee model.ui }
            , EmitIntent
                (Protocol.SetWaypointsBatch
                    { difficulty = difficulty
                    , bits = bits
                    , active = active
                    }
                )
            )

        ToggleAllDiffQuests ->
            let
                targetCompleted =
                    model.session
                        |> Maybe.map (not << allQuestsCompletedAllDiffs)
                        |> Maybe.withDefault True
            in
            ( model, EmitIntent (Protocol.SetAllDiffQuests { completed = targetCompleted }) )

        ToggleAllDiffWaypoints ->
            let
                targetActive =
                    model.session
                        |> Maybe.map (not << allWaypointsActiveAllDiffs)
                        |> Maybe.withDefault True
            in
            ( model, EmitIntent (Protocol.SetAllDiffWaypoints { active = targetActive }) )

        SelectContainerFilter filterKey ->
            ( { model | ui = UiState.setContainerFilter filterKey model.ui }, NoOut )

        ClearTempInventory ->
            ( model, EmitIntent Protocol.ClearTempInventory )

        SelectItem maybeIdx ->
            ( { model | ui = UiState.setSelectedItem maybeIdx model.ui }, NoOut )

        FocusField key initialRaw ->
            ( { model | ui = UiState.beginFocusInput key initialRaw model.ui }, NoOut )

        EditFocusedInput raw ->
            ( { model | ui = UiState.updateFocusInput raw model.ui }, NoOut )

        CancelFocusedInput ->
            ( { model | ui = UiState.clearFocusInput model.ui }, NoOut )

        CommitFocusedInput ->
            commitFocusedInput model

        ClearBanner ->
            ( { model | lastRejection = Nothing, ui = UiState.setStatusBanner Nothing model.ui }, NoOut )


characterFieldFromKey : String -> Maybe CharacterField
characterFieldFromKey key =
    case key of
        "level" ->
            Just Level

        "strength" ->
            Just Strength

        "dexterity" ->
            Just Dexterity

        "vitality" ->
            Just Vitality

        "energy" ->
            Just Energy

        "stat_points" ->
            Just StatPoints

        "gold_character" ->
            Just GoldCharacter

        "gold_stash" ->
            Just GoldStash

        _ ->
            Nothing


commitFocusedInput : Model -> ( Model, OutMsg )
commitFocusedInput model =
    case model.ui.focusedInput of
        Nothing ->
            ( model, NoOut )

        Just { key, raw } ->
            let
                nextUi =
                    UiState.clearFocusInput model.ui

                trimmed =
                    String.trim raw
            in
            if key == "char_name" then
                ( { model | ui = nextUi }, EmitIntent (Protocol.SetCharacterName { name = trimmed }) )

            else
                case characterFieldFromKey key of
                    Nothing ->
                        ( { model | ui = nextUi }, NoOut )

                    Just field ->
                        case String.toInt trimmed of
                            Just intVal ->
                                ( { model | ui = nextUi }
                                , EmitIntent (Protocol.SetCharacterField { field = field, value = intVal })
                                )

                            Nothing ->
                                ( { model
                                    | ui =
                                        UiState.setStatusBanner
                                            (Just ("Invalid integer input for " ++ key ++ ": " ++ trimmed))
                                            nextUi
                                  }
                                , NoOut
                                )


applyEnvelope : SessionEnvelope -> Model -> Model
applyEnvelope envelope model =
    let
        nextBanner =
            case envelope.rejection of
                Just rej ->
                    Just (rej.code ++ ": " ++ rej.message)

                Nothing ->
                    model.ui.statusBanner
    in
    { model
        | session =
            case envelope.session of
                Just nextSession ->
                    Just nextSession

                Nothing ->
                    model.session
        , lastRejection = envelope.rejection
        , lastDecodeError = Nothing
        , ui = UiState.setStatusBanner nextBanner model.ui
    }
