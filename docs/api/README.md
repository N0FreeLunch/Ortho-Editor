# Ortho Editor - Save Mutation API

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
