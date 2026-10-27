use crate::data::stat_costs::STAT_COSTS;
pub mod entity;
pub mod axiom;
mod budget;
mod property_value;
pub(crate) mod property_normalize;
mod retail_entry;
mod property_read;
pub(crate) mod property_list;
mod retail_list;
pub(crate) use retail_list::read_retail_property_list;
mod property_list_read;
pub(crate) mod property_entry;
pub mod parser;
mod serialization;
pub(crate) mod property_write;
mod retail_write;
pub mod attribute;
pub mod tiling_solver;
pub mod fusion_mapper;

pub use entity::{ItemProperty, ItemStats, ItemStatList, StatMaturityTier};
pub use attribute::{AttributeSection, AttributeEntry};
pub use axiom::StatsAxiom;
pub use parser::{read_property_list, parse_single_property, PropertyReaderContext};
pub use serialization::write_property_list;
pub use tiling_solver::BitSpaceTilingSolver;
pub use fusion_mapper::{StatFusionMapper, FusedStatView};
use crate::domain::forensic::registry::{get_registry, MappingInfo};

pub fn lookup_alpha_map_by_raw(raw_id: u32) -> Option<MappingInfo> {
    let reg = get_registry();
    reg.mappings.get(&raw_id.to_string()).cloned()
}

pub fn lookup_alpha_map_by_effective(effective_id: u32) -> Option<MappingInfo> {
    let reg = get_registry();
    reg.mappings.values().find(|m| m.effective_id == effective_id).cloned()
}

pub fn stat_save_add(stat_id: u32) -> Option<i32> {
    STAT_COSTS
        .iter()
        .find(|stat| stat.id == stat_id)
        .map(|stat| stat.save_add)
}

pub fn stat_save_bits(stat_id: u32) -> Option<u32> {
    STAT_COSTS
        .iter()
        .find(|stat| stat.id == stat_id)
        .map(|stat| stat.save_bits as u32)
}
