//! Expected extended stats start coordinate producer and geometry domain boundary.
//!
//! This module defines pure domain types for estimating the bit boundary of the
//! `ExtendedStats` section based on header geometry parameters.

use super::Item;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum HeaderGeometryFamily {
    /// Standard witness layout verified against the Alpha v105 hp1 fixture.
    StandardHp1,
    /// Layout family that has not been admitted or verified.
    Unadmitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CoordinateConfidence {
    /// Calculation is grounded in a verified fixture witness layout.
    FixtureVerified,
    /// Calculation outcome carries uncertainty or fallback status.
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExpectedExtendedStatsStart {
    /// Absolute starting bit position of the item within the save file stream.
    pub item_start_absolute: u64,
    /// Expected local offset (in bits) from the item start to extended stats.
    pub item_local_offset: u64,
    /// Calculated absolute bit position where extended stats are expected to start.
    pub absolute_offset: u64,
    /// The geometry family used for this calculation.
    pub family: HeaderGeometryFamily,
    /// Confidence level of the coordinate calculation.
    pub confidence: CoordinateConfidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeometryBoundaryError {
    /// The requested header geometry family is not admitted for expected calculation.
    UnadmittedFamily(HeaderGeometryFamily),
}

impl std::fmt::Display for GeometryBoundaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnadmittedFamily(family) => {
                write!(f, "Header geometry family {:?} is not admitted", family)
            }
        }
    }
}

impl std::error::Error for GeometryBoundaryError {}

/// Producer for computing expected extended stats start coordinates.
pub struct ExpectedExtendedStatsStartProducer;

impl ExpectedExtendedStatsStartProducer {
    /// Computes the expected starting coordinate of the extended stats section.
    ///
    /// The calculation is pure and depends only on explicit input facts rather than reading
    /// an active parser cursor or observed trace segment.
    pub fn compute_expected_start(
        family: HeaderGeometryFamily,
        item_start_absolute: u64,
        header_consumed_bits: u64,
        code_consumed_bits: u64,
    ) -> Result<ExpectedExtendedStatsStart, GeometryBoundaryError> {
        match family {
            HeaderGeometryFamily::StandardHp1 => {
                let item_local_offset = header_consumed_bits + code_consumed_bits;
                let absolute_offset = item_start_absolute + item_local_offset;

                Ok(ExpectedExtendedStatsStart {
                    item_start_absolute,
                    item_local_offset,
                    absolute_offset,
                    family,
                    confidence: CoordinateConfidence::FixtureVerified,
                })
            }
            HeaderGeometryFamily::Unadmitted => {
                Err(GeometryBoundaryError::UnadmittedFamily(family))
            }
        }
    }
}

/// Pure classifier for estimating the header geometry family of a live item.
pub struct LiveHeaderFamilyClassifier;

impl LiveHeaderFamilyClassifier {
    /// Classifies an Item into its HeaderGeometryFamily based on header facts.
    pub fn classify(item: &Item) -> Result<HeaderGeometryFamily, GeometryBoundaryError> {
        if item.header.is_compact && item.header.save_is_alpha && item.code.trim() == "hp1" {
            Ok(HeaderGeometryFamily::StandardHp1)
        } else {
            Err(GeometryBoundaryError::UnadmittedFamily(
                HeaderGeometryFamily::Unadmitted,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expected_geometry_standard_hp1_calculation() {
        let result = ExpectedExtendedStatsStartProducer::compute_expected_start(
            HeaderGeometryFamily::StandardHp1,
            1000,
            120,
            24,
        )
        .expect("StandardHp1 family must be admitted and successfully calculated");

        assert_eq!(result.item_start_absolute, 1000);
        assert_eq!(result.item_local_offset, 144);
        assert_eq!(result.absolute_offset, 1144);
        assert_eq!(result.family, HeaderGeometryFamily::StandardHp1);
        assert_eq!(result.confidence, CoordinateConfidence::FixtureVerified);
    }

    #[test]
    fn test_expected_geometry_unadmitted_family_rejection() {
        let err = ExpectedExtendedStatsStartProducer::compute_expected_start(
            HeaderGeometryFamily::Unadmitted,
            1000,
            120,
            24,
        )
        .expect_err("Unadmitted family must return a typed rejection error");

        assert_eq!(
            err,
            GeometryBoundaryError::UnadmittedFamily(HeaderGeometryFamily::Unadmitted)
        );
    }

    #[test]
    fn test_live_header_family_classifier_admits_standard_hp1() {
        let mut item = Item::empty_for_tests();
        item.code = " hp1 ".to_string();
        item.header.is_compact = true;
        item.header.save_is_alpha = true;

        assert_eq!(
            LiveHeaderFamilyClassifier::classify(&item),
            Ok(HeaderGeometryFamily::StandardHp1)
        );
    }

    #[test]
    fn test_live_header_family_classifier_rejects_unadmitted_family() {
        let mut item = Item::empty_for_tests();
        item.code = "tsc".to_string();
        item.header.is_compact = true;
        item.header.save_is_alpha = true;

        assert_eq!(
            LiveHeaderFamilyClassifier::classify(&item),
            Err(GeometryBoundaryError::UnadmittedFamily(
                HeaderGeometryFamily::Unadmitted,
            ))
        );
    }
}
