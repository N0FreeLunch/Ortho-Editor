//! Domain contract definitions and wire DTOs for save mutation.

pub mod batch_item;
pub mod character;
pub mod item;
pub mod progression;
pub mod save;
pub mod skill;
pub mod validation;

pub use batch_item::{BatchItemContract, BatchItemWireDto};

pub use character::{
    CharacterBoundaryValidator, CharacterMutationContract, CharacterSnapshot, CharacterWireDto,
    ContractError, LevelValue, NumericOp, StashGoldValue, StatPointValue,
};

pub use item::{
    ItemBoundaryValidator, ItemContractError, ItemMutationContract, ItemStatOpWireDto, ItemTarget,
    ItemWireDto,
};

pub use progression::{
    DifficultyLevel, ProgressionContractError, WaypointMutationContract, WaypointWireDto,
};

pub use save::{
    SaveMutationContract, SaveMutationDto, SaveMutationPipeline, SaveMutationValidator,
};

pub use skill::{
    class_base_skill_id, class_id_to_code, SkillBoundaryValidator, SkillContractError,
    SkillMutationContract, SkillOpWireDto, SkillWireDto,
};

pub use validation::{ConstraintRange, FieldViolation, ValidationErrorResponse};
