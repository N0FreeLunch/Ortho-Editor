use serde::{Deserialize, Serialize};
use std::env;
use crate::verify::SuggestedAction;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForensicIssue {
    pub kind: String,
    pub message: String,
    pub bit_offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prescription: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dna_class: Option<String>,
}

impl ForensicIssue {
    pub fn new(kind: &str, message: &str) -> Self {
        Self {
            kind: kind.to_string(),
            message: message.to_string(),
            bit_offset: None,
            context_window: None,
            prescription: None,
            dna_class: None,
        }
    }

    pub fn with_offset(mut self, offset: u64) -> Self {
        self.bit_offset = Some(offset);
        self
    }

    pub fn with_context(mut self, context: &str) -> Self {
        self.context_window = Some(context.to_string());
        self
    }

    pub fn with_prescription(mut self, prescription: &str) -> Self {
        self.prescription = Some(prescription.to_string());
        self
    }

    pub fn with_dna(mut self, dna_class: &str) -> Self {
        self.dna_class = Some(dna_class.to_string());
        self
    }
}

pub fn should_trace() -> bool {
    env::var("D2R_TRACE").is_ok()
}

#[derive(Debug, Clone)]
pub struct DnaDiagnosis {
    pub rupture_field: String,
    pub dna_class: String,
    pub prescription: String,
}

pub fn extract_dna_diagnosis(message: &str) -> Option<DnaDiagnosis> {
    let marker = " | DNA Diagnosis: Rupture Point: '";
    let start = message.find(marker)? + marker.len();
    let after_rupture = &message[start..];
    let rupture_end = after_rupture.find("', DNA Class: '")?;
    let rupture_field = after_rupture[..rupture_end].to_string();

    let after_class = &after_rupture[rupture_end + "', DNA Class: '".len()..];
    let class_end = after_class.find("', Prescription: ")?;
    let dna_class = after_class[..class_end].to_string();
    let prescription = after_class[class_end + "', Prescription: ".len()..]
        .trim()
        .to_string();

    Some(DnaDiagnosis {
        rupture_field,
        dna_class,
        prescription,
    })
}

pub fn build_self_heal_action(bit_offset: Option<u64>, diagnosis: &DnaDiagnosis) -> SuggestedAction {
    let bit_offset_val = bit_offset.unwrap_or(0);
    let command = match diagnosis.dna_class.as_str() {
        "stat_alignment_drift" => format!("d2item_desync_detector --bit-offset {}", bit_offset_val),
        "compact_geometry_shift" => format!("d2item_alignment_oracle --bit-offset {}", bit_offset_val),
        _ => format!("d2save_verify --dump-bits {} 128", bit_offset_val),
    };

    SuggestedAction {
        kind: format!("self_heal_{}", diagnosis.dna_class),
        command: format!(
            "{}  # {} | rupture_field={}",
            command, diagnosis.prescription, diagnosis.rupture_field
        ),
        confidence: 0.95,
    }
}
