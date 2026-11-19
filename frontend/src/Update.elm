module Update exposing
    ( Model
    , Msg(..)
    , OutMsg(..)
    , allQuestsCompletedAllDiffs
    , allWaypointsActiveAllDiffs
    , applyEnvelope
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
        , ItemCardView
        , ItemQualityKind(..)
        , SessionEnvelope
        , SessionView
        , ShopSpawnTarget(..)
        , UiIntent
        , sessionEnvelopeDecoder
        )
import Json.Decode
import State.UiState as UiState exposing (DragKind(..), PaintMode(..), StashTab, UiState, ViewTab)


type alias KeyShortcutPayload =
    { key : String
    , code : String
    , ctrlKey : Bool
    , altKey : Bool
    , metaKey : Bool
    , isInput : Bool
    }


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
    | SelectStatMode EditMode
    | AdjustMercLevel Int
    | SetMercLevelExact Int
    | SelectMercAct Int
    | RestoreReplacedMerc
    | ClearReplacedMerc Bool
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
    | HoverDropTargetSub (Maybe { container : ContainerKind, x : Int, y : Int }) ( Int, Int )
    | MoveTooltipHover Int Int Int Int
    | FocusField String String
    | EditFocusedInput String
    | CommitFocusedInput
    | CancelFocusedInput
    | StepCharacterField String Int
    | ClearBanner
    | ToggleTheme
    | SelectStashTab StashTab
    | ToggleCombatLab
    | SelectSkillTreeTab String
    | HoverSkill (Maybe Int)
    | ClickToggleWeaponSwap
    | ClickDisposeHeldItem
    | ClickAdjustSkillRank Int Int
    | ClickResetSkills
    | ClickResetStats
    | ClickSyncStatPointsToRules
    | SelectShopQualityTab ItemQualityKind
    | SelectShopCategory String
    | ChangeShopPage Int
    | ClickSpawnShopItem String
    | DismissToast
    | TriggerToastUndo
    | MutateItemBase Int (Maybe Int) (Maybe Int) (Maybe Int) (Maybe Int) (Maybe Bool)
    | MutateItemProperty Int Int Int
    | OpenActionPalette Int Int Int
    | OpenContextMenu Int Int Int
    | CloseItemMenu
    | QuickMoveItem Int ContainerKind
    | ContextEditItem Int
    | ContextDuplicateItem Int
    | ContextDeleteItem Int
    | OpenItemEditorModal Int
    | CloseItemEditorModal
    | SetItemEditorSearch String
    | SetItemEditorSkillSearch String String
    | ToggleItemEditorSkillDropdown String
    | SelectItemEditorInjectStat Int Int
    | SetItemEditorInjectValue String
    | SetItemEditorInjectParam String
    | AddItemProperty Int Int Int Int
    | SetItemPropertyParam Int Int Int
    | RemoveItemProperty Int Int
    | ApplyItemForgeDraft
    | DiscardItemForgeDraft
    | HandleKeyShortcut KeyShortcutPayload


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

        SelectStatMode mode ->
            ( model, EmitIntent (Protocol.SelectStatMode { mode = mode }) )

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

        RestoreReplacedMerc ->
            ( model, EmitIntent Protocol.RestoreReplacedMerc )

        ClearReplacedMerc reclaim ->
            ( model, EmitIntent (Protocol.ClearReplacedMerc { reclaimToStorage = reclaim }) )

        SelectItem maybeIdx ->
            let
                nextUi =
                    case maybeIdx of
                        Just idx ->
                            model.ui
                                |> UiState.setSelectedItem (Just idx)
                                |> applyInitialDropTarget model.session idx

                        Nothing ->
                            model.ui
                                |> UiState.setSelectedItem Nothing
                                |> UiState.setHoveredDropTarget Nothing
            in
            ( { model | ui = nextUi }
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

        StepCharacterField key delta ->
            stepCharacterField key delta model

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

        HoverSkill maybeSkillId ->
            ( { model | ui = UiState.setHoveredSkill maybeSkillId model.ui }, NoOut )

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

        ClickResetStats ->
            ( model, EmitIntent Protocol.ResetStats )

        ClickSyncStatPointsToRules ->
            ( model, EmitIntent Protocol.SyncStatPointsToRules )

        HoverItem maybeIdx ->
            ( { model | ui = UiState.setHoveredItem maybeIdx model.ui }, NoOut )

        HoverDropTarget maybeTarget ->
            ( { model | ui = UiState.setHoveredDropTarget maybeTarget model.ui }, NoOut )

        HoverDropTargetSub maybeTarget sub ->
            ( { model | ui = UiState.setHoveredDropTargetSub maybeTarget sub model.ui }, NoOut )

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

        DismissToast ->
            ( { model | ui = UiState.clearToast model.ui }, NoOut )

        TriggerToastUndo ->
            ( { model | ui = UiState.clearToast model.ui }, EmitIntent Protocol.Undo )

        MutateItemBase itemIndex defense currentDurability maxDurability quantity isEthereal ->
            let
                maybeItem =
                    findSessionItem model.session itemIndex

                clampOpt minVal maxVal maybeVal =
                    Maybe.map (clamp minVal maxVal) maybeVal

                schemaDefMax =
                    model.session |> Maybe.map (.schema >> .itemDefenseMax) |> Maybe.withDefault 2037

                schemaQtyMax =
                    model.session |> Maybe.map (.schema >> .itemQuantityMax) |> Maybe.withDefault 511

                maxDur =
                    maybeItem |> Maybe.andThen .maxDurability |> Maybe.withDefault 255

                clampedDef =
                    clampOpt 0 schemaDefMax defense

                clampedCurDur =
                    clampOpt 0 maxDur currentDurability

                clampedQty =
                    clampOpt 1 schemaQtyMax quantity

                noticeFromBase =
                    case ( defense, clampedDef ) of
                        ( Just rawDef, Just cDef ) ->
                            Just (rangeNotice "Defense" 0 schemaDefMax rawDef cDef (maybeItem |> Maybe.andThen .defense |> Maybe.map ((==) cDef) |> Maybe.withDefault False))

                        _ ->
                            case ( currentDurability, clampedCurDur ) of
                                ( Just rawDur, Just cDur ) ->
                                    Just (rangeNotice "Durability" 0 maxDur rawDur cDur (maybeItem |> Maybe.andThen .currentDurability |> Maybe.map ((==) cDur) |> Maybe.withDefault False))

                                _ ->
                                    case ( quantity, clampedQty ) of
                                        ( Just rawQty, Just cQty ) ->
                                            Just (rangeNotice "Quantity" 1 schemaQtyMax rawQty cQty (maybeItem |> Maybe.andThen .quantity |> Maybe.map ((==) cQty) |> Maybe.withDefault False))

                                        _ ->
                                            Nothing

                nextUi =
                    if model.ui.itemEditorModalIndex /= Nothing then
                        case noticeFromBase of
                            Just n ->
                                UiState.setItemEditorNotice (Just n) model.ui

                            Nothing ->
                                model.ui

                    else
                        model.ui
            in
            ( { model | ui = nextUi }
            , EmitIntent
                (Protocol.MutateItemBase
                    { itemIndex = itemIndex
                    , defense = clampedDef
                    , currentDurability = clampedCurDur
                    , maxDurability = maxDurability
                    , quantity = clampedQty
                    , isEthereal = isEthereal
                    }
                )
            )

        MutateItemProperty itemIndex statId delta ->
            let
                maybeProp =
                    findSessionItem model.session itemIndex
                        |> Maybe.andThen (\it -> it.detailedProperties |> List.filter (\p -> p.statId == statId) |> List.head)
            in
            case maybeProp of
                Just prop ->
                    let
                        rawNext =
                            prop.value + delta

                        clampedNext =
                            clamp prop.minValue prop.maxValue rawNext

                        actualDelta =
                            clampedNext - prop.value

                        notice =
                            rangeNotice
                                ("#" ++ String.fromInt prop.statId ++ " " ++ prop.name)
                                prop.minValue
                                prop.maxValue
                                rawNext
                                clampedNext
                                (actualDelta == 0)

                        nextUi =
                            if model.ui.itemEditorModalIndex /= Nothing then
                                UiState.setItemEditorNotice (Just notice) model.ui

                            else
                                model.ui
                    in
                    if actualDelta == 0 then
                        ( { model | ui = nextUi }, NoOut )

                    else
                        ( { model | ui = nextUi }
                        , EmitIntent
                            (Protocol.MutateItemProperty
                                { itemIndex = itemIndex
                                , statId = statId
                                , delta = actualDelta
                                }
                            )
                        )

                Nothing ->
                    ( model
                    , EmitIntent
                        (Protocol.MutateItemProperty
                            { itemIndex = itemIndex
                            , statId = statId
                            , delta = delta
                            }
                        )
                    )

        OpenActionPalette itemIndex x y ->
            ( { model | ui = UiState.openActionPalette itemIndex x y model.ui }, NoOut )

        OpenContextMenu itemIndex x y ->
            ( { model | ui = UiState.openContextMenu itemIndex x y model.ui }, NoOut )

        CloseItemMenu ->
            ( { model | ui = UiState.closeItemMenu model.ui }, NoOut )

        QuickMoveItem itemIndex targetContainer ->
            ( { model | ui = UiState.closeItemMenu model.ui }
            , EmitIntent
                (Protocol.QuickMoveItem
                    { itemIndex = itemIndex
                    , targetContainer = targetContainer
                    }
                )
            )

        ContextEditItem itemIndex ->
            ( { model | ui = UiState.openItemEditorModal itemIndex model.ui }
            , EmitIntent (Protocol.OpenItemForgeDraft { itemIndex = itemIndex })
            )

        OpenItemEditorModal itemIndex ->
            ( { model | ui = UiState.openItemEditorModal itemIndex model.ui }
            , EmitIntent (Protocol.OpenItemForgeDraft { itemIndex = itemIndex })
            )

        CloseItemEditorModal ->
            ( { model | ui = UiState.closeItemEditorModal model.ui }
            , EmitIntent Protocol.DiscardItemForgeDraft
            )

        ApplyItemForgeDraft ->
            ( { model | ui = UiState.closeItemEditorModal model.ui }
            , EmitIntent Protocol.ApplyItemForgeDraft
            )

        DiscardItemForgeDraft ->
            ( { model | ui = UiState.closeItemEditorModal model.ui }
            , EmitIntent Protocol.DiscardItemForgeDraft
            )

        SetItemEditorSearch query ->
            ( { model | ui = UiState.setItemEditorSearch query model.ui }
            , NoOut
            )

        SetItemEditorSkillSearch tid query ->
            ( { model | ui = UiState.setItemEditorSkillSearch tid query model.ui }
            , NoOut
            )

        ToggleItemEditorSkillDropdown tid ->
            ( { model | ui = UiState.toggleSkillDropdown tid model.ui }
            , NoOut
            )

        SelectItemEditorInjectStat statId defaultVal ->
            ( { model | ui = UiState.selectItemEditorInjectStat statId defaultVal model.ui }
            , NoOut
            )

        SetItemEditorInjectValue raw ->
            case String.toInt (String.trim raw) of
                Just val ->
                    let
                        maybeEntry =
                            model.ui.itemEditorInjectStatId
                                |> Maybe.andThen
                                    (\sid ->
                                        model.session
                                            |> Maybe.map .statCatalog
                                            |> Maybe.withDefault []
                                            |> List.filter (\s -> s.statId == sid)
                                            |> List.head
                                    )

                        ( nextVal, nextUi ) =
                            case maybeEntry of
                                Just s ->
                                    let
                                        clamped =
                                            clamp s.minValue s.maxValue val

                                        n =
                                            rangeNotice
                                                ("Inject #" ++ String.fromInt s.statId ++ " " ++ s.name ++ " (" ++ s.valueLabel ++ ")")
                                                s.minValue
                                                s.maxValue
                                                val
                                                clamped
                                                (val /= clamped || (clamped == model.ui.itemEditorInjectValue && (val < s.minValue || val > s.maxValue)))
                                    in
                                    ( clamped
                                    , model.ui
                                        |> UiState.clearFocusInput
                                        |> UiState.setItemEditorNotice (Just n)
                                    )

                                Nothing ->
                                    ( val, UiState.clearFocusInput model.ui )
                    in
                    ( { model | ui = UiState.setItemEditorInjectValue nextVal nextUi }, NoOut )

                Nothing ->
                    ( model, NoOut )

        SetItemEditorInjectParam raw ->
            case String.toInt (String.trim raw) of
                Just param ->
                    let
                        maybeEntry =
                            model.ui.itemEditorInjectStatId
                                |> Maybe.andThen
                                    (\sid ->
                                        model.session
                                            |> Maybe.map .statCatalog
                                            |> Maybe.withDefault []
                                            |> List.filter (\s -> s.statId == sid)
                                            |> List.head
                                    )

                        ( nextParam, nextUi ) =
                            case maybeEntry of
                                Just s ->
                                    let
                                        maxParam =
                                            (2 ^ min 30 s.paramBits) - 1

                                        clamped =
                                            clamp 0 maxParam param

                                        n =
                                            rangeNotice
                                                ("Inject #" ++ String.fromInt s.statId ++ " " ++ s.name ++ " (" ++ s.paramLabel ++ ")")
                                                0
                                                maxParam
                                                param
                                                clamped
                                                (param /= clamped)
                                    in
                                    ( clamped
                                    , model.ui
                                        |> UiState.clearFocusInput
                                        |> UiState.setItemEditorNotice (Just n)
                                    )

                                Nothing ->
                                    ( param, UiState.clearFocusInput model.ui )
                    in
                    ( { model | ui = UiState.setItemEditorInjectParam nextParam nextUi |> UiState.closeSkillDropdown }, NoOut )

                Nothing ->
                    ( model, NoOut )

        AddItemProperty itemIndex statId value param ->
            ( model
            , EmitIntent
                (Protocol.AddItemProperty
                    { itemIndex = itemIndex
                    , statId = statId
                    , value = value
                    , param = param
                    }
                )
            )

        SetItemPropertyParam itemIndex statId param ->
            let
                maybeProp =
                    findSessionItem model.session itemIndex
                        |> Maybe.andThen (\it -> it.detailedProperties |> List.filter (\p -> p.statId == statId) |> List.head)
            in
            case maybeProp of
                Just prop ->
                    let
                        maxParam =
                            (2 ^ min 30 prop.paramBits) - 1

                        clamped =
                            clamp 0 maxParam param

                        atBoundary =
                            clamped == prop.param && param /= prop.param

                        n =
                            rangeNotice
                                ("#" ++ String.fromInt prop.statId ++ " " ++ prop.name ++ " (" ++ prop.paramLabel ++ ")")
                                0
                                maxParam
                                param
                                clamped
                                atBoundary

                        nextUi =
                            if model.ui.itemEditorModalIndex /= Nothing then
                                model.ui
                                    |> UiState.clearFocusInput
                                    |> UiState.setItemEditorNotice (Just n)

                            else
                                model.ui
                    in
                    if clamped == prop.param then
                        ( { model | ui = nextUi }, NoOut )

                    else
                        ( { model | ui = UiState.closeSkillDropdown nextUi }
                        , EmitIntent
                            (Protocol.SetItemPropertyParam
                                { itemIndex = itemIndex
                                , statId = statId
                                , param = clamped
                                }
                            )
                        )

                Nothing ->
                    ( { model | ui = UiState.closeSkillDropdown model.ui }
                    , EmitIntent
                        (Protocol.SetItemPropertyParam
                            { itemIndex = itemIndex
                            , statId = statId
                            , param = param
                            }
                        )
                    )

        RemoveItemProperty itemIndex statId ->
            ( model
            , EmitIntent
                (Protocol.RemoveItemProperty
                    { itemIndex = itemIndex
                    , statId = statId
                    }
                )
            )

        ContextDuplicateItem itemIndex ->
            ( { model | ui = UiState.closeItemMenu model.ui }
            , EmitIntent (Protocol.DuplicateItem { itemIndex = itemIndex })
            )

        ContextDeleteItem itemIndex ->
            ( { model | ui = UiState.closeItemMenu model.ui }
            , EmitIntent (Protocol.DeleteItem { itemIndex = itemIndex })
            )

        HandleKeyShortcut payload ->
            handleKeyShortcut payload model


handleKeyShortcut : KeyShortcutPayload -> Model -> ( Model, OutMsg )
handleKeyShortcut payload model =
    let
        lowerKey =
            String.toLower payload.key

        isSpace =
            payload.key == " " || payload.code == "Space"

        heldIdx =
            Maybe.andThen .selectedMoveItemIndex model.session
    in
    if payload.isInput then
        ( model, NoOut )

    else if payload.key == "Escape" then
        if model.ui.itemEditorModalIndex /= Nothing then
            ( { model | ui = UiState.closeItemEditorModal model.ui }
            , EmitIntent Protocol.DiscardItemForgeDraft
            )

        else if model.ui.itemMenu /= UiState.NoItemMenu then
            ( { model | ui = UiState.closeItemMenu model.ui }, NoOut )

        else if heldIdx /= Nothing then
            ( { model | ui = UiState.setSelectedItem Nothing model.ui }
            , EmitIntent (Protocol.SelectItemForMove { itemIndex = Nothing })
            )

        else
            ( model, NoOut )

    else if payload.ctrlKey && not payload.altKey && not payload.metaKey then
        if lowerKey == "z" then
            ( { model | ui = UiState.closeItemMenu model.ui }, EmitIntent Protocol.Undo )

        else if lowerKey == "y" then
            ( { model | ui = UiState.closeItemMenu model.ui }, EmitIntent Protocol.Redo )

        else
            ( model, NoOut )

    else if not payload.ctrlKey && not payload.altKey && not payload.metaKey then
        if isSpace then
            case ( heldIdx, model.ui.hoveredDropTarget, model.ui.hoveredItemIndex ) of
                ( Just idx, Just tgt, _ ) ->
                    let
                        ( heldW, heldH, allowedTargets ) =
                            case model.session of
                                Just s ->
                                    let
                                        ( w, h ) =
                                            (s.items ++ s.mercItems ++ s.mercReplacedItems)
                                                |> List.filter (\it -> it.index == idx)
                                                |> List.head
                                                |> Maybe.map (\it -> ( it.width, it.height ))
                                                |> Maybe.withDefault ( 1, 1 )
                                    in
                                    ( w, h, s.allowedMoveTargets )

                                Nothing ->
                                    ( 1, 1, [] )

                        ( dropX, dropY ) =
                            UiState.resolveGridDropAnchorWithSub allowedTargets tgt.container heldW heldH tgt.x tgt.y model.ui.hoveredDropSub
                    in
                    ( { model | ui = UiState.closeItemMenu model.ui }
                    , EmitIntent
                        (Protocol.MoveItem
                            { itemIndex = idx
                            , targetContainer = tgt.container
                            , x = dropX
                            , y = dropY
                            }
                        )
                    )

                ( Nothing, _, Just hovIdx ) ->
                    let
                        nextUi =
                            model.ui
                                |> UiState.setSelectedItem (Just hovIdx)
                                |> UiState.closeItemMenu
                                |> applyInitialDropTarget model.session hovIdx
                    in
                    ( { model | ui = nextUi }
                    , EmitIntent (Protocol.SelectItemForMove { itemIndex = Just hovIdx })
                    )

                _ ->
                    ( model, NoOut )

        else if lowerKey == "w" then
            ( model, EmitIntent Protocol.ToggleWeaponSwap )

        else if lowerKey == "f" && heldIdx == Nothing && model.ui.itemEditorModalIndex == Nothing then
            let
                targetIdx =
                    case model.ui.hoveredItemIndex of
                        Just hovIdx ->
                            Just hovIdx

                        Nothing ->
                            model.ui.selectedItemIndex
            in
            case targetIdx of
                Just idx ->
                    ( { model | ui = UiState.openItemEditorModal idx model.ui }
                    , EmitIntent (Protocol.OpenItemForgeDraft { itemIndex = idx })
                    )

                Nothing ->
                    ( model, NoOut )

        else if heldIdx == Nothing then
            case model.session of
                Just session ->
                    let
                        undoableOrigin =
                            model.ui.quickMoveOrigin
                                |> Maybe.andThen
                                    (\origin ->
                                        if
                                            session.revision
                                                /= origin.sentRevision
                                                && session.canUndo
                                                && (hotkeyTarget origin.container lowerKey == Just origin.destContainer)
                                        then
                                            Just origin

                                        else
                                            Nothing
                                    )
                    in
                    case undoableOrigin of
                        Just origin ->
                            ( { model
                                | ui =
                                    model.ui
                                        |> UiState.clearQuickMoveOrigin
                                        |> UiState.setHoveredItem (Just origin.itemIndex)
                                        |> UiState.closeItemMenu
                              }
                            , EmitIntent Protocol.Undo
                            )

                        Nothing ->
                            let
                                targetHoverIdx =
                                    case model.ui.hoveredItemIndex of
                                        Just idx ->
                                            Just idx

                                        Nothing ->
                                            Maybe.map .itemIndex model.ui.quickMoveOrigin
                            in
                            case targetHoverIdx |> Maybe.andThen (findSessionItem (Just session)) of
                                Just item ->
                                    case hotkeyTarget item.container lowerKey of
                                        Just dest ->
                                            ( { model
                                                | ui =
                                                    model.ui
                                                        |> UiState.setQuickMoveOrigin
                                                            { itemIndex = item.index
                                                            , destContainer = dest
                                                            , container = item.container
                                                            , x = item.x
                                                            , y = item.y
                                                            , width = item.width
                                                            , height = item.height
                                                            , sentRevision = session.revision
                                                            }
                                                        |> UiState.closeItemMenu
                                              }
                                            , EmitIntent (Protocol.QuickMoveItem { itemIndex = item.index, targetContainer = dest })
                                            )

                                        Nothing ->
                                            ( model, NoOut )

                                Nothing ->
                                    ( model, NoOut )

                Nothing ->
                    ( model, NoOut )

        else
            ( model, NoOut )

    else
        ( model, NoOut )


hotkeyTarget : ContainerKind -> String -> Maybe ContainerKind
hotkeyTarget currentContainer lowerKey =
    case lowerKey of
        "s" ->
            Just
                (if currentContainer == Stash then
                    Inventory

                 else
                    Stash
                )

        "a" ->
            if currentContainer == Inventory then
                Nothing

            else
                Just Inventory

        "i" ->
            if currentContainer == Inventory then
                Nothing

            else
                Just Inventory

        "t" ->
            Just
                (if currentContainer == TempInventory then
                    Inventory

                 else
                    TempInventory
                )

        "e" ->
            Just
                (if currentContainer == Equipment then
                    Inventory

                 else
                    Equipment
                )

        "d" ->
            Just
                (if currentContainer == Equipment then
                    Inventory

                 else
                    Equipment
                )

        "r" ->
            Just
                (if currentContainer == MercEquipment || currentContainer == MercReplaced then
                    Inventory

                 else
                    MercEquipment
                )

        "m" ->
            Just
                (if currentContainer == MercEquipment || currentContainer == MercReplaced then
                    Inventory

                 else
                    MercEquipment
                )

        "q" ->
            Just
                (if currentContainer == Cube then
                    Inventory

                 else
                    Cube
                )

        "c" ->
            Just
                (if currentContainer == Cube then
                    Inventory

                 else
                    Cube
                )

        _ ->
            Nothing


findSessionItem : Maybe SessionView -> Int -> Maybe ItemCardView
findSessionItem maybeSession idx =
    maybeSession
        |> Maybe.map (\s -> s.items ++ s.mercItems ++ s.mercReplacedItems)
        |> Maybe.withDefault []
        |> List.filter (\it -> it.index == idx)
        |> List.head


rangeNotice : String -> Int -> Int -> Int -> Int -> Bool -> UiState.ModalNotice
rangeNotice label minVal maxVal rawVal clampedVal atBoundary =
    if rawVal /= clampedVal || atBoundary then
        UiState.NoticeWarning
            (label
                ++ " at boundary "
                ++ String.fromInt clampedVal
                ++ " (allowed "
                ++ String.fromInt minVal
                ++ ".."
                ++ String.fromInt maxVal
                ++ ")."
            )

    else
        UiState.NoticeInfo
            (label
                ++ ": "
                ++ String.fromInt clampedVal
                ++ " (allowed "
                ++ String.fromInt minVal
                ++ ".."
                ++ String.fromInt maxVal
                ++ ")"
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


applyInitialDropTarget : Maybe SessionView -> Int -> UiState -> UiState
applyInitialDropTarget maybeSession itemIdx ui =
    let
        maybeTarget =
            maybeSession
                |> Maybe.andThen
                    (\s ->
                        (s.items ++ s.mercItems ++ s.mercReplacedItems)
                            |> List.filter (\it -> it.index == itemIdx)
                            |> List.head
                    )
                |> Maybe.andThen dropTargetFromItem
    in
    UiState.setHoveredDropTarget maybeTarget ui


dropTargetFromItem : ItemCardView -> Maybe { container : ContainerKind, x : Int, y : Int }
dropTargetFromItem item =
    case item.container of
        Equipment ->
            item.slotLabel
                |> Maybe.andThen equipmentBodyLocFromLabel
                |> Maybe.map (\loc -> { container = Equipment, x = loc, y = 0 })

        MercEquipment ->
            Just { container = MercEquipment, x = item.x, y = 0 }

        Inventory ->
            Just
                { container = Inventory
                , x = item.x + ((max 1 item.width - 1) // 2)
                , y = item.y + ((max 1 item.height - 1) // 2)
                }

        Stash ->
            Just
                { container = Stash
                , x = item.x + ((max 1 item.width - 1) // 2)
                , y = item.y + ((max 1 item.height - 1) // 2)
                }

        Cube ->
            Just
                { container = Cube
                , x = item.x + ((max 1 item.width - 1) // 2)
                , y = item.y + ((max 1 item.height - 1) // 2)
                }

        Belt ->
            Just { container = Belt, x = item.x, y = item.y }

        TempInventory ->
            Just
                { container = TempInventory
                , x = item.x + ((max 1 item.width - 1) // 2)
                , y = item.y + ((max 1 item.height - 1) // 2)
                }

        _ ->
            Nothing


equipmentBodyLocFromLabel : String -> Maybe Int
equipmentBodyLocFromLabel lbl =
    case lbl of
        "Head" ->
            Just 1

        "Helm" ->
            Just 1

        "Amulet" ->
            Just 2

        "Neck" ->
            Just 2

        "Armor" ->
            Just 3

        "Torso" ->
            Just 3

        "Body" ->
            Just 3

        "Weapon1" ->
            Just 4

        "Right Hand" ->
            Just 4

        "Weapon2" ->
            Just 5

        "Left Hand" ->
            Just 5

        "Shield" ->
            Just 5

        "Ring1" ->
            Just 6

        "Left Finger" ->
            Just 6

        "Ring2" ->
            Just 7

        "Right Finger" ->
            Just 7

        "Belt" ->
            Just 8

        "Waist" ->
            Just 8

        "Boots" ->
            Just 9

        "Feet" ->
            Just 9

        "Gloves" ->
            Just 10

        "Hands" ->
            Just 10

        "Weapon1Swap" ->
            Just 11

        "Weapon2Swap" ->
            Just 12

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

                    Nothing ->
                        commitItemFocusedInput model key trimmed nextUi


stepCharacterField : String -> Int -> Model -> ( Model, OutMsg )
stepCharacterField key delta model =
    case ( model.session, characterFieldFromKey key ) of
        ( Just session, Just field ) ->
            let
                schema =
                    session.schema

                statFloor baseMin curVal =
                    if session.statMode == Protocol.Legit then
                        min baseMin curVal

                    else
                        schema.statMin

                statCeil curVal =
                    if session.statMode == Protocol.Legit then
                        min schema.statMax (curVal + max 0 session.stats.unspentStatPoints)

                    else
                        schema.statMax

                spec =
                    case key of
                        "level" ->
                            { label = "Level", currentVal = session.charLevel, minVal = schema.levelMin, maxVal = schema.levelMax }

                        "strength" ->
                            { label = "STR", currentVal = session.stats.strengthBase, minVal = statFloor schema.classBaseStr session.stats.strengthBase, maxVal = statCeil session.stats.strengthBase }

                        "dexterity" ->
                            { label = "DEX", currentVal = session.stats.dexterityBase, minVal = statFloor schema.classBaseDex session.stats.dexterityBase, maxVal = statCeil session.stats.dexterityBase }

                        "vitality" ->
                            { label = "VIT", currentVal = session.stats.vitalityBase, minVal = statFloor schema.classBaseVit session.stats.vitalityBase, maxVal = statCeil session.stats.vitalityBase }

                        "energy" ->
                            { label = "ENE", currentVal = session.stats.energyBase, minVal = statFloor schema.classBaseEne session.stats.energyBase, maxVal = statCeil session.stats.energyBase }

                        _ ->
                            { label = key, currentVal = 0, minVal = 0, maxVal = 0 }

                rawNext =
                    spec.currentVal + delta

                clamped =
                    clamp spec.minVal spec.maxVal rawNext

                nextUi =
                    model.ui
                        |> UiState.clearFocusInput
                        |> UiState.setStatusBanner
                            (if rawNext /= clamped || clamped == spec.currentVal then
                                Just (spec.label ++ " at boundary " ++ String.fromInt clamped ++ " (allowed " ++ String.fromInt spec.minVal ++ ".." ++ String.fromInt spec.maxVal ++ ").")

                             else
                                Nothing
                            )
            in
            if clamped == spec.currentVal then
                ( { model | ui = nextUi }, NoOut )

            else
                ( { model | ui = nextUi }
                , EmitIntent (Protocol.SetCharacterField { field = field, value = clamped })
                )

        _ ->
            ( model, NoOut )


commitItemFocusedInput : Model -> String -> String -> UiState -> ( Model, OutMsg )
commitItemFocusedInput model key trimmed nextUi =
    let
        findItem idx =
            model.session
                |> Maybe.map (\s -> s.items ++ s.mercItems ++ s.mercReplacedItems)
                |> Maybe.withDefault []
                |> List.filter (\it -> it.index == idx)
                |> List.head

        findCatalogStat sid =
            model.session
                |> Maybe.map .statCatalog
                |> Maybe.withDefault []
                |> List.filter (\s -> s.statId == sid)
                |> List.head

        commitClampedUi label minVal maxVal unchangedVal toResult =
            case String.toInt trimmed of
                Nothing ->
                    let
                        errMsg =
                            "Invalid integer for "
                                ++ label
                                ++ ": \""
                                ++ trimmed
                                ++ "\" (allowed "
                                ++ String.fromInt minVal
                                ++ ".."
                                ++ String.fromInt maxVal
                                ++ ")."
                    in
                    ( { model
                        | ui =
                            nextUi
                                |> UiState.setStatusBanner (Just errMsg)
                                |> UiState.setItemEditorNotice (Just (UiState.NoticeError errMsg))
                      }
                    , NoOut
                    )

                Just rawInt ->
                    let
                        clamped =
                            clamp minVal maxVal rawInt

                        uiAfterClamp =
                            if clamped /= rawInt then
                                UiState.setItemEditorNotice
                                    (Just
                                        (UiState.NoticeWarning
                                            (label
                                                ++ " clamped from "
                                                ++ String.fromInt rawInt
                                                ++ " to "
                                                ++ String.fromInt clamped
                                                ++ " (allowed "
                                                ++ String.fromInt minVal
                                                ++ ".."
                                                ++ String.fromInt maxVal
                                                ++ ")."
                                            )
                                        )
                                    )
                                    nextUi

                            else
                                nextUi

                        ( finalUi, outMsg ) =
                            if unchangedVal == Just clamped then
                                ( uiAfterClamp, NoOut )

                            else
                                toResult clamped uiAfterClamp
                    in
                    ( { model | ui = finalUi }, outMsg )

        commitClampedField label minVal maxVal unchangedVal toOut =
            commitClampedUi label minVal maxVal unchangedVal (\clamped uiState -> ( uiState, toOut clamped ))
    in
    case String.split ":" key of
        [ "item_prop", itemIdxStr, statIdStr ] ->
            case ( String.toInt itemIdxStr, String.toInt statIdStr ) of
                ( Just itemIdx, Just statId ) ->
                    let
                        maybeProp =
                            findItem itemIdx
                                |> Maybe.andThen
                                    (\it ->
                                        it.detailedProperties
                                            |> List.filter (\p -> p.statId == statId)
                                            |> List.head
                                    )

                        ( label, minVal, maxVal ) =
                            maybeProp
                                |> Maybe.map (\p -> ( "#" ++ String.fromInt statId ++ " " ++ p.name, p.minValue, p.maxValue ))
                                |> Maybe.withDefault ( "Property #" ++ String.fromInt statId, -32768, 32767 )

                        currentVal =
                            maybeProp |> Maybe.map .value
                    in
                    commitClampedField
                        label
                        minVal
                        maxVal
                        currentVal
                        (\clamped ->
                            EmitIntent
                                (Protocol.MutateItemProperty
                                    { itemIndex = itemIdx
                                    , statId = statId
                                    , delta = clamped - Maybe.withDefault clamped currentVal
                                    }
                                )
                        )

                _ ->
                    ( { model | ui = nextUi }, NoOut )

        [ "item_param", itemIdxStr, statIdStr ] ->
            case ( String.toInt itemIdxStr, String.toInt statIdStr ) of
                ( Just itemIdx, Just statId ) ->
                    let
                        maybeProp =
                            findItem itemIdx
                                |> Maybe.andThen
                                    (\it ->
                                        it.detailedProperties
                                            |> List.filter (\p -> p.statId == statId)
                                            |> List.head
                                    )

                        ( label, maxParam, currentParam ) =
                            case maybeProp of
                                Just p ->
                                    ( "#" ++ String.fromInt statId ++ " " ++ p.name ++ " Param"
                                    , (2 ^ min 30 p.paramBits) - 1
                                    , Just p.param
                                    )

                                Nothing ->
                                    ( "Property #" ++ String.fromInt statId ++ " Param", 65535, Nothing )
                    in
                    commitClampedField
                        label
                        0
                        maxParam
                        currentParam
                        (\clamped ->
                            EmitIntent
                                (Protocol.SetItemPropertyParam
                                    { itemIndex = itemIdx
                                    , statId = statId
                                    , param = clamped
                                    }
                                )
                        )

                _ ->
                    ( { model | ui = nextUi }, NoOut )

        [ "item_param_lvl", itemIdxStr, statIdStr ] ->
            case ( String.toInt itemIdxStr, String.toInt statIdStr ) of
                ( Just itemIdx, Just statId ) ->
                    let
                        maybeProp =
                            findItem itemIdx
                                |> Maybe.andThen
                                    (\it ->
                                        it.detailedProperties
                                            |> List.filter (\p -> p.statId == statId)
                                            |> List.head
                                    )

                        packedSpec =
                            model.session
                                |> Maybe.map (.paramCatalog >> .packedSkill)
                                |> Maybe.withDefault { stride = 64, minLevel = 1, maxLevel = 63 }

                        stride =
                            max 1 packedSpec.stride

                        currentParam =
                            maybeProp |> Maybe.map .param |> Maybe.withDefault 0

                        currentLvl =
                            modBy stride currentParam

                        skillId =
                            currentParam // stride

                        label =
                            maybeProp
                                |> Maybe.map (\p -> "#" ++ String.fromInt statId ++ " " ++ p.name ++ " Skill Level")
                                |> Maybe.withDefault ("Property #" ++ String.fromInt statId ++ " Skill Level")
                    in
                    commitClampedField
                        label
                        packedSpec.minLevel
                        packedSpec.maxLevel
                        (Just currentLvl)
                        (\clamped ->
                            EmitIntent
                                (Protocol.SetItemPropertyParam
                                    { itemIndex = itemIdx
                                    , statId = statId
                                    , param = skillId * stride + clamped
                                    }
                                )
                        )

                _ ->
                    ( { model | ui = nextUi }, NoOut )

        [ "inject_val", statIdStr ] ->
            case String.toInt statIdStr |> Maybe.andThen findCatalogStat of
                Just entry ->
                    commitClampedUi
                        ("#" ++ String.fromInt entry.statId ++ " " ++ entry.name ++ " (" ++ entry.valueLabel ++ ")")
                        entry.minValue
                        entry.maxValue
                        Nothing
                        (\clamped uiState -> ( UiState.setItemEditorInjectValue clamped uiState, NoOut ))

                Nothing ->
                    ( { model | ui = nextUi }, NoOut )

        [ "inject_param", statIdStr ] ->
            case String.toInt statIdStr |> Maybe.andThen findCatalogStat of
                Just entry ->
                    let
                        maxParam =
                            (2 ^ min 30 entry.paramBits) - 1
                    in
                    commitClampedUi
                        ("#" ++ String.fromInt entry.statId ++ " " ++ entry.name ++ " (" ++ entry.paramLabel ++ ")")
                        0
                        maxParam
                        Nothing
                        (\clamped uiState -> ( UiState.setItemEditorInjectParam clamped uiState, NoOut ))

                Nothing ->
                    ( { model | ui = nextUi }, NoOut )

        [ "inject_param_lvl", statIdStr ] ->
            case String.toInt statIdStr |> Maybe.andThen findCatalogStat of
                Just entry ->
                    let
                        packedSpec =
                            model.session
                                |> Maybe.map (.paramCatalog >> .packedSkill)
                                |> Maybe.withDefault { stride = 64, minLevel = 1, maxLevel = 63 }

                        stride =
                            max 1 packedSpec.stride

                        skillId =
                            model.ui.itemEditorInjectParam // stride
                    in
                    commitClampedUi
                        ("#" ++ String.fromInt entry.statId ++ " " ++ entry.name ++ " Skill Level")
                        packedSpec.minLevel
                        packedSpec.maxLevel
                        Nothing
                        (\clamped uiState -> ( UiState.setItemEditorInjectParam (skillId * stride + clamped) uiState, NoOut ))

                Nothing ->
                    ( { model | ui = nextUi }, NoOut )

        [ "item_def", itemIdxStr ] ->
            case String.toInt itemIdxStr of
                Just itemIdx ->
                    let
                        defMax =
                            model.session |> Maybe.map (.schema >> .itemDefenseMax) |> Maybe.withDefault 2037
                    in
                    commitClampedField
                        "Defense"
                        0
                        defMax
                        (findItem itemIdx |> Maybe.andThen .defense)
                        (\clamped ->
                            EmitIntent
                                (Protocol.MutateItemBase
                                    { itemIndex = itemIdx
                                    , defense = Just clamped
                                    , currentDurability = Nothing
                                    , maxDurability = Nothing
                                    , quantity = Nothing
                                    , isEthereal = Nothing
                                    }
                                )
                        )

                Nothing ->
                    ( { model | ui = nextUi }, NoOut )

        [ "item_dur", itemIdxStr ] ->
            case String.toInt itemIdxStr of
                Just itemIdx ->
                    let
                        maybeItem =
                            findItem itemIdx

                        maxDur =
                            maybeItem |> Maybe.andThen .maxDurability |> Maybe.withDefault 255
                    in
                    commitClampedField
                        "Durability"
                        0
                        maxDur
                        (maybeItem |> Maybe.andThen .currentDurability)
                        (\clamped ->
                            EmitIntent
                                (Protocol.MutateItemBase
                                    { itemIndex = itemIdx
                                    , defense = Nothing
                                    , currentDurability = Just clamped
                                    , maxDurability = Nothing
                                    , quantity = Nothing
                                    , isEthereal = Nothing
                                    }
                                )
                        )

                Nothing ->
                    ( { model | ui = nextUi }, NoOut )

        [ "item_qty", itemIdxStr ] ->
            case String.toInt itemIdxStr of
                Just itemIdx ->
                    let
                        qtyMax =
                            model.session |> Maybe.map (.schema >> .itemQuantityMax) |> Maybe.withDefault 511
                    in
                    commitClampedField
                        "Quantity"
                        1
                        qtyMax
                        (findItem itemIdx |> Maybe.andThen .quantity)
                        (\clamped ->
                            EmitIntent
                                (Protocol.MutateItemBase
                                    { itemIndex = itemIdx
                                    , defense = Nothing
                                    , currentDurability = Nothing
                                    , maxDurability = Nothing
                                    , quantity = Just clamped
                                    , isEthereal = Nothing
                                    }
                                )
                        )

                Nothing ->
                    ( { model | ui = nextUi }, NoOut )

        _ ->
            ( { model | ui = nextUi }, NoOut )


applyEnvelope : SessionEnvelope -> Model -> Model
applyEnvelope envelope model =
    let
        nextBanner =
            case envelope.rejection of
                Just rej ->
                    Just (rej.rawCode ++ ": " ++ rej.message)

                Nothing ->
                    model.ui.statusBanner

        uiWithToast =
            case envelope.rejection of
                Just rej ->
                    let
                        withToast =
                            model.ui
                                |> UiState.clearQuickMoveOrigin
                                |> UiState.setToast rej
                    in
                    if withToast.itemEditorModalIndex /= Nothing then
                        UiState.setItemEditorNotice (Just (UiState.NoticeError (rej.rawCode ++ ": " ++ rej.message))) withToast

                    else
                        withToast

                Nothing ->
                    model.ui

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
                                -- Merge cached static shopCatalog and statCatalog if incoming session omitted them
                                let
                                    mergedCatalog =
                                        if List.isEmpty newSession.shopCatalog then
                                            currentSession.shopCatalog

                                        else
                                            newSession.shopCatalog

                                    mergedStatCatalog =
                                        if List.isEmpty newSession.statCatalog then
                                            currentSession.statCatalog

                                        else
                                            newSession.statCatalog

                                    mergedParamCatalog =
                                        if List.isEmpty newSession.paramCatalog.skills then
                                            currentSession.paramCatalog

                                        else
                                            newSession.paramCatalog
                                in
                                Just
                                    { newSession
                                        | shopCatalog = mergedCatalog
                                        , statCatalog = mergedStatCatalog
                                        , paramCatalog = mergedParamCatalog
                                    }

                        Nothing ->
                            Just newSession

                Nothing ->
                    model.session

        wasMoving =
            Maybe.andThen .selectedMoveItemIndex model.session /= Nothing

        isNowMoving =
            Maybe.andThen .selectedMoveItemIndex nextSession /= Nothing

        uiAfterMove =
            if wasMoving && not isNowMoving then
                case ( nextSession, uiWithToast.hoveredDropTarget ) of
                    ( Just s, Just tgt ) ->
                        uiWithToast
                            |> UiState.setHoveredItem (findItemAtDropTarget s tgt)
                            |> UiState.setHoveredDropTarget Nothing

                    _ ->
                        uiWithToast

            else
                uiWithToast
    in
    { model
        | session = nextSession
        , lastRejection = envelope.rejection
        , lastDecodeError = Nothing
        , ui = UiState.setStatusBanner nextBanner uiAfterMove
    }


findItemAtDropTarget : SessionView -> { container : ContainerKind, x : Int, y : Int } -> Maybe Int
findItemAtDropTarget session tgt =
    let
        equipAcceptedLabels bodyLoc =
            case bodyLoc of
                1 ->
                    [ "Head", "Helm" ]

                2 ->
                    [ "Amulet", "Neck" ]

                3 ->
                    [ "Armor", "Torso", "Body" ]

                4 ->
                    [ "Weapon1", "Right Hand" ]

                5 ->
                    [ "Weapon2", "Left Hand", "Shield" ]

                6 ->
                    [ "Ring1", "Left Finger" ]

                7 ->
                    [ "Ring2", "Right Finger" ]

                8 ->
                    [ "Belt", "Waist" ]

                9 ->
                    [ "Boots", "Feet" ]

                10 ->
                    [ "Gloves", "Hands" ]

                11 ->
                    [ "Weapon1Swap" ]

                12 ->
                    [ "Weapon2Swap" ]

                _ ->
                    []

        pool =
            case tgt.container of
                MercEquipment ->
                    session.mercItems

                MercReplaced ->
                    session.mercReplacedItems

                _ ->
                    session.items

        matchesTarget it =
            if it.container /= tgt.container then
                False

            else
                case tgt.container of
                    Equipment ->
                        case it.slotLabel of
                            Just lbl ->
                                List.member lbl (equipAcceptedLabels tgt.x)

                            Nothing ->
                                False

                    MercEquipment ->
                        it.x == tgt.x

                    MercReplaced ->
                        it.x == tgt.x

                    _ ->
                        let
                            w =
                                max 1 it.width

                            h =
                                max 1 it.height
                        in
                        tgt.x >= it.x && tgt.x < it.x + w && tgt.y >= it.y && tgt.y < it.y + h
    in
    pool
        |> List.filter matchesTarget
        |> List.head
        |> Maybe.map .index
