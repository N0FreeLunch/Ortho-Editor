pub mod args;
pub mod bit_diff;
pub mod desync;
pub mod mutation;
pub mod sba;
pub mod save_integrity;
pub mod symmetry;

pub trait Verifier {
    fn verify(&self, fixture: &[u8], reproduced: &[u8]) -> VerificationReport;
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct VerificationReport {
    pub is_success: bool,
    pub issues: Vec<VerificationIssue>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct VerificationIssue {
    pub bit_offset: u64,
    pub bit_length: u64,
    pub expected: Vec<u8>,
    pub actual: Vec<u8>,
    pub label: Option<String>,
    pub message: String,
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
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportStatus {
    Ok,
    Fail,
    Warn,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ReportMetadata {
    pub tool: String,
    pub file: String,
    pub version: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ReportIssue {
    pub kind: String,
    pub message: String,
    pub bit_offset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Report<T> {
    pub metadata: ReportMetadata,
    pub status: ReportStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_results: Option<T>,
    pub issues: Vec<ReportIssue>,
    pub hints: Vec<String>,
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
            status,
            scan_results: None,
            issues: Vec::new(),
            hints: Vec::new(),
        }
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
}

use std::fs::{self, File};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct OutputManager {
    writer: Option<Box<dyn Write>>,
    is_antigravity: bool,
    is_json: bool,
}

impl OutputManager {
    pub fn new(tool_name: &str, args: &args::ParsedArgs) -> Self {
        let is_json = args.is_json();
        let is_antigravity = args.is_set("antigravity");
        let output_path = args.get("output");

        let mut writer = None;

        if is_antigravity {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            
            let _ = fs::create_dir_all("antigravity/outputs");
            
            let path = format!("antigravity/outputs/{}_{}.txt", tool_name, timestamp);
            if let Ok(f) = File::create(&path) {
                println!("[ANTIGRAVITY] Log saved to: {}", path);
                writer = Some(Box::new(f) as Box<dyn Write>);
            }
        } else if let Some(path) = output_path {
            if let Ok(f) = File::create(path) {
                writer = Some(Box::new(f) as Box<dyn Write>);
            }
        }

        Self {
            writer,
            is_antigravity,
            is_json,
        }
    }

    pub fn println(&mut self, text: &str) {
        if let Some(w) = &mut self.writer {
            let _ = writeln!(w, "{}", text);
        }
        
        if !self.is_antigravity || self.is_json {
            println!("{}", text);
        }
    }
    
    pub fn summary(&mut self, text: &str) {
        println!("{}", text);
        if let Some(w) = &mut self.writer {
            let _ = writeln!(w, "{}", text);
        }
    }
    
    pub fn is_json(&self) -> bool {
        self.is_json
    }
}
