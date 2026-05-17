pub mod args;
pub mod bit_diff;
pub mod desync;
pub mod mutation;
pub mod sba;
pub mod save_integrity;
pub mod symmetry;
pub mod v2;
pub mod delta;


pub trait Verifier {
    fn verify(&self, fixture: &[u8], reproduced: &[u8]) -> VerificationReport;
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]

pub struct VerificationReport {
    pub is_success: bool,
    pub issues: Vec<VerificationIssue>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]

pub struct VerificationIssue {
    pub bit_offset: u64,
    pub bit_length: u64,
    pub expected: Vec<u8>,
    pub actual: Vec<u8>,
    pub label: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]

pub struct ForensicContext {
    pub args: Vec<String>,
    pub env: std::collections::HashMap<String, String>,
    pub version: String,
}

impl ForensicContext {
    pub fn current() -> Self {
        let mut env_map = std::collections::HashMap::new();
        for (key, value) in std::env::vars() {
            if key.starts_with("D2R_") || key == "GITHUB_ACTIONS" {
                env_map.insert(key, value);
            }
        }
        Self {
            args: std::env::args().collect(),
            env: env_map,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

impl VerificationReport {
    pub fn success() -> Self {
        Self {
            is_success: true,
            issues: Vec::new(),
        }
    }

    pub fn failure(issues: Vec<VerificationIssue>) -> Self {
        Self {
            is_success: false,
            issues,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FailureCategory {
    Integrity,
    Symmetry,
    Baseline,
    ToolError,
    ShadowMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MismatchFamily {
    ItemCount,
    ItemContent,
    ItemLength,
    Metadata,
    Structural,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ShadowAuditResult {
    pub is_match: bool,
    pub mismatch_count: usize,
    pub mismatch_family: Option<MismatchFamily>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportStatus {
    Ok,
    Fail,
    Warn,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]

pub struct ReportMetadata {
    pub tool: String,
    pub file: String,
    pub version: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]

pub struct ReportIssue {
    pub kind: String,
    pub message: String,
    pub bit_offset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]

pub struct SuggestedAction {
    pub kind: String,
    pub command: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]

pub struct Report<T> {
    pub metadata: ReportMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forensic_context: Option<ForensicContext>,
    pub status: ReportStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_results: Option<T>,
    pub issues: Vec<ReportIssue>,
    pub hints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_audit: Option<ShadowAuditResult>,
    pub suggested_actions: Vec<SuggestedAction>,
}

impl ReportMetadata {
    pub fn new(tool: &str, file: &str, version: &str) -> Self {
        Self {
            tool: tool.to_string(),
            file: file.to_string(),
            version: version.to_string(),
            timestamp: "".to_string(),
        }
    }
}

impl<T> Report<T> {
    pub fn new(metadata: ReportMetadata, status: ReportStatus) -> Self {
        Self {
            metadata,
            forensic_context: None,
            status,
            scan_results: None,
            issues: Vec::new(),
            hints: Vec::new(),
            shadow_audit: None,
            suggested_actions: Vec::new(),
        }
    }

    pub fn with_forensic_context(mut self) -> Self {
        self.forensic_context = Some(ForensicContext::current());
        self
    }

    pub fn with_results(mut self, results: T) -> Self {
        self.scan_results = Some(results);
        self
    }

    pub fn with_issues(mut self, issues: Vec<ReportIssue>) -> Self {
        self.issues = issues;
        self
    }

    pub fn with_hints(mut self, hints: Vec<String>) -> Self {
        self.hints = hints;
        self
    }

    pub fn with_shadow_audit(mut self, shadow: ShadowAuditResult) -> Self {
        self.shadow_audit = Some(shadow);
        self
    }

    pub fn with_actions(mut self, actions: Vec<SuggestedAction>) -> Self {
        self.suggested_actions = actions;
        self
    }
}

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct OutputManager {
    writer: Option<Box<dyn Write>>,
    is_token_efficient: bool,
    is_json: bool,
}

impl OutputManager {
    pub fn new(tool_name: &str, args: &args::ParsedArgs) -> Self {
        let is_json = args.is_json();
        let is_token_efficient = args.is_set("token-efficient");
        let output_path = args.get("output");

        let mut writer = None;

        if is_token_efficient {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            
            let root = std::env::var("AGENT_ARTIFACTS_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    let base = std::env::var("D2R_CORE_PATH")
                        .map(PathBuf::from)
                        .unwrap_or_else(|_| PathBuf::from("."));
                    base.join("agent_artifacts")
                });

            let _ = fs::create_dir_all(&root);
            
            let path = root.join(format!("{}_{}.txt", tool_name, timestamp));
            if let Ok(f) = File::create(&path) {
                eprintln!("[TOKEN-EFFICIENT] Detail saved to: {}", path.display());
                writer = Some(Box::new(f) as Box<dyn Write>);
            }
        } else if let Some(path) = output_path {
            if let Ok(f) = File::create(path) {
                writer = Some(Box::new(f) as Box<dyn Write>);
            }
        }

        Self {
            writer,
            is_token_efficient,
            is_json,
        }
    }

    pub fn println(&mut self, text: &str) {
        if !self.is_json {
            if let Some(w) = &mut self.writer {
                let _ = writeln!(w, "{}", text);
            }
        }
        
        if self.is_json {
            eprintln!("{}", text);
        } else if !self.is_token_efficient {
            println!("{}", text);
        }
    }
    
    pub fn json(&mut self, text: &str) {
        if let Some(w) = &mut self.writer {
            let _ = writeln!(w, "{}", text);
        }
        println!("{}", text);
    }
    
    pub fn summary(&mut self, text: &str) {
        if self.is_json {
            eprintln!("{}", text);
        } else {
            let formatted = if self.is_token_efficient {
                format!("[TOKEN-EFFICIENT] {}", text)
            } else {
                text.to_string()
            };
            println!("{}", formatted);
            if let Some(w) = &mut self.writer {
                let _ = writeln!(w, "{}", text);
            }
        }
    }
    
    pub fn is_json(&self) -> bool {
        self.is_json
    }

    pub fn is_token_efficient(&self) -> bool {
        self.is_token_efficient
    }
}

