//! Domain contracts, wire DTOs, and operations for batch item modifications.
//!
//! Provides mass-convenience capabilities such as full inventory repair,
//! bulk item identification, and mass socket unlocking.

use serde::{Deserialize, Serialize};

/// Inbound wire DTO representing batch item mutation requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BatchItemWireDto {
    /// Restore all damaged items in inventory, body, and stash to maximum durability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair_all: Option<bool>,

    /// Identify all unidentified items across all character inventory containers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identify_all: Option<bool>,

    /// Add sockets to all unsocketed eligible items up to their base max (or capped at count).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket_all: Option<u8>,
}

/// Infallible domain contract for validated batch item operations.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BatchItemContract {
    pub repair_all: bool,
    pub identify_all: bool,
    pub socket_all: Option<u8>,
}

impl From<BatchItemWireDto> for BatchItemContract {
    fn from(dto: BatchItemWireDto) -> Self {
        Self {
            repair_all: dto.repair_all.unwrap_or(false),
            identify_all: dto.identify_all.unwrap_or(false),
            socket_all: dto.socket_all,
        }
    }
}
