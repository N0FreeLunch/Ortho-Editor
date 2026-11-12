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
        , ContainerKind(..)
        , DifficultyMode(..)
        , EditMode(..)
        , ItemQualityKind(..)
        , SessionEnvelope
        , SessionView
        , ShopSpawnTarget(..)
        , UiIntent
        , sessionEnvelopeDecoder
        )
import Json.Decode
import State.UiState as UiState exposing (DragKind(..), PaintMode(..), StashTab, UiState, ViewTab)


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
    | ExtendMarquee DragKind String
    | CommitMarquee DifficultyMode
    | CancelMarquee
    | SetQuestsBatch DifficultyMode (List String) Bool
    | SetWaypointsBatch DifficultyMode (List Int) Bool
    | ToggleAllDiffQuests
    | ToggleAllDiffWaypoints
    | SelectContainerFilter (Maybe ContainerKind)
    | ClearTempInventory
    | SelectItem (Maybe Int)
    | MoveSelectedItemTo ContainerKind Int Int
    | ClickUndo
    | ClickRedo
    | HoverItem (Maybe Int)
    | HoverDropTarget (Maybe { container : ContainerKind, x : Int, y : Int })
    | MoveTooltipHover Int Int Int Int
    | FocusField String String
    | EditFocusedInput String
    | CommitFocusedInput
    | CancelFocusedInput
    | ClearBanner
    | ToggleTheme
    | SelectStashTab StashTab
    | ToggleCombatLab
    | SelectSkillTreeTab String
    | ClickToggleWeaponSwap
    | ClickDisposeHeldItem
    | ClickAdjustSkillRank Int Int
    | ClickResetSkills
    | SelectShopQualityTab ItemQualityKind
    | SelectShopCategory String
    | ChangeShopPage Int
    | ClickSpawnShopItem String


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

        ExtendMarquee kind tileId ->
            ( { model | ui = UiState.extendMarquee kind tileId model.ui }, NoOut )

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
            ( { model | ui = UiState.setSelectedItem maybeIdx model.ui }
            , EmitIntent (Protocol.SelectItemForMove { itemIndex = maybeIdx })
            )

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

        ToggleTheme ->
            ( { model | ui = UiState.toggleTheme model.ui }, NoOut )

        SelectStashTab tab ->
            ( { model | ui = UiState.setStashTab tab model.ui }, NoOut )

        ToggleCombatLab ->
            ( { model | ui = UiState.toggleCombatLab model.ui }, NoOut )

        SelectSkillTreeTab treeId ->
            ( { model | ui = UiState.setSkillTreeTab treeId model.ui }, NoOut )

        ClickToggleWeaponSwap ->
            ( model, EmitIntent Protocol.ToggleWeaponSwap )

        ClickDisposeHeldItem ->
            ( model, EmitIntent Protocol.DisposeHeldItem )

        ClickAdjustSkillRank skillId delta ->
            ( model
            , EmitIntent
                (Protocol.AdjustSkillRank
                    { skillId = skillId
                    , delta = delta
                    }
                )
            )

        ClickResetSkills ->
            ( model, EmitIntent Protocol.ResetSkills )

        HoverItem maybeIdx ->
            ( { model | ui = UiState.setHoveredItem maybeIdx model.ui }, NoOut )

        HoverDropTarget maybeTarget ->
            ( { model | ui = UiState.setHoveredDropTarget maybeTarget model.ui }, NoOut )

        MoveSelectedItemTo targetContainer x y ->
            case Maybe.andThen .selectedMoveItemIndex model.session of
                Just idx ->
                    ( model
                    , EmitIntent
                        (Protocol.MoveItem
                            { itemIndex = idx
                            , targetContainer = targetContainer
                            , x = x
                            , y = y
                            }
                        )
                    )

                Nothing ->
                    ( model, NoOut )

        ClickUndo ->
            ( model, EmitIntent Protocol.Undo )

        ClickRedo ->
            ( model, EmitIntent Protocol.Redo )

        MoveTooltipHover x y vw vh ->
            ( { model | ui = UiState.setHoverMousePos x y vw vh model.ui }, NoOut )

        SelectShopQualityTab tab ->
            ( { model | ui = UiState.setShopQualityTab tab model.ui }, NoOut )

        SelectShopCategory cat ->
            ( { model | ui = UiState.setShopCategoryFilter cat model.ui }, NoOut )

        ChangeShopPage delta ->
            ( { model | ui = UiState.setShopPage (model.ui.shopPage + delta) model.ui }, NoOut )

        ClickSpawnShopItem catalogId ->
            ( model
            , EmitIntent
                (Protocol.SpawnShopItem
                    { catalogId = catalogId
                    , targetContainer = SpawnAuto
                    }
                )
            )


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

        nextSession =
            case envelope.session of
                Just newSession ->
                    case model.session of
                        Just currentSession ->
                            let
                                isStaleWithinSession =
                                    newSession.fixtureLabel == currentSession.fixtureLabel
                                        && currentSession.dirty
                                        && newSession.dirty
                                        && newSession.revision > 0
                                        && currentSession.revision > 0
                                        && newSession.revision < currentSession.revision
                            in
                            if isStaleWithinSession then
                                -- Drop stale out-of-order response within the same dirty editing session
                                model.session

                            else
                                -- Merge cached static shopCatalog if incoming session omitted it
                                let
                                    mergedCatalog =
                                        if List.isEmpty newSession.shopCatalog then
                                            currentSession.shopCatalog

                                        else
                                            newSession.shopCatalog
                                in
                                Just { newSession | shopCatalog = mergedCatalog }

                        Nothing ->
                            Just newSession

                Nothing ->
                    model.session
    in
    { model
        | session = nextSession
        , lastRejection = envelope.rejection
        , lastDecodeError = Nothing
        , ui = UiState.setStatusBanner nextBanner model.ui
    }
