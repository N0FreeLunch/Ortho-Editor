This is the public bootstrap index for AI agents to locate implementation files and verification tools. **Read this first if you are lost.**

## 🌐 Tripartite Navigation Map

| Repository | Role | Navigator |
| :--- | :--- | :--- |
| **`d2r-core`** | **Implementation** (Public Logic) | [NAVIGATOR.md](./NAVIGATOR.md) |
| **`d2r-data`** | **Game Data** (Extracted Tables) | [NAVIGATOR.md](./d2r-data/NAVIGATOR.md) |
| **`d2r-spec`** | **Specification** (Private Overlay, if present) | [NAVIGATOR.md](../d2r-spec/NAVIGATOR.md) |

> [!NOTE]
> This navigator intentionally lists only what `d2r-core` owns. Specification, research, and task details are maintained in the private overlay; this file does not duplicate them. When you need the reasoning behind a domain, open `../d2r-spec/NAVIGATOR.md` (if present) and look up the **same domain name** in its *Domain-to-Private-Artifact Map* section.

## 0. Navigation Precedence (Read Order)
1. `AGENTS.md` (global safety + operational constraints)
2. `NAVIGATOR.md` (this file: public bootstrap map)
3. Model entrypoint (`GEMINI.md` or `CLAUDE.md`). When the private overlay is present, prefer its counterpart: `../d2r-spec/GEMINI.md` or `../d2r-spec/CLAUDE.md`.
4. Private overlay entrypoints when available: `../d2r-spec/NAVIGATOR.md`, `../d2r-spec/AGENTS.md`.

If rules conflict, keep `AGENTS.md` safety constraints (`No Automatic Push`, data boundary, anti-loop, verification-first) as non-negotiable.

## 1. Core Domains & File Map

Domain names match the private overlay's map, so you can look them up by name there.

| Domain | Primary Implementation | Verification |
| :--- | :--- | :--- |
| **Bitstream / Save Header** | `src/save.rs` | `d2save_map`, `d2save_verify` |
| **Item parsing / Decrypt** | `src/item.rs`, `src/domain/stats/` | `d2item_inspect`, `d2stat_fidelity_radar`, `d2stat_probe_oracle`, `d2stat_tiling_prober`, `d2save_equipped_invariant` |
| **Inventory / Grid** | `src/inventory.rs` | `d2save_inventory_check` |
| **Status (Attrs/Skills)** | `src/save.rs` | `d2save_status_inspect` |
| **Option Logic / Display** | `src/engine/validation.rs`, `src/engine/formatter.rs` | `tests/item_validation_test.rs`, `tests/option_rendering_test.rs` |
| **Quest / Waypoint (v105)** | `src/save.rs` | `d2save_progression_audit` |
| **Save Verification** | - | `d2save_verify`, `d2save_equipped_invariant` |
| **Game Data / Copyright Boundary** | `src/data/mod.rs` (gateway only) | `cargo check`; see [d2r-data/NAVIGATOR.md](./d2r-data/NAVIGATOR.md) |
| **Environment / Path Normalization** | `.env` | `tests/common.rs` |
| **UI / Orchestration** | `src/main.rs` | Elm-rs generated types |

## 2. Core Principles (Must Know)
- **Rust + Elm**: Core logic is in Rust; UI is in Elm.
- **Type Safety**: Use `elm-rs` for 1:1 type mapping. No manual JSON types.
- **Verification-First**: Never consider a code change "done" until verified with a tool in `src/bin/verify/`.
- **D2R/DLC Aware**: We prioritize D2R/DLC support over classic LoD logic.
- **External Data Boundary**: Extracted tables are maintained in the sibling `d2r-data` repo. In `d2r-core`, only `src/data/mod.rs` should bridge into that data.
- **Environment First**: All paths MUST be retrieved via `tests/common.rs` or environment variables (Source of Truth: `.env`). Do not hardcode relative paths to sibling repositories.
- **Rust-First for Data Processing**: For persistent or high-volume data processing, prefer integrated Rust tools over ad-hoc PowerShell loops to keep path integrity and token efficiency.

## 3. Git-Aware Context Recovery
When you need to know **why** a specific byte offset or bit width was chosen:
- `git log -p <file>`: Understand the evolution of complex bit-parsing logic.
- `git blame <file>`: Find the specific commit that introduced a change.
- `git log --grep="<keyword>"`: Find code changes related to a topic.

## 4. Important Paths
- **Fixtures**: `./tests/fixtures/savegames/` (Reference binary files)
- **Verification Tools**: `./src/bin/verify/` (Standalone CLI tools for testing)
- **Data Gateway**: `./src/data/mod.rs` (thin `#[path]` gateway into the external data repo)
- **External Game Data Repo**: `./d2r-data/` (symlink to `../d2r-data`; extracted tables stay outside `d2r-core` history)
- **Private overlay (reference only)**: `../d2r-spec/NAVIGATOR.md`

## 5. How to Research (Agentic Loop)
1. **Check `NAVIGATOR.md`**: Find the logical domain and matching implementation file.
2. **Locate Specification**: If `../d2r-spec/NAVIGATOR.md` exists, look up the same domain name there and continue from its pointers.
3. **Read Spec**: Read the corresponding domain specification before coding.
4. **Git Research**: Use `git log -p` if the "why" behind existing code is unclear.
5. **Locate Patterns**: Search `src/` for similar implementation patterns.
6. **Verify**: Identify and run the matching `src/bin/verify/` tool.
7. **Check Data Boundary**: If a change involves extracted game tables/assets, route that scope to `d2r-data` instead of editing it from `d2r-core`.

## 6. Verification Tool Catalog

This catalog lists the **public verification tools** residing in `d2r-core` (all are core-owned, so they are documented here). Tools live in `src/bin/verify/`, `src/bin/`, and `examples/`; run any tool with `--help`.

| Tool Name | Scope | Description & Primary Usage |
| :--- | :--- | :--- |
| **`d2save_verify`** | Save | **Standard Auditor**: Validates checksum, JM markers, and serialization fidelity. Supports `--diff-baseline` for regression gating and `--dump-residue-map` for residue/gap mapping (with `--json`). |
| **`d2save_batch`** | Save | **Batch Runner**: Orchestrates `d2save_verify` and `SymmetryBitDiff` at scale with categorization. |
| **`d2save_map`** | Save | Dumps the memory map of a `.d2s` file (JM offsets, item counts). |
| **`d2save_gap`** | Save | **GAP Core**: Brute-forces header lengths and calculates alignment scores. |
| **`SymmetryBitDiff`** | Item | **Roundtrip Master**: Compares bit-by-bit symmetry. Use `--roundtrip` for instant verification. |
| **`d2save_diff`** | Save | Byte-level diff between two saves (header vs item section). |
| **`d2save_bit_xor_diff`** | Save | **Semantic Shift Detector**: Bit-level XOR diff with section context. |
| **`d2save_item_diff`** | Save | Compares only the item bitstream, masking header noise. |
| **`d2save_structural_diff`** | Save | **Structural Diff**: Compares structural metadata between two saves to detect layout shifts. |
| **`d2save_equipped_invariant`** | Save | Verifies equipped-item identification invariants for character and mercenary. |
| **`d2save_mercenary_audit`** | Save | Audits mercenary data (experience, name, class, subtype) and its consistency with the related save section. |
| **`d2save_json_dump`** | Save | Dumps save structure (items, skills, etc.) to JSON for machine comparison. |
| **`d2save_taxonomy`** | Save | **Structural Auditor**: Categorizes fixtures by version and bitstream shape. |
| **`d2save_arch`** | Save | **Bit Archaeologist**: Unified bit-level `dump` and `diff` for deep forensic exploration. |
| **`d2save_section_bits`** | Save | **Section Analyzer**: Dumps the bit layout and offsets for each save section (Header, Attrs, Skills, Items). |
| **`d2save_progression_audit`** | Save | **Progression Master**: Semantic state audit and mutation exporter (Quest/WP). |
| **`d2save_poison`** | Save | **Forensic XOR**: Flips a single bit at a given offset (LSB-first) for corruption testing. |
| **`d2save_inject`** | Item | Injects a raw `.d2i` item into a specific save file. |
| **`d2save_inventory_check`** | Logic | Verifies inventory grid integrity (no overlaps, valid coordinates). |
| **`d2save_status_inspect`** | Status | Dumps character attributes and skills (v105/v1 compatible). |
| **`d2save_status_diff`** | Status | **Status Diff**: Compares character attributes and skills between two save files. |
| **`dump_character`** | Save | **Comprehensive**: Dumps full character status, skills, and item map. |
| **`v105_unlocker`** | Save | CLI example (`examples/`) for unlocking progression bits in Alpha v105. |
| **`v105_attribute_search`** | Status | **Attribute Search**: Scans Alpha v105 saves for specific attribute bit-patterns. |
| **`v105_lab`** | Save | **v105 Experimental**: Sandbox tool for Alpha v105 progression and bit-archaeology experiments. |
| **`d2item_inspect`** | Item | Decomposes a `.d2i` or `.d2s` item into its bit-fields and props. |
| **`d2item_grep`** | Item | Searches raw bit patterns (0/1) in save or item files. |
| **`d2item_bit_visualizer`** | Item | **Semantic Visualizer**: Hierarchical bitstream mapping with desync context. |
| **`d2item_xray`** | Item | **Bit X-Ray**: Detailed bit-level visualization and decomposition of item property streams. |
| **`d2item_bit_dump`** | Item | **Bitstream Dumper**: Dumps raw bits from an absolute offset with grouping and JSON support. |
| **`d2item_extract`** | Item | Extracts a raw item bit-payload from a save into a `.d2i` file. |
| **`d2item_chunk_verify`** | Item | Verifies extracted item chunks. |
| **`d2item_bve`** | Item | Bit-level verification utility for item payloads. |
| **`d2item_section_prefix_trace`** | Item | **Prefix Parity Auditor**: Compares local item parity and cumulative section-prefix traces, including isolated provenance inspect comparisons. |
| **`d2item_serialization_audit`** | Item | **Fidelity Master**: Visual bitstream alignment and forensic audit report. |
| **`d2item_global_audit`** | Item | **Batch Auditor**: Scans directories for serialization parity mismatches. `analyze-report <report.json> --json --token-efficient` summarizes a saved report without parsing saves; detects inconsistent counts and groups timeout stages. Analysis exit 0 does not mean the saved audit passed. |
| **`d2item_opaque_census`** | Item | Counts and classifies opaque (unresolved) item regions across fixtures. |
| **`d2item_rhythm_oracle`** | Item | **Rhythm Oracle**: Validates 9-bit property stream rhythms and terminators. |
| **`d2item_alignment_oracle`** | Item | **Alignment Oracle**: Detects bit-drift and calculates required offsets. |
| **`d2stat_fidelity_radar`** | Item | **Fidelity Radar**: Fast parallel multi-fixture bit replay and property completeness scanner with atomic JSON receipts. |
| **`d2stat_template_oracle`** | Item | **Deterministic Option Oracle**: Cross-references parsed item properties against static Unique/Set/Runeword templates. |
| **`d2stat_tiling_prober`** | Item | **Deterministic CSP Prober**: Solves the unique reachability path to the `0x1FF` terminator via constraint backtracking. |
| **`semantic-bit-diff`** | Item | **Semantic Diff**: Read-only forensic CLI to find bit divergence anchored at JM markers. Outputs JSON. (Source: `src/bin/verify/semantic_bit_diff.rs`; run `cargo run --bin semantic-bit-diff -- --base <path> --mutated <path> --json`) |
| **`d2item_registry_recommender`** | Forensic | **Triage Oracle**: Suggests bit-width updates from audit logs. |
| **`d2item_registry_patcher`** | Forensic | **Auto-Patcher**: Applies recommendations to the registry atomically. |

### 🔍 Related Toolsets
- **Game Data & Extraction**: See [`d2r-data/NAVIGATOR.md`](./d2r-data/NAVIGATOR.md).
- **Forensic research and tooling background**: See [`d2r-spec/NAVIGATOR.md`](../d2r-spec/NAVIGATOR.md) (private overlay, if present).

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
Before implementing, classify the request:
- `Core-only`: implementation/verifier updates in `d2r-core` only.
- `Data-only`: table/extraction updates in `d2r-data` only.
- `Cross-boundary`: split into separate core/data slices and document file boundaries before coding.

## 8. Directive Filenames and Reference Links
Canonical filenames: `AGENTS.md`, `GEMINI.md`, `CLAUDE.md`, `CONSTITUTION.md`.

When the private overlay at `../d2r-spec/` is present, its `GEMINI.md`, `CLAUDE.md`, and `CONSTITUTION.md` are the primary authority for model-specific operating rules. The core-side files are public bootstrap entrypoints that redirect there.

## 9. Navigator Update Integrity Protocol
When editing this navigator:
1. Run conflict check against `AGENTS.md` safety constraints.
2. Apply smallest possible patch for contradictions.
3. Append clarifications if no contradiction exists.
4. Keep paths relative from repository root.
5. Keep this file to what `d2r-core` owns. Do not add `d2r-spec` or `d2r-data` internal details (document IDs, task/skill paths, extraction tooling). Add only a reference pointer to the other navigator and, if needed, a domain name to look up there.
