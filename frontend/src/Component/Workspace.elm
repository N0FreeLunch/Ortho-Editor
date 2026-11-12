module Component.Workspace exposing (view)

import Component.CharacterPanel as CharacterPanel
import Component.ContainersPanel as ContainersPanel
import Component.MercenaryPanel as MercenaryPanel
import Component.ProgressionPanel as ProgressionPanel
import Generated.Protocol exposing (SessionView)
import Html exposing (Html, button, div, h1, h2, p, section, span, text)
import Html.Attributes exposing (attribute, class, classList, disabled, title)
import Html.Events exposing (on, onClick)
import Json.Decode
import State.UiState exposing (PaintMode(..), ThemeMode(..), ViewTab(..))
import Update exposing (Model, Msg(..))


view : Model -> Html Msg
view model =
    div
        [ classList
            [ ( "ortho-shell", True )
            , ( "theme-light", model.ui.theme == Light )
            ]
        ]
        [ viewTopHeader model
        , case model.ui.statusBanner of
            Just banner ->
                div [ class "status-banner", attribute "data-testid" "status-banner" ]
                    [ span [] [ text banner ]
                    , button [ class "btn-ghost", onClick ClearBanner ] [ text "Dismiss" ]
                    ]

            Nothing ->
                text ""
        , case model.session of
            Nothing ->
                section [ class "empty-state", attribute "data-testid" "empty-session" ]
                    [ h2 [] [ text "No save loaded" ]
                    , p [] [ text "Waiting for Rust host to emit a SessionView snapshot over ports." ]
                    ]

            Just session ->
                div [ class "main-workspace" ]
                    [ if not session.exportSupported then
                        div [ class "export-warning-banner", attribute "data-testid" "export-unsupported-notice" ]
                            [ text
                                (session.exportUnsupportedReason
                                    |> Maybe.withDefault
                                        ("Binary .d2s export is unavailable for save v"
                                            ++ String.fromInt session.saveVersion
                                            ++ "."
                                        )
                                )
                            ]

                      else
                        text ""
                    , case model.ui.activeView of
                        ViewA ->
                            ContainersPanel.view model.ui session

                        ViewB ->
                            let
                                panelMouseAttrs =
                                    case model.ui.marquee of
                                        Just _ ->
                                            [ on "mouseup" (Json.Decode.succeed (CommitMarquee model.ui.progressionDifficulty))
                                            , on "mouseleave" (Json.Decode.succeed CancelMarquee)
                                            ]

                                        Nothing ->
                                            []
                            in
                            Html.main_ [ class "spec-grid", attribute "data-testid" "view-b-workspace" ]
                                [ CharacterPanel.view model.ui session
                                , CharacterPanel.viewSkillTreePreview model.ui session
                                , section
                                    ([ classList
                                        [ ( "spec-progression-wrap", True )
                                        , ( "progression-panel", True )
                                        , ( "is-marquee-mode", model.ui.paintMode /= Nothing )
                                        , ( "mode-activate", model.ui.paintMode == Just Activate )
                                        , ( "mode-deactivate", model.ui.paintMode == Just Deactivate )
                                        ]
                                     , attribute "data-testid" "progression-panel"
                                     ]
                                        ++ panelMouseAttrs
                                    )
                                    [ ProgressionPanel.viewQuestsPanel model.ui session.progression
                                    , div [ class "spec-col4-stack" ]
                                        [ ProgressionPanel.viewWaypointsPanel model.ui session.progression
                                        , MercenaryPanel.view session.merc session.mercItems
                                        , CharacterPanel.viewCombatLabAccordion model.ui session
                                        ]
                                    ]
                                ]
                    ]
        ]


viewTopHeader : Model -> Html Msg
viewTopHeader model =
    let
        isDirty =
            model.session
                |> Maybe.map .dirty
                |> Maybe.withDefault False

        themeLabel =
            case model.ui.theme of
                Dark ->
                    "Theme: Dark"

                Light ->
                    "Theme: Light"

        heroBadge =
            case model.session of
                Nothing ->
                    div [ class "char-badge-container" ]
                        [ div [ class "char-class-icon" ] [ text "OE" ]
                        , div [ class "char-meta" ]
                            [ h1 [ class "brand-title" ]
                                [ text "Ortho Editor"
                                , span [ class "badge-gold" ] [ text "Rust-as-Store" ]
                                ]
                            , div [ class "char-subtitle" ]
                                [ text "Pure Elm Renderer · Waiting for SessionView snapshot" ]
                            ]
                        ]

                Just session ->
                    viewHeroCharacterBadge session isDirty
    in
    Html.header [ class "header-panel" ]
        [ heroBadge
        , div [ class "header-center" ]
            [ div [ class "view-tabs" ]
                [ button
                    [ classList
                        [ ( "view-tab-btn", True )
                        , ( "tab-btn", True )
                        , ( "active", model.ui.activeView == ViewA )
                        , ( "is-active", model.ui.activeView == ViewA )
                        ]
                    , attribute "data-testid" "tab-view-a"
                    , onClick (SwitchView ViewA)
                    ]
                    [ text "View A · Items" ]
                , button
                    [ classList
                        [ ( "view-tab-btn", True )
                        , ( "tab-btn", True )
                        , ( "active", model.ui.activeView == ViewB )
                        , ( "is-active", model.ui.activeView == ViewB )
                        ]
                    , attribute "data-testid" "tab-view-b"
                    , onClick (SwitchView ViewB)
                    ]
                    [ text "View B · Character & Progression" ]
                ]
            ]
        , div [ class "header-right" ]
            [ div [ class "history-controls" ]
                [ button
                    [ attribute "data-testid" "undo-preview-btn"
                    , disabled True
                    , title "Preview only: Undo history requires Rust session slice"
                    ]
                    [ text "Undo" ]
                , button
                    [ attribute "data-testid" "redo-preview-btn"
                    , disabled True
                    , title "Preview only: Redo history requires Rust session slice"
                    ]
                    [ text "Redo" ]
                ]
            , button
                [ class "theme-toggle-btn"
                , attribute "data-testid" "theme-toggle-btn"
                , onClick ToggleTheme
                ]
                [ text themeLabel ]
            ]
        ]


viewHeroCharacterBadge : SessionView -> Bool -> Html Msg
viewHeroCharacterBadge session isDirty =
    let
        classGlyph =
            String.left 3 session.charClass

        expansionTag =
            if session.isExpansion then
                "Expansion"

            else
                "Classic"

        exportBadge =
            if session.exportSupported then
                span [ class "export-badge is-supported", attribute "data-testid" "export-support-badge" ]
                    [ text ("v" ++ String.fromInt session.saveVersion ++ " · .d2s Export Ready") ]

            else
                span [ class "export-badge is-unsupported", attribute "data-testid" "export-support-badge" ]
                    [ text ("Read-only v" ++ String.fromInt session.saveVersion ++ " · Export requires v105") ]
    in
    div [ class "char-badge-container" ]
        [ div [ class "char-class-icon" ] [ text classGlyph ]
        , div [ class "char-meta" ]
            [ h1 [ class "brand-title" ]
                [ text session.charName
                , span [ class "badge-gold" ] [ text ("Lv " ++ String.fromInt session.charLevel ++ " " ++ session.charClass) ]
                , exportBadge
                , if isDirty then
                    span [ class "unsaved-badge", attribute "data-testid" "unsaved-badge" ] [ text "Unsaved changes" ]

                  else
                    text ""
                ]
            , div [ class "char-subtitle" ]
                [ text
                    (expansionTag
                        ++ " · "
                        ++ session.itemDialect
                        ++ " · "
                        ++ session.fixtureLabel
                    )
                ]
            , div [ class "header-status-pills" ]
                [ span [ class "pill" ] [ text ("Inv Gold: " ++ String.fromInt session.stats.goldInventory) ]
                , span [ class "pill" ] [ text ("Stash Gold: " ++ String.fromInt session.stats.goldStash) ]
                , span [ class "pill" ] [ text ("Items: " ++ String.fromInt (List.length session.items)) ]
                ]
            ]
        ]
