module Component.ProgressionPanel exposing (view)

import Generated.Protocol
    exposing
        ( DifficultyMode(..)
        , ProgressionSummaryView
        , QuestCatalogEntryView
        , WaypointCatalogEntryView
        )
import Html exposing (Html, button, div, h2, h3, section, span, text)
import Html.Attributes exposing (attribute, class, classList)
import Html.Events exposing (onClick)
import State.UiState exposing (DragKind(..), MarqueeSelection, PaintMode(..), UiState)
import Update exposing (Msg(..))


view : UiState -> ProgressionSummaryView -> Html Msg
view ui prog =
    let
        activeDiff =
            ui.progressionDifficulty

        completedQuests =
            questsForDiff activeDiff prog

        activeWaypoints =
            waypointsForDiff activeDiff prog
    in
    section
        [ classList
            [ ( "panel", True )
            , ( "progression-panel", True )
            , ( "is-marquee-mode", ui.paintMode /= Nothing )
            , ( "mode-activate", ui.paintMode == Just Activate )
            , ( "mode-deactivate", ui.paintMode == Just Deactivate )
            ]
        , attribute "data-testid" "progression-panel"
        ]
        [ div [ class "panel-header" ]
            [ h2 [] [ text "Quests & Waypoints" ]
            , div [ class "segmented" ]
                [ progDiffButton activeDiff Normal "Normal"
                , progDiffButton activeDiff Nightmare "Nightmare"
                , progDiffButton activeDiff Hell "Hell"
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
                viewMarqueePreview activeDiff box

            Nothing ->
                text ""
        , div [ class "matrix-split" ]
            [ div [ class "quest-matrix", attribute "data-testid" "quest-matrix" ]
                [ h3 [ class "subhead" ]
                    [ text ("Quests (" ++ String.fromInt (List.length completedQuests) ++ "/" ++ String.fromInt (List.length prog.questCatalog) ++ ")") ]
                , div [ class "tile-grid" ]
                    (List.map (viewQuestTile activeDiff completedQuests ui.marquee) prog.questCatalog)
                ]
            , div [ class "waypoint-matrix", attribute "data-testid" "waypoint-matrix" ]
                [ h3 [ class "subhead" ]
                    [ text ("Waypoints (" ++ String.fromInt (List.length activeWaypoints) ++ "/" ++ String.fromInt (List.length prog.waypointCatalog) ++ ")") ]
                , div [ class "tile-grid" ]
                    (List.map (viewWaypointTile activeDiff activeWaypoints ui.marquee) prog.waypointCatalog)
                ]
            ]
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


viewQuestTile : DifficultyMode -> List String -> Maybe MarqueeSelection -> QuestCatalogEntryView -> Html Msg
viewQuestTile diff completed maybeMarquee entry =
    let
        isCompleted =
            List.member entry.key completed

        inMarquee =
            case maybeMarquee of
                Just box ->
                    box.kind == DragQuests && List.member entry.key box.candidateIds

                Nothing ->
                    False
    in
    button
        [ classList
            [ ( "matrix-tile", True )
            , ( "is-completed", isCompleted )
            , ( "is-marquee-target", inMarquee )
            ]
        , attribute "data-testid" ("quest-tile-" ++ entry.key)
        , onClick (ClickQuestTile diff entry.key)
        ]
        [ span [ class "tile-code" ] [ text entry.key ]
        , span [ class "tile-name" ] [ text entry.name ]
        ]


viewWaypointTile : DifficultyMode -> List Int -> Maybe MarqueeSelection -> WaypointCatalogEntryView -> Html Msg
viewWaypointTile diff activeBits maybeMarquee entry =
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
        [ classList
            [ ( "matrix-tile", True )
            , ( "is-completed", isActive )
            , ( "is-marquee-target", inMarquee )
            ]
        , attribute "data-testid" ("wp-tile-" ++ bitKey)
        , onClick (ClickWaypointTile diff entry.bit)
        ]
        [ span [ class "tile-code" ] [ text ("WP " ++ bitKey) ]
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
