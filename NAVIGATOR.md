This is the public bootstrap index for AI agents to locate implementation files, verification tools, and the private Strategy Hub entrypoints resolved from `D2R_SPEC_PATH`. **Read this first if you are lost.**

## 🌐 Tripartite Navigation Map

| Repository | Role | Navigator |
| :--- | :--- | :--- |
| **`d2r-core`** | **Implementation** (Public Logic) | [NAVIGATOR.md](./NAVIGATOR.md) |
| **`d2r-data`** | **Game Data** (Extracted Tables) | [NAVIGATOR.md](./d2r-data/NAVIGATOR.md) |
| **`d2r-spec`** | **Specification** (Private Overlay) | [NAVIGATOR.md](../d2r-spec/NAVIGATOR.md) |

## 0. Navigation Precedence (Read Order)
1. `AGENTS.md` (global safety + operational constraints)
2. `NAVIGATOR.md` (this file: public bootstrap map)
3. Model entrypoint (`GEMINI.md` or `CLAUDE.md`) — when Strategy Hub is present, prefer its spec-side counterpart:
   - **`../d2r-spec/GEMINI.md`** (Gemini-class primary authority)
   - **`../d2r-spec/CLAUDE.md`** (Claude-class primary authority)
4. Private overlay when available:
   - `../d2r-spec/NAVIGATOR.md`
   - `../d2r-spec/AGENTS.md`
   - `../d2r-spec/AI_WORKFLOW.md`
   - active `../d2r-spec/.agents/tasks/*.md`

If rules conflict, keep `AGENTS.md` safety constraints (`No Automatic Push`, data boundary, anti-loop, verification-first) as non-negotiable.

## ⚡ Quick Access: Active Components

| Component | Implementation | Specification / Context | Verification |
| :--- | :--- | :--- | :--- |
| **Parsing Engine** | `src/item.rs`, `src/save.rs` | `discussion/0016`, `0023` | `d2item_inspect` |
| **Validation Engine** | `src/engine/validation.rs` | `discussion/0034` | `item_validation_test` |
| **Interpretation** | `src/engine/formatter.rs` | `discussion/0033`, `0034` | `option_rendering_test` |
| **Inventory Grid** | `src/inventory.rs` | `discussion/0021` | `d2save_inventory_check`|
| **Game Data Gateway**| `src/data/mod.rs` | `discussion/0035`, `0104` | `cargo check`, `forge --verify` |
| **Quest/Waypoint (v105)** | `src/save.rs`, `d2r-data/quests.rs`, `d2r-data/waypoints.rs` | `discussion/0068`, `0069` | `d2item_chunk_verify`, `v105_unlocker` |

## 1. Core Domains & File Map

| Domain | Specification (Truth) | Primary Implementation | Verification Tool |
| :--- | :--- | :--- | :--- |
| **Bitstream / Save Header** | `../d2r-spec/NAVIGATOR.md` -> Bitstream / Save Header domain (private overlay, if present) | `src/save.rs` | `src/bin/verify/d2save_map.rs`, `src/bin/verify/d2save_verify.rs` |
| **Item parsing / Decrypt** | `../d2r-spec/NAVIGATOR.md` -> Item parsing / Decrypt domain (private overlay, if present) | `src/item.rs` | `src/bin/verify/d2item_inspect.rs` |
| **Inventory / Grid** | `../d2r-spec/NAVIGATOR.md` -> Inventory / Grid domain (private overlay, if present) | `src/inventory.rs` | `src/bin/verify/d2save_inventory_check.rs` |
| **Status (Attrs/Skills)** | `../d2r-spec/NAVIGATOR.md` -> Status (Attrs/Skills) domain (private overlay, if present) | `src/save.rs` | `src/bin/d2save_status_inspect.rs` |
| **Save Verification** | `../d2r-spec/NAVIGATOR.md` -> Save Verification domain (private overlay, if present) | - | `src/bin/verify/d2save_verify.rs` |
| **UI / Orchestration** | `../d2r-spec/NAVIGATOR.md` -> UI / Orchestration domain (private overlay, if present) | `src/main.rs` | Elm-rs generated types |
| **Game Data Access / Copyright Boundary** | `../d2r-spec/discussion/0035`, `0104` | `src/data/mod.rs`, `d2r-data/` | [d2r-data/NAVIGATOR.md](./d2r-data/NAVIGATOR.md) |
| **Item Validation**      | `../d2r-spec/discussion/0034-item-option-interpretation.md`, `0079` | `src/engine/validation.rs` | `tests/item_validation_test.rs` |
| **Environment / Paths** | `../d2r-spec/discussion/0036-environment-path-normalization.md` | `.env` | `tests/common.rs` |
| **Workflow / Rules**   | `AGENTS.md` (public bootstrap), `../d2r-spec/AGENTS.md`, `../d2r-spec/AI_WORKFLOW.md` (private overlay) | `../d2r-spec/.agents/tasks/` preferred, `./.agents/tasks/` public-safe fallback | - |

## 2. Recent Architectural Decisions (Must Know)
- **Rust + Elm**: Core logic is in Rust; UI is in Elm.
- **Type Safety**: Use `elm-rs` for 1:1 type mapping. No manual JSON types.
- **Verification-First**: Never consider a code change "done" until verified with a tool in `src/bin/verify/`.
- **D2R/DLC Aware**: We prioritize D2R/DLC support over classic LoD logic.
- **External Data Boundary**: Extracted tables are maintained in `d2r-data/` (root link to sibling repo). In `d2r-core`, only `src/data/mod.rs` should bridge into that data.
- **Environment First**: All paths MUST be retrieved via `tests/common.rs` or environment variables (Source of Truth: `.env`). Do not hardcode relative paths like `../../d2r-data`.
- **PowerShell Minimization & Rust-First for Data**: For persistent data processing or high-volume extraction (`0079`), Rust-based integrated tools (`d2data-forge`) MUST be prioritized over PowerShell loops to ensure path integrity (`0087`) and token efficiency.


## 3. Git-Aware Context Recovery
When you need to know **why** a specific byte offset or bit width was chosen:
- `git log -p <file>`: Understand the evolution of complex bit-parsing logic.
- `git blame <file>`: Find the specific commit/task that introduced a change.
- `git log --grep="00XX"`: Find all code changes related to a specific specification.
- **Tip**: Our commit messages are in English for core logic, and Korean for specific d2r-spec discussions.

## 4. Important Paths
- **Specs/Discussions**: `../d2r-spec/discussion/` (Private design context and internal reasoning, if the local overlay exists)
- **Private Navigator**: `../d2r-spec/NAVIGATOR.md` (Private domain-to-research map)
- **Private Overlay**: `../d2r-spec/AGENTS.md` (Private extension for internal reasoning and workflow)
- **Private Workflow**: `../d2r-spec/AI_WORKFLOW.md` (Private operational workflow, if present)
- **Agent Tasks**: `../d2r-spec/.agents/tasks/` preferred, `./.agents/tasks/` only as a sanitized public-safe fallback
- **Fixtures**: `./tests/fixtures/savegames/` (Reference binary files)
- **Verification Tools**: `./src/bin/verify/` (Standalone CLI tools for testing)
- **Data Gateway (Core)**: `./src/data/mod.rs` (thin `#[path]` gateway into external data repo)
- **External Game Data Repo**: `./d2r-data/` (symlink to `../d2r-data`; extracted tables stay outside `d2r-core` history)

## 5. How to Research (Agentic Loop)
1.  **Check `NAVIGATOR.md`**: Find the logical domain and matching implementation file.
2.  **Locate Specification**: If `../d2r-spec/NAVIGATOR.md` exists, find the matching internal document for that domain and continue into the private overlay.
3.  **Check Private Task Specs**: Prefer `../d2r-spec/.agents/tasks/` for active execution plans; use `./.agents/tasks/` only for sanitized public-safe fallbacks.
4.  **Read Spec**: Always read the corresponding domain specification before coding.
5.  **Git Research**: Use `git log -p` if the "why" behind existing code is unclear.
6.  **Locate Patterns**: Search `src/` for similar implementation patterns.
7.  **Verify**: Identify and run the matching `src/bin/verify/` tool.
8.  **Escalate Correctly**: If the task is `3+ files` or deep logic, refresh the task spec and route to a stronger model.
9. **Check Data Boundary**: If requested changes involve extracted game tables/assets, route that scope to `d2r-data` planning and prioritize building/using integrated Rust tools (`d2data-forge`) over manual PowerShell loops.


## 6. Verification Tool Catalog

This catalog includes **public verification tools** residing in the `d2r-core` repository. For data extraction or internal forensic tools, consult the respective navigators.

| Tool Name | Scope | Description & Primary Usage |
| :--- | :--- | :--- |
| **`d2save_verify`** | Save | **Standard Auditor**: Validates checksum, JM markers, and serialization fidelity. Supports `--diff-baseline` for regression gating. |
| **`d2save_batch`**  | Save | **Batch Runner**: Orchestrates `d2save_verify` and `SymmetryBitDiff` at scale with categorization. |
| **`d2save_map`**    | Save | Dumps the memory map of a `.d2s` file (JM offsets, item counts). |
| **`d2save_gap`**    | Save | **GAP Core**: Brute-forces header lengths and calculates alignment scores. |
| **`SymmetryBitDiff`**| Item | **Roundtrip Master**: Compares bit-by-bit symmetry. Use `--roundtrip` for instant verification. |
| **`d2save_diff`**   | Save | Byte-level diff between two saves (header vs item section). |
| **`d2save_bit_xor_diff`**| Save | **Semantic Shift Detector**: Bit-level XOR diff with section context. |
| **`d2save_item_diff`**| Save | **Crucial**: Compares only the item bitstream, masking header noise. |
| **`d2item_inspect`** | Item | Decomposes a `.d2i` or `.d2s` item into its bit-fields and props. |
| **`d2item_extract`** | Item | Extracts a raw item bit-payload from a save into a `.d2i` file. |
| **`d2save_inject`** | Item | Injects a raw `.d2i` item into a specific save file. |
| **`d2save_status_inspect`**| Status | Dumps character attributes and skills (v105/v1 compatible). |
| **`dump_character`** | Save | **Comprehensive**: Dumps full character status, skills, and item map. |
| **`d2save_inventory_check`**| Logic | Verifies inventory grid integrity (no overlaps, valid coordinates). |
| **`v105_unlocker`** | Save | CLI example for unlocking progression bits in Alpha v105. |
| **`d2save_progression_audit`** | Save | **Progression Master**: Semantic state audit and mutation exporter (Quest/WP). |
| **`d2save_poison`** | Save | **Forensic XOR**: Flips a single bit at a given offset (LSB-first) for corruption testing. |
| **`d2save_arch`**   | Save | **Bit Archaeologist**: Unified bit-level `dump` and `diff` for deep forensic exploration. |
| **`d2item_serialization_audit`** | Item | **Fidelity Master**: Visual bitstream alignment and forensic audit report. |
| **`d2item_global_audit`** | Item | **Batch Auditor**: Scans directories for serialization parity mismatches. |
| **`d2save_taxonomy`** | Save | **Structural Auditor**: Categorizes fixtures by version and bitstream shape. |
| **`d2item_rhythm_oracle`** | Item | **Rhythm Oracle**: Validates 9-bit property stream rhythms and terminators. |
| **`d2item_alignment_oracle`** | Item | **Alignment Oracle**: Detects bit-drift and calculates required offsets. |
| **`d2item_registry_recommender`** | Forensic | **Triage Oracle**: Suggests bit-width updates from audit logs. |
| **`d2item_registry_patcher`** | Forensic | **Auto-Patcher**: Applies recommendations to the registry atomically. |

### 🔍 Related Toolsets
- **Game Data & Extraction**: See [`d2r-data/NAVIGATOR.md`](./d2r-data/NAVIGATOR.md) for `d2r-data-extractor` and `probe`.
- **Forensic Research**: See [`d2r-spec/NAVIGATOR.md`](../d2r-spec/NAVIGATOR.md) for `d2item_oracle_mapper`, `v5_peek`, and `chunk_verify`.
- **Tooling Proposals**: See [`d2r-spec/discussion/0173-efficiency-tooling-proposals.md`](../d2r-spec/discussion/0173-efficiency-tooling-proposals.md) for future productivity roadmap.

### 🚀 Common Verification Commands

```powershell
# 1. Verify save checksum/magic
cargo run --bin d2save_verify -- path/to/save.d2s

# 2. Compare item data only
cargo run --bin d2save_item_diff -- actual.d2s expected.d2s

# 3. Inspect character status (attrs/skills)
cargo run --bin d2save_status_inspect -- path/to/save.d2s

# 4. Verify against established baseline (Regression Gate)
cargo run --bin d2save_verify -- <fixture_path> --diff-baseline <baseline_json_path>
```

## 7. Navigation Routing Gate (Fast Classification)
Consult the [`navigation-classification` skill](../d2r-spec/.agents/skills/navigation-classification/SKILL.md) for rules on organizing code and tools across the tripartite structure.
Before implementing, classify the request:
- `Core-only`: implementation/verifier updates in `d2r-core` only.
- `Data-only`: table/extraction updates in `d2r-data` only.
- `Cross-boundary`: split into separate core/data slices and document file boundaries in task specs before coding.

## 8. Canonical Directive Filenames
Use these exact filenames for navigation and policy lookup:
- `AGENTS.md`
- `gemini.md` (`GEMINI.md` is a case-only alias)
- `CLAUDE.md` (`cloude.md` is a typo alias; normalize to `CLAUDE.md`)
- `CONSTITUTION.md`

## 9. Navigator Update Integrity Protocol
When editing this navigator:
1. Run conflict check against `AGENTS.md` safety constraints.
2. Apply smallest possible patch for contradictions.
3. Append clarifications if no contradiction exists.
4. Keep paths relative from repository root.

## 10. Strategy Hub Model Entrypoints (Canonical)
When the local Strategy Hub at `../d2r-spec/` is present, use the following spec-side files as the **primary authority** for model-specific operating rules:
- **`../d2r-spec/GEMINI.md`** — Gemini-class research and planning model entrypoint
- **`../d2r-spec/CLAUDE.md`** — Claude-class bounded implementation and review model entrypoint
- **`../d2r-spec/CONSTITUTION.md`** — Strategy Hub private constitutional companion

The core-side files (`GEMINI.md`, `CLAUDE.md`, `CONSTITUTION.md`) are public bootstrap entrypoints that redirect to these spec-side files when the Hub is available.
