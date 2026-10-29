# Ortho Editor Mutation API Specification

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
