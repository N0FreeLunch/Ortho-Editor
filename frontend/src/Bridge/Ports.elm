port module Bridge.Ports exposing
    ( KeyShortcutPayload
    , MarqueeMovePayload
    , emitHarnessSnapshot
    , receiveHarnessCommand
    , receiveKeyShortcut
    , receiveMarqueeCancel
    , receiveMarqueeMove
    , receiveMarqueeUp
    , receiveSessionEnvelope
    , sendIntent
    )

import Json.Decode
import Json.Encode


type alias KeyShortcutPayload =
    { key : String
    , code : String
    , ctrlKey : Bool
    , altKey : Bool
    , metaKey : Bool
    , isInput : Bool
    }


type alias MarqueeMovePayload =
    { x : Float
    , y : Float
    , candidateIds : List String
    }


{-| Receives a serialized `SessionEnvelope` from the Rust host (WebView or Node harness).
-}
port receiveSessionEnvelope : (Json.Decode.Value -> msg) -> Sub msg


{-| Receives keyboard shortcut events from the browser host `keydown` listener.
-}
port receiveKeyShortcut : (KeyShortcutPayload -> msg) -> Sub msg


{-| Sends a serialized `UiIntent` from Elm to the Rust store.
-}
port sendIntent : Json.Encode.Value -> Cmd msg


{-| Receives scripted headless test commands in `Platform.worker` tests.
-}
port receiveHarnessCommand : (Json.Decode.Value -> msg) -> Sub msg


{-| Emits serialized `Model` state snapshots for headless `Platform.worker` assertions.
-}
port emitHarnessSnapshot : Json.Encode.Value -> Cmd msg


{-| Receives live marquee pointer coordinates and DOM-intersected candidate IDs from the host.
-}
port receiveMarqueeMove : (MarqueeMovePayload -> msg) -> Sub msg


{-| Receives global mouseup notification to commit an active marquee selection.
-}
port receiveMarqueeUp : (() -> msg) -> Sub msg


{-| Receives global cancel notification (Escape / window blur) for an active marquee selection.
-}
port receiveMarqueeCancel : (() -> msg) -> Sub msg
