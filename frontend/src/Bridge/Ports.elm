port module Bridge.Ports exposing
    ( KeyShortcutPayload
    , emitHarnessSnapshot
    , receiveHarnessCommand
    , receiveKeyShortcut
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
