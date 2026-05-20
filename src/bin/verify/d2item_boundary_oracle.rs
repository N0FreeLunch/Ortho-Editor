use d2r_core::verify::args::{ArgParser, ArgSpec};
use d2r_core::verify::OutputManager;
use serde::Serialize;
use std::env;
use std::path::PathBuf;

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AnchorKind {
    PreviousJm,
    NextJm,
}

#[derive(Serialize, Debug)]
pub struct Anchor {
    pub kind: AnchorKind,
    pub bit_offset: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resynced: Option<bool>,
}

#[derive(Serialize, Debug)]
pub struct TargetSpan {
    pub start_bit: u64,
    pub end_bit: u64,
    pub policy: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum LocalClosure {
    Passed,
    Failed,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum DownstreamStatus {
    Quarantined,
    Actionable,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    LocalClosed,
    LocalOpen,
    SameBudgetPatchOnly,
    UnsafeNoRightAnchor,
}

#[derive(Serialize, Debug)]
pub struct BoundaryReport {
    pub fixture: PathBuf,
    pub domain: String,
    pub item_index: Option<usize>,
    pub left_anchor: Anchor,
    pub target_span: TargetSpan,
    pub right_anchor: Anchor,
    pub local_closure: LocalClosure,
    pub downstream_status: DownstreamStatus,
    pub verdict: Verdict,
    pub allowed_next_action: String,
    pub hint: String,
}

fn main() {
    let mut parser = ArgParser::new("d2item_boundary_oracle");
    parser.add_spec(ArgSpec::option("fixture", None, Some("fixture"), "Path to save file (d2s)"));
    parser.add_spec(ArgSpec::option("domain", None, Some("domain"), "Domain to isolate (e.g. item.compact, item.stats.huffman)"));
    parser.add_spec(ArgSpec::option("item-index", None, Some("item-index"), "Index of the item in the save").optional());
    
    // Note: --json and --output are handled automatically by OutputManager if present in args
    
    use d2r_core::verify::args::ArgError;
    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            std::process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}", e);
            eprintln!("\n{}", parser.usage());
            std::process::exit(1);
        }
    };

    let mut out = OutputManager::new("d2item_boundary_oracle", &parsed);

    let fixture = PathBuf::from(parsed.get("fixture").unwrap_or(&"dummy.d2s".to_string()));
    let domain = parsed.get("domain").cloned().unwrap_or_else(|| "unknown".to_string());
    let item_index = parsed.get("item-index").and_then(|s| s.parse::<usize>().ok());

    let report = BoundaryReport {
        fixture,
        domain,
        item_index,
        left_anchor: Anchor {
            kind: AnchorKind::PreviousJm,
            bit_offset: 0,
            confidence: Some(1.0),
            resynced: None,
        },
        target_span: TargetSpan {
            start_bit: 0,
            end_bit: 0,
            policy: "same_budget_patch_only".to_string(),
        },
        right_anchor: Anchor {
            kind: AnchorKind::NextJm,
            bit_offset: 0,
            confidence: None,
            resynced: Some(false),
        },
        local_closure: LocalClosure::Failed,
        downstream_status: DownstreamStatus::Quarantined,
        verdict: Verdict::LocalOpen,
        allowed_next_action: "fix_target_span_only".to_string(),
        hint: "Mock output (Skeleton)".to_string(),
    };

    if out.is_json() {
        let json = serde_json::to_string_pretty(&report).unwrap();
        out.json(&json);
    } else {
        out.println(&format!("Boundary Oracle Report for domain: {}", report.domain));
        out.println(&format!("  Verdict: {:?}", report.verdict));
        out.println(&format!("  Hint: {}", report.hint));
        out.println("  (Use --json for detailed bitstream isolation boundaries)");
    }
}
