use std::collections::HashMap;
use std::sync::OnceLock;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct AlphaForensics {
    pub version: String,
    pub stats: HashMap<String, StatInfo>,
    pub mappings: HashMap<String, MappingInfo>,
    pub axioms: HashMap<String, u64>,
    pub item_overrides: Option<HashMap<String, HashMap<String, u32>>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MappingInfo {
    pub effective_id: u32,
    pub name: String,
    pub save_bits: Option<u32>,
    pub save_add: Option<i32>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StatInfo {
    pub name: String,
    pub width: u32,
}

static REGISTRY: OnceLock<AlphaForensics> = OnceLock::new();

pub fn get_registry() -> &'static AlphaForensics {
    REGISTRY.get_or_init(|| {
        load_registry().expect("Failed to load Alpha v105 forensic registry")
    })
}

fn load_registry() -> anyhow::Result<AlphaForensics> {
    let base_path = std::env::var("D2R_DATA_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("../d2r-data"));
    
    let registry_path = base_path.join("constants/alpha_v105_forensics.json");
    let content = std::fs::read_to_string(registry_path)?;
    let registry: AlphaForensics = serde_json::from_str(&content)?;
    Ok(registry)
}
