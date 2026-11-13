module Main exposing (main)

import Bridge.Ports as Ports
import Browser
import Component.Workspace as Workspace
import Generated.Protocol exposing (uiIntentEncoder)
import Html exposing (Html)
import Json.Decode
import Update exposing (Model, Msg(..), OutMsg(..))


main : Program Json.Decode.Value Model Msg
main =
    Browser.element
        { init = init
        , view = view
        , update = updateWithPorts
        , subscriptions =
            \_ ->
                Sub.batch
                    [ Ports.receiveSessionEnvelope ReceiveSessionEnvelope
                    , Ports.receiveKeyShortcut HandleKeyShortcut
                    ]
        }


init : Json.Decode.Value -> ( Model, Cmd Msg )
init initialEnvelope =
    case Json.Decode.decodeValue (Json.Decode.null ()) initialEnvelope of
        Ok () ->
            ( Update.init, Cmd.none )

        Err _ ->
            let
                ( nextModel, _ ) =
                    Update.update (ReceiveSessionEnvelope initialEnvelope) Update.init
            in
            ( nextModel, Cmd.none )


updateWithPorts : Msg -> Model -> ( Model, Cmd Msg )
updateWithPorts msg model =
    let
        ( nextModel, outMsg ) =
            Update.update msg model
    in
    ( nextModel, outMsgToCmd outMsg )


outMsgToCmd : OutMsg -> Cmd Msg
outMsgToCmd outMsg =
    case outMsg of
        NoOut ->
            Cmd.none

        EmitIntent intent ->
            Ports.sendIntent (uiIntentEncoder intent)

        ReportDecodeError _ ->
            Cmd.none


view : Model -> Html Msg
view model =
    Workspace.view model
