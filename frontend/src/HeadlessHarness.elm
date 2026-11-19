module HeadlessHarness exposing (main)

import Bridge.Ports as Ports
import Generated.Protocol as Protocol
    exposing
        ( ContainerKind(..)
        , DifficultyMode(..)
        , EditMode(..)
        , RejectionCode(..)
        , SuggestedRecovery(..)
        , difficultyModeDecoder
        , editModeDecoder
        , uiIntentEncoder
        )
import Json.Decode
import Json.Encode
import State.UiState exposing (DragKind(..), PaintMode(..), ViewTab(..))
import Update exposing (Model, Msg(..), OutMsg(..))


type HarnessMsg
    = FromUpdate Msg
    | FromHarnessCommand Json.Decode.Value


main : Program Json.Decode.Value Model HarnessMsg
main =
    Platform.worker
        { init = \_ -> ( Update.init, emitSnapshot Update.init )
        , update = update
        , subscriptions =
            \_ ->
                Sub.batch
                    [ Ports.receiveSessionEnvelope (ReceiveSessionEnvelope >> FromUpdate)
                    , Ports.receiveHarnessCommand FromHarnessCommand
                    ]
        }


update : HarnessMsg -> Model -> ( Model, Cmd HarnessMsg )
update hMsg model =
    case hMsg of
        FromUpdate msg ->
            let
                ( nextModel, outMsg ) =
                    Update.update msg model
            in
            ( nextModel
            , Cmd.batch
                [ outMsgToCmd outMsg
                , emitSnapshot nextModel
                ]
            )

        FromHarnessCommand rawCmd ->
            case Json.Decode.decodeValue commandDecoder rawCmd of
                Ok msg ->
                    let
                        ( nextModel, outMsg ) =
                            Update.update msg model
                    in
                    ( nextModel
                    , Cmd.batch
                        [ outMsgToCmd outMsg
                        , emitSnapshot nextModel
                        ]
                    )

                Err err ->
                    let
                        detail =
                            Json.Decode.errorToString err

                        nextUi =
                            State.UiState.setStatusBanner (Just ("Harness command decode error: " ++ detail)) model.ui

                        nextModel =
                            { model | lastDecodeError = Just detail, ui = nextUi }
                    in
                    ( nextModel, emitSnapshot nextModel )


outMsgToCmd : OutMsg -> Cmd HarnessMsg
outMsgToCmd outMsg =
    case outMsg of
        NoOut ->
            Cmd.none

        EmitIntent intent ->
            Ports.sendIntent (uiIntentEncoder intent)

        EmitIntents intents ->
            Cmd.batch (List.map (uiIntentEncoder >> Ports.sendIntent) intents)

        ReportDecodeError _ ->
            Cmd.none


commandDecoder : Json.Decode.Decoder Msg
commandDecoder =
    Json.Decode.field "kind" Json.Decode.string
        |> Json.Decode.andThen
            (\kind ->
                case kind of
                    "SwitchView" ->
                        Json.Decode.field "view" viewTabDecoder
                            |> Json.Decode.map SwitchView

                    "SelectResDifficulty" ->
                        Json.Decode.field "difficulty" difficultyModeDecoder
                            |> Json.Decode.map SelectResDifficulty

                    "SelectSkillMode" ->
                        Json.Decode.field "mode" editModeDecoder
                            |> Json.Decode.map SelectSkillMode

                    "AdjustMercLevel" ->
                        Json.Decode.field "delta" Json.Decode.int
                            |> Json.Decode.map AdjustMercLevel

                    "SetMercLevelExact" ->
                        Json.Decode.field "level" Json.Decode.int
                            |> Json.Decode.map SetMercLevelExact

                    "SelectMercAct" ->
                        Json.Decode.field "actIndex" Json.Decode.int
                            |> Json.Decode.map SelectMercAct

                    "SelectProgressionDifficulty" ->
                        Json.Decode.field "difficulty" difficultyModeDecoder
                            |> Json.Decode.map SelectProgressionDifficulty

                    "TogglePaintMode" ->
                        Json.Decode.field "mode" paintModeDecoder
                            |> Json.Decode.map TogglePaintMode

                    "ClickQuestTile" ->
                        Json.Decode.map2 ClickQuestTile
                            (Json.Decode.field "difficulty" difficultyModeDecoder)
                            (Json.Decode.field "key" Json.Decode.string)

                    "ClickWaypointTile" ->
                        Json.Decode.map2 ClickWaypointTile
                            (Json.Decode.field "difficulty" difficultyModeDecoder)
                            (Json.Decode.field "bit" Json.Decode.int)

                    "BeginMarquee" ->
                        Json.Decode.map4 BeginMarquee
                            (Json.Decode.field "dragKind" dragKindDecoder)
                            (Json.Decode.field "x" Json.Decode.float)
                            (Json.Decode.field "y" Json.Decode.float)
                            (Json.Decode.field "ids" (Json.Decode.list Json.Decode.string))

                    "MoveMarquee" ->
                        Json.Decode.map3 MoveMarquee
                            (Json.Decode.field "x" Json.Decode.float)
                            (Json.Decode.field "y" Json.Decode.float)
                            (Json.Decode.field "ids" (Json.Decode.list Json.Decode.string))

                    "ExtendMarquee" ->
                        Json.Decode.map2 ExtendMarquee
                            (Json.Decode.field "dragKind" dragKindDecoder)
                            (Json.Decode.field "id" Json.Decode.string)

                    "CommitMarquee" ->
                        Json.Decode.field "difficulty" difficultyModeDecoder
                            |> Json.Decode.map CommitMarquee

                    "CancelMarquee" ->
                        Json.Decode.succeed CancelMarquee

                    "SetQuestsBatch" ->
                        Json.Decode.map3 SetQuestsBatch
                            (Json.Decode.field "difficulty" difficultyModeDecoder)
                            (Json.Decode.field "keys" (Json.Decode.list Json.Decode.string))
                            (Json.Decode.field "completed" Json.Decode.bool)

                    "SetWaypointsBatch" ->
                        Json.Decode.map3 SetWaypointsBatch
                            (Json.Decode.field "difficulty" difficultyModeDecoder)
                            (Json.Decode.field "bits" (Json.Decode.list Json.Decode.int))
                            (Json.Decode.field "active" Json.Decode.bool)

                    "ToggleAllDiffQuests" ->
                        Json.Decode.succeed ToggleAllDiffQuests

                    "ToggleAllDiffWaypoints" ->
                        Json.Decode.succeed ToggleAllDiffWaypoints

                    "SelectContainerFilter" ->
                        Json.Decode.field "filter" (Json.Decode.nullable Json.Decode.string)
                            |> Json.Decode.map
                                (\maybeStr ->
                                    case maybeStr of
                                        Just "stash" ->
                                            SelectContainerFilter (Just Stash)

                                        Just "inventory" ->
                                            SelectContainerFilter (Just Inventory)

                                        Just "belt" ->
                                            SelectContainerFilter (Just Belt)

                                        Just "cube" ->
                                            SelectContainerFilter (Just Cube)

                                        Just "equipment" ->
                                            SelectContainerFilter (Just Equipment)

                                        Just "temp_inventory" ->
                                            SelectContainerFilter (Just TempInventory)

                                        _ ->
                                            SelectContainerFilter Nothing
                                )

                    "ClearTempInventory" ->
                        Json.Decode.succeed ClearTempInventory

                    "RestoreReplacedMerc" ->
                        Json.Decode.succeed RestoreReplacedMerc

                    "ClearReplacedMerc" ->
                        Json.Decode.field "reclaimToStorage" Json.Decode.bool
                            |> Json.Decode.map ClearReplacedMerc

                    "SelectItem" ->
                        Json.Decode.field "index" (Json.Decode.nullable Json.Decode.int)
                            |> Json.Decode.map SelectItem

                    "FocusField" ->
                        Json.Decode.map2 FocusField
                            (Json.Decode.field "key" Json.Decode.string)
                            (Json.Decode.field "initial" Json.Decode.string)

                    "EditFocusedInput" ->
                        Json.Decode.field "raw" Json.Decode.string
                            |> Json.Decode.map EditFocusedInput

                    "CommitFocusedInput" ->
                        Json.Decode.succeed CommitFocusedInput

                    "CancelFocusedInput" ->
                        Json.Decode.succeed CancelFocusedInput

                    "ClearBanner" ->
                        Json.Decode.succeed ClearBanner

                    "DismissToast" ->
                        Json.Decode.succeed DismissToast

                    "TriggerToastUndo" ->
                        Json.Decode.succeed TriggerToastUndo

                    "Undo" ->
                        Json.Decode.succeed ClickUndo

                    "Redo" ->
                        Json.Decode.succeed ClickRedo

                    "MutateItemBase" ->
                        Json.Decode.map6 MutateItemBase
                            (Json.Decode.field "itemIndex" Json.Decode.int)
                            (Json.Decode.maybe (Json.Decode.field "defense" Json.Decode.int))
                            (Json.Decode.maybe (Json.Decode.field "currentDurability" Json.Decode.int))
                            (Json.Decode.maybe (Json.Decode.field "maxDurability" Json.Decode.int))
                            (Json.Decode.maybe (Json.Decode.field "quantity" Json.Decode.int))
                            (Json.Decode.maybe (Json.Decode.field "isEthereal" Json.Decode.bool))

                    "MutateItemProperty" ->
                        Json.Decode.map3 MutateItemProperty
                            (Json.Decode.field "itemIndex" Json.Decode.int)
                            (Json.Decode.field "statId" Json.Decode.int)
                            (Json.Decode.field "delta" Json.Decode.int)

                    "HoverItem" ->
                        Json.Decode.field "index" (Json.Decode.nullable Json.Decode.int)
                            |> Json.Decode.map HoverItem

                    "KeyShortcut" ->
                        Json.Decode.map6
                            (\k c ctrl alt meta isIn ->
                                HandleKeyShortcut
                                    { key = k
                                    , code = c
                                    , ctrlKey = ctrl
                                    , altKey = alt
                                    , metaKey = meta
                                    , isInput = isIn
                                    }
                            )
                            (Json.Decode.field "key" Json.Decode.string)
                            (Json.Decode.field "code" Json.Decode.string)
                            (Json.Decode.field "ctrlKey" Json.Decode.bool)
                            (Json.Decode.field "altKey" Json.Decode.bool)
                            (Json.Decode.field "metaKey" Json.Decode.bool)
                            (Json.Decode.field "isInput" Json.Decode.bool)

                    "ContextDuplicateItem" ->
                        Json.Decode.field "itemIndex" Json.Decode.int
                            |> Json.Decode.map ContextDuplicateItem

                    "ContextDeleteItem" ->
                        Json.Decode.field "itemIndex" Json.Decode.int
                            |> Json.Decode.map ContextDeleteItem

                    "OpenItemEditorModal" ->
                        Json.Decode.field "itemIndex" Json.Decode.int
                            |> Json.Decode.map OpenItemEditorModal

                    "CloseItemEditorModal" ->
                        Json.Decode.succeed CloseItemEditorModal

                    "AddItemProperty" ->
                        Json.Decode.map4 AddItemProperty
                            (Json.Decode.field "itemIndex" Json.Decode.int)
                            (Json.Decode.field "statId" Json.Decode.int)
                            (Json.Decode.field "value" Json.Decode.int)
                            (Json.Decode.oneOf
                                [ Json.Decode.field "param" Json.Decode.int
                                , Json.Decode.succeed 0
                                ]
                            )

                    "RemoveItemProperty" ->
                        Json.Decode.map2 RemoveItemProperty
                            (Json.Decode.field "itemIndex" Json.Decode.int)
                            (Json.Decode.field "statId" Json.Decode.int)

                    "SetItemPropertyParam" ->
                        Json.Decode.map3 SetItemPropertyParam
                            (Json.Decode.field "itemIndex" Json.Decode.int)
                            (Json.Decode.field "statId" Json.Decode.int)
                            (Json.Decode.field "param" Json.Decode.int)

                    "OpenItemForgeDraft" ->
                        Json.Decode.field "itemIndex" Json.Decode.int
                            |> Json.Decode.map OpenItemEditorModal

                    "ApplyItemForgeDraft" ->
                        Json.Decode.succeed ApplyItemForgeDraft

                    "DiscardItemForgeDraft" ->
                        Json.Decode.succeed DiscardItemForgeDraft

                    "SelectStatMode" ->
                        Json.Decode.field "mode"
                            (Json.Decode.string
                                |> Json.Decode.map
                                    (\s ->
                                        if String.toLower s == "free" then
                                            Protocol.Free

                                        else
                                            Protocol.Legit
                                    )
                            )
                            |> Json.Decode.map SelectStatMode

                    "ResetStats" ->
                        Json.Decode.succeed ClickResetStats

                    "SyncStatPointsToRules" ->
                        Json.Decode.succeed ClickSyncStatPointsToRules

                    other ->
                        Json.Decode.fail ("Unknown harness command: " ++ other)
            )


viewTabDecoder : Json.Decode.Decoder ViewTab
viewTabDecoder =
    Json.Decode.string
        |> Json.Decode.andThen
            (\v ->
                case v of
                    "ViewA" ->
                        Json.Decode.succeed ViewA

                    "ViewB" ->
                        Json.Decode.succeed ViewB

                    _ ->
                        Json.Decode.fail ("Unknown ViewTab: " ++ v)
            )


paintModeDecoder : Json.Decode.Decoder PaintMode
paintModeDecoder =
    Json.Decode.string
        |> Json.Decode.andThen
            (\v ->
                case v of
                    "Activate" ->
                        Json.Decode.succeed Activate

                    "Deactivate" ->
                        Json.Decode.succeed Deactivate

                    _ ->
                        Json.Decode.fail ("Unknown PaintMode: " ++ v)
            )


dragKindDecoder : Json.Decode.Decoder DragKind
dragKindDecoder =
    Json.Decode.string
        |> Json.Decode.andThen
            (\v ->
                case v of
                    "DragQuests" ->
                        Json.Decode.succeed DragQuests

                    "DragWaypoints" ->
                        Json.Decode.succeed DragWaypoints

                    _ ->
                        Json.Decode.fail ("Unknown DragKind: " ++ v)
            )


emitSnapshot : Model -> Cmd HarnessMsg
emitSnapshot model =
    let
        activeViewName =
            case model.ui.activeView of
                ViewA ->
                    "ViewA"

                ViewB ->
                    "ViewB"

        paintModeValue =
            case model.ui.paintMode of
                Nothing ->
                    Json.Encode.null

                Just Activate ->
                    Json.Encode.string "Activate"

                Just Deactivate ->
                    Json.Encode.string "Deactivate"

        marqueeTargetsValue =
            case model.ui.marquee of
                Nothing ->
                    Json.Encode.null

                Just box ->
                    Json.Encode.list Json.Encode.string box.candidateIds

        charNameValue =
            model.session
                |> Maybe.map (\s -> Json.Encode.string s.charName)
                |> Maybe.withDefault Json.Encode.null

        mercLevelValue =
            model.session
                |> Maybe.map (\s -> Json.Encode.int s.merc.stats.level)
                |> Maybe.withDefault Json.Encode.null

        mercActValue =
            model.session
                |> Maybe.map (\s -> Json.Encode.int s.merc.actIndex)
                |> Maybe.withDefault Json.Encode.null

        mercPersistenceSupportedValue =
            model.session
                |> Maybe.map (\s -> Json.Encode.bool s.merc.persistenceSupported)
                |> Maybe.withDefault Json.Encode.null

        exportSupportedValue =
            model.session
                |> Maybe.map (\s -> Json.Encode.bool s.exportSupported)
                |> Maybe.withDefault Json.Encode.null

        decodeErrorValue =
            model.lastDecodeError
                |> Maybe.map Json.Encode.string
                |> Maybe.withDefault Json.Encode.null

        rejectionCodeValue =
            model.lastRejection
                |> Maybe.map (\r -> Json.Encode.string r.rawCode)
                |> Maybe.withDefault Json.Encode.null

        dirtyValue =
            model.session
                |> Maybe.map .dirty
                |> Maybe.withDefault False
                |> Json.Encode.bool

        selectedItemValue =
            model.ui.selectedItemIndex
                |> Maybe.map Json.Encode.int
                |> Maybe.withDefault Json.Encode.null

        statusBannerValue =
            model.ui.statusBanner
                |> Maybe.map Json.Encode.string
                |> Maybe.withDefault Json.Encode.null

        resDifficultyValue =
            model.session
                |> Maybe.map
                    (\s ->
                        case s.resDifficulty of
                            Normal ->
                                "Normal"

                            Nightmare ->
                                "Nightmare"

                            Hell ->
                                "Hell"
                    )
                |> Maybe.map Json.Encode.string
                |> Maybe.withDefault Json.Encode.null

        fireResValue =
            model.session
                |> Maybe.map (.resistances >> .fire >> Json.Encode.int)
                |> Maybe.withDefault Json.Encode.null

        tempCountValue =
            model.session
                |> Maybe.map (.containerCounts >> .tempInventory >> Json.Encode.int)
                |> Maybe.withDefault Json.Encode.null

        hellCompletedValue =
            model.session
                |> Maybe.map (.progression >> .hellQuestsCompleted >> List.length >> Json.Encode.int)
                |> Maybe.withDefault Json.Encode.null

        hellWpCountValue =
            model.session
                |> Maybe.map (.progression >> .hellWaypointsActive >> List.length >> Json.Encode.int)
                |> Maybe.withDefault Json.Encode.null

        charLevelValue =
            model.session
                |> Maybe.map (.charLevel >> Json.Encode.int)
                |> Maybe.withDefault Json.Encode.null

        strengthBaseValue =
            model.session
                |> Maybe.map (.stats >> .strengthBase >> Json.Encode.int)
                |> Maybe.withDefault Json.Encode.null

        goldInventoryValue =
            model.session
                |> Maybe.map (.stats >> .goldInventory >> Json.Encode.int)
                |> Maybe.withDefault Json.Encode.null

        goldStashValue =
            model.session
                |> Maybe.map (.stats >> .goldStash >> Json.Encode.int)
                |> Maybe.withDefault Json.Encode.null

        itemCountValue =
            model.session
                |> Maybe.map (.items >> List.length >> Json.Encode.int)
                |> Maybe.withDefault Json.Encode.null

        canUndoValue =
            model.session
                |> Maybe.map (.canUndo >> Json.Encode.bool)
                |> Maybe.withDefault (Json.Encode.bool False)

        canRedoValue =
            model.session
                |> Maybe.map (.canRedo >> Json.Encode.bool)
                |> Maybe.withDefault (Json.Encode.bool False)

        focusedKeyValue =
            model.ui.focusedInput
                |> Maybe.map (.key >> Json.Encode.string)
                |> Maybe.withDefault Json.Encode.null

        focusedRawValue =
            model.ui.focusedInput
                |> Maybe.map (.raw >> Json.Encode.string)
                |> Maybe.withDefault Json.Encode.null
    in
    Ports.emitHarnessSnapshot
        (Json.Encode.object
            [ ( "hasSession", Json.Encode.bool (model.session /= Nothing) )
            , ( "charName", charNameValue )
            , ( "charLevel", charLevelValue )
            , ( "dirty", dirtyValue )
            , ( "canUndo", canUndoValue )
            , ( "canRedo", canRedoValue )
            , ( "activeView", Json.Encode.string activeViewName )
            , ( "paintMode", paintModeValue )
            , ( "marqueeTargets", marqueeTargetsValue )
            , ( "containerFilter"
              , model.ui.selectedContainerFilter
                    |> Maybe.map State.UiState.containerLabel
                    |> Maybe.withDefault "all"
                    |> Json.Encode.string
              )
            , ( "selectedItemIndex", selectedItemValue )
            , ( "statusBanner", statusBannerValue )
            , ( "resDifficulty", resDifficultyValue )
            , ( "fireRes", fireResValue )
            , ( "strengthBase", strengthBaseValue )
            , ( "goldInventory", goldInventoryValue )
            , ( "goldStash", goldStashValue )
            , ( "itemCount", itemCountValue )
            , ( "focusedInputKey", focusedKeyValue )
            , ( "focusedInputRaw", focusedRawValue )
            , ( "tempCount", tempCountValue )
            , ( "hellQuestCount", hellCompletedValue )
            , ( "hellWaypointCount", hellWpCountValue )
            , ( "mercLevel", mercLevelValue )
            , ( "mercAct", mercActValue )
            , ( "mercPersistenceSupported", mercPersistenceSupportedValue )
            , ( "exportSupported", exportSupportedValue )
            , ( "rejectionCode", rejectionCodeValue )
            , ( "hasToast", Json.Encode.bool (model.ui.toast /= Nothing) )
            , ( "toastRawCode"
              , model.ui.toast
                    |> Maybe.map (.rawCode >> Json.Encode.string)
                    |> Maybe.withDefault Json.Encode.null
              )
            , ( "toastHasUndo"
              , model.ui.toast
                    |> Maybe.map
                        (\t ->
                            case t.suggestedAction of
                                Just Protocol.SuggestUndo ->
                                    True

                                _ ->
                                    case t.code of
                                        Protocol.RejectionInvalidPlacement ->
                                            True

                                        Protocol.RejectionSlotOccupied ->
                                            True

                                        _ ->
                                            False
                        )
                    |> Maybe.map Json.Encode.bool
                    |> Maybe.withDefault Json.Encode.null
              )
            , ( "decodeError", decodeErrorValue )
            ]
        )
