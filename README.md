# d2r-core

A bit-level save file engine for Diablo II: Resurrected. It parses, validates, and rewrites `.d2s` saves for the modern DLC track and Alpha v105 while preserving data it cannot yet interpret.

[Overview](#overview) · [Capabilities](#capabilities) · [Methodology](#methodology) · [Tooling](#tooling) · [Transparency & Data Boundary](#transparency--data-boundary) · [License](#license)

## Overview

Diablo II saves are variable-width bitstreams with no field delimiters. `d2r-core` reconstructs them into typed domain models (character, skills, progression, items) and writes them back with integrity checks, so that parsing results can be inspected, verified, and edited safely.

## Compatibility & Status

- **Modern/DLC Track**: Includes support for the current DLC-oriented spec track used by this repository.
- **Retro-Archaeology**: Includes Alpha v105 (`0x69` / 105) save parsing and forensic bitstream reconstruction.
- **State**: Core parsing and verification tooling cover both the modern/DLC track and Alpha v105. Data-integrated formatting and presentation are still evolving.

## Capabilities

> The cards below are illustrative renderings of the engine's parsed output. Exact formatting depends on the front end consuming the library.

### Character Status Editing

Modify core character attributes and skills with full integrity validation.

```text
┌────────────────────────────────────────────────────────────────────────┐
│  Character: TESTDRUID (Druid) · Level 36 · Experience: 11,297,637      │
│  Gold: 141,476 (Inventory) / 611,974 (Stash) · Stat Points Left: 100   │
├────────────────────────────────────────────────────────────────────────┤
│  [Attributes & Vitals]                                                 │
│  Strength: 60       Dexterity: 20       Vitality: 60      Energy: 20   │
│  Life: 220 / 197    Mana: 49 / 90       Stamina: 179 / 154             │
│                                                                        │
│  [Skill Allocation]                                                    │
│  11 skills allocated across the class skill trees (level per skill)    │
└────────────────────────────────────────────────────────────────────────┘
```

- **Level & Experience**: Patch character level (1-99) and synchronize header/stat values.
- **Attributes**: Surgical modification of Strength, Dexterity, Vitality, Energy, and Stat points.
- **Skills**: Edit skill point distribution for all character classes.
- **Integrity**: Automatically recalculates checksums and file sizes after any status change.

### Inventory & Item Editing

A comprehensive suite for managing items and storage.

```text
┌────────────────────────────────────────────────────────────────────────┐
│  Cuirass [xrs] · Runeword · Sockets 3/3                                │
│  Grid: (5, 0) · Size: 2×3                                              │
├────────────────────────────────────────────────────────────────────────┤
│  [Formatted Properties]                                                │
│  +2 to Class Skills                                                    │
│  +20% Faster Hit Recovery                                              │
│  Fire Resist +30%                                                      │
│  Requirements -15%                                                     │
│  +38% Enhanced Defense                                                 │
│                                                                        │
│  [Socketed Children]                                                   │
│  ├─ Socket 0: Hel Rune                                                 │
│  ├─ Socket 1: Shael Rune                                               │
│  └─ Socket 2: Ral Rune                                                 │
├────────────────────────────────────────────────────────────────────────┤
│  [Bitstream & Property Forensics]                                      │
│   ID   Stat Name              Param    Raw   Val   Bit Range           │
│   99   item_fastergethitrate      0     20    20   traceable           │
│   39   fireresist                 0     30    30   traceable           │
│   91   item_req_percent           0    -15   -15   traceable           │
│   16   item_armor_percent         0     38    38   traceable           │
└────────────────────────────────────────────────────────────────────────┘
```

- **Inventory Mapping**: Classify and map items across Inventory, Stash (Personal/Shared), Horadric Cube, and Equipment.
- **Item Modification**: Modify item flags (Ethereal, Socketed, Personalize), IDs, and raw bit-properties.
- **Socketing Engine**: Manage socketed items and their children with recursive bitstream reconstruction.
- **Huffman Engine**: Full support for original Huffman compression of item codes.

### Progression & World Editing

Unlock world features and progression markers.

```text
┌────────────────────────────────────────────────────────────────────────┐
│  [World Progression]                  Quests ✓ done · ○ open           │
├────────────────────────────────────────────────────────────────────────┤
│  Normal      Act I ✓   Act II ✓   Act III ✓   Act IV ✓   Act V ✓       │
│  Nightmare   Act I ○   Act II ○   Act III ○   Act IV ○   Act V ○       │
│  Hell        Act I ○   Act II ○   Act III ○   Act IV ○   Act V ○       │
│                                                                        │
│  [Waypoints]   per act · per difficulty (active / inactive)            │
└────────────────────────────────────────────────────────────────────────┘
```

- **Waypoints**: Activate or deactivate waypoints across all acts and difficulties (Normal, NM, Hell).
- **Quests**: Modify quest completion flags and semantic triggers (e.g., unlocking the Act 3 Durance gate).
- **NPC Status**: Track and modify NPC interaction flags and world state markers.

### Core Parsing & Reconstruction

Standalone features that do not require external game data assets.

- **Save Integrity**: Validate magic numbers, calculate CRC32 checksums, and finalize binary layouts.
- **Bitstream Engine**: Advanced bit-level reading/writing for variable-width Diablo II bitstreams.
- **Section Splicing**: Dynamically rebuild save sections (`gf`, `if`, `JM`) while preserving unknown data anchors.

### Data-Integrated Features (Requires `d2r-data`)

These features use the sibling `d2r-data` repository to provide semantic context.

- **Human-Readable Names**: Resolve internal codes and affix IDs to localized strings.
- **Advanced Stat Resolution**: Interpret complex item properties using game-specific cost tables.
- **Legitimacy Verification**: Validate item base stats, runeword eligibility, and level requirements.

## Methodology

Where the format is undocumented, parsing rules are established through reverse-engineering practice and then enforced by verification, not assumed.

1. **Constraint-based tiling**: Property lists are decoded as a search over candidate field widths that must land exactly on the list terminator, rather than by guessing lengths.
2. **Sequence alignment**: Mutated and reference item bitstreams are aligned to locate desynchronization points and infer unknown field widths.
3. **Domain invariants**: Results only count as valid when they satisfy in-game rules (for example, equipped items must be identified), which filters out structurally plausible but impossible parses.
4. **Non-destructive isolation**: Bits that cannot be interpreted are preserved in opaque segments with a recorded reason, keeping round-trips bit-exact instead of discarding data.

## Tooling

Verification and forensic CLI binaries are indexed in [NAVIGATOR.md](NAVIGATOR.md).

## Transparency & Data Boundary

This project enforces a strict **Data Boundary** between the parsing engine's structural logic and the game's proprietary content.

1.  **Structural Disclosure (The Schema)**: We expose the **Rust structs and parsing logic** to prove the engine's integrity and ensure "No Malice." By revealing the *format* of the data, we provide full transparency into how the editor operates without distributing the *content*.
2.  **No Obligation for Data Content**: This repository provides the "vessel" (the code), but is under no obligation to disclose, and does not provide, the actual internal game data or extracted tables. Actual game values are treated as external dependencies to be handled privately by the user.

This approach ensures that while the technical mechanics are open for research and verification, the proprietary assets of the game creators are strictly respected and excluded from the public domain.

## License

This project is licensed under the **PolyForm Noncommercial License 1.0.0**. This means the software is free for personal and noncommercial use, but commercial use is strictly prohibited.

**Rationale**: This noncommercial restriction is established to prevent unauthorized commercial exploitation of reverse-engineered game data structures and to ensure the project remains a tool for research and personal use only.

See the [LICENSE](LICENSE) file for the full license text and [NOTICE](NOTICE) for trademark disclaimers.

### Disclaimer

This project is not affiliated with, authorized by, or endorsed by Blizzard Entertainment. Diablo II: Resurrected is a trademark of Blizzard Entertainment. This repository contains only original source code for the parsing engine and does not distribute any copyrighted game assets.
