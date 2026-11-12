module Component.Workspace exposing (view)

import Component.CharacterPanel as CharacterPanel
import Component.ContainersPanel as ContainersPanel
import Component.MercenaryPanel as MercenaryPanel
import Component.ProgressionPanel as ProgressionPanel
import Html exposing (Html, button, div, h1, h2, p, section, span, text)
import Html.Attributes exposing (attribute, class, classList)
import Html.Events exposing (onClick)
import State.UiState exposing (ViewTab(..))
import Update exposing (Model, Msg(..))


view : Model -> Html Msg
view model =
    div [ class "ortho-shell" ]
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
                case model.ui.activeView of
                    ViewA ->
                        ContainersPanel.view model.ui session

                    ViewB ->
                        Html.main_ [ class "viewb-grid", attribute "data-testid" "view-b-workspace" ]
                            [ CharacterPanel.view model.ui session
                            , div [ class "viewb-right-col" ]
                                [ MercenaryPanel.view session.merc session.mercItems
                                , ProgressionPanel.view model.ui session.progression
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
    in
    Html.header [ class "top-header" ]
        [ div [ class "brand-block" ]
            [ div [ class "brand-title-row" ]
                [ h1 [ class "brand-title" ] [ text "Ortho Editor" ]
                , if isDirty then
                    span [ class "unsaved-badge", attribute "data-testid" "unsaved-badge" ] [ text "Unsaved changes" ]

                  else
                    text ""
                ]
            , span [ class "brand-sub" ] [ text "Rust-as-Store / Elm Pure Renderer" ]
            ]
        , div [ class "view-tabs" ]
            [ button
                [ classList [ ( "tab-btn", True ), ( "is-active", model.ui.activeView == ViewA ) ]
                , attribute "data-testid" "tab-view-a"
                , onClick (SwitchView ViewA)
                ]
                [ text "View A · Items" ]
            , button
                [ classList [ ( "tab-btn", True ), ( "is-active", model.ui.activeView == ViewB ) ]
                , attribute "data-testid" "tab-view-b"
                , onClick (SwitchView ViewB)
                ]
                [ text "View B · Character & Progression" ]
            ]
        ]
