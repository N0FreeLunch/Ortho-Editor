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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemMutationContract {
    pub target: ItemTarget,
    pub defense: Option<u32>,
    pub durability: Option<u32>,
    pub max_durability: Option<u32>,
    pub quantity: Option<u32>,
    pub sockets: Option<u8>,
    pub is_ethereal: Option<bool>,
    pub is_identified: Option<bool>,
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

        // 5. Sockets Validation
        let evaluated_sockets = if let Some(sockets) = dto.sockets {
            if sockets > Self::MAX_SOCKETS {
                errors.entry("item.sockets".to_string()).or_default().push(
                    FieldViolation::new(
                        "invalid_range",
                        format!("Socket count {} exceeds maximum allowable of {}.", sockets, Self::MAX_SOCKETS),
                        format!("Item sockets must be an integer between {} and {}.", Self::MIN_SOCKETS, Self::MAX_SOCKETS),
                    )
                    .with_rejected_value(sockets)
                    .with_constraint(Self::MIN_SOCKETS as u64, Self::MAX_SOCKETS as u64),
                );
                None
            } else {
                Some(sockets)
            }
        } else {
            None
        };

        // 6. Stats Validation
        let mut evaluated_stats = Vec::new();
        for (idx, stat_op) in dto.stats.iter().enumerate() {
            let field_name = format!("item.stats[{}].value", idx);
            let current = current_item
                .and_then(|i| i.stats.properties.iter().find(|p| p.stat_id == stat_op.stat_id))
                .map(|p| p.raw_value)
                .unwrap_or(0);

            match Self::evaluate_i32_op(&field_name, stat_op.op, current, Self::MIN_STAT_VALUE, Self::MAX_STAT_VALUE) {
                Ok(val) => evaluated_stats.push((stat_op.stat_id, val)),
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
