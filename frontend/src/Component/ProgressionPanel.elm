module Component.ProgressionPanel exposing (view, viewQuestsPanel, viewWaypointsPanel)

import Generated.Protocol
    exposing
        ( DifficultyMode(..)
        , ProgressionSummaryView
        , QuestCatalogEntryView
        , WaypointCatalogEntryView
        )
import Html exposing (Html, button, div, section, span, text)
import Html.Attributes exposing (attribute, class, classList, title)
import Html.Events exposing (on, onClick, stopPropagationOn)
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
            [ ( "spec-progression-wrap", True )
            , ( "progression-panel", True )
            , ( "is-marquee-mode", ui.paintMode /= Nothing )
            , ( "mode-activate", ui.paintMode == Just Activate )
            , ( "mode-deactivate", ui.paintMode == Just Deactivate )
            ]
         , attribute "data-testid" "progression-panel"
         ]
            ++ panelMouseAttrs
        )
        [ viewQuestsPanel ui prog
        , viewWaypointsPanel ui prog
        ]


viewQuestsPanel : UiState -> ProgressionSummaryView -> Html Msg
viewQuestsPanel ui prog =
    section [ class "glass-panel quests-panel", attribute "data-testid" "quests-panel" ]
        [ div [ class "panel-header" ]
            [ div [ class "panel-title" ] [ text "Quests (5 Acts x 3 Diff)" ]
            , div [ class "segmented" ]
                [ progDiffButton ui.progressionDifficulty Normal "Normal"
                , progDiffButton ui.progressionDifficulty Nightmare "Nightmare"
                , progDiffButton ui.progressionDifficulty Hell "Hell"
                ]
            ]
        , div [ class "paint-toolbar", attribute "data-testid" "paint-toolbar" ]
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
            , button
                [ class "seg-btn"
                , attribute "data-testid" "all-diff-quests-btn"
                , onClick ToggleAllDiffQuests
                ]
                [ text "All Diff Quests" ]
            , button
                [ class "seg-btn"
                , attribute "data-testid" "all-diff-waypoints-btn"
                , onClick ToggleAllDiffWaypoints
                ]
                [ text "All Diff WPs" ]
            ]
        , case ui.marquee of
            Just box ->
                viewMarqueePreview ui.progressionDifficulty box

            Nothing ->
                text ""
        , div [ class "quests-matrix-stack", attribute "data-testid" "quest-matrix" ]
            [ viewQuestDiffCard ui prog Normal "diff-normal" "Normal"
            , viewQuestDiffCard ui prog Nightmare "diff-nm" "Nightmare"
            , viewQuestDiffCard ui prog Hell "diff-hell" "Hell"
            ]
        ]


viewQuestDiffCard : UiState -> ProgressionSummaryView -> DifficultyMode -> String -> String -> Html Msg
viewQuestDiffCard ui prog diff diffClass titleLabel =
    let
        completed =
            questsForDiff diff prog

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
            (List.map (viewQuestActRow diff isActiveDiff ui.paintMode completed ui.marquee prog.questCatalog) acts)
        ]


viewQuestActRow :
    DifficultyMode
    -> Bool
    -> Maybe PaintMode
    -> List String
    -> Maybe MarqueeSelection
    -> List QuestCatalogEntryView
    -> Int
    -> Html Msg
viewQuestActRow diff isActiveDiff paintMode completed maybeMarquee catalog actNum =
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
            (List.map (viewQuestDot diff isActiveDiff paintMode completed maybeMarquee) actEntries
                ++ List.repeat padCount (div [ class "quest-cell-dot empty" ] [])
            )
        ]


viewQuestDot :
    DifficultyMode
    -> Bool
    -> Maybe PaintMode
    -> List String
    -> Maybe MarqueeSelection
    -> QuestCatalogEntryView
    -> Html Msg
viewQuestDot diff isActiveDiff paintMode completed maybeMarquee entry =
    let
        isCompleted =
            List.member entry.key completed

        inMarquee =
            isActiveDiff
                && (case maybeMarquee of
                        Just box ->
                            box.kind == DragQuests && List.member entry.key box.candidateIds

                        Nothing ->
                            False
                   )

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
            , ( "is-marquee-target", inMarquee )
            ]
         , attribute "data-testid" testIdAttr
         , title (entry.key ++ " · " ++ entry.name)
         ]
            ++ tileInteractionAttrs paintMode diff DragQuests entry.key (ClickQuestTile diff entry.key)
        )
        [ text ("Q" ++ String.fromInt entry.index) ]


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
    section [ class "glass-panel waypoints-panel", attribute "data-testid" "waypoint-matrix" ]
        [ div [ class "panel-header" ]
            [ div [ class "panel-title" ] [ text "Waypoints (5 Acts)" ]
            , span [ class "meta-line" ]
                [ text (String.fromInt (List.length activeWaypoints) ++ "/" ++ String.fromInt (List.length prog.waypointCatalog)) ]
            ]
        , div [ class "waypoint-box" ]
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


tileInteractionAttrs : Maybe PaintMode -> DifficultyMode -> DragKind -> String -> Msg -> List (Html.Attribute Msg)
tileInteractionAttrs paintMode diff kind tileId clickMsg =
    case paintMode of
        Nothing ->
            [ onClick clickMsg ]

        Just _ ->
            [ on "mousedown" (Json.Decode.succeed (BeginMarquee kind 0 0 [ tileId ]))
            , on "mouseenter" (Json.Decode.succeed (ExtendMarquee kind tileId))
            , stopPropagationOn "mouseup" (Json.Decode.succeed ( CommitMarquee diff, True ))
            ]


viewWaypointTile : DifficultyMode -> Maybe PaintMode -> List Int -> Maybe MarqueeSelection -> WaypointCatalogEntryView -> Html Msg
viewWaypointTile diff paintMode activeBits maybeMarquee entry =
    let
        isActive =
            List.member entry.bit activeBits

        bitKey =
            String.fromInt entry.bit

        inMarquee =
            case maybeMarquee of
                Just box ->
                    box.kind == DragWaypoints && List.member bitKey box.candidateIds

                Nothing ->
                    False
    in
    button
        ([ classList
            [ ( "matrix-tile", True )
            , ( "wp-node", True )
            , ( "is-completed", isActive )
            , ( "active", isActive )
            , ( "is-marquee-target", inMarquee )
            ]
         , attribute "data-testid" ("wp-tile-" ++ bitKey)
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
