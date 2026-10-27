//! Domain contract definitions and wire DTOs for save mutation.

pub mod character;
pub mod progression;
pub mod validation;

pub use character::{
    CharacterBoundaryValidator, CharacterMutationContract, CharacterSnapshot, CharacterWireDto,
    ContractError, LevelValue, NumericOp, StashGoldValue, StatPointValue,
};

pub use progression::{
    DifficultyLevel, ProgressionContractError, WaypointMutationContract, WaypointWireDto,
};

pub use validation::{ConstraintRange, FieldViolation, ValidationErrorResponse};
