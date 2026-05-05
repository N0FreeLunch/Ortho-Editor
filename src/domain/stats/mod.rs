use crate::data::stat_costs::STAT_COSTS;
pub mod entity;
pub mod axiom;
pub mod parser;
pub mod attribute;

pub use entity::{ItemProperty, ItemStats};
pub use attribute::{AttributeSection, AttributeEntry};
pub use axiom::StatsAxiom;
pub use parser::{read_property_list, parse_single_property};
use crate::domain::forensic::registry::{get_registry, MappingInfo};

pub fn lookup_alpha_map_by_raw(raw_id: u32) -> Option<MappingInfo> {
    let reg = get_registry();
    reg.mappings.get(&raw_id.to_string()).cloned()
}

pub fn lookup_alpha_map_by_effective(effective_id: u32) -> Option<MappingInfo> {
    let reg = get_registry();
    reg.mappings.values().find(|m| m.effective_id == effective_id).cloned()
}

pub fn stat_save_bits(stat_id: u32) -> Option<u32> {
    STAT_COSTS
        .iter()
        .find(|stat| stat.id == stat_id)
        .map(|stat| stat.save_bits as u32)
}
