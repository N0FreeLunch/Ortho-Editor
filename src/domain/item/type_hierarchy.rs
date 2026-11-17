//! Query source item-type ancestry independently of validation policy.

use crate::data::DataRepository;
use crate::data::item_types::ITEM_TYPES;
use std::collections::HashSet;

pub fn get_all_item_types(item_code: &str) -> Vec<&'static str> {
    let trimmed = item_code.trim();
    let template = match DataRepository::get_item_template(trimmed) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let mut seeds = Vec::new();
    if let Some(t1) = template.item_type {
        seeds.push(t1);
    }
    if let Some(t2) = template.item_type2 {
        seeds.push(t2);
    }

    let mut visited = HashSet::new();
    let mut result = Vec::new();
    let mut current_level = seeds;
    let mut depth = 0;

    while !current_level.is_empty() && depth < 10 {
        let mut next_level = Vec::new();
        for code in current_level {
            if visited.insert(code) {
                result.push(code);
                if let Some(it) = ITEM_TYPES.iter().find(|it| it.code == code) {
                    if let Some(e1) = it.equiv1 {
                        next_level.push(e1);
                    }
                    if let Some(e2) = it.equiv2 {
                        next_level.push(e2);
                    }
                }
            }
        }
        current_level = next_level;
        depth += 1;
    }

    result
}
