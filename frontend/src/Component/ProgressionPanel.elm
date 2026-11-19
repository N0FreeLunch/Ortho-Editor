module Component.ProgressionPanel exposing (view, viewMarqueeOverlay, viewMarqueePreview, viewQuestsPanel, viewWaypointsPanel)

import Generated.Protocol
    exposing
        ( DifficultyMode(..)
        , ProgressionSummaryView
        , QuestCatalogEntryView
        , QuestSemanticStatusView(..)
        , QuestStateEntryView
        , WaypointCatalogEntryView
        )
import Html exposing (Html, button, div, section, span, text)
import Html.Attributes exposing (attribute, class, classList, disabled, id, style, title)
import Html.Events exposing (custom, on, onClick)
import Json.Decode
import State.UiState exposing (DragKind(..), MarqueeSelection, PaintMode(..), UiState)
import Update exposing (Msg(..))


view : UiState -> ProgressionSummaryView -> Html Msg
view ui prog =
    let
        panelMouseAttrs =
            case ui.marquee of
                Just _ ->
                    [ on "mouseup" (Json.Decode.succeed (CommitMarquee ui.progressionDifficulty))
                    , on "mouseleave" (Json.Decode.succeed CancelMarquee)
                    ]

                Nothing ->
                    []
    in
    section
        ([ classList
            [ ( "glass-panel", True )
            , ( "spec-progression-wrap", True )
            , ( "progression-panel", True )
            , ( "is-marquee-mode", ui.paintMode /= Nothing )
            , ( "mode-activate", ui.paintMode == Just Activate )
            , ( "mode-deactivate", ui.paintMode == Just Deactivate )
            ]
         , attribute "data-testid" "progression-panel"
         ]
            ++ panelMouseAttrs
        )
        [ viewProgressionHeader ui
        , viewQuestsPanel ui prog
        , viewWaypointsPanel ui prog
        ]


viewProgressionHeader : UiState -> Html Msg
viewProgressionHeader ui =
    div [ class "panel-header progression-shared-header", attribute "data-testid" "progression-header" ]
        [ div [ class "paint-toolbar", attribute "data-testid" "paint-toolbar" ]
            [ button
                [ classList [ ( "paint-btn", True ), ( "paint-activate", True ), ( "is-active", ui.paintMode == Just Activate ) ]
                , attribute "data-testid" "paint-mode-activate"
                , onClick (TogglePaintMode Activate)
                ]
                [ text "Activate Box (Green)" ]
            , button
                [ classList [ ( "paint-btn", True ), ( "paint-deactivate", True ), ( "is-active", ui.paintMode == Just Deactivate ) ]
                , attribute "data-testid" "paint-mode-deactivate"
                , onClick (TogglePaintMode Deactivate)
                ]
                [ text "Deactivate Box (Red)" ]
            ]
        , div [ class "segmented", attribute "data-testid" "progression-diff-tabs" ]
            [ progDiffButton ui.progressionDifficulty Normal "Normal"
            , progDiffButton ui.progressionDifficulty Nightmare "Nightmare"
            , progDiffButton ui.progressionDifficulty Hell "Hell"
            ]
        ]


viewQuestsPanel : UiState -> ProgressionSummaryView -> Html Msg
viewQuestsPanel ui prog =
    section [ class "progression-section quests-panel", attribute "data-testid" "quests-panel" ]
        [ div [ class "progression-section-header" ]
            [ div [ class "progression-section-title" ]
                [ text
                    (if prog.isExpansion then
                        "Quests (5 Acts x 3 Diff)"

                     else
                        "Quests (4 Acts x 3 Diff)"
                    )
                ]
            , div [ class "progression-section-actions" ]
                [ span [ class "meta-line" ]
                    [ text
                        (diffLabel ui.progressionDifficulty
                            ++ " · "
                            ++ String.fromInt (List.length (questsForDiff ui.progressionDifficulty prog))
                            ++ "/"
                            ++ String.fromInt (List.length prog.questCatalog)
                        )
                    ]
                , button
                    [ class "seg-btn"
                    , attribute "data-testid" "all-diff-quests-btn"
                    , title ("Toggle all quests in " ++ diffLabel ui.progressionDifficulty)
                    , onClick ToggleCurrentDiffQuests
                    ]
                    [ text "Diff Quests" ]
                ]
            ]
        , div
            ([ classList
                [ ( "quests-matrix-stack", True )
                , ( "is-marquee-mode", ui.paintMode /= Nothing )
                ]
             , attribute "data-testid" "quest-matrix"
             , attribute "data-marquee-container" "quests"
             ]
                ++ containerMarqueeAttrs ui.paintMode DragQuests
            )
            [ viewQuestDiffCard ui prog Normal "diff-normal" "Normal"
            , viewQuestDiffCard ui prog Nightmare "diff-nm" "Nightmare"
            , viewQuestDiffCard ui prog Hell "diff-hell" "Hell"
            ]
        , viewQuestDetailBar ui prog
        ]


viewQuestDiffCard : UiState -> ProgressionSummaryView -> DifficultyMode -> String -> String -> Html Msg
viewQuestDiffCard ui prog diff diffClass titleLabel =
    let
        completed =
            questsForDiff diff prog

        questStates =
            questStatesForDiff diff prog

        isActiveDiff =
            ui.progressionDifficulty == diff

        acts =
            if prog.isExpansion then
                [ 1, 2, 3, 4, 5 ]

            else
                [ 1, 2, 3, 4 ]
    in
    div
        [ classList
            [ ( "quest-diff-card", True )
            , ( diffClass, True )
            , ( "is-active-diff", isActiveDiff )
            ]
        ]
        [ div [ class "quest-diff-header" ]
            [ span [] [ text titleLabel ]
            , span [] [ text (String.fromInt (List.length completed) ++ "/" ++ String.fromInt (List.length prog.questCatalog)) ]
            ]
        , div [ class "quest-matrix" ]
            (List.map (viewQuestActRow diff isActiveDiff ui.paintMode completed questStates ui.selectedQuestTarget ui.marquee prog.questCatalog) acts)
        ]


viewQuestDetailBar : UiState -> ProgressionSummaryView -> Html Msg
viewQuestDetailBar ui prog =
    case ui.selectedQuestTarget of
        Nothing ->
            div [ class "quest-detail-bar is-empty", attribute "data-testid" "quest-detail-bar" ]
                [ div [ class "quest-detail-bar-empty" ]
                    [ text "퀘스트를 클릭하면 상세 설명과 4단계 상태 편집 버튼이 표시됩니다." ]
                ]

        Just target ->
            let
                maybeEntry =
                    prog.questCatalog
                        |> List.filter (\q -> q.key == target.key)
                        |> List.head
            in
            case maybeEntry of
                Nothing ->
                    div [ class "quest-detail-bar is-empty", attribute "data-testid" "quest-detail-bar" ]
                        [ div [ class "quest-detail-bar-empty" ]
                            [ text "퀘스트를 클릭하면 상세 설명과 4단계 상태 편집 버튼이 표시됩니다." ]
                        ]

                Just entry ->
                    let
                        maybeState =
                            questStatesForDiff target.difficulty prog
                                |> List.filter (\s -> s.key == entry.key)
                                |> List.head

                        currentStatus =
                            Maybe.map .status maybeState
                                |> Maybe.withDefault
                                    (if List.member entry.key (questsForDiff target.difficulty prog) then
                                        QuestCompleted

                                     else
                                        QuestUnstarted
                                    )

                        hexLabel =
                            case maybeState of
                                Just st ->
                                    "0x" ++ toHex4 st.rawState

                                Nothing ->
                                    "-"

                        titleText =
                            if String.isEmpty entry.nameKo || entry.nameKo == entry.name then
                                diffLabel target.difficulty ++ " · " ++ entry.key ++ " · " ++ entry.name

                            else
                                diffLabel target.difficulty ++ " · " ++ entry.key ++ " · " ++ entry.name ++ " (" ++ entry.nameKo ++ ")"

                        descText =
                            if not (String.isEmpty entry.descKo) then
                                entry.descKo

                            else
                                entry.descEn

                        diffCardClass =
                            case target.difficulty of
                                Normal ->
                                    "diff-normal"

                                Nightmare ->
                                    "diff-nm"

                                Hell ->
                                    "diff-hell"
                    in
                    div [ class ("quest-detail-bar " ++ diffCardClass), attribute "data-testid" "quest-detail-bar" ]
                        [ div [ class "quest-detail-bar-head" ]
                            [ div [ class "quest-detail-bar-title" ]
                                [ text titleText
                                , span [ class "meta-line" ] [ text (" [" ++ hexLabel ++ "]") ]
                                ]
                            , button
                                [ class "btn-ghost btn-tiny"
                                , attribute "data-testid" "quest-detail-close"
                                , onClick (SelectQuestTarget Nothing)
                                ]
                                [ text "×" ]
                            ]
                        , if String.isEmpty descText then
                            text ""

                          else
                            div [ class "quest-detail-bar-desc" ] [ text descText ]
                        , div [ class "quest-detail-bar-actions" ]
                            [ button
                                [ classList [ ( "seg-btn quest-status-btn status-unstarted", True ), ( "is-active", currentStatus == QuestUnstarted ) ]
                                , attribute "data-testid" "quest-status-unstarted"
                                , onClick (SetQuestSemanticStatus target.difficulty entry.key QuestUnstarted)
                                ]
                                [ text "Unstarted" ]
                            , button
                                [ classList [ ( "seg-btn quest-status-btn status-in-progress", True ), ( "is-active", currentStatus == QuestInProgress ) ]
                                , attribute "data-testid" "quest-status-in-progress"
                                , onClick (SetQuestSemanticStatus target.difficulty entry.key QuestInProgress)
                                ]
                                [ text "In Progress" ]
                            , button
                                [ classList [ ( "seg-btn quest-status-btn status-reward-pending", True ), ( "is-active", currentStatus == QuestRewardPending ) ]
                                , attribute "data-testid" "quest-status-reward-pending"
                                , disabled (not entry.hasNpcReward)
                                , title
                                    (if entry.hasNpcReward then
                                        "Set Reward Pending (0x0002) to claim NPC reward in-game"

                                     else
                                        "No repeatable NPC reward for this quest"
                                    )
                                , onClick (SetQuestSemanticStatus target.difficulty entry.key QuestRewardPending)
                                ]
                                [ text "Reward Pending" ]
                            , button
                                [ classList [ ( "seg-btn quest-status-btn status-completed", True ), ( "is-active", currentStatus == QuestCompleted ) ]
                                , attribute "data-testid" "quest-status-completed"
                                , onClick (SetQuestSemanticStatus target.difficulty entry.key QuestCompleted)
                                ]
                                [ text "Completed" ]
                            ]
                        ]


viewQuestActRow :
    DifficultyMode
    -> Bool
    -> Maybe PaintMode
    -> List String
    -> List QuestStateEntryView
    -> Maybe { difficulty : DifficultyMode, key : String }
    -> Maybe MarqueeSelection
    -> List QuestCatalogEntryView
    -> Int
    -> Html Msg
viewQuestActRow diff isActiveDiff paintMode completed questStates selectedTarget maybeMarquee catalog actNum =
    let
        actEntries =
            catalog
                |> List.filter (\q -> q.act == actNum)
                |> List.sortBy .index

        padCount =
            max 0 (6 - List.length actEntries)
    in
    div [ class "quest-matrix-row" ]
        [ span [ class "quest-matrix-act-label" ] [ text ("Act " ++ String.fromInt actNum) ]
        , div [ class "quest-matrix-cells" ]
            (List.map (viewQuestDot diff isActiveDiff paintMode completed questStates selectedTarget maybeMarquee) actEntries
                ++ List.repeat padCount (div [ class "quest-cell-dot empty" ] [])
            )
        ]


viewQuestDot :
    DifficultyMode
    -> Bool
    -> Maybe PaintMode
    -> List String
    -> List QuestStateEntryView
    -> Maybe { difficulty : DifficultyMode, key : String }
    -> Maybe MarqueeSelection
    -> QuestCatalogEntryView
    -> Html Msg
viewQuestDot diff isActiveDiff paintMode completed questStates selectedTarget maybeMarquee entry =
    let
        maybeState =
            questStates
                |> List.filter (\s -> s.key == entry.key)
                |> List.head

        isCompleted =
            List.member entry.key completed

        isRewardPending =
            not isCompleted && (Maybe.map .status maybeState == Just QuestRewardPending)

        isInProgress =
            not isCompleted && (Maybe.map .status maybeState == Just QuestInProgress)

        isSelected =
            selectedTarget == Just { difficulty = diff, key = entry.key }

        marqueeId =
            diffSlug diff ++ ":" ++ entry.key

        marqueeMatch =
            case maybeMarquee of
                Just box ->
                    if box.kind == DragQuests && (List.member marqueeId box.candidateIds || (isActiveDiff && List.member entry.key box.candidateIds)) then
                        Just box.mode

                    else
                        Nothing

                Nothing ->
                    Nothing

        inMarquee =
            marqueeMatch /= Nothing

        interactionId =
            if isActiveDiff then
                entry.key

            else
                marqueeId

        testIdAttr =
            if isActiveDiff then
                "quest-tile-" ++ entry.key

            else
                "quest-tile-" ++ diffSlug diff ++ "-" ++ entry.key
    in
    button
        ([ classList
            [ ( "matrix-tile", True )
            , ( "quest-cell-dot", True )
            , ( "is-completed", isCompleted )
            , ( "completed", isCompleted )
            , ( "is-reward-pending", isRewardPending )
            , ( "is-in-progress", isInProgress )
            , ( "is-selected", isSelected )
            , ( "is-marquee-target", inMarquee )
            , ( "marquee-hit-activate", marqueeMatch == Just Activate )
            , ( "marquee-hit-deactivate", marqueeMatch == Just Deactivate )
            ]
         , attribute "data-testid" testIdAttr
         , attribute "data-marquee-kind" "quests"
         , attribute "data-marquee-id" marqueeId
         , title (formatQuestTooltip entry maybeState isCompleted)
         ]
            ++ tileInteractionAttrs paintMode diff DragQuests interactionId (CycleOrSelectQuest diff entry.key)
        )
        [ text ("Q" ++ String.fromInt entry.index) ]


formatQuestTooltip : QuestCatalogEntryView -> Maybe QuestStateEntryView -> Bool -> String
formatQuestTooltip entry maybeState isCompleted =
    let
        titleLine =
            if String.isEmpty entry.nameKo || entry.nameKo == entry.name then
                entry.key ++ " · " ++ entry.name

            else
                entry.key ++ " · " ++ entry.name ++ " (" ++ entry.nameKo ++ ")"

        statusText =
            case Maybe.map .status maybeState of
                Just QuestCompleted ->
                    "Completed"

                Just QuestRewardPending ->
                    "Reward Pending"

                Just QuestInProgress ->
                    "In Progress"

                Just QuestUnstarted ->
                    "Unstarted"

                Nothing ->
                    if isCompleted then
                        "Completed"

                    else
                        "Unstarted"

        rawHex =
            case maybeState of
                Just st ->
                    " (0x" ++ toHex4 st.rawState ++ ")"

                Nothing ->
                    ""

        descLines =
            [ if String.isEmpty entry.descEn then
                Nothing

              else
                Just entry.descEn
            , if String.isEmpty entry.descKo || entry.descKo == entry.descEn then
                Nothing

              else
                Just entry.descKo
            ]
                |> List.filterMap identity
    in
    String.join "\n"
        ([ titleLine
         , "Status: " ++ statusText ++ rawHex
         ]
            ++ descLines
        )


toHex4 : Int -> String
toHex4 val =
    let
        hexDigit n =
            case n of
                10 -> "A"
                11 -> "B"
                12 -> "C"
                13 -> "D"
                14 -> "E"
                15 -> "F"
                _ -> String.fromInt n

        clamped =
            modBy 65536 (max 0 val)
    in
    hexDigit (modBy 16 (clamped // 4096))
        ++ hexDigit (modBy 16 (clamped // 256))
        ++ hexDigit (modBy 16 (clamped // 16))
        ++ hexDigit (modBy 16 clamped)


questStatesForDiff : DifficultyMode -> ProgressionSummaryView -> List QuestStateEntryView
questStatesForDiff diff prog =
    case diff of
        Normal ->
            prog.normalQuestStates

        Nightmare ->
            prog.nightmareQuestStates

        Hell ->
            prog.hellQuestStates


viewWaypointsPanel : UiState -> ProgressionSummaryView -> Html Msg
viewWaypointsPanel ui prog =
    let
        activeDiff =
            ui.progressionDifficulty

        activeWaypoints =
            waypointsForDiff activeDiff prog

        acts =
            if prog.isExpansion then
                [ 1, 2, 3, 4, 5 ]

            else
                [ 1, 2, 3, 4 ]
    in
    section [ class "progression-section waypoints-panel", attribute "data-testid" "waypoint-matrix" ]
        [ div [ class "progression-section-header" ]
            [ div [ class "progression-section-title" ]
                [ text
                    (if prog.isExpansion then
                        "Waypoints (5 Acts)"

                     else
                        "Waypoints (4 Acts)"
                    )
                ]
            , div [ class "progression-section-actions" ]
                [ span [ class "meta-line" ]
                    [ text
                        (diffLabel activeDiff
                            ++ " · "
                            ++ String.fromInt (List.length activeWaypoints)
                            ++ "/"
                            ++ String.fromInt (List.length prog.waypointCatalog)
                        )
                    ]
                , button
                    [ class "seg-btn"
                    , attribute "data-testid" "all-diff-waypoints-btn"
                    , title ("Toggle all waypoints in " ++ diffLabel activeDiff)
                    , onClick ToggleCurrentDiffWaypoints
                    ]
                    [ text "Diff WPs" ]
                ]
            ]
        , div
            ([ classList
                [ ( "waypoint-box", True )
                , ( "is-classic", not prog.isExpansion )
                , ( "is-marquee-mode", ui.paintMode /= Nothing )
                ]
             , attribute "data-marquee-container" "waypoints"
             ]
                ++ containerMarqueeAttrs ui.paintMode DragWaypoints
            )
            (List.map (viewWaypointActCard activeDiff ui.paintMode activeWaypoints ui.marquee prog.waypointCatalog) acts)
        ]


viewWaypointActCard :
    DifficultyMode
    -> Maybe PaintMode
    -> List Int
    -> Maybe MarqueeSelection
    -> List WaypointCatalogEntryView
    -> Int
    -> Html Msg
viewWaypointActCard diff paintMode activeBits maybeMarquee catalog actNum =
    let
        entries =
            catalog
                |> List.filter (\w -> w.act == actNum)
                |> List.sortBy .index

        activeCount =
            entries
                |> List.filter (\w -> List.member w.bit activeBits)
                |> List.length
    in
    div [ class ("wp-act-card act" ++ String.fromInt actNum) ]
        [ div [ class "wp-act-header" ]
            [ span [] [ text ("Act " ++ String.fromInt actNum) ]
            , span [ class "meta-line" ] [ text (String.fromInt activeCount ++ "/" ++ String.fromInt (List.length entries)) ]
            ]
        , div [ class "wp-act-container" ]
            (List.map (viewWaypointTile diff paintMode activeBits maybeMarquee) entries)
        ]


viewMarqueePreview : DifficultyMode -> MarqueeSelection -> Html Msg
viewMarqueePreview activeDiff box =
    div
        [ classList
            [ ( "marquee-box", True )
            , ( "marquee-green", box.mode == Activate )
            , ( "marquee-red", box.mode == Deactivate )
            ]
        , attribute "data-testid" "marquee-box"
        ]
        [ span []
            [ text
                ("Marquee Targets: "
                    ++ String.fromInt (List.length box.candidateIds)
                )
            ]
        , button
            [ class "seg-btn"
            , attribute "data-testid" "commit-marquee-btn"
            , onClick (CommitMarquee activeDiff)
            ]
            [ text "Apply Selection" ]
        , button
            [ class "btn-ghost"
            , attribute "data-testid" "cancel-marquee-btn"
            , onClick CancelMarquee
            ]
            [ text "Cancel" ]
        ]


viewMarqueeOverlay : MarqueeSelection -> Html Msg
viewMarqueeOverlay box =
    let
        left =
            min box.anchorX box.currentX

        top =
            min box.anchorY box.currentY

        width =
            abs (box.currentX - box.anchorX)

        height =
            abs (box.currentY - box.anchorY)

        isMoved =
            width >= 4 || height >= 4
    in
    div
        [ id "drag-marquee-box"
        , classList
            [ ( "marquee-selection-box", True )
            , ( "is-deactivate", box.mode == Deactivate )
            ]
        , attribute "data-testid" "drag-marquee-box"
        , style "display"
            (if isMoved then
                "block"

             else
                "none"
            )
        , style "left" (String.fromFloat left ++ "px")
        , style "top" (String.fromFloat top ++ "px")
        , style "width" (String.fromFloat width ++ "px")
        , style "height" (String.fromFloat height ++ "px")
        ]
        []


marqueeMouseDownDecoder : (Float -> Float -> Msg) -> Bool -> Json.Decode.Decoder { message : Msg, stopPropagation : Bool, preventDefault : Bool }
marqueeMouseDownDecoder toMsg stopProp =
    Json.Decode.map3
        (\_ x y ->
            { message = toMsg x y
            , stopPropagation = stopProp
            , preventDefault = True
            }
        )
        (Json.Decode.oneOf [ Json.Decode.field "button" Json.Decode.int, Json.Decode.succeed 0 ]
            |> Json.Decode.andThen
                (\btn ->
                    if btn == 0 then
                        Json.Decode.succeed ()

                    else
                        Json.Decode.fail "Not left mouse button"
                )
        )
        (Json.Decode.oneOf [ Json.Decode.field "clientX" Json.Decode.float, Json.Decode.succeed 0 ])
        (Json.Decode.oneOf [ Json.Decode.field "clientY" Json.Decode.float, Json.Decode.succeed 0 ])


containerMarqueeAttrs : Maybe PaintMode -> DragKind -> List (Html.Attribute Msg)
containerMarqueeAttrs paintMode kind =
    case paintMode of
        Nothing ->
            []

        Just _ ->
            [ custom "mousedown" (marqueeMouseDownDecoder (\x y -> BeginMarquee kind x y []) False) ]


tileInteractionAttrs : Maybe PaintMode -> DifficultyMode -> DragKind -> String -> Msg -> List (Html.Attribute Msg)
tileInteractionAttrs paintMode diff kind tileId clickMsg =
    case paintMode of
        Nothing ->
            [ onClick clickMsg ]

        Just _ ->
            [ custom "mousedown" (marqueeMouseDownDecoder (\x y -> BeginMarquee kind x y [ tileId ]) True)
            , on "mouseenter" (Json.Decode.succeed (ExtendMarquee kind tileId))
            , custom "mouseup"
                (Json.Decode.succeed
                    { message = CommitMarquee diff
                    , stopPropagation = True
                    , preventDefault = False
                    }
                )
            ]


viewWaypointTile : DifficultyMode -> Maybe PaintMode -> List Int -> Maybe MarqueeSelection -> WaypointCatalogEntryView -> Html Msg
viewWaypointTile diff paintMode activeBits maybeMarquee entry =
    let
        isActive =
            List.member entry.bit activeBits

        bitKey =
            String.fromInt entry.bit

        marqueeMatch =
            case maybeMarquee of
                Just box ->
                    if box.kind == DragWaypoints && List.member bitKey box.candidateIds then
                        Just box.mode

                    else
                        Nothing

                Nothing ->
                    Nothing

        inMarquee =
            marqueeMatch /= Nothing
    in
    button
        ([ classList
            [ ( "matrix-tile", True )
            , ( "wp-node", True )
            , ( "is-completed", isActive )
            , ( "active", isActive )
            , ( "is-marquee-target", inMarquee )
            , ( "marquee-hit-activate", marqueeMatch == Just Activate )
            , ( "marquee-hit-deactivate", marqueeMatch == Just Deactivate )
            ]
         , attribute "data-testid" ("wp-tile-" ++ bitKey)
         , attribute "data-marquee-kind" "waypoints"
         , attribute "data-marquee-id" bitKey
         , title
            (if String.isEmpty entry.nameKo || entry.nameKo == entry.name then
                entry.name

             else
                entry.name ++ " (" ++ entry.nameKo ++ ")"
            )
         ]
            ++ tileInteractionAttrs paintMode diff DragWaypoints bitKey (ClickWaypointTile diff entry.bit)
        )
        [ span [ class "wp-dot" ] []
        , span [ class "tile-name" ] [ text entry.name ]
        ]


progDiffButton : DifficultyMode -> DifficultyMode -> String -> Html Msg
progDiffButton active target label =
    button
        [ classList [ ( "seg-btn", True ), ( "is-active", active == target ) ]
        , attribute "data-testid" ("prog-diff-" ++ String.toLower label)
        , onClick (SelectProgressionDifficulty target)
        ]
        [ text label ]


diffLabel : DifficultyMode -> String
diffLabel diff =
    case diff of
        Normal ->
            "Normal"

        Nightmare ->
            "Nightmare"

        Hell ->
            "Hell"


diffSlug : DifficultyMode -> String
diffSlug diff =
    case diff of
        Normal ->
            "normal"

        Nightmare ->
            "nightmare"

        Hell ->
            "hell"


questsForDiff : DifficultyMode -> ProgressionSummaryView -> List String
questsForDiff diff prog =
    case diff of
        Normal ->
            prog.normalQuestsCompleted

        Nightmare ->
            prog.nightmareQuestsCompleted

        Hell ->
            prog.hellQuestsCompleted


waypointsForDiff : DifficultyMode -> ProgressionSummaryView -> List Int
waypointsForDiff diff prog =
    case diff of
        Normal ->
            prog.normalWaypointsActive

        Nightmare ->
            prog.nightmareWaypointsActive

        Hell ->
            prog.hellWaypointsActive
