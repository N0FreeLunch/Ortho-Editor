//! Item appearance identity and catalog selection, independent of files and HTTP.
use super::{Item, ItemQuality};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ImageQuality {
    Base,
    Unique { id: Option<u32> },
    Set { id: Option<u32> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ImageGraphic {
    Default,
    Indexed { index: u8 },
    Missing,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemImageKey {
    pub code: String,
    pub quality: ImageQuality,
    pub graphic: ImageGraphic,
    pub is_residue: bool,
}

impl From<&Item> for ItemImageKey {
    fn from(item: &Item) -> Self {
        let id = item.unique_id.map(u32::from);
        let quality = match (item.header.is_runeword, item.header.quality) {
            (false, Some(ItemQuality::Unique)) => ImageQuality::Unique { id },
            (false, Some(ItemQuality::Set)) => ImageQuality::Set { id },
            _ => ImageQuality::Base,
        };
        let graphic = if item.has_multiple_graphics {
            item.multi_graphics_bits
                .map_or(ImageGraphic::Missing, |index| ImageGraphic::Indexed {
                    index,
                })
        } else {
            ImageGraphic::Default
        };
        Self {
            code: item.code.trim().to_owned(),
            quality,
            graphic,
            is_residue: item.is_residue(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageRef {
    pub asset_key: String,
    pub invfile: Option<String>,
    pub transform: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageMatchReason {
    BaseCode,
    UniqueId,
    SetId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageFallbackReason {
    MissingQualityId,
    MissingQualityMap,
    MissingVariant,
    HdVariantUnverified,
    HdVariantAssetMissing,
    VariantProvenanceMissing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageUnresolvedReason {
    CatalogUnavailable,
    UnknownCode,
    ParserResidue,
    IdCodeMismatch,
    MissingHdMap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ItemImageBinding {
    Resolved {
        image: ImageRef,
        reason: ImageMatchReason,
    },
    Representative {
        image: ImageRef,
        reason: ImageFallbackReason,
    },
    Unresolved {
        reason: ImageUnresolvedReason,
    },
}

impl ItemImageBinding {
    pub fn image(&self) -> Option<&ImageRef> {
        match self {
            Self::Resolved { image, .. } | Self::Representative { image, .. } => Some(image),
            Self::Unresolved { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImageBase {
    pub family: String,
    pub asset: Option<String>,
    #[serde(rename = "type")]
    pub item_type: Option<String>,
    pub invfile: Option<String>,
    pub uniqueinvfile: Option<String>,
    pub setinvfile: Option<String>,
    pub normcode: Option<String>,
    pub ubercode: Option<String>,
    pub ultracode: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImageQualityRecord {
    pub code: Option<String>,
    pub asset: Option<String>,
    pub invfile: Option<String>,
    pub invtransform: Option<String>,
    pub tiers: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ImageTypeRecord {
    #[serde(default)]
    pub graphics: Vec<Option<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImageFileRecord {
    pub file: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemImageCatalog {
    pub schema: String,
    pub base: BTreeMap<String, ImageBase>,
    #[serde(default)]
    pub unique: BTreeMap<String, ImageQualityRecord>,
    #[serde(default)]
    pub set: BTreeMap<String, ImageQualityRecord>,
    #[serde(default)]
    pub types: BTreeMap<String, ImageTypeRecord>,
    #[serde(default)]
    pub graphics: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default)]
    pub assets: BTreeMap<String, ImageFileRecord>,
}

pub fn safe_image_asset(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_/-".contains(&byte)
        })
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn present(value: Option<&String>) -> Option<&String> {
    value.filter(|value| !value.is_empty())
}

pub fn resolve_item_image(
    key: &ItemImageKey,
    catalog: Option<&ItemImageCatalog>,
) -> ItemImageBinding {
    use ImageFallbackReason as F;
    use ImageUnresolvedReason as U;
    let missing = |reason| ItemImageBinding::Unresolved { reason };
    let Some(catalog) = catalog.filter(|catalog| catalog.schema == "item-image-catalog/v1") else {
        return missing(U::CatalogUnavailable);
    };
    let code = key.code.trim();
    let Some(base) = catalog.base.get(code) else {
        return missing(U::UnknownCode);
    };
    if key.is_residue {
        return missing(U::ParserResidue);
    }
    let mut asset = present(base.asset.as_ref());
    let mut invfile = base.invfile.clone();
    let mut transform = None;
    let mut matched = ImageMatchReason::BaseCode;
    let mut fallback = None;
    let quality = match key.quality {
        ImageQuality::Unique { id } => Some((&catalog.unique, id, true)),
        ImageQuality::Set { id } => Some((&catalog.set, id, false)),
        ImageQuality::Base => None,
    };
    if let Some((table, id, unique)) = quality {
        if let Some(row) = id.and_then(|id| table.get(&id.to_string())) {
            let lineage = [&base.normcode, &base.ubercode, &base.ultracode];
            if row.code.as_deref() != Some(code)
                && !lineage.iter().any(|parent| {
                    parent
                        .as_ref()
                        .is_some_and(|value| Some(value) == row.code.as_ref())
                })
            {
                return missing(U::IdCodeMismatch);
            }
            transform = row.invtransform.clone();
            invfile = present(row.invfile.as_ref())
                .or_else(|| {
                    present(if unique {
                        base.uniqueinvfile.as_ref()
                    } else {
                        base.setinvfile.as_ref()
                    })
                })
                .cloned()
                .or_else(|| base.invfile.clone());
            let tier = if base.ultracode.as_deref() == Some(code) {
                "ultra"
            } else if base.ubercode.as_deref() == Some(code) {
                "uber"
            } else {
                "normal"
            };
            if let Some(selected) = present(row.tiers.as_ref().and_then(|tiers| tiers.get(tier)))
                .or_else(|| present(row.asset.as_ref()))
            {
                asset = Some(selected);
                matched = if unique {
                    ImageMatchReason::UniqueId
                } else {
                    ImageMatchReason::SetId
                };
            } else {
                fallback = Some(F::MissingQualityMap);
            }
        } else {
            fallback = Some(F::MissingQualityId);
        }
    }
    let item_type = base.item_type.as_deref().unwrap_or("");
    let variants = catalog
        .types
        .get(item_type)
        .map(|row| row.graphics.as_slice())
        .unwrap_or(&[]);
    let mut graphic_asset = None;
    match key.graphic {
        ImageGraphic::Indexed { index } => {
            if let Some(variant) = variants
                .get(usize::from(index))
                .and_then(|value| value.as_ref())
                .filter(|value| !value.is_empty())
            {
                invfile = Some(variant.clone());
                fallback = Some(F::HdVariantUnverified);
                if let Some(mapped) = catalog
                    .graphics
                    .get(item_type)
                    .and_then(|rows| rows.get(variant))
                    .filter(|mapped| {
                        safe_image_asset(mapped) && mapped.starts_with(&format!("{}/", base.family))
                    })
                {
                    if catalog.assets.contains_key(mapped) {
                        graphic_asset = Some(mapped);
                    } else {
                        fallback = Some(F::HdVariantAssetMissing);
                    }
                }
            } else {
                fallback = Some(F::MissingVariant);
            }
        }
        ImageGraphic::Missing => fallback = Some(F::MissingVariant),
        ImageGraphic::Unknown if !variants.is_empty() => {
            fallback = Some(F::VariantProvenanceMissing)
        }
        _ => {}
    }
    let Some(asset) = asset.filter(|asset| safe_image_asset(asset)) else {
        return missing(U::MissingHdMap);
    };
    if !safe_image_asset(&base.family) {
        return missing(U::MissingHdMap);
    }
    let image = ImageRef {
        asset_key: graphic_asset
            .cloned()
            .unwrap_or_else(|| format!("{}/{asset}", base.family)),
        invfile,
        transform,
    };
    match fallback {
        Some(reason) => ItemImageBinding::Representative { image, reason },
        None => ItemImageBinding::Resolved {
            image,
            reason: matched,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2r_macros::settled_contract_test;
    use serde_json::json;

    fn catalog() -> ItemImageCatalog {
        serde_json::from_value(json!({"schema":"item-image-catalog/v1","base":{
            "cap":{"family":"armor","asset":"helmet/cap","type":"helm"},
            "rin":{"family":"misc","asset":"ring/ring","type":"ring"}},
            "unique":{"0":{"code":"cap","asset":"helmet/special"}},
            "types":{"ring":{"graphics":["ring1","ring2"]}},
            "graphics":{"ring":{"ring2":"misc/ring2"}},
            "assets":{"misc/ring2":{"file":"placeholder.png"}}}))
        .unwrap()
    }
    fn key(code: &str) -> ItemImageKey {
        ItemImageKey {
            code: code.into(),
            quality: ImageQuality::Base,
            graphic: ImageGraphic::Default,
            is_residue: false,
        }
    }

    #[settled_contract_test]
    fn quality_zero_unknown_residue_and_mismatch_are_distinct() {
        let catalog = catalog();
        let mut input = key("cap ");
        input.quality = ImageQuality::Unique { id: Some(0) };
        assert_eq!(
            resolve_item_image(&input, Some(&catalog))
                .image()
                .unwrap()
                .asset_key,
            "armor/helmet/special"
        );
        input.code = "rin".into();
        assert_eq!(
            resolve_item_image(&input, Some(&catalog)),
            ItemImageBinding::Unresolved {
                reason: ImageUnresolvedReason::IdCodeMismatch
            }
        );
        input = key("missing");
        assert_eq!(
            resolve_item_image(&input, Some(&catalog)),
            ItemImageBinding::Unresolved {
                reason: ImageUnresolvedReason::UnknownCode
            }
        );
        input = key("cap");
        input.is_residue = true;
        assert_eq!(
            resolve_item_image(&input, Some(&catalog)),
            ItemImageBinding::Unresolved {
                reason: ImageUnresolvedReason::ParserResidue
            }
        );
        assert_eq!(
            resolve_item_image(&input, None),
            ItemImageBinding::Unresolved {
                reason: ImageUnresolvedReason::CatalogUnavailable
            }
        );
    }

    #[settled_contract_test]
    fn graphics_preserve_default_missing_and_unknown_provenance() {
        let mut catalog = catalog();
        let mut input = key("rin");
        input.graphic = ImageGraphic::Indexed { index: 1 };
        assert!(matches!(
            resolve_item_image(&input, Some(&catalog)),
            ItemImageBinding::Representative {
                reason: ImageFallbackReason::HdVariantUnverified,
                ..
            }
        ));
        assert_eq!(
            resolve_item_image(&input, Some(&catalog))
                .image()
                .unwrap()
                .asset_key,
            "misc/ring2"
        );
        catalog.assets.clear();
        assert!(matches!(
            resolve_item_image(&input, Some(&catalog)),
            ItemImageBinding::Representative {
                reason: ImageFallbackReason::HdVariantAssetMissing,
                ..
            }
        ));
        input.graphic = ImageGraphic::Missing;
        assert!(matches!(
            resolve_item_image(&input, Some(&catalog)),
            ItemImageBinding::Representative {
                reason: ImageFallbackReason::MissingVariant,
                ..
            }
        ));
        input.graphic = ImageGraphic::Unknown;
        assert!(matches!(
            resolve_item_image(&input, Some(&catalog)),
            ItemImageBinding::Representative {
                reason: ImageFallbackReason::VariantProvenanceMissing,
                ..
            }
        ));
        input.graphic = ImageGraphic::Default;
        assert!(matches!(
            resolve_item_image(&input, Some(&catalog)),
            ItemImageBinding::Resolved { .. }
        ));
    }

    #[settled_contract_test]
    fn unresolved_cannot_deserialize_an_image_or_incomplete_resolved_state() {
        assert!(serde_json::from_value::<ItemImageBinding>(json!({"status":"unresolved","reason":"unknown-code","image":{"asset_key":"misc/x","invfile":null,"transform":null}})).is_err());
        assert!(
            serde_json::from_value::<ItemImageBinding>(
                json!({"status":"resolved","reason":"base-code"})
            )
            .is_err()
        );
        assert!(!safe_image_asset("../secret"));
        assert!(!safe_image_asset("misc//x"));
        assert!(!safe_image_asset("C:/private"));
    }
}
