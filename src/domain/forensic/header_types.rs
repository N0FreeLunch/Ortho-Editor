//! Provisional Header scan contracts shared by forensic probes and scanners.
//! These contracts do not establish canonical item identity or ownership.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaScannerGapProfile {
    Extended,
    RhythmOnly,
    WitnessSubset,
    ExtendedExceptMask(u8),
}

pub type ItemHeaderPeek = (u8, u8, u8, String, u32, u8, bool, u64, i8, bool);

impl AlphaScannerGapProfile {
    pub fn label(self) -> String {
        match self {
            Self::Extended => "extended".to_string(),
            Self::RhythmOnly => "rhythm-only".to_string(),
            Self::WitnessSubset => "witness-subset".to_string(),
            Self::ExtendedExceptMask(mask) => format!("extended-except-mask-{mask}"),
        }
    }
}
