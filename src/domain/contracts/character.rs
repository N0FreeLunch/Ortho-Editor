//! Wire DTOs and validation contracts for character attributes and profile.

use std::str::FromStr;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use validator::Validate;
use schemars::JsonSchema;

/// Mutation operator supporting absolute assignment and relative delta operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", content = "value")]
pub enum NumericOp<T> {
    /// Absolute assignment: sets the field directly to the given value.
    Set(T),
    /// Relative addition: increments current value by delta.
    Add(T),
    /// Relative subtraction: decrements current value by delta.
    Sub(T),
}

impl<T> NumericOp<T> {
    pub fn set(val: T) -> Self {
        Self::Set(val)
    }

    pub fn add(val: T) -> Self {
        Self::Add(val)
    }

    pub fn sub(val: T) -> Self {
        Self::Sub(val)
    }
}

impl<T: FromStr> FromStr for NumericOp<T> {
    type Err = String;

    /// Parses expressions like "250", "+10", or "-5".
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if let Some(rest) = trimmed.strip_prefix('+') {
            let val = rest.parse::<T>().map_err(|_| format!("Invalid number after '+': {}", rest))?;
            Ok(NumericOp::Add(val))
        } else if let Some(rest) = trimmed.strip_prefix('-') {
            let val = rest.parse::<T>().map_err(|_| format!("Invalid number after '-': {}", rest))?;
            Ok(NumericOp::Sub(val))
        } else {
            let val = trimmed.parse::<T>().map_err(|_| format!("Invalid number: {}", trimmed))?;
            Ok(NumericOp::Set(val))
        }
    }
}

/// Errors that can occur during boundary contract validation and parsing.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContractError {
    #[error("Boundary validation failed: {0}")]
    ValidationFailed(String),

    #[error("Invalid character level: {0} (must be 1..=99)")]
    InvalidLevel(u8),

    #[error("Invalid stat value: {0} (exceeds allowable range)")]
    InvalidStatPoint(u32),

    #[error("Invalid stash gold: {0} (max 2,500,000)")]
    InvalidStashGold(u32),

    #[error("Invalid character name: '{0}'")]
    InvalidName(String),

    #[error("Boundary overflow for '{field}': current {current} with delta/target {delta_or_target} exceeds maximum allowable {max}")]
    Overflow {
        field: &'static str,
        current: u64,
        delta_or_target: u64,
        max: u64,
    },

    #[error("Boundary underflow for '{field}': current {current} with delta/target {delta_or_target} falls below minimum allowable {min}")]
    Underflow {
        field: &'static str,
        current: u64,
        delta_or_target: u64,
        min: u64,
    },
}

impl From<validator::ValidationErrors> for ContractError {
    fn from(err: validator::ValidationErrors) -> Self {
        Self::ValidationFailed(err.to_string())
    }
}

impl From<ContractError> for super::validation::ValidationErrorResponse {
    fn from(err: ContractError) -> Self {
        use super::validation::FieldViolation;
        match err {
            ContractError::ValidationFailed(msg) => {
                Self::single("request", FieldViolation::new("validation_failed", msg, "Ensure the request format satisfies validation rules."))
            }
            ContractError::InvalidLevel(lvl) => {
                Self::single(
                    "level",
                    FieldViolation::new(
                        "invalid_range",
                        format!("Character level {} is outside the allowable range.", lvl),
                        "Level must be an integer between 1 and 99.",
                    )
                    .with_rejected_value(lvl)
                    .with_constraint(1, 99),
                )
            }
            ContractError::InvalidStatPoint(val) => {
                Self::single(
                    "stat",
                    FieldViolation::new(
                        "invalid_range",
                        format!("Stat value {} exceeds allowable maximum.", val),
                        "Stat point value must be between 1 and 1023.",
                    )
                    .with_rejected_value(val)
                    .with_constraint(1, 1023),
                )
            }
            ContractError::InvalidStashGold(val) => {
                Self::single(
                    "gold_stash",
                    FieldViolation::new(
                        "cap_exceeded",
                        format!("Private stash gold {} exceeds maximum capacity.", val),
                        "Private stash gold cannot exceed 2,500,000.",
                    )
                    .with_rejected_value(val)
                    .with_constraint(0, 2_500_000),
                )
            }
            ContractError::InvalidName(name) => {
                Self::single(
                    "name",
                    FieldViolation::new(
                        "invalid_format",
                        format!("Character name '{}' is invalid.", name),
                        "Name must contain 2 to 15 alphanumeric characters without reserved symbols.",
                    )
                    .with_rejected_value(name),
                )
            }
            ContractError::Overflow { field, current, delta_or_target, max } => {
                Self::single(
                    field,
                    FieldViolation::new(
                        "overflow",
                        format!("Applying mutation to '{}' exceeds maximum allowable limit {}.", field, max),
                        format!(
                            "Must result in a value <= {}. Current value: {}, attempted modification: {} (would result in {}).",
                            max, current, delta_or_target, current.saturating_add(delta_or_target)
                        ),
                    )
                    .with_rejected_value(delta_or_target)
                    .with_constraint(None, max),
                )
            }
            ContractError::Underflow { field, current, delta_or_target, min } => {
                Self::single(
                    field,
                    FieldViolation::new(
                        "underflow",
                        format!("Applying mutation to '{}' falls below minimum allowable limit {}.", field, min),
                        format!(
                            "Must result in a value >= {}. Current value: {}, attempted subtraction: {}.",
                            min, current, delta_or_target
                        ),
                    )
                    .with_rejected_value(delta_or_target)
                    .with_constraint(min, None),
                )
            }
        }
    }
}

/// Strongly typed Value Object representing a verified character level (1..=99).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
pub struct LevelValue(u8);

impl LevelValue {
    /// Creates a new `LevelValue`, enforcing the 1..=99 invariant.
    pub fn new(val: u8) -> Result<Self, ContractError> {
        match val {
            1..=99 => Ok(Self(val)),
            _ => Err(ContractError::InvalidLevel(val)),
        }
    }

    /// Returns the primitive level value.
    pub fn get(self) -> u8 {
        self.0
    }
}

/// Strongly typed Value Object representing a verified stat attribute or unspent point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
pub struct StatPointValue(u32);

impl StatPointValue {
    /// Creates a new `StatPointValue`, enforcing upper bound invariant (e.g. 1023).
    pub fn new(val: u32, max: u32) -> Result<Self, ContractError> {
        if val <= max {
            Ok(Self(val))
        } else {
            Err(ContractError::InvalidStatPoint(val))
        }
    }

    /// Returns the primitive stat value.
    pub fn get(self) -> u32 {
        self.0
    }
}

/// Strongly typed Value Object representing private stash gold (0..=2,500,000).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
pub struct StashGoldValue(u32);

impl StashGoldValue {
    pub const MAX_STASH_GOLD: u32 = 2_500_000;

    /// Creates a new `StashGoldValue`, enforcing the stash gold cap.
    pub fn new(val: u32) -> Result<Self, ContractError> {
        if val <= Self::MAX_STASH_GOLD {
            Ok(Self(val))
        } else {
            Err(ContractError::InvalidStashGold(val))
        }
    }

    /// Returns the primitive gold value.
    pub fn get(self) -> u32 {
        self.0
    }
}

/// Wire DTO for character profile and attribute mutations.
/// Supports both absolute assignment and relative delta operations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Validate, JsonSchema)]
pub struct CharacterWireDto {
    /// Character name (2 to 15 alphanumeric characters).
    #[validate(length(min = 2, max = 15, message = "Character name must be 2 to 15 characters"))]
    pub name: Option<String>,

    /// Character level operation.
    pub level: Option<NumericOp<u8>>,

    /// Character experience points.
    pub experience: Option<NumericOp<u32>>,

    /// Base strength operation.
    pub strength: Option<NumericOp<u32>>,

    /// Base dexterity operation.
    pub dexterity: Option<NumericOp<u32>>,

    /// Base vitality operation.
    pub vitality: Option<NumericOp<u32>>,

    /// Base energy operation.
    pub energy: Option<NumericOp<u32>>,

    /// Unspent stat points operation.
    pub stat_points: Option<NumericOp<u32>>,

    /// Gold in character inventory operation.
    pub gold_character: Option<NumericOp<u32>>,

    /// Gold in private stash operation.
    pub gold_stash: Option<NumericOp<u32>>,
}

/// Domain contract for character mutations with verified operators.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterMutationContract {
    pub name: Option<String>,
    pub level: Option<NumericOp<LevelValue>>,
    pub experience: Option<NumericOp<u32>>,
    pub strength: Option<NumericOp<StatPointValue>>,
    pub dexterity: Option<NumericOp<StatPointValue>>,
    pub vitality: Option<NumericOp<StatPointValue>>,
    pub energy: Option<NumericOp<StatPointValue>>,
    pub stat_points: Option<NumericOp<StatPointValue>>,
    pub gold_character: Option<NumericOp<u32>>,
    pub gold_stash: Option<NumericOp<StashGoldValue>>,
}

/// Current state of character attributes before mutation, used to evaluate resulting values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharacterSnapshot {
    pub level: u8,
    pub experience: u32,
    pub strength: u32,
    pub dexterity: u32,
    pub vitality: u32,
    pub energy: u32,
    pub stat_points: u32,
    pub gold_character: u32,
    pub gold_stash: u32,
}

/// Boundary and invariant validator for character mutations.
/// Inspects the resulting values of relative or absolute operations against in-game and bitstream bounds.
pub struct CharacterBoundaryValidator;

impl CharacterBoundaryValidator {
    /// Evaluates the target operation against min/max bounds and calculates the resulting value.
    pub fn evaluate_bound(
        field: &'static str,
        current: u64,
        op: NumericOp<u64>,
        min: u64,
        max: u64,
    ) -> Result<u64, ContractError> {
        match op {
            NumericOp::Set(val) => {
                if val < min {
                    return Err(ContractError::Underflow {
                        field,
                        current,
                        delta_or_target: val,
                        min,
                    });
                }
                if val > max {
                    return Err(ContractError::Overflow {
                        field,
                        current,
                        delta_or_target: val,
                        max,
                    });
                }
                Ok(val)
            }
            NumericOp::Add(delta) => {
                let resulting = current.saturating_add(delta);
                if resulting > max {
                    return Err(ContractError::Overflow {
                        field,
                        current,
                        delta_or_target: delta,
                        max,
                    });
                }
                Ok(resulting)
            }
            NumericOp::Sub(delta) => {
                if delta > current || current - delta < min {
                    return Err(ContractError::Underflow {
                        field,
                        current,
                        delta_or_target: delta,
                        min,
                    });
                }
                Ok(current - delta)
            }
        }
    }

    /// Validates all mutation operations against the character snapshot and game rules.
    pub fn validate_mutation(
        snapshot: &CharacterSnapshot,
        contract: &CharacterMutationContract,
    ) -> Result<CharacterSnapshot, ContractError> {
        let mut result = *snapshot;

        // 1. Level validation (1..=99)
        if let Some(lvl_op) = contract.level {
            let op = match lvl_op {
                NumericOp::Set(v) => NumericOp::Set(v.get() as u64),
                NumericOp::Add(v) => NumericOp::Add(v.get() as u64),
                NumericOp::Sub(v) => NumericOp::Sub(v.get() as u64),
            };
            result.level = Self::evaluate_bound("level", snapshot.level as u64, op, 1, 99)? as u8;
        }

        // 2. Base attributes validation (1..=1023)
        if let Some(str_op) = contract.strength {
            let op = match str_op {
                NumericOp::Set(v) => NumericOp::Set(v.get() as u64),
                NumericOp::Add(v) => NumericOp::Add(v.get() as u64),
                NumericOp::Sub(v) => NumericOp::Sub(v.get() as u64),
            };
            result.strength = Self::evaluate_bound("strength", snapshot.strength as u64, op, 1, 1023)? as u32;
        }

        if let Some(dex_op) = contract.dexterity {
            let op = match dex_op {
                NumericOp::Set(v) => NumericOp::Set(v.get() as u64),
                NumericOp::Add(v) => NumericOp::Add(v.get() as u64),
                NumericOp::Sub(v) => NumericOp::Sub(v.get() as u64),
            };
            result.dexterity = Self::evaluate_bound("dexterity", snapshot.dexterity as u64, op, 1, 1023)? as u32;
        }

        if let Some(vit_op) = contract.vitality {
            let op = match vit_op {
                NumericOp::Set(v) => NumericOp::Set(v.get() as u64),
                NumericOp::Add(v) => NumericOp::Add(v.get() as u64),
                NumericOp::Sub(v) => NumericOp::Sub(v.get() as u64),
            };
            result.vitality = Self::evaluate_bound("vitality", snapshot.vitality as u64, op, 1, 1023)? as u32;
        }

        if let Some(enr_op) = contract.energy {
            let op = match enr_op {
                NumericOp::Set(v) => NumericOp::Set(v.get() as u64),
                NumericOp::Add(v) => NumericOp::Add(v.get() as u64),
                NumericOp::Sub(v) => NumericOp::Sub(v.get() as u64),
            };
            result.energy = Self::evaluate_bound("energy", snapshot.energy as u64, op, 1, 1023)? as u32;
        }

        // 3. Stat points validation (0..=1023)
        if let Some(pts_op) = contract.stat_points {
            let op = match pts_op {
                NumericOp::Set(v) => NumericOp::Set(v.get() as u64),
                NumericOp::Add(v) => NumericOp::Add(v.get() as u64),
                NumericOp::Sub(v) => NumericOp::Sub(v.get() as u64),
            };
            result.stat_points = Self::evaluate_bound("stat_points", snapshot.stat_points as u64, op, 0, 1023)? as u32;
        }

        // 4. Gold stash validation (0..=2,500,000)
        if let Some(stash_op) = contract.gold_stash {
            let op = match stash_op {
                NumericOp::Set(v) => NumericOp::Set(v.get() as u64),
                NumericOp::Add(v) => NumericOp::Add(v.get() as u64),
                NumericOp::Sub(v) => NumericOp::Sub(v.get() as u64),
            };
            result.gold_stash = Self::evaluate_bound("gold_stash", snapshot.gold_stash as u64, op, 0, 2_500_000)? as u32;
        }

        // 5. Character gold validation dynamically bound to resulting level: (level * 10,000)
        if let Some(char_gold_op) = contract.gold_character {
            let op = match char_gold_op {
                NumericOp::Set(v) => NumericOp::Set(v as u64),
                NumericOp::Add(v) => NumericOp::Add(v as u64),
                NumericOp::Sub(v) => NumericOp::Sub(v as u64),
            };
            let char_gold_max = (result.level as u64) * 10_000;
            result.gold_character = Self::evaluate_bound("gold_character", snapshot.gold_character as u64, op, 0, char_gold_max)? as u32;
        }

        // 6. Experience validation (0..=u32::MAX)
        if let Some(exp_op) = contract.experience {
            let op = match exp_op {
                NumericOp::Set(v) => NumericOp::Set(v as u64),
                NumericOp::Add(v) => NumericOp::Add(v as u64),
                NumericOp::Sub(v) => NumericOp::Sub(v as u64),
            };
            result.experience = Self::evaluate_bound("experience", snapshot.experience as u64, op, 0, u32::MAX as u64)? as u32;
        }

        Ok(result)
    }
}

fn convert_numeric_op<T, U, F>(op: NumericOp<T>, converter: F) -> Result<NumericOp<U>, ContractError>
where
    F: Fn(T) -> Result<U, ContractError>,
{
    match op {
        NumericOp::Set(v) => Ok(NumericOp::Set(converter(v)?)),
        NumericOp::Add(v) => Ok(NumericOp::Add(converter(v)?)),
        NumericOp::Sub(v) => Ok(NumericOp::Sub(converter(v)?)),
    }
}

impl TryFrom<CharacterWireDto> for CharacterMutationContract {
    type Error = ContractError;

    /// Parses and converts an external `CharacterWireDto` into an infallible `CharacterMutationContract`.
    fn try_from(dto: CharacterWireDto) -> Result<Self, Self::Error> {
        // Stage 1: Syntactic validation
        dto.validate()?;

        // Stage 2: Value Object conversion ("Parse, don't validate")
        let level = dto.level.map(|op| convert_numeric_op(op, LevelValue::new)).transpose()?;
        let strength = dto.strength.map(|op| convert_numeric_op(op, |v| StatPointValue::new(v, 1023))).transpose()?;
        let dexterity = dto.dexterity.map(|op| convert_numeric_op(op, |v| StatPointValue::new(v, 1023))).transpose()?;
        let vitality = dto.vitality.map(|op| convert_numeric_op(op, |v| StatPointValue::new(v, 1023))).transpose()?;
        let energy = dto.energy.map(|op| convert_numeric_op(op, |v| StatPointValue::new(v, 1023))).transpose()?;
        let stat_points = dto.stat_points.map(|op| convert_numeric_op(op, |v| StatPointValue::new(v, 1023))).transpose()?;
        let gold_stash = dto.gold_stash.map(|op| convert_numeric_op(op, StashGoldValue::new)).transpose()?;

        Ok(Self {
            name: dto.name,
            level,
            experience: dto.experience,
            strength,
            dexterity,
            vitality,
            energy,
            stat_points,
            gold_character: dto.gold_character,
            gold_stash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numeric_op_parsing() {
        assert_eq!("250".parse::<NumericOp<u32>>().unwrap(), NumericOp::Set(250));
        assert_eq!("+10".parse::<NumericOp<u32>>().unwrap(), NumericOp::Add(10));
        assert_eq!("-5".parse::<NumericOp<u32>>().unwrap(), NumericOp::Sub(5));
    }

    #[test]
    fn test_valid_character_wire_dto_with_operators() {
        let dto = CharacterWireDto {
            name: Some("Hero".to_string()),
            level: Some(NumericOp::Set(99)),
            experience: Some(NumericOp::Set(3_520_485_254)),
            strength: Some(NumericOp::Add(50)),
            dexterity: Some(NumericOp::Sub(10)),
            vitality: Some(NumericOp::Set(300)),
            energy: Some(NumericOp::Set(50)),
            stat_points: Some(NumericOp::Set(0)),
            gold_character: Some(NumericOp::Set(990_000)),
            gold_stash: Some(NumericOp::Add(500_000)),
        };

        assert!(dto.validate().is_ok());

        let contract = CharacterMutationContract::try_from(dto).expect("Parsing into contract should succeed");
        assert_eq!(contract.level.unwrap(), NumericOp::Set(LevelValue(99)));
        assert_eq!(contract.strength.unwrap(), NumericOp::Add(StatPointValue(50)));
        assert_eq!(contract.dexterity.unwrap(), NumericOp::Sub(StatPointValue(10)));
    }

    #[test]
    fn test_invalid_level_boundary() {
        let mut dto = CharacterWireDto::default();
        dto.level = Some(NumericOp::Set(0));
        assert!(CharacterMutationContract::try_from(dto).is_err());

        let mut dto2 = CharacterWireDto::default();
        dto2.level = Some(NumericOp::Set(100));
        assert!(CharacterMutationContract::try_from(dto2).is_err());
    }

    #[test]
    fn test_invalid_strength_cap() {
        let mut dto = CharacterWireDto::default();
        dto.strength = Some(NumericOp::Set(1024));
        assert!(CharacterMutationContract::try_from(dto).is_err());
    }

    #[test]
    fn test_boundary_validator_overflow_and_underflow() {
        let snapshot = CharacterSnapshot {
            level: 1,
            experience: 0,
            strength: 1000,
            dexterity: 20,
            vitality: 50,
            energy: 50,
            stat_points: 5,
            gold_character: 5_000,
            gold_stash: 2_400_000,
        };

        // 1. Strength overflow: 1000 + 50 = 1050 > 1023
        let overflow_str_dto = CharacterWireDto {
            strength: Some(NumericOp::Add(50)),
            ..Default::default()
        };
        let contract = CharacterMutationContract::try_from(overflow_str_dto).unwrap();
        let res = CharacterBoundaryValidator::validate_mutation(&snapshot, &contract);
        assert_eq!(
            res.unwrap_err(),
            ContractError::Overflow {
                field: "strength",
                current: 1000,
                delta_or_target: 50,
                max: 1023,
            }
        );

        // 2. Dexterity underflow: 20 - 25 = -5 < 1
        let underflow_dex_dto = CharacterWireDto {
            dexterity: Some(NumericOp::Sub(25)),
            ..Default::default()
        };
        let contract = CharacterMutationContract::try_from(underflow_dex_dto).unwrap();
        let res = CharacterBoundaryValidator::validate_mutation(&snapshot, &contract);
        assert_eq!(
            res.unwrap_err(),
            ContractError::Underflow {
                field: "dexterity",
                current: 20,
                delta_or_target: 25,
                min: 1,
            }
        );

        // 3. Dynamic gold character limit at level 1: max is 1 * 10,000 = 10,000
        // Setting 15,000 should fail with Overflow
        let gold_overflow_dto = CharacterWireDto {
            gold_character: Some(NumericOp::Set(15_000)),
            ..Default::default()
        };
        let contract = CharacterMutationContract::try_from(gold_overflow_dto).unwrap();
        let res = CharacterBoundaryValidator::validate_mutation(&snapshot, &contract);
        assert_eq!(
            res.unwrap_err(),
            ContractError::Overflow {
                field: "gold_character",
                current: 5000,
                delta_or_target: 15_000,
                max: 10_000,
            }
        );

        // 4. Stash gold overflow: 2,400,000 + 200,000 = 2,600,000 > 2,500,000
        let stash_overflow_dto = CharacterWireDto {
            gold_stash: Some(NumericOp::Add(200_000)),
            ..Default::default()
        };
        let contract = CharacterMutationContract::try_from(stash_overflow_dto).unwrap();
        let res = CharacterBoundaryValidator::validate_mutation(&snapshot, &contract);
        assert_eq!(
            res.unwrap_err(),
            ContractError::Overflow {
                field: "gold_stash",
                current: 2_400_000,
                delta_or_target: 200_000,
                max: 2_500_000,
            }
        );
    }
}
