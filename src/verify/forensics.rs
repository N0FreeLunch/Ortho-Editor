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

pub fn visualize_bits(
    bytes: &[u8],
    start_bit: usize,
    width: usize,
    version: u32,
    items: &[crate::item::Item],
    jm_markers: &[usize],
    use_colors: bool,
) -> String {
    let mut result = String::new();
    result.push_str(&format!(
        "Visualizing bits from {} to {} (width: {}, version: {})\n",
        start_bit,
        start_bit + width,
        width,
        version
    ));

    let mut last_label = String::new();

    for i in 0..width {
        let bit_idx = start_bit + i;
        let byte_idx = bit_idx / 8;
        let bit_in_byte = bit_idx % 8;

        if byte_idx >= bytes.len() {
            result.push_str("X"); // Out of bounds
            continue;
        }

        let bit = (bytes[byte_idx] >> bit_in_byte) & 1 == 1;

        // Semantic labeling (Recursive search)
        let mut semantic = items.iter().find_map(|it| {
            if let Some(s) = it.query_bit(bit_idx as u64) {
                Some(s.label.clone())
            } else if (bit_idx as u64) >= it.range.start && (bit_idx as u64) < it.range.end {
                Some(format!("Item({})", it.code.trim()))
            } else {
                None
            }
        });

        // Label JM markers if not already in an item range
        if semantic.is_none() {
            for &jm_pos in jm_markers {
                let jm_bit = (jm_pos as u64) * 8;
                if (bit_idx as u64) >= jm_bit && (bit_idx as u64) < jm_bit + 16 {
                    semantic = Some("JM Marker".to_string());
                    break;
                } else if (bit_idx as u64) >= jm_bit + 16 && (bit_idx as u64) < jm_bit + 32 {
                    semantic = Some("Item Count".to_string());
                    break;
                }
            }
        }

        if use_colors {
            if let Some(label) = &semantic {
                // Colorize based on semantic (Simplified)
                if label.contains("JM") {
                    result.push_str("\x1b[91m");
                }
                // Bright Red for JM
                else if label.contains("Stats") {
                    result.push_str("\x1b[93m");
                }
                // Bright Yellow for Stats
                else {
                    result.push_str("\x1b[32m");
                } // Green
            } else if bit {
                result.push_str("\x1b[32m"); // Green for 1
            } else {
                result.push_str("\x1b[34m"); // Blue for 0
            }
        }

        result.push(if bit { '1' } else { '0' });

        if use_colors {
            result.push_str("\x1b[0m");
        }

        if (i + 1) % 8 == 0 {
            result.push(' ');
        }

        if let Some(label) = semantic {
            if label != last_label {
                last_label = label;
            }
        }

        if (i + 1) % 64 == 0 {
            if !last_label.is_empty() {
                result.push_str(&format!(" | {}", last_label));
            }
            result.push('\n');
        }
    }
    result.push('\n');
    result
}

/// Zero-Dependency ANSI Strip Helper
pub fn strip_ansi_codes(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_escape = false;
    for c in input.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c == 'm' || c == 'K' {
                in_escape = false;
            }
        } else {
            output.push(c);
        }
    }
    output
}

/// Bit-state mapping to colored::Color for rhythmic grid / bit heatmap visualization.
pub fn get_bit_color(
    bit_idx: u64,
    items: &[crate::item::Item],
    section_bit_offset: u64,
) -> Option<colored::Color> {
    items.iter().find_map(|it| {
        let rel_start = it.range.start - section_bit_offset;
        let rel_end = it.range.end - section_bit_offset;
        if bit_idx >= rel_start && bit_idx < rel_end {
            if it.is_residue() {
                Some(colored::Color::TrueColor { r: 80, g: 80, b: 80 })
            } else if it
                .modules
                .iter()
                .any(|m| matches!(m, crate::item::ItemModule::SemiOpaque { .. }))
            {
                Some(colored::Color::Yellow)
            } else if it.is_opaque() {
                Some(colored::Color::Red)
            } else {
                Some(colored::Color::Green)
            }
        } else {
            None
        }
    })
}
