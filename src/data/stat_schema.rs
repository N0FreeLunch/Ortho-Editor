use std::collections::HashMap;
use std::sync::OnceLock;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawStatRule {
    pub id: u16,
    pub name: String,
    pub save_bits: usize,
    pub save_add: i32,
    pub save_param_bits: usize,
    pub signed: bool,
    pub val_shift: usize,
    pub encode: u32,
    pub descfunc: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawStatSchema {
    pub version: String,
    pub chained_stats: HashMap<String, Vec<u16>>,
    pub composite_decoders: HashMap<String, String>,
    pub stats: HashMap<String, RawStatRule>,
}

#[derive(Debug, Clone)]
pub struct StatRule {
    pub id: u16,
    pub name: String,
    pub save_bits: usize,
    pub save_add: i32,
    pub save_param_bits: usize,
    pub signed: bool,
    pub val_shift: usize,
    pub encode: u32,
    pub descfunc: u32,
    pub chain_next: Vec<u16>,
}

#[derive(Debug, Clone)]
pub struct VersionStatSchema {
    pub version: String,
    pub stats: HashMap<u16, StatRule>,
    pub chained_stats: HashMap<u16, Vec<u16>>,
    pub composite_decoders: HashMap<u16, String>,
}

impl VersionStatSchema {
    pub fn from_raw(raw: RawStatSchema) -> Self {
        let mut chained_stats: HashMap<u16, Vec<u16>> = HashMap::new();
        for (k, v) in raw.chained_stats {
            if let Ok(id) = k.parse::<u16>() {
                chained_stats.insert(id, v);
            }
        }

        let mut composite_decoders: HashMap<u16, String> = HashMap::new();
        for (k, v) in raw.composite_decoders {
            if let Ok(id) = k.parse::<u16>() {
                composite_decoders.insert(id, v);
            }
        }

        let mut stats: HashMap<u16, StatRule> = HashMap::new();
        for (k, rule) in raw.stats {
            if let Ok(id) = k.parse::<u16>() {
                let chain = chained_stats.get(&id).cloned().unwrap_or_default();
                stats.insert(
                    id,
                    StatRule {
                        id: rule.id,
                        name: rule.name,
                        save_bits: rule.save_bits,
                        save_add: rule.save_add,
                        save_param_bits: rule.save_param_bits,
                        signed: rule.signed,
                        val_shift: rule.val_shift,
                        encode: rule.encode,
                        descfunc: rule.descfunc,
                        chain_next: chain,
                    },
                );
            }
        }

        Self {
            version: raw.version,
            stats,
            chained_stats,
            composite_decoders,
        }
    }

    pub fn get_rule(&self, stat_id: u16) -> Option<&StatRule> {
        self.stats.get(&stat_id)
    }

    pub fn get_chain_next(&self, stat_id: u16) -> &[u16] {
        self.chained_stats
            .get(&stat_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

pub struct StatSchemaRegistry {
    schemas: HashMap<String, VersionStatSchema>,
}

static REGISTRY: OnceLock<StatSchemaRegistry> = OnceLock::new();

impl StatSchemaRegistry {
    pub fn global() -> &'static StatSchemaRegistry {
        REGISTRY.get_or_init(|| {
            let mut schemas = HashMap::new();

            const RAW_V96: &str = include_str!("../../../d2r-data/constants/stat_schema_v96.json");
            const RAW_V99: &str = include_str!("../../../d2r-data/constants/stat_schema_v99.json");

            if let Ok(raw_v96) = serde_json::from_str::<RawStatSchema>(RAW_V96) {
                schemas.insert("v96".to_string(), VersionStatSchema::from_raw(raw_v96));
            }
            if let Ok(raw_v99) = serde_json::from_str::<RawStatSchema>(RAW_V99) {
                schemas.insert("v99".to_string(), VersionStatSchema::from_raw(raw_v99));
            }

            StatSchemaRegistry { schemas }
        })
    }

    pub fn get(&self, version: &str) -> Option<&VersionStatSchema> {
        self.schemas.get(version)
    }

    pub fn default_v99() -> &'static VersionStatSchema {
        Self::global()
            .get("v99")
            .expect("v99 stat schema must be embedded and valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_v99_stat_schema() {
        let registry = StatSchemaRegistry::global();
        let schema = registry.get("v99").expect("v99 schema should load");
        
        // Test strength (id 0)
        let str_rule = schema.get_rule(0).expect("strength stat should exist");
        assert_eq!(str_rule.name, "strength");
        assert_eq!(str_rule.save_bits, 8);
        assert_eq!(str_rule.save_add, 32);

        // Test firemindam (id 48) chained to firemaxdam (id 49)
        let fire_rule = schema.get_rule(48).expect("firemindam stat should exist");
        assert_eq!(fire_rule.chain_next, vec![49]);

        // Test coldmindam (id 54) chained to [55, 56]
        let cold_rule = schema.get_rule(54).expect("coldmindam stat should exist");
        assert_eq!(cold_rule.chain_next, vec![55, 56]);
    }
}
