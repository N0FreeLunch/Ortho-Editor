//! Code-synchronized API documentation generator for Ortho Editor.
//!
//! Generates JSON Schema definitions and Markdown specifications directly from
//! Rust DTO contracts in `d2r_core::domain::contracts`.
//!
//! Supports `--check` flag to verify that on-disk documentation matches code.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use schemars::schema_for;

use d2r_core::domain::contracts::{
    BatchItemWireDto, CharacterWireDto, ItemWireDto, SaveMutationDto, SkillWireDto,
    ValidationErrorResponse, WaypointWireDto,
};

fn get_core_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
}

fn build_schema_json<T: schemars::JsonSchema>() -> String {
    let schema = schema_for!(T);
    let mut json = serde_json::to_string_pretty(&schema).expect("schema serialization should succeed");
    json.push('\n');
    json
}

fn build_readme_markdown() -> String {
    r#"# Ortho Editor - Save Mutation API

The **Ortho Editor Mutation API** provides a strongly typed, atomic, and contract-validated interface for inspecting and mutating Diablo II: Resurrected save files (`.d2s`).

## Architecture & Integration

All mutation requests are expressed through wire Data Transfer Objects (DTOs) adhering to strict domain invariants.

- **CLI Usage**: Pass a mutation payload JSON via `--payload <path>` or inline:
  ```bash
  ortho.exe save.d2s --payload mutation.json -o modified.d2s
  ```
- **Web / IPC / GUI Integration**: DTO definitions are directly serializable and deserializable via `serde`, and validated against RFC 7807 Problem Details on invalid inputs.

## Published Schemas

All JSON Schemas in this directory are generated directly from the Rust source code contracts (`d2r_core::domain::contracts`) and are verified on every test pass.

| Schema | Target Wire DTO | Description |
| :--- | :--- | :--- |
| [`save_mutation.schema.json`](./schema/save_mutation.schema.json) | `SaveMutationDto` | Top-level root batch & composite mutation payload. |
| [`character_wire.schema.json`](./schema/character_wire.schema.json) | `CharacterWireDto` | Character level, experience, core attributes, and gold. |
| [`item_wire.schema.json`](./schema/item_wire.schema.json) | `ItemWireDto` | Single-item defense, durability, sockets, ethereal, and stat modifiers. |
| [`skill_wire.schema.json`](./schema/skill_wire.schema.json) | `SkillWireDto` | Individual skill levels, skill points, and complete respec. |
| [`waypoint_wire.schema.json`](./schema/waypoint_wire.schema.json) | `WaypointWireDto` | Waypoint unlock and progression across difficulties. |
| [`batch_item_wire.schema.json`](./schema/batch_item_wire.schema.json) | `BatchItemWireDto` | Mass item convenience operations (repair all, identify all, socket all). |
| [`validation_error_response.schema.json`](./schema/validation_error_response.schema.json) | `ValidationErrorResponse` | RFC 7807 422 Unprocessable Entity error response schema. |

## Detailed Specification

For comprehensive field-by-field reference, numeric operators, and examples, see [MUTATION_API.md](./MUTATION_API.md).
"#.to_string()
}

fn build_mutation_api_markdown() -> String {
    r#"# Ortho Editor Mutation API Specification

This document provides the authoritative reference for all payload structures, operations, and validation rules supported by the Ortho Editor mutation pipeline.

## 1. Top-Level Payload: `SaveMutationDto`

The top-level payload unites multiple mutation domains into a single atomic transaction.

```json
{
  "character": { ... },
  "waypoints": { ... },
  "skills": { ... },
  "items": [ ... ],
  "batch_items": { ... }
}
```

### Fields

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `character` | [`CharacterWireDto`](#2-character-mutations-characterwiredto) | No | Level, base attributes, experience, and gold. |
| `waypoints` | [`WaypointWireDto`](#3-waypoint-mutations-waypointwiredto) | No | Difficulty and waypoint activation. |
| `skills` | [`SkillWireDto`](#4-skill-mutations-skillwiredto) | No | Skill points, individual skill allocations, and full respec. |
| `items` | Array of [`ItemWireDto`](#5-item-mutations-itemwiredto) | No | Targeted item modifications (defense, sockets, durability, stats). |
| `batch_items` | [`BatchItemWireDto`](#6-batch-item-operations-batchitemwiredto) | No | Bulk inventory convenience operations (repair all, identify all). |

---

## 2. Numeric Mutation Operators (`NumericOp<T>`)

Most numeric fields accept a flexible `NumericOp` object or CLI shorthand:

| JSON Syntax | Shorthand | Operation | Behavior |
| :--- | :--- | :--- | :--- |
| `{"op": "Set", "value": 90}` | `=90` or `90` | Absolute Assignment | Directly sets the value to 90. |
| `{"op": "Add", "value": 5}` | `+5` | Relative Addition | Increments the current value by 5. |
| `{"op": "Sub", "value": 10}` | `-10` | Relative Subtraction | Decrements the current value by 10. |

All operations are validated against domain boundaries and underflow/overflow rules prior to execution.

---

## 3. Character Mutations: `CharacterWireDto`

Modifies character attributes, stats, and progression.

```json
{
  "name": "Aragorn",
  "level": { "op": "Set", "value": 99 },
  "strength": { "op": "Add", "value": 50 },
  "dexterity": { "op": "Set", "value": 200 },
  "vitality": { "op": "Set", "value": 400 },
  "energy": { "op": "Set", "value": 50 },
  "stat_points": { "op": "Set", "value": 0 },
  "gold_character": { "op": "Set", "value": 1000000 },
  "gold_stash": { "op": "Set", "value": 2500000 }
}
```

### Constraints
- `name`: 2 to 15 characters, alphanumeric.
- `level`: 1 to 99.
- `strength`, `dexterity`, `vitality`, `energy`: 1 to 1023.
- `gold_character`: 0 to `level * 10,000`.
- `gold_stash`: 0 to 2,500,000.

---

## 4. Item Mutations: `ItemWireDto`

Modifies individual items identified by slot, code, or index.

```json
{
  "target": {
    "slot": "head"
  },
  "defense": { "op": "Set", "value": 500 },
  "durability": { "op": "Set", "value": 100 },
  "max_durability": { "op": "Set", "value": 100 },
  "sockets": 2,
  "is_ethereal": false,
  "is_identified": true,
  "is_indestructible": false,
  "stats": [
    { "stat_id": 127, "op": { "op": "Add", "value": 1 } }
  ]
}
```

### Target Selection (`ItemTarget`)
- `slot`: Semantic equipment slot (`"head"`, `"neck"`, `"torso"`, `"right_arm"`, `"left_arm"`, `"right_ring"`, `"left_ring"`, `"belt"`, `"feet"`, `"gloves"`).
- `code`: 3-letter item code (e.g. `"amu"`, `"rin"`, `"wa2"`, `"xrs"`).
- `index`: 0-based index in player item stream.

### Boundary Rules
- `sockets`: Must not exceed the item type's maximum allowed sockets (`max_sockets`).
- `is_indestructible`: If true, durability values cannot be modified to contradictory values.

---

## 5. Batch Item Operations: `BatchItemWireDto`

Performs bulk convenience operations across player items.

```json
{
  "repair_all": true,
  "identify_all": true,
  "socket_all": 6
}
```

- `repair_all`: Restores durability to maximum on all repairable items.
- `identify_all`: Sets identified flag on all unidentified items.
- `socket_all`: Sockets all unsocketed items up to their base item maximum (or capped at count).

---

## 6. Skill Mutations: `SkillWireDto`

```json
{
  "respec": false,
  "skill_points": { "op": "Set", "value": 5 },
  "skills": [
    { "skill_id": 54, "op": { "op": "Set", "value": 20 } }
  ]
}
```

- `respec`: If `true`, resets all allocated skills to 0 and refunds spent points to `skill_points`.
- `skills`: Class-specific skill level modifications (0..=20).

---

## 7. Waypoint Mutations: `WaypointWireDto`

```json
{
  "difficulty": 0,
  "unlock_all": true,
  "activate_waypoints": ["Act 1 - Town", "Act 2 - Town"]
}
```

- `difficulty`: 0 = Normal, 1 = Nightmare, 2 = Hell. If omitted, applies to all difficulties.
- `unlock_all`: Unlocks all 39 game waypoints in the target difficulty.

---

## 8. Validation & Error Handling (RFC 7807 Problem Details)

When a mutation payload violates domain boundaries or invariants, the system rejects the transaction and emits a 422 Unprocessable Entity `ValidationErrorResponse`:

```json
{
  "status": "error",
  "code": 422,
  "message": "Validation failed for 1 field(s).",
  "errors": {
    "level": [
      {
        "code": "invalid_range",
        "message": "Character level 105 is outside the allowable range.",
        "hint": "Level must be an integer between 1 and 99.",
        "rejected_value": 105,
        "constraint": {
          "min": 1,
          "max": 99
        }
      }
    ]
  }
}
```
"#.to_string()
}

fn generate_all_artifacts() -> BTreeMap<PathBuf, String> {
    let mut files = BTreeMap::new();
    let root = get_core_root();
    let api_dir = root.join("docs").join("api");
    let schema_dir = api_dir.join("schema");

    files.insert(
        schema_dir.join("save_mutation.schema.json"),
        build_schema_json::<SaveMutationDto>(),
    );
    files.insert(
        schema_dir.join("character_wire.schema.json"),
        build_schema_json::<CharacterWireDto>(),
    );
    files.insert(
        schema_dir.join("item_wire.schema.json"),
        build_schema_json::<ItemWireDto>(),
    );
    files.insert(
        schema_dir.join("skill_wire.schema.json"),
        build_schema_json::<SkillWireDto>(),
    );
    files.insert(
        schema_dir.join("waypoint_wire.schema.json"),
        build_schema_json::<WaypointWireDto>(),
    );
    files.insert(
        schema_dir.join("batch_item_wire.schema.json"),
        build_schema_json::<BatchItemWireDto>(),
    );
    files.insert(
        schema_dir.join("validation_error_response.schema.json"),
        build_schema_json::<ValidationErrorResponse>(),
    );
    files.insert(
        api_dir.join("README.md"),
        build_readme_markdown(),
    );
    files.insert(
        api_dir.join("MUTATION_API.md"),
        build_mutation_api_markdown(),
    );

    files
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let check_mode = args.iter().any(|arg| arg == "--check");

    let artifacts = generate_all_artifacts();

    if check_mode {
        let mut mismatches = Vec::new();
        for (path, expected_content) in &artifacts {
            if !path.exists() {
                mismatches.push(format!("Missing file: {}", path.display()));
                continue;
            }
            let actual = fs::read_to_string(path).unwrap_or_default().replace("\r\n", "\n");
            let expected = expected_content.replace("\r\n", "\n");
            if actual != expected {
                mismatches.push(format!("Content divergence in: {}", path.display()));
            }
        }

        if !mismatches.is_empty() {
            eprintln!("API Documentation Check Failed!");
            for m in mismatches {
                eprintln!("  - {}", m);
            }
            eprintln!("\nPlease regenerate API docs by running:\n  cargo run --bin gen_api_docs\n");
            std::process::exit(1);
        } else {
            println!("API documentation is strictly synchronized with code contracts (0-diff).");
        }
    } else {
        println!("Generating API documentation and JSON schemas...");
        for (path, content) in &artifacts {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("failed to create directory");
            }
            fs::write(path, content.replace("\r\n", "\n")).expect("failed to write artifact");
            println!("  [OK] Written: {}", path.display());
        }
        println!("\nSuccessfully generated all API documentation artifacts in docs/api/.");
    }
}
