//! Wire DTOs, boundary validators, and domain contracts for item mutations.
//!
//! Provides granular mutation operators (assignment, relative delta) for item
//! defense, durability, sockets, ethereal status, identification, and properties/stats.

use std::collections::HashMap;
use std::str::FromStr;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::character::NumericOp;
use super::validation::{FieldViolation, ValidationErrorResponse};
use crate::domain::item::Item;

/// Target specification to find an item within a character's inventory or body slots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ItemTarget {
    /// 0-based index in the character's item list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,

    /// 3-letter item code (e.g. "amu", "rin", "mau"). Case-insensitive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,

    /// Semantic equipment slot name (e.g. "head", "neck", "torso", "left_arm", "right_arm").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
}

impl ItemTarget {
    pub fn by_index(index: usize) -> Self {
        Self {
            index: Some(index),
            code: None,
            slot: None,
        }
    }

    pub fn by_code(code: impl Into<String>) -> Self {
        Self {
            index: None,
            code: Some(code.into()),
            slot: None,
        }
    }

    pub fn by_slot(slot: impl Into<String>) -> Self {
        Self {
            index: None,
            code: None,
            slot: Some(slot.into()),
        }
    }

    /// Evaluates whether an item matches this target filter.
    pub fn matches(&self, idx: usize, item: &Item, equipment_slot: Option<&str>) -> bool {
        if let Some(target_idx) = self.index {
            if target_idx != idx {
                return false;
            }
        }

        if let Some(ref code) = self.code {
            if !item.code.trim().eq_ignore_ascii_case(code.trim()) {
                return false;
            }
        }

        if let Some(ref target_slot) = self.slot {
            match equipment_slot {
                Some(slot) => {
                    if !slot.eq_ignore_ascii_case(target_slot) {
                        return false;
                    }
                }
                None => return false,
            }
        }

        // If no criteria specified, match the first item (idx == 0)
        if self.index.is_none() && self.code.is_none() && self.slot.is_none() {
            return idx == 0;
        }

        true
    }
}

/// Stat operation DTO for modifying an individual item property.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStatOpWireDto {
    /// Numerical stat ID (e.g. 9 for max mana, 127 for all skills).
    pub stat_id: u32,

    /// Mutation operation (Set, Add, or Sub).
    pub op: NumericOp<i32>,
}

impl ItemStatOpWireDto {
    pub fn new(stat_id: u32, op: NumericOp<i32>) -> Self {
        Self { stat_id, op }
    }
}

impl FromStr for ItemStatOpWireDto {
    type Err = String;

    /// Parses string formatted as "stat_id:op" (e.g. "9:+20", "127:=2", "31:-5").
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(format!("Expected 'stat_id:op' format, found: {}", s));
        }

        let stat_id = parts[0]
            .trim()
            .parse::<u32>()
            .map_err(|_| format!("Invalid stat_id: {}", parts[0]))?;

        let op_str = parts[1].trim();
        let op = if let Some(stripped) = op_str.strip_prefix('=') {
            let val = stripped.parse::<i32>().map_err(|_| format!("Invalid number: {}", stripped))?;
            NumericOp::Set(val)
        } else {
            NumericOp::<i32>::from_str(op_str)?
        };

        Ok(ItemStatOpWireDto { stat_id, op })
    }
}

/// Inbound wire DTO representing an item mutation request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ItemWireDto {
    /// Target criteria to locate the item.
    #[serde(default)]
    pub target: ItemTarget,

    /// Defense modifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defense: Option<NumericOp<u32>>,

    /// Current durability modifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub durability: Option<NumericOp<u32>>,

    /// Maximum durability modifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_durability: Option<NumericOp<u32>>,

    /// Stack quantity modifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<NumericOp<u32>>,

    /// Total socket count (0..=6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sockets: Option<u8>,

    /// Ethereal status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ethereal: Option<bool>,

    /// Identified status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_identified: Option<bool>,

    /// Enhanced damage modifier (%) (stat 17).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enhanced_damage: Option<NumericOp<i32>>,

    /// Flat minimum damage bonus (stat 21).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_damage: Option<NumericOp<i32>>,

    /// Flat maximum damage bonus (stat 22).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_damage: Option<NumericOp<i32>>,

    /// Replenish durability rate (stat 252).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replenish_durability: Option<NumericOp<i32>>,

    /// Replenish quantity rate (stat 253).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replenish_quantity: Option<NumericOp<i32>>,

    /// Indestructible status (stat 152).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_indestructible: Option<bool>,

    /// Property and stat modifications.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stats: Vec<ItemStatOpWireDto>,
}

/// Errors occurring during item mutation contract validation or resolution.
#[derive(Debug, Error)]
pub enum ItemContractError {
    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("Target item not found matching criteria: {0}")]
    ItemNotFound(String),

    #[error("Field validation violations encountered")]
    FieldViolations(HashMap<String, Vec<FieldViolation>>),
}

impl From<ItemContractError> for ValidationErrorResponse {
    fn from(err: ItemContractError) -> Self {
        match err {
            ItemContractError::ValidationFailed(msg) => {
                Self::single(
                    "item",
                    FieldViolation::new(
                        "validation_failed",
                        msg,
                        "Ensure the item payload matches specification.",
                    ),
                )
            }
            ItemContractError::ItemNotFound(msg) => {
                Self::single(
                    "item.target",
                    FieldViolation::new(
                        "not_found",
                        msg,
                        "Check the item target index, code, or slot.",
                    ),
                )
            }
            ItemContractError::FieldViolations(errors) => Self {
                status: "validation_error",
                code: 422,
                message: "Validation failed for item mutation payload.",
                errors,
            },
        }
    }
}

/// Infallible domain contract for item mutations with validated values.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemMutationContract {
    pub target: ItemTarget,
    pub defense: Option<u32>,
    pub durability: Option<u32>,
    pub max_durability: Option<u32>,
    pub quantity: Option<u32>,
    pub sockets: Option<u8>,
    pub is_ethereal: Option<bool>,
    pub is_identified: Option<bool>,
    pub is_indestructible: Option<bool>,
    pub enhanced_damage: Option<i32>,
    pub min_damage: Option<i32>,
    pub max_damage: Option<i32>,
    pub replenish_durability: Option<i32>,
    pub replenish_quantity: Option<i32>,
    pub stats: Vec<(u32, i32)>,
}

/// Boundary validator applying domain rules and invariants to item mutations.
pub struct ItemBoundaryValidator;

impl ItemBoundaryValidator {
    pub const MIN_DEFENSE: u32 = 0;
    pub const MAX_DEFENSE: u32 = 10_000;

    pub const MIN_DURABILITY: u32 = 0;
    pub const MAX_DURABILITY: u32 = 255;

    pub const MIN_QUANTITY: u32 = 0;
    pub const MAX_QUANTITY: u32 = 511;

    pub const MIN_SOCKETS: u8 = 0;
    pub const MAX_SOCKETS: u8 = 6;

    pub const MIN_STAT_VALUE: i32 = -32_768;
    pub const MAX_STAT_VALUE: i32 = 32_767;

    pub const MIN_ENHANCED_DAMAGE: i32 = -100;
    pub const MAX_ENHANCED_DAMAGE: i32 = 1000;

    pub const MIN_DAMAGE_BONUS: i32 = 0;
    pub const MAX_DAMAGE_BONUS: i32 = 500;

    pub const MIN_REPLENISH_DURABILITY: i32 = 1;
    pub const MAX_REPLENISH_DURABILITY: i32 = 100;

    pub const MIN_REPLENISH_QUANTITY: i32 = 1;
    pub const MAX_REPLENISH_QUANTITY: i32 = 100;

    /// Validates an inbound wire DTO against domain boundaries and current item state.
    pub fn validate(
        dto: &ItemWireDto,
        current_item: Option<&Item>,
    ) -> Result<ItemMutationContract, ItemContractError> {
        let mut errors: HashMap<String, Vec<FieldViolation>> = HashMap::new();

        // 1. Defense Validation
        let evaluated_defense = if let Some(ref op) = dto.defense {
            let current = current_item.and_then(|i| i.defense).unwrap_or(0);
            match Self::evaluate_u32_op("item.defense", *op, current, Self::MIN_DEFENSE, Self::MAX_DEFENSE) {
                Ok(val) => Some(val),
                Err(violation) => {
                    errors.entry("item.defense".to_string()).or_default().push(violation);
                    None
                }
            }
        } else {
            None
        };

        // 2. Max Durability Validation
        let evaluated_max_durability = if let Some(ref op) = dto.max_durability {
            let current = current_item.and_then(|i| i.max_durability).unwrap_or(0);
            match Self::evaluate_u32_op("item.max_durability", *op, current, Self::MIN_DURABILITY, Self::MAX_DURABILITY) {
                Ok(val) => Some(val),
                Err(violation) => {
                    errors.entry("item.max_durability".to_string()).or_default().push(violation);
                    None
                }
            }
        } else {
            None
        };

        // 3. Durability Validation
        let effective_max_durability = evaluated_max_durability
            .or_else(|| current_item.and_then(|i| i.max_durability))
            .unwrap_or(Self::MAX_DURABILITY);

        let evaluated_durability = if let Some(ref op) = dto.durability {
            let current = current_item.and_then(|i| i.current_durability).unwrap_or(0);
            match Self::evaluate_u32_op("item.durability", *op, current, Self::MIN_DURABILITY, effective_max_durability) {
                Ok(val) => Some(val),
                Err(violation) => {
                    errors.entry("item.durability".to_string()).or_default().push(violation);
                    None
                }
            }
        } else {
            None
        };

        // 4. Quantity Validation
        let evaluated_quantity = if let Some(ref op) = dto.quantity {
            let current = current_item.and_then(|i| i.quantity).unwrap_or(0);
            match Self::evaluate_u32_op("item.quantity", *op, current, Self::MIN_QUANTITY, Self::MAX_QUANTITY) {
                Ok(val) => Some(val),
                Err(violation) => {
                    errors.entry("item.quantity".to_string()).or_default().push(violation);
                    None
                }
            }
        } else {
            None
        };

        // 5. Dynamic Sockets Validation based on Item Type and ilvl
        let effective_max_sockets = if let Some(item) = current_item {
            let types = crate::engine::validation::get_all_item_types(&item.code);
            let ilvl = item.header.level.unwrap_or(99);
            let mut max_sock = None;
            for type_code in types {
                if let Some(rule) = crate::data::legitimacy::SOCKET_RULES.iter().find(|r| r.item_type == type_code) {
                    let m = if ilvl >= 41 {
                        rule.max_sock_high
                    } else if ilvl >= 26 {
                        rule.max_sock_mid
                    } else {
                        rule.max_sock_low
                    };
                    max_sock = Some(m);
                    break;
                }
            }
            max_sock.unwrap_or(Self::MAX_SOCKETS)
        } else {
            Self::MAX_SOCKETS
        };

        let evaluated_sockets = if let Some(sockets) = dto.sockets {
            if sockets > effective_max_sockets {
                let item_code = current_item.map(|i| i.code.trim()).unwrap_or("item");
                let hint = if effective_max_sockets == 0 {
                    format!("Item '{}' cannot have sockets.", item_code)
                } else {
                    format!("Item sockets must be an integer between {} and {}.", Self::MIN_SOCKETS, effective_max_sockets)
                };
                errors.entry("item.sockets".to_string()).or_default().push(
                    FieldViolation::new(
                        "invalid_range",
                        format!("Socket count {} exceeds maximum allowable of {} for item '{}'.", sockets, effective_max_sockets, item_code),
                        hint,
                    )
                    .with_rejected_value(sockets)
                    .with_constraint(Self::MIN_SOCKETS as u64, effective_max_sockets as u64),
                );
                None
            } else {
                Some(sockets)
            }
        } else {
            None
        };

        // 6. Ethereal Invariant Validation
        if dto.is_ethereal == Some(true) {
            if let Some(item) = current_item {
                let non_ethereal_types = ["ring", "amul", "char", "csch", "mcha", "scha", "rune", "scro", "poti"];
                let types = crate::engine::validation::get_all_item_types(&item.code);
                if types.iter().any(|t| non_ethereal_types.contains(t)) {
                    errors.entry("item.is_ethereal".to_string()).or_default().push(
                        FieldViolation::new(
                            "invalid_attribute",
                            format!("Item '{}' cannot have ethereal property.", item.code.trim()),
                            "Jewelry, charms, runes, scrolls, and potions cannot possess ethereal status.",
                        )
                        .with_rejected_value(true),
                    );
                }
            }
        }

        // 7. Stackable Quantity vs Durability Invariant
        if dto.quantity.is_some() {
            if let Some(item) = current_item {
                let base_spec = crate::data::item_specs::get_base_item_spec(&item.code);
                if item.quantity.is_none() && base_spec.map_or(false, |s| s.durability > 0) {
                    errors.entry("item.quantity".to_string()).or_default().push(
                        FieldViolation::new(
                            "not_stackable",
                            format!("Item '{}' is not a stackable or throwing item.", item.code.trim()),
                            "Quantity can only be modified on stackable items (throwing weapons, potions, keys, arrows/bolts).",
                        ),
                    );
                }
            }
        }

        if dto.durability.is_some() {
            if let Some(item) = current_item {
                if item.current_durability.is_none() && item.quantity.is_some() {
                    errors.entry("item.durability".to_string()).or_default().push(
                        FieldViolation::new(
                            "no_durability",
                            format!("Stackable item '{}' does not use durability.", item.code.trim()),
                            "Throwing weapons and consumables track stack quantity rather than durability.",
                        ),
                    );
                }
            }
        }

        // 8. Indestructible Invariant Validation
        let is_indestructible_active = dto.is_indestructible.unwrap_or_else(|| {
            current_item.map_or(false, |it| {
                it.stats.properties.iter().any(|p| p.stat_id == 152 && p.raw_value > 0)
                    || it.properties.iter().any(|p| p.stat_id == 152 && p.value > 0)
            })
        });

        if is_indestructible_active {
            if evaluated_durability == Some(0) {
                errors.entry("item.durability".to_string()).or_default().push(
                    FieldViolation::new(
                        "invalid_state",
                        "Indestructible items cannot have 0 durability.",
                        "Indestructible items never lose durability; durability must equal maximum durability.",
                    )
                    .with_rejected_value(0),
                );
            } else if let Some(cur) = evaluated_durability {
                if cur < effective_max_durability {
                    errors.entry("item.durability".to_string()).or_default().push(
                        FieldViolation::new(
                            "invalid_state",
                            format!("Indestructible item durability ({}) cannot be less than max durability ({}).", cur, effective_max_durability),
                            "Indestructible items maintain full durability at all times.",
                        )
                        .with_rejected_value(cur)
                        .with_constraint(effective_max_durability as u64, effective_max_durability as u64),
                    );
                }
            }
        }

        // 9. Stats & High-Level Damage / Replenishment Evaluation
        let mut evaluated_stats = Vec::new();

        let mut upsert_stat = |stat_id: u32, value: i32| {
            if let Some(pos) = evaluated_stats.iter().position(|(id, _)| *id == stat_id) {
                evaluated_stats[pos].1 = value;
            } else {
                evaluated_stats.push((stat_id, value));
            }
        };

        let get_current_stat = |stat_id: u32| -> i32 {
            current_item
                .and_then(|i| {
                    i.stats
                        .properties
                        .iter()
                        .find(|p| p.stat_id == stat_id)
                        .map(|p| p.raw_value)
                        .or_else(|| {
                            i.properties
                                .iter()
                                .find(|p| p.stat_id == stat_id)
                                .map(|p| p.value)
                        })
                })
                .unwrap_or(0)
        };

        // Enhanced damage (stat 17)
        let evaluated_ed = if let Some(ref op) = dto.enhanced_damage {
            let current = get_current_stat(17);
            match Self::evaluate_i32_op("item.enhanced_damage", *op, current, Self::MIN_ENHANCED_DAMAGE, Self::MAX_ENHANCED_DAMAGE) {
                Ok(val) => {
                    upsert_stat(17, val);
                    Some(val)
                }
                Err(v) => {
                    errors.entry("item.enhanced_damage".to_string()).or_default().push(v);
                    None
                }
            }
        } else {
            None
        };

        // Flat minimum damage bonus (stat 21)
        let evaluated_min_dmg = if let Some(ref op) = dto.min_damage {
            let current = get_current_stat(21);
            match Self::evaluate_i32_op("item.min_damage", *op, current, Self::MIN_DAMAGE_BONUS, Self::MAX_DAMAGE_BONUS) {
                Ok(val) => {
                    upsert_stat(21, val);
                    Some(val)
                }
                Err(v) => {
                    errors.entry("item.min_damage".to_string()).or_default().push(v);
                    None
                }
            }
        } else {
            None
        };

        // Flat maximum damage bonus (stat 22)
        let evaluated_max_dmg = if let Some(ref op) = dto.max_damage {
            let current = get_current_stat(22);
            match Self::evaluate_i32_op("item.max_damage", *op, current, Self::MIN_DAMAGE_BONUS, Self::MAX_DAMAGE_BONUS) {
                Ok(val) => {
                    upsert_stat(22, val);
                    Some(val)
                }
                Err(v) => {
                    errors.entry("item.max_damage".to_string()).or_default().push(v);
                    None
                }
            }
        } else {
            None
        };

        // Replenish durability rate (stat 252)
        let evaluated_rep_dur = if let Some(ref op) = dto.replenish_durability {
            let current = get_current_stat(252);
            match Self::evaluate_i32_op("item.replenish_durability", *op, current, Self::MIN_REPLENISH_DURABILITY, Self::MAX_REPLENISH_DURABILITY) {
                Ok(val) => {
                    upsert_stat(252, val);
                    Some(val)
                }
                Err(v) => {
                    errors.entry("item.replenish_durability".to_string()).or_default().push(v);
                    None
                }
            }
        } else {
            None
        };

        // Replenish quantity rate (stat 253)
        let evaluated_rep_quant = if let Some(ref op) = dto.replenish_quantity {
            let current = get_current_stat(253);
            match Self::evaluate_i32_op("item.replenish_quantity", *op, current, Self::MIN_REPLENISH_QUANTITY, Self::MAX_REPLENISH_QUANTITY) {
                Ok(val) => {
                    upsert_stat(253, val);
                    Some(val)
                }
                Err(v) => {
                    errors.entry("item.replenish_quantity".to_string()).or_default().push(v);
                    None
                }
            }
        } else {
            None
        };

        // Indestructible flag (stat 152)
        if let Some(indestructible) = dto.is_indestructible {
            upsert_stat(152, if indestructible { 1 } else { 0 });
        }

        // Raw custom stat operations
        for (idx, stat_op) in dto.stats.iter().enumerate() {
            let field_name = format!("item.stats[{}].value", idx);
            let current = get_current_stat(stat_op.stat_id);

            match Self::evaluate_i32_op(&field_name, stat_op.op, current, Self::MIN_STAT_VALUE, Self::MAX_STAT_VALUE) {
                Ok(val) => upsert_stat(stat_op.stat_id, val),
                Err(violation) => {
                    errors.entry(field_name).or_default().push(violation);
                }
            }
        }

        if !errors.is_empty() {
            return Err(ItemContractError::FieldViolations(errors));
        }

        Ok(ItemMutationContract {
            target: dto.target.clone(),
            defense: evaluated_defense,
            durability: evaluated_durability,
            max_durability: evaluated_max_durability,
            quantity: evaluated_quantity,
            sockets: evaluated_sockets,
            is_ethereal: dto.is_ethereal,
            is_identified: dto.is_identified,
            is_indestructible: dto.is_indestructible,
            enhanced_damage: evaluated_ed,
            min_damage: evaluated_min_dmg,
            max_damage: evaluated_max_dmg,
            replenish_durability: evaluated_rep_dur,
            replenish_quantity: evaluated_rep_quant,
            stats: evaluated_stats,
        })
    }

    fn evaluate_u32_op(
        field: &str,
        op: NumericOp<u32>,
        current: u32,
        min: u32,
        max: u32,
    ) -> Result<u32, FieldViolation> {
        match op {
            NumericOp::Set(target) => {
                if target < min || target > max {
                    Err(FieldViolation::new(
                        "invalid_range",
                        format!("Field '{}' target value {} is outside allowable range [{}, {}].", field, target, min, max),
                        format!("Enter an integer between {} and {}.", min, max),
                    )
                    .with_rejected_value(target)
                    .with_constraint(min as u64, max as u64))
                } else {
                    Ok(target)
                }
            }
            NumericOp::Add(delta) => {
                let resulting = current.saturating_add(delta);
                if resulting > max {
                    Err(FieldViolation::new(
                        "overflow",
                        format!("Field '{}' resulting value {} (current {} + delta {}) exceeds maximum of {}.", field, resulting, current, delta, max),
                        format!("Max allowable increment is {}.", max.saturating_sub(current)),
                    )
                    .with_rejected_value(delta)
                    .with_constraint(min as u64, max as u64))
                } else {
                    Ok(resulting)
                }
            }
            NumericOp::Sub(delta) => {
                if delta > current || current.saturating_sub(delta) < min {
                    let resulting = current.saturating_sub(delta);
                    Err(FieldViolation::new(
                        "underflow",
                        format!("Field '{}' resulting value {} (current {} - delta {}) falls below minimum of {}.", field, resulting, current, delta, min),
                        format!("Max allowable decrement is {}.", current.saturating_sub(min)),
                    )
                    .with_rejected_value(delta)
                    .with_constraint(min as u64, max as u64))
                } else {
                    Ok(current - delta)
                }
            }
        }
    }

    fn evaluate_i32_op(
        field: &str,
        op: NumericOp<i32>,
        current: i32,
        min: i32,
        max: i32,
    ) -> Result<i32, FieldViolation> {
        match op {
            NumericOp::Set(target) => {
                if target < min || target > max {
                    Err(FieldViolation::new(
                        "invalid_range",
                        format!("Field '{}' target value {} is outside allowable range [{}, {}].", field, target, min, max),
                        format!("Enter an integer between {} and {}.", min, max),
                    )
                    .with_rejected_value(target))
                } else {
                    Ok(target)
                }
            }
            NumericOp::Add(delta) => {
                let resulting = (current as i64) + (delta as i64);
                if resulting > max as i64 {
                    Err(FieldViolation::new(
                        "overflow",
                        format!("Field '{}' resulting value {} (current {} + delta {}) exceeds maximum of {}.", field, resulting, current, delta, max),
                        format!("Max allowable increment is {}.", (max as i64) - (current as i64)),
                    )
                    .with_rejected_value(delta))
                } else {
                    Ok(resulting as i32)
                }
            }
            NumericOp::Sub(delta) => {
                let resulting = (current as i64) - (delta as i64);
                if resulting < min as i64 {
                    Err(FieldViolation::new(
                        "underflow",
                        format!("Field '{}' resulting value {} (current {} - delta {}) falls below minimum of {}.", field, resulting, current, delta, min),
                        format!("Max allowable decrement is {}.", (current as i64) - (min as i64)),
                    )
                    .with_rejected_value(delta))
                } else {
                    Ok(resulting as i32)
                }
            }
        }
    }
}
