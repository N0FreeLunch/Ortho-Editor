use d2r_core::domain::forensic::registry::get_registry;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;

#[derive(Debug, Deserialize)]
struct MismatchRow {
    item_label: String,
    #[serde(rename = "code")]
    _code: String,
    mismatch_type: String,
    segment: String,
    first_mismatch_offset: Option<usize>,
    #[serde(default)]
    original_len: Option<usize>,
    #[serde(default)]
    target_len: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct AuditFileResult {
    filename: String,
    #[serde(default)]
    mismatch_rows: Vec<MismatchRow>,
}

#[derive(Debug, Deserialize)]
struct AuditReport {
    results: Vec<AuditFileResult>,
}

#[derive(Debug, Serialize)]
struct Recommendation {
    stat_id: u32,
    occurrences: usize,
    current_context: Context,
    recommended_action: String,
    confidence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    recommended_width: Option<u32>,
    evidence_examples: Vec<Evidence>,
}

#[derive(Debug, Serialize)]
struct Context {
    width: Option<u32>,
    save_bits: Option<u32>,
    mapping_name: String,
}

#[derive(Debug, Serialize)]
struct Evidence {
    file: String,
    item: String,
    mismatch_type: String,
    offset: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    original_len: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_len: Option<usize>,
}

#[derive(Debug, Serialize)]
struct RecommenderOutput {
    recommendations: Vec<Recommendation>,
    unresolved_count: usize,
}

fn extract_stat_id(segment: &str, re: &Regex) -> Option<u32> {
    re.captures(segment)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse().ok())
}

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2item_registry_recommender");
    parser.add_spec(ArgSpec::option(
        "input",
        Some('i'),
        Some("input"),
        "Path to audit report JSON",
    ));
    parser.add_spec(ArgSpec::option(
        "top",
        None,
        Some("top"),
        "Top N recommendations to show (default: 10)",
    ));
    parser.add_spec(ArgSpec::option(
        "output",
        Some('o'),
        Some("output"),
        "Optional path to write recommendations JSON",
    ));

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}", e);
            eprintln!("\n{}", parser.usage());
            std::process::exit(1);
        }
    };

    let input_path = parsed
        .get("input")
        .ok_or_else(|| anyhow::anyhow!("--input is required"))?;
    let top_n: usize = parsed.get("top").and_then(|s| s.parse().ok()).unwrap_or(10);
    let output_path = parsed.get("output");

    let content = fs::read_to_string(input_path)?;
    let report: AuditReport = serde_json::from_str(&content)?;
    let registry = get_registry();

    let re_stat = Regex::new(r"Stat\((\d+)\)")?;
    let mut stat_mismatches: HashMap<u32, Vec<(String, MismatchRow)>> = HashMap::new();
    let mut unresolved_count = 0;

    for file_res in report.results {
        for row in file_res.mismatch_rows {
            if let Some(stat_id) = extract_stat_id(&row.segment, &re_stat) {
                stat_mismatches
                    .entry(stat_id)
                    .or_default()
                    .push((file_res.filename.clone(), row));
            } else {
                unresolved_count += 1;
            }
        }
    }

    let mut recommendations = Vec::new();
    let mut sorted_stats: Vec<_> = stat_mismatches.keys().cloned().collect();
    sorted_stats.sort_by_key(|id| std::cmp::Reverse(stat_mismatches[id].len()));

    for stat_id in sorted_stats {
        let rows = &stat_mismatches[&stat_id];
        let count = rows.len();
        let stat_str = stat_id.to_string();

        let reg_stat = registry.stats.get(&stat_str);
        let reg_mapping = registry.mappings.get(&stat_str);

        let context = Context {
            width: reg_stat.map(|s| s.width),
            save_bits: reg_mapping.and_then(|m| m.save_bits),
            mapping_name: reg_mapping
                .map(|m| m.name.clone())
                .or_else(|| reg_stat.map(|s| s.name.clone()))
                .unwrap_or_else(|| "unknown".to_string()),
        };

        let mut recommended_width = None;
        let (action, confidence) = if reg_stat.is_none() && reg_mapping.is_none() {
            ("ADD_TO_REGISTRY", "HIGH")
        } else if reg_mapping.is_some() && reg_mapping.unwrap().save_bits.is_none() {
            ("DEFINE_SAVE_BITS", "HIGH")
        } else {
            let is_length_mismatch = rows.iter().all(|(_, r)| r.mismatch_type == "Length");
            if is_length_mismatch {
                if let Some(curr_bits) = context.save_bits {
                    let mut consistent_delta = None;
                    let mut all_have_lens = true;
                    for (_, r) in rows {
                        if let (Some(orig), Some(targ)) = (r.original_len, r.target_len) {
                            let delta = orig as i32 - targ as i32;
                            if let Some(prev) = consistent_delta {
                                if prev != delta {
                                    all_have_lens = false;
                                    break;
                                }
                            } else {
                                consistent_delta = Some(delta);
                            }
                        } else {
                            all_have_lens = false;
                            break;
                        }
                    }
                    if all_have_lens {
                        if let Some(delta) = consistent_delta {
                            let proposed = curr_bits as i32 + delta;
                            if proposed > 0 && proposed <= 32 {
                                match stat_id {
                                    // No representative family is proven yet.
                                    _ => {
                                        recommended_width = None;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            ("INSPECT_WIDTH_OR_PARSER", "MEDIUM")
        };

        recommendations.push(Recommendation {
            stat_id,
            occurrences: count,
            current_context: context,
            recommended_action: action.to_string(),
            confidence: confidence.to_string(),
            recommended_width,
            evidence_examples: rows
                .iter()
                .take(3)
                .map(|(fname, r)| Evidence {
                    file: fname.clone(),
                    item: r.item_label.clone(),
                    mismatch_type: r.mismatch_type.clone(),
                    offset: r.first_mismatch_offset,
                    original_len: r.original_len,
                    target_len: r.target_len,
                })
                .collect(),
        });
    }

    // Terminal output
    println!(
        "\n=== Alpha v105 Registry Hardening Recommendations (Top {}) ===",
        top_n
    );
    println!(
        "{:<8} | {:<6} | {:<15} | {:<25} | {:<4}",
        "Stat ID", "Occur", "Current (bits)", "Recommended Action", "Conf"
    );
    println!("{:-<75}", "");

    for rec in recommendations.iter().take(top_n) {
        let ctx = &rec.current_context;
        let bits_info = format!(
            "b:{}/w:{}",
            ctx.save_bits
                .map(|b| b.to_string())
                .unwrap_or_else(|| "-".to_string()),
            ctx.width
                .map(|w| w.to_string())
                .unwrap_or_else(|| "-".to_string())
        );
        println!(
            "{:<8} | {:<6} | {:<15} | {:<25} | {:<4}",
            rec.stat_id, rec.occurrences, bits_info, rec.recommended_action, rec.confidence
        );
    }
    println!("\nUnresolved rows: {}", unresolved_count);

    if let Some(out) = output_path {
        let output_data = RecommenderOutput {
            recommendations,
            unresolved_count,
        };
        let json = serde_json::to_string_pretty(&output_data)?;
        fs::write(out, json)?;
        println!("\nFull recommendations written to: {}", out);
    }

    Ok(())
}
