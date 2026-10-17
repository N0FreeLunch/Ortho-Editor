//! HTTP adapter for the pure item image binding contract.
use d2r_core::domain::item::image::{
    ImageGraphic, ImageQuality, ItemImageBinding, ItemImageCatalog, ItemImageKey,
    resolve_item_image,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_ITEMS: usize = 4096;

#[derive(Debug, Deserialize)]
pub struct ImageProjection {
    code: String,
    quality: Option<String>,
    unique_id: Option<u32>,
    has_multiple_graphics: Option<bool>,
    multi_graphics_bits: Option<u8>,
    parse_status: Option<String>,
    image_key: Option<ItemImageKey>,
}

impl ImageProjection {
    fn key(&self) -> Result<ItemImageKey, String> {
        let quality = match self.quality.as_deref() {
            Some("Unique") => ImageQuality::Unique { id: self.unique_id },
            Some("Set") => ImageQuality::Set { id: self.unique_id },
            _ => ImageQuality::Base,
        };
        let graphic = match self.has_multiple_graphics {
            Some(false) => ImageGraphic::Default,
            Some(true) => self
                .multi_graphics_bits
                .map_or(ImageGraphic::Missing, |index| ImageGraphic::Indexed {
                    index,
                }),
            None => ImageGraphic::Unknown,
        };
        let key = ItemImageKey {
            code: self.code.trim().to_owned(),
            quality,
            graphic,
            is_residue: self.parse_status.as_deref() == Some("residue"),
        };
        if self.image_key.as_ref().is_some_and(|typed| typed != &key) {
            return Err("image_key conflicts with the item projection".to_owned());
        }
        Ok(key)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageDeliveryReason {
    UnresolvedBinding,
    AssetMissing,
    InvalidAssetFile,
    FileMissing,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ImageDelivery {
    Available {
        url: String,
        width: Option<u32>,
        height: Option<u32>,
    },
    Missing {
        reason: ImageDeliveryReason,
    },
}

#[derive(Debug, Serialize)]
pub struct ImageResponse {
    pub key: ItemImageKey,
    pub binding: ItemImageBinding,
    pub delivery: ImageDelivery,
    pub tint_verified: bool,
}

#[derive(Debug, Serialize)]
pub struct ImageBatchResponse {
    pub schema: &'static str,
    pub images: Vec<ImageResponse>,
}

fn image_root() -> Option<PathBuf> {
    let data = PathBuf::from(std::env::var_os("D2R_DATA_PATH")?);
    let root = if data.is_absolute() {
        data
    } else {
        PathBuf::from(std::env::var_os("D2R_SPEC_PATH")?).join(data)
    };
    Some(root.join("assets/item-images"))
}

fn safe_filename(value: &str) -> bool {
    value.len() == 68
        && value.ends_with(".png")
        && value.as_bytes()[..64]
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

fn response(
    key: ItemImageKey,
    catalog: Option<&ItemImageCatalog>,
    root: Option<&Path>,
) -> ImageResponse {
    let binding = resolve_item_image(&key, catalog);
    let missing = |reason| ImageDelivery::Missing { reason };
    let delivery = match binding.image() {
        None => missing(ImageDeliveryReason::UnresolvedBinding),
        Some(image) => match catalog.and_then(|catalog| catalog.assets.get(&image.asset_key)) {
            None => missing(ImageDeliveryReason::AssetMissing),
            Some(file) if !safe_filename(&file.file) => {
                missing(ImageDeliveryReason::InvalidAssetFile)
            }
            Some(file) => {
                let exists = root
                    .and_then(|root| {
                        Some((
                            root.canonicalize().ok()?,
                            root.join(&file.file).canonicalize().ok()?,
                        ))
                    })
                    .is_some_and(|(root, candidate)| {
                        candidate.starts_with(root) && candidate.is_file()
                    });
                if exists {
                    ImageDelivery::Available {
                        url: format!("/item-images/{}", file.file),
                        width: file.width,
                        height: file.height,
                    }
                } else {
                    missing(ImageDeliveryReason::FileMissing)
                }
            }
        },
    };
    ImageResponse {
        key,
        binding,
        delivery,
        tint_verified: false,
    }
}

fn resolve_batch(
    body: &[u8],
    catalog: Option<&ItemImageCatalog>,
    root: Option<&Path>,
) -> Result<ImageBatchResponse, String> {
    let projections: Vec<ImageProjection> = serde_json::from_slice(body)
        .map_err(|_| "Expected an array of valid item projections".to_owned())?;
    if projections.len() > MAX_ITEMS {
        return Err(format!(
            "At most {MAX_ITEMS} item images may be resolved per request"
        ));
    }
    let keys = projections
        .iter()
        .map(ImageProjection::key)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ImageBatchResponse {
        schema: "item-image-bindings/v1",
        images: keys
            .into_iter()
            .map(|key| response(key, catalog, root))
            .collect(),
    })
}

pub fn resolve(body: &[u8]) -> Result<ImageBatchResponse, String> {
    let root = image_root();
    let catalog = root
        .as_ref()
        .and_then(|root| std::fs::read(root.join("catalog.json")).ok())
        .and_then(|bytes| serde_json::from_slice::<ItemImageCatalog>(&bytes).ok());
    resolve_batch(body, catalog.as_ref(), root.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2r_macros::settled_contract_test;
    use serde_json::json;

    #[settled_contract_test]
    fn missing_catalog_and_conflicting_typed_key_have_separate_outcomes() {
        let batch=resolve_batch(br#"[{"code":"rin","quality":"Rare","has_multiple_graphics":true,"multi_graphics_bits":3}]"#,None,None).unwrap();
        let result = serde_json::to_value(batch).unwrap();
        assert_eq!(
            result["images"][0]["key"]["graphic"],
            json!({"kind":"indexed","index":3})
        );
        assert_eq!(
            result["images"][0]["binding"]["reason"],
            "catalog-unavailable"
        );
        assert_eq!(result["images"][0]["delivery"]["status"], "missing");
        assert!(resolve_batch(br#"[{"code":"rin","image_key":{"code":"wrong","quality":{"kind":"base"},"graphic":{"kind":"unknown"},"is_residue":false}}]"#,None,None).is_err());
        assert!(resolve_batch(br#"{"code":"rin"}"#, None, None).is_err());
        assert!(
            resolve_batch(
                &serde_json::to_vec(&vec![json!({"code":"rin"}); MAX_ITEMS + 1]).unwrap(),
                None,
                None
            )
            .is_err()
        );
    }

    #[settled_contract_test]
    fn known_shape_can_have_missing_or_unsafe_delivery() {
        let mut catalog:ItemImageCatalog=serde_json::from_value(json!({"schema":"item-image-catalog/v1","base":{"x":{"family":"misc","asset":"known"}},"assets":{"misc/known":{"file":"../secret.png"}}})).unwrap();
        let request = br#"[{"code":"x"}]"#;
        let result =
            serde_json::to_value(resolve_batch(request, Some(&catalog), None).unwrap()).unwrap();
        assert_eq!(result["images"][0]["binding"]["status"], "resolved");
        assert_eq!(
            result["images"][0]["delivery"]["reason"],
            "invalid-asset-file"
        );
        catalog.assets.get_mut("misc/known").unwrap().file = format!("{}.png", "a".repeat(64));
        let result =
            serde_json::to_value(resolve_batch(request, Some(&catalog), None).unwrap()).unwrap();
        assert_eq!(result["images"][0]["delivery"]["reason"], "file-missing");
        assert!(result["images"][0]["delivery"].get("url").is_none());
    }
}
