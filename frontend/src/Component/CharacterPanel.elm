module Component.CharacterPanel exposing (view, viewCombatLabAccordion, viewSkillTreePreview)

import Generated.Protocol
    exposing
        ( CharacterSchemaView
        , CharacterStatsView
        , DifficultyMode(..)
        , EditMode(..)
        , ResistanceCardView
        , SessionView
        , SkillDamageView
        , SkillNodeView
        )
import Html exposing (Html, button, div, h2, input, p, section, span, text)
import Html.Attributes exposing (attribute, class, classList, disabled, style, title, type_, value)
import Html.Events exposing (on, onBlur, onClick, onFocus, onInput, onMouseEnter, onMouseLeave)
import Json.Decode
import State.UiState exposing (UiState)
import Update exposing (Msg(..))


view : UiState -> SessionView -> Html Msg
view ui session =
    let
        schema =
            session.schema
    in
    section [ class "glass-panel col1-card", attribute "data-testid" "character-summary" ]
        [ div [ class "panel-header" ]
            [ div [ class "panel-title" ] [ text "Character Profile & Stats" ]
            , span [ class "fixture-chip" ] [ text session.fixtureLabel ]
            ]
        , div [ class "identity-row" ]
            [ div []
                [ h2 [ attribute "data-testid" "char-name" ] [ text session.charName ]
                , div [ class "meta-line" ]
                    [ text (session.charClass ++ " · Lv " ++ String.fromInt session.charLevel)
                    , text (" · v" ++ String.fromInt session.saveVersion ++ " (" ++ session.itemDialect ++ ")")
                    ]
                ]
            ]
        , div [ class "stat-strip" ]
            [ editableFieldChip ui
                "char_name"
                "Name"
                session.charName
                (String.fromInt schema.nameMinLen ++ ".." ++ String.fromInt schema.nameMaxLen ++ " ASCII")
                "input-char-name"
            , editableStepperFieldChip ui
                "level"
                "Level"
                (String.fromInt session.charLevel)
                (String.fromInt schema.levelMin ++ ".." ++ String.fromInt schema.levelMax)
                "input-char-level"
            ]
        , viewStatModeStrip ui schema session.statMode session.stats
        , div [ class "stat-breakdown-grid" ]
            [ editableStatChip ui session.statMode "strength" "STR" session.stats.strengthBase session.stats.strengthBonus schema.classBaseStr schema "input-stat-strength"
            , editableStatChip ui session.statMode "dexterity" "DEX" session.stats.dexterityBase session.stats.dexterityBonus schema.classBaseDex schema "input-stat-dexterity"
            , editableStatChip ui session.statMode "vitality" "VIT" session.stats.vitalityBase session.stats.vitalityBonus schema.classBaseVit schema "input-stat-vitality"
            , editableStatChip ui session.statMode "energy" "ENE" session.stats.energyBase session.stats.energyBonus schema.classBaseEne schema "input-stat-energy"
            ]
        , viewVitalsGauges session.stats
        , div [ class "stat-strip" ]
            [ editableFieldChip ui
                "gold_character"
                "Gold Inv"
                (String.fromInt session.stats.goldInventory)
                ("<= " ++ String.fromInt schema.goldCharacterMax)
                "input-gold-inventory"
            , editableFieldChip ui
                "gold_stash"
                "Gold Stash"
                (String.fromInt session.stats.goldStash)
                ("<= " ++ String.fromInt schema.goldStashMax)
                "input-gold-stash"
            ]
        , viewResistancesSection session.resistances
        ]


viewVitalsGauges : CharacterStatsView -> Html Msg
viewVitalsGauges stats =
    div [ class "vitals-group", attribute "data-testid" "vitals-gauges" ]
        [ vitalBar "Life" "life" stats.life stats.maxLife stats.lifePure stats.lifeBonus
        , vitalBar "Mana" "mana" stats.mana stats.maxMana stats.manaPure stats.manaBonus
        , vitalBar "Stamina" "stamina" stats.stamina stats.maxStamina stats.staminaPure stats.staminaBonus
        ]


vitalBar : String -> String -> Int -> Int -> Int -> Int -> Html Msg
vitalBar label barClass current maxVal pureVal gearBonus =
    let
        pct =
            clamp 0 100 ((max 0 current * 100) // max 1 maxVal)

        bonusSign =
            if gearBonus >= 0 then
                "+"

            else
                ""
    in
    div [ class "vital-gauge-wrapper" ]
        [ div [ class "vital-gauge-label" ]
            [ span [] [ text label ]
            , span [] [ text (String.fromInt current ++ " / " ++ String.fromInt maxVal) ]
            ]
        , div [ class "gauge-bar-outer" ]
            [ div
                [ class ("gauge-bar-inner " ++ barClass)
                , style "width" (String.fromInt pct ++ "%")
                ]
                []
            ]
        , div [ class "vital-breakdown", attribute "data-testid" ("vital-breakdown-" ++ barClass) ]
            [ text ("Pure " ++ String.fromInt pureVal ++ " + Gear " ++ bonusSign ++ String.fromInt gearBonus) ]
        ]


viewStatModeStrip : UiState -> CharacterSchemaView -> EditMode -> CharacterStatsView -> Html Msg
viewStatModeStrip ui schema activeMode stats =
    let
        statModeBtn targetMode label =
            button
                [ classList [ ( "seg-btn", True ), ( "active", activeMode == targetMode ), ( "is-active", activeMode == targetMode ) ]
                , attribute "data-testid" ("stat-mode-" ++ String.toLower label)
                , onClick (SelectStatMode targetMode)
                ]
                [ text label ]

        ruleExpectedUnspent =
            max 0 (stats.statPointsRuleCap - stats.statPointsInvested)

        isOutOfSync =
            stats.unspentStatPoints /= ruleExpectedUnspent
    in
    div [ class "res-section", attribute "data-testid" "stat-mode-panel" ]
        [ div [ class "res-header" ]
            [ span [] [ text "Stat Mode" ]
            , div [ class "segmented" ]
                [ statModeBtn Legit "Legit"
                , statModeBtn Free "Free"
                ]
            ]
        , div [ class "stat-strip" ]
            [ if activeMode == Free then
                editableFieldChip ui
                    "stat_points"
                    "Unspent Stats"
                    (String.fromInt stats.unspentStatPoints)
                    ("/ " ++ String.fromInt stats.statPointsRuleCap ++ " (Free)")
                    "input-stat-points"

              else
                plainChip "Unspent Stats" (String.fromInt stats.unspentStatPoints ++ " / " ++ String.fromInt stats.statPointsRuleCap)
            , plainChip "Invested" (String.fromInt stats.statPointsInvested ++ " / " ++ String.fromInt stats.statPointsRuleCap)
            ]
        , div [ class "stat-mode-actions" ]
            [ button
                [ class "mini-btn"
                , attribute "data-testid" "reset-stats-btn"
                , onClick ClickResetStats
                ]
                [ text "Reset Stats" ]
            , button
                [ classList [ ( "mini-btn", True ), ( "is-attention", isOutOfSync ) ]
                , attribute "data-testid" "sync-stat-points-btn"
                , title ("Set unspent stat points to rule expected (" ++ String.fromInt ruleExpectedUnspent ++ ")")
                , onClick ClickSyncStatPointsToRules
                ]
                [ text ("Sync (" ++ String.fromInt ruleExpectedUnspent ++ ")") ]
            ]
        ]


viewSkillModeStrip : UiState -> CharacterSchemaView -> EditMode -> CharacterStatsView -> Html Msg
viewSkillModeStrip _ _ activeMode _ =
    div [ class "res-section", attribute "data-testid" "skill-mode-panel" ]
        [ div [ class "res-header" ]
            [ span [] [ text "Skill Mode" ]
            , div [ class "segmented" ]
                [ modeButton activeMode Legit "Legit"
                , modeButton activeMode Free "Free"
                ]
            ]
        ]


viewResistancesSection : ResistanceCardView -> Html Msg
viewResistancesSection res =
    div [ class "res-section", attribute "data-testid" "resistances-panel" ]
        [ div [ class "res-header" ]
            [ span [] [ text "Resistances" ]
            , div [ class "segmented" ]
                [ diffButton res.difficulty Normal "Normal"
                , diffButton res.difficulty Nightmare "Nightmare"
                , diffButton res.difficulty Hell "Hell"
                ]
            ]
        , div [ class "resistance-grid res-grid-2x2" ]
            [ resCard "Fire" res.fire res.fireCap
            , resCard "Cold" res.cold res.coldCap
            , resCard "Lightning" res.light res.lightCap
            , resCard "Poison" res.poison res.poisonCap
            ]
        , div [ class "meta-line" ]
            [ text
                ("Anya +"
                    ++ String.fromInt res.anyaBonus
                    ++ "% · Diff Penalty -"
                    ++ String.fromInt res.difficultyPenalty
                    ++ "%"
                )
            ]
        ]


viewSkillTreePreview : UiState -> SessionView -> Html Msg
viewSkillTreePreview ui session =
    let
        activeTreeIndex =
            case ui.skillTreeTab of
                "tree1" ->
                    1

                "tree2" ->
                    2

                _ ->
                    0

        treeSkills =
            List.filter (\s -> s.treeIndex == activeTreeIndex) session.skills

        treeTab key label =
            button
                [ classList [ ( "skill-tab-btn", True ), ( "active", ui.skillTreeTab == key ) ]
                , attribute "data-testid" ("skill-tree-tab-" ++ key)
                , onClick (SelectSkillTreeTab key)
                ]
                [ text label ]
    in
    section [ class "glass-panel col2-skill-panel", attribute "data-testid" "skill-tree-preview" ]
        [ div [ class "panel-header" ]
            [ div [ class "panel-title" ] [ text ("Skill Tree (" ++ session.charClass ++ ")") ]
            , button
                [ class "btn-danger-ghost"
                , attribute "data-testid" "reset-skills-btn"
                , onClick ClickResetSkills
                ]
                [ text "Reset Skills" ]
            ]
        , viewSkillModeStrip ui session.schema session.skillMode session.stats
        , div [ class "meta-line" ]
            [ text
                ("Unspent Skills: "
                    ++ String.fromInt session.stats.unspentSkillPoints
                    ++ " · Active Skills: "
                    ++ String.fromInt session.stats.activeSkillCount
                    ++ " · All Skills Bonus: +"
                    ++ String.fromInt session.stats.allSkillsBonus
                )
            ]
        , div [ class "skill-subtabs" ]
            [ treeTab "tree0" "Tree I"
            , treeTab "tree1" "Tree II"
            , treeTab "tree2" "Tree III"
            ]
        , div [ class "skill-dep-legend", attribute "data-testid" "skill-dep-legend" ]
            [ span [ class "legend-item legend-prereq" ] [ span [ class "legend-dot" ] [], text "REQ 선행" ]
            , span [ class "legend-item legend-synergy" ] [ span [ class "legend-dot" ] [], text "SYN 시너지" ]
            , span [ class "legend-item legend-unlock" ] [ span [ class "legend-dot" ] [], text "NEXT 해금" ]
            ]
        , let
            hoveredSkill =
                ui.hoveredSkillId
                    |> Maybe.andThen (\hid -> List.filter (\s -> s.skillId == hid) session.skills |> List.head)
          in
          div [ class "skill-tree-workspace" ]
            [ div [ class "skill-tree-preview-grid", attribute "data-testid" "skill-tree-grid" ]
                (List.map (viewSkillNode session hoveredSkill) treeSkills)
            , viewSkillDetailPanel session hoveredSkill
            ]
        ]


viewSkillNode : SessionView -> Maybe SkillNodeView -> SkillNodeView -> Html Msg
viewSkillNode session hoveredSkill s =
    let
        maxRank =
            case session.skillMode of
                Free ->
                    session.schema.skillFreeMax

                Legit ->
                    session.schema.skillLegitMax

        prereqsMet =
            List.all
                (\pid -> List.any (\other -> other.skillId == pid && other.level >= 1) session.skills)
                s.prereqIds

        canDecrease =
            s.level > 0

        canIncrease =
            case session.skillMode of
                Free ->
                    s.level < maxRank

                Legit ->
                    s.level < maxRank
                        && session.stats.unspentSkillPoints > 0
                        && session.charLevel >= s.reqLevel
                        && prereqsMet

        isActiveHover =
            Maybe.map .skillId hoveredSkill == Just s.skillId

        isPrereq =
            Maybe.map (\h -> List.member s.skillId h.prereqIds) hoveredSkill |> Maybe.withDefault False

        isSynergy =
            Maybe.map (\h -> List.member s.skillId h.synergyIds) hoveredSkill |> Maybe.withDefault False

        isUnlock =
            Maybe.map (\h -> List.member s.skillId h.unlockIds) hoveredSkill |> Maybe.withDefault False

        depTags =
            [ if isPrereq then
                Just (span [ class "skill-dep-tag dep-prereq-tag" ] [ text "REQ" ])

              else
                Nothing
            , if isSynergy then
                Just (span [ class "skill-dep-tag dep-synergy-tag" ] [ text "SYN" ])

              else
                Nothing
            , if isUnlock then
                Just (span [ class "skill-dep-tag dep-unlock-tag" ] [ text "NEXT" ])

              else
                Nothing
            ]
                |> List.filterMap identity

        depTag =
            if List.isEmpty depTags then
                []

            else
                [ div [ class "skill-dep-tags" ] depTags ]
    in
    div
        [ classList
            [ ( "skill-node-card", True )
            , ( "is-allocated", s.level > 0 )
            , ( "active-hover", isActiveHover )
            , ( "dep-prereq", isPrereq )
            , ( "dep-unlock", isUnlock )
            , ( "dep-synergy", isSynergy )
            ]
        , style "grid-row" (String.fromInt s.row)
        , style "grid-column" (String.fromInt s.col)
        , attribute "data-testid" ("skill-node-" ++ String.fromInt s.skillId)
        , onMouseEnter (HoverSkill (Just s.skillId))
        , onMouseLeave (HoverSkill Nothing)
        ]
        (depTag
            ++ [ div [ class "skill-node-header" ]
                    [ span [ class "skill-node-name" ] [ text s.nameEn ]
                    , span [ class "skill-node-req" ] [ text ("Lv " ++ String.fromInt s.reqLevel) ]
                    ]
               , div [ class "node-split-overlay" ]
                    [ button
                        [ class "node-half-zone node-half-left"
                        , attribute "data-testid" ("skill-minus-" ++ String.fromInt s.skillId)
                        , title "-1 Skill"
                        , disabled (not canDecrease)
                        , onClick (ClickAdjustSkillRank s.skillId -1)
                        ]
                        [ span [ class "zone-glyph" ] [ text "−" ] ]
                    , button
                        [ class "node-half-zone node-half-right"
                        , attribute "data-testid" ("skill-plus-" ++ String.fromInt s.skillId)
                        , title "+1 Skill"
                        , disabled (not canIncrease)
                        , onClick (ClickAdjustSkillRank s.skillId 1)
                        ]
                        [ span [ class "zone-glyph" ] [ text "+" ] ]
                    ]
               , div
                    [ class "node-pts-track"
                    , attribute "data-testid" ("skill-rank-" ++ String.fromInt s.skillId)
                    ]
                    [ text
                        (String.fromInt s.level
                            ++ "/"
                            ++ String.fromInt maxRank
                            ++ (if s.bonusLevel > 0 && s.level > 0 then
                                    " (+" ++ String.fromInt s.bonusLevel ++ ")"

                                else
                                    ""
                               )
                        )
                    ]
               ]
        )


viewSkillDetailPanel : SessionView -> Maybe SkillNodeView -> Html Msg
viewSkillDetailPanel session hoveredSkill =
    let
        skillNameById sid =
            session.skills
                |> List.filter (\sk -> sk.skillId == sid)
                |> List.head
                |> Maybe.map .nameEn
                |> Maybe.withDefault ("Skill #" ++ String.fromInt sid)

        renderDepRow label cls ids =
            if List.isEmpty ids then
                text ""

            else
                div [ class "skill-detail-dep-group" ]
                    [ div [ class "skill-detail-dep-label" ] [ text label ]
                    , div [ class "skill-detail-dep-chips" ]
                        (List.map
                            (\sid -> span [ class ("dep-chip " ++ cls) ] [ text (skillNameById sid) ])
                            ids
                        )
                    ]
    in
    div [ class "skill-detail-panel", attribute "data-testid" "skill-detail-panel" ]
        (case hoveredSkill of
            Nothing ->
                [ div [ class "skill-detail-empty" ]
                    [ text "Hover a skill node to inspect prerequisites, synergies, and description." ]
                ]

            Just h ->
                let
                    effectiveRank =
                        if h.level > 0 then
                            h.level + h.bonusLevel

                        else
                            0
                in
                [ div [ class "skill-detail-title" ] [ text h.nameEn ]
                , if String.isEmpty h.nameKo || h.nameKo == h.nameEn then
                    text ""

                  else
                    div [ class "skill-detail-subtitle" ] [ text h.nameKo ]
                , div [ class "skill-detail-meta" ]
                    [ text
                        ("Req Lv "
                            ++ String.fromInt h.reqLevel
                            ++ " · Rank "
                            ++ String.fromInt h.level
                            ++ (if h.bonusLevel > 0 && h.level > 0 then
                                    " (+" ++ String.fromInt h.bonusLevel ++ " = " ++ String.fromInt effectiveRank ++ ")"

                                else
                                    ""
                               )
                        )
                    ]
                , if String.isEmpty h.descEn then
                    text ""

                  else
                    p [ class "skill-detail-desc" ] [ text h.descEn ]
                , if String.isEmpty h.descKo || h.descKo == h.descEn then
                    text ""

                  else
                    p [ class "skill-detail-desc skill-detail-desc-ko" ] [ text h.descKo ]
                , viewSkillDamageBox h.bonusLevel h.damage
                , renderDepRow "Prerequisites (선행)" "dep-prereq" h.prereqIds
                , renderDepRow "Synergies (시너지)" "dep-synergy" h.synergyIds
                , renderDepRow "Unlocks (해금)" "dep-unlock" h.unlockIds
                ]
        )


viewSkillDamageBox : Int -> Maybe SkillDamageView -> Html Msg
viewSkillDamageBox bonusLevel maybeDmg =
    case maybeDmg of
        Nothing ->
            text ""

        Just d ->
            let
                rangeText lo hi =
                    if lo == hi then
                        String.fromInt lo

                    else
                        String.fromInt lo ++ " – " ++ String.fromInt hi

                plusRangeText lo hi =
                    if lo == hi then
                        "+" ++ String.fromInt lo

                    else
                        "+" ++ String.fromInt lo ++ " – +" ++ String.fromInt hi
            in
            div [ class "skill-damage-box", attribute "data-testid" "skill-damage-breakdown" ]
                [ div [ class "skill-damage-header" ]
                    [ span [] [ text ("Damage · " ++ d.elemType) ]
                    , if d.previewOnly then
                        span [ class "skill-damage-preview-tag" ] [ text "Lv 1 Preview" ]

                      else
                        text ""
                    ]
                , div [ class "skill-damage-row" ]
                    [ span [ class "dmg-label" ] [ text "순수 스킬 (Pure)" ]
                    , span [ class "dmg-val" ] [ text (rangeText d.pureMin d.pureMax) ]
                    ]
                , div [ class "skill-damage-row dmg-synergy" ]
                    [ span [ class "dmg-label" ] [ text ("시너지 보정 (+" ++ String.fromInt d.synergyPct ++ "%)") ]
                    , span [ class "dmg-val" ] [ text (plusRangeText d.synergyBonusMin d.synergyBonusMax) ]
                    ]
                , div [ class "skill-damage-row dmg-gear" ]
                    [ span [ class "dmg-label" ] [ text ("장비 보정 (+" ++ String.fromInt bonusLevel ++ " Skills)") ]
                    , span [ class "dmg-val" ] [ text (plusRangeText d.gearBonusMin d.gearBonusMax) ]
                    ]
                , div [ class "skill-damage-row skill-damage-total" ]
                    [ span [ class "dmg-label" ] [ text "최종 데미지 (Total)" ]
                    , span [ class "dmg-val" ] [ text (rangeText d.totalMin d.totalMax) ]
                    ]
                ]


viewCombatLabAccordion : UiState -> SessionView -> Html Msg
viewCombatLabAccordion ui session =
    div [ class "combat-lab-accordion", attribute "data-testid" "combat-lab-accordion" ]
        [ button
            [ class "combat-lab-header"
            , attribute "data-testid" "combat-lab-toggle"
            , onClick ToggleCombatLab
            ]
            [ span [] [ text "Combat Lab (Preview)" ]
            , span []
                [ text
                    (if ui.combatLabOpen then
                        "Hide ▲"

                     else
                        "Expand ▼"
                    )
                ]
            ]
        , if ui.combatLabOpen then
            div [ class "combat-lab-content", attribute "data-testid" "combat-lab-content" ]
                [ div [ class "stat-strip", attribute "data-testid" "combat-lab-breakpoints" ]
                    [ plainChip "FCR" (String.fromInt session.stats.fcr ++ "%")
                    , plainChip "FHR" (String.fromInt session.stats.fhr ++ "%")
                    , plainChip "FRW" (String.fromInt session.stats.frw ++ "%")
                    , plainChip "IAS" (String.fromInt session.stats.ias ++ "%")
                    , plainChip "MF" (String.fromInt session.stats.magicFind ++ "%")
                    , plainChip "+Skills" ("+" ++ String.fromInt session.stats.allSkillsBonus)
                    ]
                , p []
                    [ text
                        ("Combat Sheet Projection ("
                            ++ session.charClass
                            ++ " Lv "
                            ++ String.fromInt session.charLevel
                            ++ ", "
                            ++ String.fromInt session.containerCounts.equipment
                            ++ " equipped items, EXP "
                            ++ String.fromInt session.experience
                            ++ ")."
                        )
                    ]
                ]

          else
            text ""
        ]


diffButton : DifficultyMode -> DifficultyMode -> String -> Html Msg
diffButton active target label =
    button
        [ classList [ ( "seg-btn", True ), ( "is-active", active == target ) ]
        , attribute "data-testid" ("res-diff-" ++ String.toLower label)
        , onClick (SelectResDifficulty target)
        ]
        [ text label ]


modeButton : EditMode -> EditMode -> String -> Html Msg
modeButton active target label =
    button
        [ classList [ ( "seg-btn", True ), ( "is-active", active == target ) ]
        , attribute "data-testid" ("skill-mode-" ++ String.toLower label)
        , onClick (SelectSkillMode target)
        ]
        [ text label ]


resCard : String -> Int -> Int -> Html Msg
resCard label effective maxCap =
    div
        [ classList [ ( "res-card", True ), ( "is-gold-res", effective >= 75 ) ]
        , attribute "data-testid" ("res-card-" ++ String.toLower label)
        ]
        [ span [ class "res-label" ] [ text label ]
        , span [ class "res-value" ] [ text (String.fromInt effective ++ "%") ]
        , span [ class "res-cap" ] [ text ("Max " ++ String.fromInt maxCap ++ "%") ]
        ]


onEnterOrEscape : Html.Attribute Msg
onEnterOrEscape =
    on "keydown"
        (Json.Decode.field "key" Json.Decode.string
            |> Json.Decode.andThen
                (\k ->
                    case k of
                        "Enter" ->
                            Json.Decode.succeed CommitFocusedInput

                        "Escape" ->
                            Json.Decode.succeed CancelFocusedInput

                        _ ->
                            Json.Decode.fail "ignore key"
                )
        )


draftInputEl : UiState -> String -> String -> String -> Html Msg
draftInputEl ui key serverVal tid =
    input
        [ class "stat-input"
        , type_ "text"
        , value (State.UiState.focusedDraftValue ui key serverVal)
        , attribute "data-testid" tid
        , onFocus (FocusField key serverVal)
        , onInput EditFocusedInput
        , onBlur CommitFocusedInput
        , onEnterOrEscape
        ]
        []


editableFieldChip : UiState -> String -> String -> String -> String -> String -> Html Msg
editableFieldChip ui key label serverVal hint tid =
    div [ class "stat-chip" ]
        [ span [ class "stat-label" ] [ text label ]
        , draftInputEl ui key serverVal tid
        , span [ class "stat-sub" ] [ text hint ]
        ]


editableStepperFieldChip : UiState -> String -> String -> String -> String -> String -> Html Msg
editableStepperFieldChip ui key label serverVal hint tid =
    div [ class "stat-chip stat-chip-stepper" ]
        [ div [ class "stepper-input-row" ]
            [ span [ class "stat-label" ] [ text label ]
            , draftInputEl ui key serverVal tid
            , span [ class "stat-sub" ] [ text hint ]
            ]
        , stepButtonRow key
        ]


editableStatChip : UiState -> EditMode -> String -> String -> Int -> Int -> Int -> CharacterSchemaView -> String -> Html Msg
editableStatChip ui statMode key label baseVal bonusVal classBase schema tid =
    let
        minHint =
            if statMode == Legit then
                min classBase baseVal

            else
                schema.statMin
    in
    div [ class "stat-chip stat-chip-stepper" ]
        [ div [ class "stepper-input-row" ]
            [ span [ class "stat-label" ] [ text label ]
            , draftInputEl ui key (String.fromInt baseVal) tid
            , span [ class "stat-sub" ]
                [ text
                    ("+"
                        ++ String.fromInt bonusVal
                        ++ " ("
                        ++ String.fromInt minHint
                        ++ ".."
                        ++ String.fromInt schema.statMax
                        ++ ")"
                    )
                ]
            ]
        , stepButtonRow key
        ]


stepButtonRow : String -> Html Msg
stepButtonRow key =
    div [ class "stepper-btn-row" ]
        (List.map
            (\( delta, lbl ) ->
                button
                    [ class "mini-btn"
                    , attribute "data-testid" ("step-" ++ key ++ "-" ++ String.fromInt delta)
                    , onClick (StepCharacterField key delta)
                    ]
                    [ text lbl ]
            )
            [ ( -1, "-1" ), ( 1, "+1" ), ( -10, "-10" ), ( 10, "+10" ) ]
        )


plainChip : String -> String -> Html Msg
plainChip label val =
    div [ class "stat-chip" ]
        [ span [ class "stat-label" ] [ text label ]
        , span [ class "stat-val" ] [ text val ]
        ]
