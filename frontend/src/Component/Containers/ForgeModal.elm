module Component.Containers.ForgeModal exposing (viewItemEditorModal)

import Component.Containers.Shared exposing (..)
import Generated.Protocol exposing (CharacterSchemaView, CharacterStatsView, ContainerCountsView, ContainerKind(..), ItemCardView, ItemQualityKind(..), MercSummaryView, PackedSkillSpecView, ParamCatalogView, ParamOptionView, SessionView, SetBonusView, ShopCatalogItemView, SocketedItemView, StatCatalogEntryView, StatParamKind(..))
import Html exposing (Html, button, div, h2, h3, img, input, li, option, section, select, span, text, ul)
import Html.Attributes exposing (alt, attribute, checked, class, classList, disabled, id, placeholder, readonly, selected, src, style, title, type_, value)
import Html.Events exposing (custom, on, onBlur, onClick, onFocus, onInput, onMouseEnter, onMouseLeave)
import Json.Decode as Decode
import State.UiState as UiState exposing (ItemMenuState(..), ModalNotice(..), StashTab(..), UiState)
import Update exposing (Msg(..))


viewItemEditorModal : UiState -> SessionView -> Html Msg
viewItemEditorModal ui session =
    case ui.itemEditorModalIndex of
        Nothing ->
            text ""

        Just targetIdx ->
            let
                draftItem =
                    session.activeItemDraft
                        |> Maybe.andThen
                            (\d ->
                                if d.itemIndex == targetIdx then
                                    Just d.card

                                else
                                    Nothing
                            )

                resolvedItem =
                    case draftItem of
                        Just dCard ->
                            Just dCard

                        Nothing ->
                            (session.items ++ session.mercItems ++ session.mercReplacedItems)
                                |> List.filter (\it -> it.index == targetIdx)
                                |> List.head
            in
            case resolvedItem of
                Nothing ->
                    text ""

                Just item ->
                    let
                        isDraftDirty =
                            session.activeItemDraft
                                |> Maybe.map (\d -> d.itemIndex == targetIdx && d.dirty)
                                |> Maybe.withDefault False

                        queryLower =
                            String.trim (String.toLower ui.itemEditorSearch)

                        existingStatIds =
                            List.map .statId item.detailedProperties

                        filteredCatalog : List StatCatalogEntryView
                        filteredCatalog =
                            session.statCatalog
                                |> List.filter
                                    (\s ->
                                        not (List.member s.statId existingStatIds)
                                            && (String.isEmpty queryLower
                                                    || String.contains queryLower (String.toLower s.name)
                                                    || String.contains queryLower (String.fromInt s.statId)
                                                    || String.contains queryLower ("#" ++ String.fromInt s.statId)
                                               )
                                    )

                        selectedInjectStat : Maybe StatCatalogEntryView
                        selectedInjectStat =
                            ui.itemEditorInjectStatId
                                |> Maybe.andThen
                                    (\sid ->
                                        session.statCatalog
                                            |> List.filter (\s -> s.statId == sid)
                                            |> List.head
                                    )

                        stopPropClick =
                            custom "click" (Decode.succeed { message = CloseItemMenu, stopPropagation = True, preventDefault = False })
                    in
                    div
                        [ class "item-editor-backdrop"
                        , id "item-editor-modal"
                        , attribute "data-testid" "item-editor-modal"
                        , onClick CloseItemEditorModal
                        ]
                        [ div [ class "item-editor-dialog", stopPropClick ]
                            [ div [ class "item-editor-header" ]
                                [ div [ class "item-editor-title" ]
                                    [ span [] [ text "⚒️" ]
                                    , span [] [ text ("Item Forge Studio · " ++ item.nameEn) ]
                                    ]
                                , button
                                    [ class "item-editor-close-btn"
                                    , attribute "data-testid" "item-editor-close-btn"
                                    , title "Close (Esc)"
                                    , onClick CloseItemEditorModal
                                    ]
                                    [ text "✕" ]
                                ]
                            , div [ class "item-editor-preview-bar" ]
                                [ div [ class "item-editor-preview-img-box" ]
                                    [ case item.imageUrl of
                                        Just url ->
                                            img [ src url, alt item.nameEn ] []

                                        Nothing ->
                                            span [ class "meta-line" ] [ text item.code ]
                                    ]
                                , div [ class "item-editor-preview-meta" ]
                                    [ div [ class "item-editor-preview-row1" ]
                                        [ span [ class ("item-editor-preview-name " ++ qualityClass item.quality) ] [ text item.nameEn ]
                                        , span [ class "item-editor-preview-pill" ] [ text (UiState.qualityLabel item.quality) ]
                                        , span [ class "item-editor-preview-sub" ] [ text ("iLvl: " ++ String.fromInt item.itemLevel) ]
                                        , span [ class "item-editor-preview-sub" ] [ text (UiState.containerLabel item.container) ]
                                        ]
                                    , div [ class "item-editor-preview-summary-line" ]
                                        [ text (String.join " · " (List.take 4 item.properties)) ]
                                    ]
                                ]
                            , div [ class "item-editor-body" ]
                                [ div [ class "item-editor-col item-editor-col-1" ]
                                    [ div [ class "item-editor-col-title" ]
                                        [ span [] [ text "Base Stats & Flags" ]
                                        , span [ class "meta-line" ] [ text "COL 1" ]
                                        ]
                                    , div [ class "item-editor-field-row" ]
                                        [ span [ class "item-editor-field-label" ] [ text "Ethereal" ]
                                        , button
                                            [ class "mini-btn"
                                            , attribute "data-testid" "modal-ethereal-toggle"
                                            , onClick (MutateItemBase item.index Nothing Nothing Nothing Nothing (Just (not item.isEthereal)))
                                            ]
                                            [ text
                                                (if item.isEthereal then
                                                    "Yes (Toggle)"

                                                 else
                                                    "No (Toggle)"
                                                )
                                            ]
                                        ]
                                    , case item.defense of
                                        Just def ->
                                            div [ class "item-editor-field-row" ]
                                                [ span [ class "item-editor-field-label" ] [ text "Defense" ]
                                                , div [ class "stepper-stack" ]
                                                    [ viewInlineIntInput ui "item-editor-input item-editor-prop-num" ("item_def:" ++ String.fromInt item.index) def "modal-defense-input"
                                                    , div [ class "stepper-btn-row" ]
                                                        [ button [ class "mini-btn", onClick (MutateItemBase item.index (Just (def - 1)) Nothing Nothing Nothing Nothing) ] [ text "-1" ]
                                                        , button [ class "mini-btn", onClick (MutateItemBase item.index (Just (def + 1)) Nothing Nothing Nothing Nothing) ] [ text "+1" ]
                                                        , button [ class "mini-btn", onClick (MutateItemBase item.index (Just (def - 10)) Nothing Nothing Nothing Nothing) ] [ text "-10" ]
                                                        , button [ class "mini-btn", onClick (MutateItemBase item.index (Just (def + 10)) Nothing Nothing Nothing Nothing) ] [ text "+10" ]
                                                        ]
                                                    ]
                                                ]

                                        Nothing ->
                                            text ""
                                    , case ( item.currentDurability, item.maxDurability ) of
                                        ( Just cur, Just mx ) ->
                                            div [ class "item-editor-field-row" ]
                                                [ span [ class "item-editor-field-label" ] [ text "Durability" ]
                                                , div [ class "row-actions" ]
                                                    [ button [ class "mini-btn", onClick (MutateItemBase item.index Nothing (Just mx) Nothing Nothing Nothing) ] [ text "Repair" ]
                                                    , viewInlineIntInput ui "item-editor-input item-editor-prop-num" ("item_dur:" ++ String.fromInt item.index) cur "modal-durability-input"
                                                    , span [ class "meta-line" ] [ text ("/ " ++ String.fromInt mx) ]
                                                    ]
                                                ]

                                        _ ->
                                            text ""
                                    , case item.quantity of
                                        Just qty ->
                                            div [ class "item-editor-field-row" ]
                                                [ span [ class "item-editor-field-label" ] [ text "Quantity" ]
                                                , div [ class "stepper-stack" ]
                                                    [ viewInlineIntInput ui "item-editor-input item-editor-prop-num" ("item_qty:" ++ String.fromInt item.index) qty "modal-quantity-input"
                                                    , div [ class "stepper-btn-row" ]
                                                        [ button [ class "mini-btn", onClick (MutateItemBase item.index Nothing Nothing Nothing (Just (qty - 1)) Nothing) ] [ text "-1" ]
                                                        , button [ class "mini-btn", onClick (MutateItemBase item.index Nothing Nothing Nothing (Just (qty + 1)) Nothing) ] [ text "+1" ]
                                                        , button [ class "mini-btn", onClick (MutateItemBase item.index Nothing Nothing Nothing (Just (qty - 10)) Nothing) ] [ text "-10" ]
                                                        , button [ class "mini-btn", onClick (MutateItemBase item.index Nothing Nothing Nothing (Just (qty + 10)) Nothing) ] [ text "+10" ]
                                                        ]
                                                    ]
                                                ]

                                        Nothing ->
                                            text ""
                                    , div [ class "item-editor-field-row" ]
                                        [ span [ class "item-editor-field-label" ] [ text "Item Code" ]
                                        , input [ class "item-editor-input", value item.code, readonly True, disabled True ] []
                                        ]
                                    , viewSocketedItemsSection item
                                    ]
                                , div [ class "item-editor-col item-editor-col-2" ]
                                    [ div [ class "item-editor-col-title" ]
                                        [ span [] [ text ("Existing Properties (" ++ String.fromInt (List.length item.detailedProperties) ++ ")") ]
                                        , span [ class "meta-line" ] [ text "COL 2" ]
                                        ]
                                    , if List.isEmpty item.detailedProperties then
                                        div [ class "meta-line" ] [ text "No editable properties on this item." ]

                                      else
                                        div []
                                            (List.map
                                                (\prop ->
                                                    div [ class "item-editor-prop-row", attribute "data-testid" ("modal-prop-row-" ++ String.fromInt prop.statId) ]
                                                        ([ div [ class "item-editor-prop-header" ]
                                                            [ div [ class "item-editor-prop-title" ]
                                                                [ span [ class "item-editor-stat-id-badge" ] [ text ("#" ++ String.fromInt prop.statId) ]
                                                                , span [] [ text prop.name ]
                                                                , if prop.param /= 0 then
                                                                    span [ class "item-editor-stat-id-badge" ] [ text ("p:" ++ String.fromInt prop.param) ]

                                                                  else
                                                                    text ""
                                                                ]
                                                            , button
                                                                [ class "item-editor-prop-delete-btn"
                                                                , attribute "data-testid" ("modal-remove-prop-" ++ String.fromInt prop.statId)
                                                                , title "Remove property"
                                                                , onClick (RemoveItemProperty item.index prop.statId)
                                                                ]
                                                                [ text "🗑️" ]
                                                            ]
                                                        , div [ class "item-editor-prop-controls" ]
                                                            [ div [ class "stepper-stack" ]
                                                                [ div [ class "stepper-input-row" ]
                                                                    [ viewInlineIntInput ui
                                                                        "item-editor-input item-editor-prop-num"
                                                                        ("item_prop:" ++ String.fromInt item.index ++ ":" ++ String.fromInt prop.statId)
                                                                        prop.value
                                                                        ("modal-prop-input-" ++ String.fromInt prop.statId)
                                                                    , span [ class "item-editor-param-label" ] [ text (prop.valueLabel ++ " (" ++ String.fromInt prop.minValue ++ ".." ++ String.fromInt prop.maxValue ++ ")") ]
                                                                    ]
                                                                , div [ class "stepper-btn-row" ]
                                                                    [ button [ class "mini-btn", onClick (MutateItemProperty item.index prop.statId -1) ] [ text "-1" ]
                                                                    , button [ class "mini-btn", onClick (MutateItemProperty item.index prop.statId 1) ] [ text "+1" ]
                                                                    , button [ class "mini-btn", onClick (MutateItemProperty item.index prop.statId -10) ] [ text "-10" ]
                                                                    , button [ class "mini-btn", onClick (MutateItemProperty item.index prop.statId 10) ] [ text "+10" ]
                                                                    ]
                                                                ]
                                                            ]
                                                        ]
                                                            ++ (if prop.paramBits > 0 then
                                                                    let
                                                                        maxParam =
                                                                            (2 ^ min 30 prop.paramBits) - 1

                                                                        rawInput =
                                                                            viewNumericStepper
                                                                                ui
                                                                                ("item_param:" ++ String.fromInt item.index ++ ":" ++ String.fromInt prop.statId)
                                                                                ("modal-prop-param-input-" ++ String.fromInt prop.statId)
                                                                                0
                                                                                maxParam
                                                                                prop.param
                                                                                (\newParam -> SetItemPropertyParam item.index prop.statId newParam)
                                                                    in
                                                                    viewParamRows
                                                                        ui
                                                                        session.paramCatalog
                                                                        prop.paramKind
                                                                        prop.paramLabel
                                                                        maxParam
                                                                        prop.param
                                                                        ("item_param_lvl:" ++ String.fromInt item.index ++ ":" ++ String.fromInt prop.statId)
                                                                        ("modal-prop-param-input-" ++ String.fromInt prop.statId)
                                                                        (\newParam -> SetItemPropertyParam item.index prop.statId newParam)
                                                                        rawInput
                                                                        prop.paramPreview

                                                                else
                                                                    []
                                                               )
                                                        )
                                                )
                                                item.detailedProperties
                                            )
                                    ]
                                , div [ class "item-editor-col item-editor-col-3" ]
                                    [ div [ class "item-editor-col-title" ]
                                        [ span [] [ text "Search & Inject Option" ]
                                        , span [ class "meta-line" ]
                                            [ text
                                                (String.fromInt (List.length filteredCatalog)
                                                    ++ " / "
                                                    ++ String.fromInt (List.length session.statCatalog)
                                                    ++ " stats"
                                                )
                                            ]
                                        ]
                                    , div [ class "item-editor-search-box" ]
                                        [ input
                                            [ class "item-editor-input"
                                            , attribute "data-testid" "item-editor-stat-search"
                                            , placeholder "Search stat name or #id..."
                                            , value ui.itemEditorSearch
                                            , onInput SetItemEditorSearch
                                            ]
                                            []
                                        ]
                                    , div [ class "item-editor-search-results-list", attribute "data-testid" "item-editor-stat-results" ]
                                        (List.map
                                            (\entry ->
                                                let
                                                    isSel =
                                                        ui.itemEditorInjectStatId == Just entry.statId

                                                    defaultVal =
                                                        clamp entry.minValue entry.maxValue 10
                                                in
                                                div
                                                    [ classList [ ( "item-editor-search-item", True ), ( "selected", isSel ) ]
                                                    , attribute "data-testid" ("modal-catalog-stat-" ++ String.fromInt entry.statId)
                                                    , onClick (SelectItemEditorInjectStat entry.statId defaultVal)
                                                    ]
                                                    [ span [] [ text ("#" ++ String.fromInt entry.statId ++ " " ++ entry.name) ]
                                                    , span [ class "meta-line" ]
                                                        [ text
                                                            (String.fromInt entry.minValue
                                                                ++ ".."
                                                                ++ String.fromInt entry.maxValue
                                                                ++ (if entry.paramBits > 0 then
                                                                        " · param"

                                                                    else
                                                                        ""
                                                                   )
                                                            )
                                                        ]
                                                    ]
                                            )
                                            filteredCatalog
                                        )
                                    , case selectedInjectStat of
                                        Nothing ->
                                            div [ class "meta-line" ] [ text "Select a stat above to inject." ]

                                        Just sel ->
                                            let
                                                maxParam =
                                                    (2 ^ min 30 sel.paramBits) - 1
                                            in
                                            div [ class "item-editor-injector-panel", attribute "data-testid" "item-editor-injector-panel" ]
                                                ([ div [ class "item-editor-prop-title" ] [ text ("#" ++ String.fromInt sel.statId ++ " " ++ sel.name) ]
                                                 , div [ class "item-editor-prop-controls" ]
                                                    [ viewNumericStepperWithAction
                                                        ui
                                                        ("inject_val:" ++ String.fromInt sel.statId)
                                                        "item-editor-inject-value"
                                                        ui.itemEditorInjectValue
                                                        (\v -> SetItemEditorInjectValue (String.fromInt v))
                                                        [ span [ class "item-editor-param-label" ] [ text (sel.valueLabel ++ " (" ++ String.fromInt sel.minValue ++ ".." ++ String.fromInt sel.maxValue ++ ")") ]
                                                        , button
                                                            [ class "mini-btn"
                                                            , attribute "data-testid" "btn-inject-stat"
                                                            , onClick (AddItemProperty item.index sel.statId ui.itemEditorInjectValue ui.itemEditorInjectParam)
                                                            ]
                                                            [ text "➕ Add" ]
                                                        ]
                                                    ]
                                                 ]
                                                    ++ (if sel.paramBits > 0 then
                                                            let
                                                                rawParamInput =
                                                                    viewNumericStepper ui ("inject_param:" ++ String.fromInt sel.statId) "item-editor-inject-param" 0 maxParam ui.itemEditorInjectParam (\p -> SetItemEditorInjectParam (String.fromInt p))
                                                            in
                                                            viewParamRows
                                                                ui
                                                                session.paramCatalog
                                                                sel.paramKind
                                                                sel.paramLabel
                                                                maxParam
                                                                ui.itemEditorInjectParam
                                                                ("inject_param_lvl:" ++ String.fromInt sel.statId)
                                                                "item-editor-inject-param"
                                                                (\newParam -> SetItemEditorInjectParam (String.fromInt (clamp 0 maxParam newParam)))
                                                                rawParamInput
                                                                Nothing

                                                        else
                                                            []
                                                       )
                                                )
                                    ]
                                ]
                            , viewItemEditorNoticeBar ui session.schema session.statCatalog session.paramCatalog.packedSkill item
                            , div [ class "item-editor-footer" ]
                                [ div [ class "item-editor-integrity-badge" ]
                                    [ text
                                        (if isDraftDirty then
                                            "🟡 Unsaved Draft Changes · Save to Commit"

                                         else
                                            "✅ Rust Draft Ready · Undoable on Save"
                                        )
                                    ]
                                , div [ class "row-actions" ]
                                    [ button
                                        [ class "mini-btn"
                                        , attribute "data-testid" "btn-forge-apply"
                                        , disabled (not isDraftDirty)
                                        , onClick ApplyItemForgeDraft
                                        ]
                                        [ text "💾 Save" ]
                                    , button
                                        [ class "mini-btn"
                                        , attribute "data-testid" "btn-forge-discard"
                                        , onClick DiscardItemForgeDraft
                                        ]
                                        [ text "↩️ Discard (Esc)" ]
                                    ]
                                ]
                            ]
                        ]


viewParamRows : UiState -> ParamCatalogView -> StatParamKind -> String -> Int -> Int -> String -> String -> (Int -> Msg) -> Html Msg -> Maybe String -> List (Html Msg)
viewParamRows ui catalog kind paramLabel maxParam currentParam lvlFieldKey tid onSelect fallbackInput maybePreview =
    let
        renderSelect selectTid selectedVal optionsList onPick =
            if List.isEmpty optionsList then
                fallbackInput

            else
                select
                    [ class "item-editor-input"
                    , attribute "data-testid" selectTid
                    , onInput
                        (\raw ->
                            String.toInt raw
                                |> Maybe.withDefault selectedVal
                                |> onPick
                        )
                    ]
                    (List.map
                        (\opt ->
                            option
                                [ value (String.fromInt opt.value)
                                , selected (opt.value == selectedVal)
                                ]
                                [ text opt.label ]
                        )
                        optionsList
                    )

        previewOrRange =
            case maybePreview of
                Just preview ->
                    span [ class "item-editor-stat-id-badge" ] [ text preview ]

                Nothing ->
                    span [ class "meta-line" ] [ text ("(0.." ++ String.fromInt maxParam ++ ")") ]

        singleRow controlNode =
            [ div [ class "item-editor-prop-controls" ]
                [ controlNode
                , span [ class "item-editor-param-label" ] [ text paramLabel ]
                , previewOrRange
                ]
            ]
    in
    case kind of
        ParamClass ->
            singleRow (renderSelect tid currentParam catalog.classes onSelect)

        ParamElement ->
            singleRow (renderSelect tid currentParam catalog.elements onSelect)

        ParamSkillTab ->
            singleRow (renderSelect tid currentParam catalog.skillTabs onSelect)

        ParamSkill ->
            singleRow (renderSelect tid currentParam catalog.skills onSelect)

        ParamPackedSkill ->
            if List.isEmpty catalog.skills then
                singleRow fallbackInput

            else
                let
                    stride =
                        max 1 catalog.packedSkill.stride

                    minLvl =
                        catalog.packedSkill.minLevel

                    maxLvl =
                        catalog.packedSkill.maxLevel

                    skillId =
                        currentParam // stride

                    skillLevel =
                        modBy stride currentParam
                in
                [ div [ class "item-editor-prop-controls" ]
                    [ renderSelect tid skillId catalog.skills (\newSid -> onSelect (newSid * stride + clamp minLvl maxLvl skillLevel))
                    , span [ class "item-editor-param-label" ] [ text paramLabel ]
                    ]
                , div [ class "item-editor-prop-controls" ]
                    [ viewNumericStepperWithAction ui lvlFieldKey (tid ++ "-level") skillLevel (\lvl -> onSelect (skillId * stride + lvl)) [ span [ class "item-editor-param-label" ] [ text ("Skill Lv (" ++ String.fromInt minLvl ++ ".." ++ String.fromInt maxLvl ++ ")") ] ]
                    ]
                ]

        _ ->
            singleRow fallbackInput


viewNumericStepper : UiState -> String -> String -> Int -> Int -> Int -> (Int -> Msg) -> Html Msg
viewNumericStepper ui fieldKey tid _ _ currentVal onSet =
    viewNumericStepperWithAction ui fieldKey tid currentVal onSet []


viewNumericStepperWithAction : UiState -> String -> String -> Int -> (Int -> Msg) -> List (Html Msg) -> Html Msg
viewNumericStepperWithAction ui fieldKey tid currentVal onSet trailingNodes =
    div [ class "stepper-stack" ]
        [ div [ class "stepper-input-row" ]
            (viewInlineIntInput ui "item-editor-input item-editor-prop-num" fieldKey currentVal tid :: trailingNodes)
        , div [ class "stepper-btn-row" ]
            [ button
                [ class "mini-btn"
                , onClick (onSet (currentVal - 1))
                ]
                [ text "-1" ]
            , button
                [ class "mini-btn"
                , onClick (onSet (currentVal + 1))
                ]
                [ text "+1" ]
            , button
                [ class "mini-btn"
                , onClick (onSet (currentVal - 10))
                ]
                [ text "-10" ]
            , button
                [ class "mini-btn"
                , onClick (onSet (currentVal + 10))
                ]
                [ text "+10" ]
            ]
        ]


viewItemEditorNoticeBar : UiState -> CharacterSchemaView -> List StatCatalogEntryView -> PackedSkillSpecView -> ItemCardView -> Html Msg
viewItemEditorNoticeBar ui schema statCatalog packedSkill item =
    let
        focusedHint =
            ui.focusedInput
                |> Maybe.andThen
                    (\focused ->
                        case String.split ":" focused.key of
                            [ "item_def", _ ] ->
                                Just ("Defense: allowed range 0 .. " ++ String.fromInt schema.itemDefenseMax)

                            [ "item_dur", _ ] ->
                                let
                                    maxDur =
                                        item.maxDurability |> Maybe.withDefault 255
                                in
                                Just ("Durability: allowed range 0 .. " ++ String.fromInt maxDur)

                            [ "item_qty", _ ] ->
                                Just ("Quantity: allowed range 1 .. " ++ String.fromInt schema.itemQuantityMax)

                            [ "item_prop", _, statIdStr ] ->
                                String.toInt statIdStr
                                    |> Maybe.andThen
                                        (\sid ->
                                            item.detailedProperties
                                                |> List.filter (\p -> p.statId == sid)
                                                |> List.head
                                        )
                                    |> Maybe.map
                                        (\p ->
                                            "#"
                                                ++ String.fromInt p.statId
                                                ++ " "
                                                ++ p.name
                                                ++ " ("
                                                ++ p.valueLabel
                                                ++ "): allowed range "
                                                ++ String.fromInt p.minValue
                                                ++ " .. "
                                                ++ String.fromInt p.maxValue
                                        )

                            [ "item_param", _, statIdStr ] ->
                                String.toInt statIdStr
                                    |> Maybe.andThen
                                        (\sid ->
                                            item.detailedProperties
                                                |> List.filter (\p -> p.statId == sid)
                                                |> List.head
                                        )
                                    |> Maybe.map
                                        (\p ->
                                            let
                                                maxParam =
                                                    (2 ^ min 30 p.paramBits) - 1

                                                previewSuffix =
                                                    p.paramPreview
                                                        |> Maybe.map (\pv -> " · " ++ pv)
                                                        |> Maybe.withDefault ""
                                            in
                                            "#"
                                                ++ String.fromInt p.statId
                                                ++ " "
                                                ++ p.name
                                                ++ " ("
                                                ++ p.paramLabel
                                                ++ "): allowed range 0 .. "
                                                ++ String.fromInt maxParam
                                                ++ previewSuffix
                                        )

                            [ "item_param_lvl", _, statIdStr ] ->
                                String.toInt statIdStr
                                    |> Maybe.andThen
                                        (\sid ->
                                            item.detailedProperties
                                                |> List.filter (\p -> p.statId == sid)
                                                |> List.head
                                        )
                                    |> Maybe.map
                                        (\p ->
                                            "#"
                                                ++ String.fromInt p.statId
                                                ++ " "
                                                ++ p.name
                                                ++ " (Skill Level): allowed range "
                                                ++ String.fromInt packedSkill.minLevel
                                                ++ " .. "
                                                ++ String.fromInt packedSkill.maxLevel
                                        )

                            [ "inject_val", statIdStr ] ->
                                String.toInt statIdStr
                                    |> Maybe.andThen (\sid -> statCatalog |> List.filter (\s -> s.statId == sid) |> List.head)
                                    |> Maybe.map
                                        (\s ->
                                            "Inject #"
                                                ++ String.fromInt s.statId
                                                ++ " "
                                                ++ s.name
                                                ++ " ("
                                                ++ s.valueLabel
                                                ++ "): allowed range "
                                                ++ String.fromInt s.minValue
                                                ++ " .. "
                                                ++ String.fromInt s.maxValue
                                        )

                            [ "inject_param", statIdStr ] ->
                                String.toInt statIdStr
                                    |> Maybe.andThen (\sid -> statCatalog |> List.filter (\s -> s.statId == sid) |> List.head)
                                    |> Maybe.map
                                        (\s ->
                                            let
                                                maxParam =
                                                    (2 ^ min 30 s.paramBits) - 1
                                            in
                                            "Inject #"
                                                ++ String.fromInt s.statId
                                                ++ " "
                                                ++ s.name
                                                ++ " ("
                                                ++ s.paramLabel
                                                ++ "): allowed range 0 .. "
                                                ++ String.fromInt maxParam
                                        )

                            [ "inject_param_lvl", statIdStr ] ->
                                String.toInt statIdStr
                                    |> Maybe.andThen (\sid -> statCatalog |> List.filter (\s -> s.statId == sid) |> List.head)
                                    |> Maybe.map
                                        (\s ->
                                            "Inject #"
                                                ++ String.fromInt s.statId
                                                ++ " "
                                                ++ s.name
                                                ++ " (Skill Level): allowed range "
                                                ++ String.fromInt packedSkill.minLevel
                                                ++ " .. "
                                                ++ String.fromInt packedSkill.maxLevel
                                        )

                            _ ->
                                Nothing
                    )

        ( levelClass, messageText ) =
            case ui.itemEditorNotice of
                Just (NoticeError msg) ->
                    ( "is-error", "❌ " ++ msg )

                Just (NoticeWarning msg) ->
                    ( "is-warning", "⚠️ " ++ msg )

                Just (NoticeInfo msg) ->
                    ( "is-info", "💡 " ++ msg )

                Nothing ->
                    case focusedHint of
                        Just hint ->
                            ( "is-info", "💡 " ++ hint )

                        Nothing ->
                            ( "is-idle", "Tip: Click any number to type directly (Enter to apply, Esc to cancel)." )
    in
    div
        [ class ("item-editor-notice-bar " ++ levelClass)
        , attribute "data-testid" "item-editor-notice"
        ]
        [ text messageText ]


