use crate::domain::stats::entity::{ItemProperty, ItemStatList};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FusedStatView {
    pub stat_id: u32,
    pub name: String,
    pub param: u32,
    pub aggregated_value: i32,
    pub source_indices: Vec<usize>,
}

pub struct StatFusionMapper;

impl StatFusionMapper {
    /// Fuses/stacks individual raw stat entries sharing the same (stat_id, param) into unified UI display views.
    pub fn fuse(list: &ItemStatList) -> Vec<FusedStatView> {
        let mut grouped: HashMap<(u32, u32), FusedStatView> = HashMap::new();
        let mut order: Vec<(u32, u32)> = Vec::new();

        for (idx, stat) in list.stats.iter().enumerate() {
            let key = (stat.stat_id, stat.param);
            if let Some(entry) = grouped.get_mut(&key) {
                entry.aggregated_value += stat.value;
                entry.source_indices.push(idx);
            } else {
                order.push(key);
                grouped.insert(
                    key,
                    FusedStatView {
                        stat_id: stat.stat_id,
                        name: stat.name.clone(),
                        param: stat.param,
                        aggregated_value: stat.value,
                        source_indices: vec![idx],
                    },
                );
            }
        }

        order.into_iter().filter_map(|k| grouped.remove(&k)).collect()
    }

    /// Projects edited UI display views back into individual raw stat properties losslessly.
    pub fn project_back(views: &[FusedStatView], original_list: &ItemStatList) -> ItemStatList {
        let mut new_stats = original_list.stats.clone();

        for view in views {
            if view.source_indices.is_empty() {
                continue;
            }

            // Calculate existing aggregate
            let mut current_aggregate = 0;
            for &idx in &view.source_indices {
                if idx < new_stats.len() {
                    current_aggregate += new_stats[idx].value;
                }
            }

            let delta = view.aggregated_value - current_aggregate;
            if delta != 0 {
                // Apply delta to the first primary source entry
                let primary_idx = view.source_indices[0];
                if primary_idx < new_stats.len() {
                    new_stats[primary_idx].value += delta;
                    // Also adjust raw_value accordingly
                    new_stats[primary_idx].raw_value += delta;
                }
            }
        }

        ItemStatList { stats: new_stats }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::item::entity::ItemBitRange;

    #[test]
    fn test_stat_fusion_and_bidirectional_projection() {
        let mut list = ItemStatList::new();
        // Base defense +50
        list.stats.push(ItemProperty::new(
            31,
            "item_defense_percent".to_string(),
            0,
            50,
            50,
            ItemBitRange::default(),
        ));
        // Runeword defense +200
        list.stats.push(ItemProperty::new(
            31,
            "item_defense_percent".to_string(),
            0,
            200,
            200,
            ItemBitRange::default(),
        ));
        // All skills +1
        list.stats.push(ItemProperty::new(
            127,
            "item_allskills".to_string(),
            0,
            1,
            1,
            ItemBitRange::default(),
        ));

        // 1. Fuse stats for UI display
        let fused = StatFusionMapper::fuse(&list);
        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0].stat_id, 31);
        assert_eq!(fused[0].aggregated_value, 250);
        assert_eq!(fused[0].source_indices, vec![0, 1]);
        assert_eq!(fused[1].stat_id, 127);
        assert_eq!(fused[1].aggregated_value, 1);

        // 2. Edit UI value (e.g. increase total defense % to 300)
        let mut edited_views = fused.clone();
        edited_views[0].aggregated_value = 300;

        // 3. Project back to raw properties
        let projected = StatFusionMapper::project_back(&edited_views, &list);
        assert_eq!(projected.stats.len(), 3);
        // Primary entry should receive +50 delta (50 -> 100)
        assert_eq!(projected.stats[0].value, 100);
        // Secondary entry unchanged
        assert_eq!(projected.stats[1].value, 200);
        // All skills unchanged
        assert_eq!(projected.stats[2].value, 1);

        // 4. Re-fuse to verify 300 total
        let re_fused = StatFusionMapper::fuse(&projected);
        assert_eq!(re_fused[0].aggregated_value, 300);
    }
}
