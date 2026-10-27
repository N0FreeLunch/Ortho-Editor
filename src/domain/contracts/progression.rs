//! Progression and waypoint mutation contracts and DTOs.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use validator::Validate;

/// Game difficulty level (0 = Normal, 1 = Nightmare, 2 = Hell).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DifficultyLevel {
    Normal = 0,
    Nightmare = 1,
    Hell = 2,
}

impl DifficultyLevel {
    pub fn from_u8(val: u8) -> Result<Self, ProgressionContractError> {
        match val {
            0 => Ok(Self::Normal),
            1 => Ok(Self::Nightmare),
            2 => Ok(Self::Hell),
            _ => Err(ProgressionContractError::InvalidDifficulty(val)),
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }

    pub fn all() -> [Self; 3] {
        [Self::Normal, Self::Nightmare, Self::Hell]
    }
}

/// Errors occurring during progression and waypoint contract validation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProgressionContractError {
    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("Invalid difficulty level: {0} (must be 0=Normal, 1=Nightmare, 2=Hell)")]
    InvalidDifficulty(u8),

    #[error("Unknown waypoint name: '{0}'")]
    UnknownWaypoint(String),
}

impl From<validator::ValidationErrors> for ProgressionContractError {
    fn from(err: validator::ValidationErrors) -> Self {
        Self::ValidationFailed(err.to_string())
    }
}

impl From<ProgressionContractError> for super::validation::ValidationErrorResponse {
    fn from(err: ProgressionContractError) -> Self {
        use super::validation::FieldViolation;
        match err {
            ProgressionContractError::ValidationFailed(msg) => {
                Self::single("request", FieldViolation::new("validation_failed", msg, "Ensure the request format satisfies validation rules."))
            }
            ProgressionContractError::InvalidDifficulty(val) => {
                Self::single(
                    "difficulty",
                    FieldViolation::new(
                        "invalid_range",
                        format!("Difficulty level {} is invalid.", val),
                        "Difficulty must be 0 (Normal), 1 (Nightmare), or 2 (Hell).",
                    )
                    .with_rejected_value(val)
                    .with_constraint(0, 2),
                )
            }
            ProgressionContractError::UnknownWaypoint(name) => {
                Self::single(
                    "activate_waypoints",
                    FieldViolation::new(
                        "unknown_target",
                        format!("Waypoint '{}' does not exist in the game registry.", name),
                        "Provide a valid in-game waypoint name (e.g. 'Act 1 - Town', 'Act 1 - Wilderness 2').",
                    )
                    .with_rejected_value(name),
                )
            }
        }
    }
}

/// External Wire DTO for waypoint mutations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct WaypointWireDto {
    /// Target difficulty (0 = Normal, 1 = Nightmare, 2 = Hell). If None, applies to all unlocked/all difficulties.
    pub difficulty: Option<u8>,

    /// If true, unlocks all waypoints in the target difficulty (or all difficulties if difficulty is None).
    pub unlock_all: bool,

    /// Explicit list of waypoint names to activate.
    #[serde(default)]
    pub activate_waypoints: Vec<String>,
}

/// Verified domain contract for waypoint mutations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaypointMutationContract {
    /// Target difficulty, or None for all 3 difficulties.
    pub target_difficulty: Option<DifficultyLevel>,
    /// Unlock all waypoints flag.
    pub unlock_all: bool,
    /// List of verified waypoint names to activate.
    pub activate_waypoints: Vec<String>,
}

impl TryFrom<WaypointWireDto> for WaypointMutationContract {
    type Error = ProgressionContractError;

    fn try_from(dto: WaypointWireDto) -> Result<Self, Self::Error> {
        dto.validate()?;

        let target_difficulty = dto
            .difficulty
            .map(DifficultyLevel::from_u8)
            .transpose()?;

        // Verify that all waypoint names exist in the game registry
        for name in &dto.activate_waypoints {
            let exists = crate::data::waypoints::WAYPOINTS.iter().any(|w| w.name == name);
            if !exists {
                return Err(ProgressionContractError::UnknownWaypoint(name.clone()));
            }
        }

        Ok(Self {
            target_difficulty,
            unlock_all: dto.unlock_all,
            activate_waypoints: dto.activate_waypoints,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_waypoint_wire_dto() {
        let dto = WaypointWireDto {
            difficulty: Some(0),
            unlock_all: false,
            activate_waypoints: vec!["Act 1 - Town".to_string(), "Act 1 - Wilderness 2".to_string()],
        };

        let contract = WaypointMutationContract::try_from(dto).expect("Contract parsing should succeed");
        assert_eq!(contract.target_difficulty, Some(DifficultyLevel::Normal));
        assert_eq!(contract.activate_waypoints.len(), 2);
    }

    #[test]
    fn test_invalid_difficulty() {
        let dto = WaypointWireDto {
            difficulty: Some(3),
            unlock_all: true,
            activate_waypoints: Vec::new(),
        };

        let err = WaypointMutationContract::try_from(dto).unwrap_err();
        assert_eq!(err, ProgressionContractError::InvalidDifficulty(3));
    }

    #[test]
    fn test_unknown_waypoint_name() {
        let dto = WaypointWireDto {
            difficulty: Some(1),
            unlock_all: false,
            activate_waypoints: vec!["Invalid WP Name".to_string()],
        };

        let err = WaypointMutationContract::try_from(dto).unwrap_err();
        assert_eq!(
            err,
            ProgressionContractError::UnknownWaypoint("Invalid WP Name".to_string())
        );
    }
}
