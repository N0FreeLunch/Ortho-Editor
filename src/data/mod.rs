#[path = "../../d2r-data/mod.rs"]
pub mod generated;

pub mod bit_cursor;
pub mod repository;
pub mod alignment_oracle;
pub mod mercenary;

#[cfg(test)]
mod alignment_oracle_test;

pub use bit_cursor::BitCursor;
pub use repository::DataRepository;

pub use generated::{
    affixes, item_codes, item_specs, item_types, localization, monsters, property_map,
    runewords, set_items, sets, rare_names, skills, stat_costs, unique_items,
    legitimacy, char_stats, waypoints, quests, runes,
};
