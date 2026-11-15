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
    | SelectItemEditorInjectStat Int Int
    | SetItemEditorInjectValue String
    | AddItemProperty Int Int Int
    | RemoveItemProperty Int Int
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

        DismissToast ->
            ( { model | ui = UiState.clearToast model.ui }, NoOut )

        TriggerToastUndo ->
            ( { model | ui = UiState.clearToast model.ui }, EmitIntent Protocol.Undo )

        MutateItemBase itemIndex defense currentDurability maxDurability quantity isEthereal ->
            ( model
            , EmitIntent
                (Protocol.MutateItemBase
                    { itemIndex = itemIndex
                    , defense = defense
                    , currentDurability = currentDurability
                    , maxDurability = maxDurability
                    , quantity = quantity
                    , isEthereal = isEthereal
                    }
                )
            )

        MutateItemProperty itemIndex statId delta ->
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
            , NoOut
            )

        OpenItemEditorModal itemIndex ->
            ( { model | ui = UiState.openItemEditorModal itemIndex model.ui }
            , NoOut
            )

        CloseItemEditorModal ->
            ( { model | ui = UiState.closeItemEditorModal model.ui }
            , NoOut
            )

        SetItemEditorSearch query ->
            ( { model | ui = UiState.setItemEditorSearch query model.ui }
            , NoOut
            )

        SelectItemEditorInjectStat statId defaultVal ->
            ( { model | ui = UiState.selectItemEditorInjectStat statId defaultVal model.ui }
            , NoOut
            )

        SetItemEditorInjectValue raw ->
            case String.toInt (String.trim raw) of
                Just val ->
                    ( { model | ui = UiState.setItemEditorInjectValue val model.ui }, NoOut )

                Nothing ->
                    ( model, NoOut )

        AddItemProperty itemIndex statId value ->
            ( model
            , EmitIntent
                (Protocol.AddItemProperty
                    { itemIndex = itemIndex
                    , statId = statId
                    , value = value
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
            ( { model | ui = UiState.closeItemEditorModal model.ui }, NoOut )

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
                        ( heldW, heldH ) =
                            model.session
                                |> Maybe.andThen
                                    (\s ->
                                        (s.items ++ s.mercItems ++ s.mercReplacedItems)
                                            |> List.filter (\it -> it.index == idx)
                                            |> List.head
                                    )
                                |> Maybe.map (\it -> ( it.width, it.height ))
                                |> Maybe.withDefault ( 1, 1 )

                        ( dropX, dropY ) =
                            UiState.clampDropAnchor tgt.container heldW heldH tgt.x tgt.y
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
                    ( { model | ui = model.ui |> UiState.setSelectedItem (Just hovIdx) |> UiState.closeItemMenu }
                    , EmitIntent (Protocol.SelectItemForMove { itemIndex = Just hovIdx })
                    )

                _ ->
                    ( model, NoOut )

        else if lowerKey == "w" then
            ( model, EmitIntent Protocol.ToggleWeaponSwap )

        else if heldIdx == Nothing then
            case ( model.session, model.ui.hoveredItemIndex ) of
                ( Just session, Just hovIdx ) ->
                    let
                        currentContainer =
                            (session.items ++ session.mercItems ++ session.mercReplacedItems)
                                |> List.filter (\it -> it.index == hovIdx)
                                |> List.head
                                |> Maybe.map .container
                                |> Maybe.withDefault Inventory

                        maybeTarget =
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
                    in
                    case maybeTarget of
                        Just dest ->
                            ( { model | ui = UiState.closeItemMenu model.ui }
                            , EmitIntent (Protocol.QuickMoveItem { itemIndex = hovIdx, targetContainer = dest })
                            )

                        Nothing ->
                            ( model, NoOut )

                _ ->
                    ( model, NoOut )

        else
            ( model, NoOut )

    else
        ( model, NoOut )


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
                    Just (rej.rawCode ++ ": " ++ rej.message)

                Nothing ->
                    model.ui.statusBanner

        uiWithToast =
            case envelope.rejection of
                Just rej ->
                    UiState.setToast rej model.ui

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
                                in
                                Just { newSession | shopCatalog = mergedCatalog, statCatalog = mergedStatCatalog }

                        Nothing ->
                            Just newSession

                Nothing ->
                    model.session
    in
    { model
        | session = nextSession
        , lastRejection = envelope.rejection
        , lastDecodeError = Nothing
        , ui = UiState.setStatusBanner nextBanner uiWithToast
    }
