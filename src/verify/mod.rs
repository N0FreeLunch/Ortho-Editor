pub mod alpha_inventory_routing;
pub mod args;
pub mod bit_diff;
pub mod dedup_pool;
pub mod delta;
pub mod desync;
pub mod forensics;
pub mod mutation;
pub mod player_stream_observer;
pub mod save_integrity;
pub mod sba;
pub mod symmetry;
pub mod symmetry_analyzer;
pub mod v2;

pub use dedup_pool::{CachedVerificationReceipt, ItemMemoizationPool, ItemSignature, PoolStats};

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
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub forensic_issues: Vec<forensics::ForensicIssue>,
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
            forensic_issues: Vec::new(),
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

    pub fn with_forensic_issues(mut self, issues: Vec<forensics::ForensicIssue>) -> Self {
        self.forensic_issues = issues;
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

#[cfg(unix)]
fn file_identity(file: &File) -> std::io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(std::io::Error::other(
            "file identity requires a regular file",
        ));
    }
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn file_identity(file: &File) -> std::io::Result<(u64, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other(
            "file identity requires a regular file",
        ));
    }
    let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // The handle is live for this call and Windows initializes the structure on success.
    let success =
        unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, info.as_mut_ptr()) };
    if success == 0 {
        return Err(std::io::Error::last_os_error());
    }
    // GetFileInformationByHandle returned success and initialized every field.
    let info = unsafe { info.assume_init() };
    Ok((
        u64::from(info.dwVolumeSerialNumber),
        (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    ))
}

#[cfg(not(any(unix, windows)))]
fn file_identity(_file: &File) -> std::io::Result<(u64, u64)> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "file identity is unsupported on this platform",
    ))
}

fn create_protected_output(path: &str, inputs: &[&File]) -> std::io::Result<File> {
    let output = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    let output_identity = file_identity(&output)?;
    for input in inputs {
        if file_identity(input)? == output_identity {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "output destination refers to an input file",
            ));
        }
    }
    output.set_len(0)?;
    Ok(output)
}

fn create_automatic_output(
    root: &std::path::Path,
    tool_name: &str,
    timestamp: u64,
) -> std::io::Result<(File, PathBuf)> {
    for suffix in 0..=u64::MAX {
        let name = if suffix == 0 {
            format!("{tool_name}_{timestamp}.txt")
        } else {
            format!("{tool_name}_{timestamp}_{suffix}.txt")
        };
        let path = root.join(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "automatic output names exhausted",
    ))
}

enum DeferredOutput {
    StdoutLine(String),
    StdoutFragment(String),
    Stderr(String),
}

pub struct OutputManager {
    writer: Option<Box<dyn Write>>,
    is_token_efficient: bool,
    is_json: bool,
    deferred: Vec<DeferredOutput>,
}

impl OutputManager {
    pub fn new(tool_name: &str, args: &args::ParsedArgs) -> Self {
        Self::create(tool_name, args, false, None).unwrap_or_else(|_| Self {
            writer: None,
            is_token_efficient: args.is_set("token-efficient"),
            is_json: args.is_json(),
            deferred: Vec::new(),
        })
    }

    pub fn try_new(tool_name: &str, args: &args::ParsedArgs) -> std::io::Result<Self> {
        Self::create(tool_name, args, true, None)
    }

    /// Protects already-open input files from explicit output truncation.
    /// Callers must read these same handles rather than reopening their paths.
    pub fn try_new_protecting(
        tool_name: &str,
        args: &args::ParsedArgs,
        inputs: &[&File],
    ) -> std::io::Result<Self> {
        Self::create(tool_name, args, true, Some(inputs))
    }

    fn create(
        tool_name: &str,
        args: &args::ParsedArgs,
        strict: bool,
        protected_inputs: Option<&[&File]>,
    ) -> std::io::Result<Self> {
        let is_json = args.is_json();
        let is_token_efficient = args.is_set("token-efficient");
        let output_path = args.get("output");

        let mut writer = None;

        if let Some(path) = output_path {
            let result = match protected_inputs {
                Some(inputs) => create_protected_output(path, inputs),
                None => File::create(path),
            };
            match result {
                Ok(f) => writer = Some(Box::new(f) as Box<dyn Write>),
                Err(error) if strict => return Err(error),
                Err(_) => {}
            }
        } else if is_token_efficient {
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

            if let Err(error) = fs::create_dir_all(&root)
                && strict
            {
                return Err(error);
            }

            match create_automatic_output(&root, tool_name, timestamp) {
                Ok((f, path)) => {
                    eprintln!("[TOKEN-EFFICIENT] Detail output path: {}", path.display());
                    writer = Some(Box::new(f) as Box<dyn Write>);
                }
                Err(error) if strict => return Err(error),
                Err(_) => {}
            }
        }

        Ok(Self {
            writer,
            is_token_efficient,
            is_json,
            deferred: Vec::new(),
        })
    }

    fn write_detail(&mut self, text: &str, newline: bool) -> std::io::Result<()> {
        if let Some(writer) = &mut self.writer {
            if newline {
                writeln!(writer, "{}", text)?;
            } else {
                write!(writer, "{}", text)?;
            }
        }
        Ok(())
    }

    pub fn println(&mut self, text: &str) {
        if !self.is_json {
            let _ = self.write_detail(text, true);
        }

        if self.is_json {
            eprintln!("{}", text);
        } else if !self.is_token_efficient {
            println!("{}", text);
        }
    }

    pub fn print(&mut self, text: &str) {
        if !self.is_json {
            let _ = self.write_detail(text, false);
        }

        if self.is_json {
            eprint!("{}", text);
        } else if !self.is_token_efficient {
            print!("{}", text);
        }
    }

    pub fn json(&mut self, text: &str) {
        let _ = self.write_detail(text, true);
        if !self.is_token_efficient {
            println!("{}", text);
        }
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
            let _ = self.write_detail(text, true);
        }
    }

    pub fn try_println(&mut self, text: &str) -> std::io::Result<()> {
        if !self.is_json {
            self.write_detail(text, true)?;
        }
        if self.is_json {
            if self.deferred.is_empty() {
                eprintln!("{}", text);
            } else {
                self.deferred
                    .push(DeferredOutput::Stderr(format!("{text}\n")));
            }
        } else if !self.is_token_efficient {
            println!("{}", text);
        }
        Ok(())
    }

    pub fn try_print(&mut self, text: &str) -> std::io::Result<()> {
        if !self.is_json {
            self.write_detail(text, false)?;
        }
        if self.is_json {
            if self.deferred.is_empty() {
                eprint!("{}", text);
            } else {
                self.deferred.push(DeferredOutput::Stderr(text.to_string()));
            }
        } else if !self.is_token_efficient {
            print!("{}", text);
        }
        Ok(())
    }

    pub fn try_json(&mut self, text: &str) -> std::io::Result<()> {
        self.write_detail(text, true)?;
        if !self.is_token_efficient {
            if self.is_json && self.writer.is_some() {
                // Release reports only after the destination is flushed successfully.
                self.deferred
                    .push(DeferredOutput::StdoutLine(text.to_string()));
            } else {
                println!("{}", text);
            }
        }
        Ok(())
    }

    /// Releases a stdout line after the selected destination is flushed.
    pub fn defer_stdout_line(&mut self, text: &str) {
        if self.writer.is_some() {
            self.deferred
                .push(DeferredOutput::StdoutLine(text.to_string()));
        } else {
            println!("{}", text);
        }
    }

    /// Defers a JSON diagnostic until the selected destination is flushed.
    /// Call `try_finish` after writing the report to release it in order.
    pub fn try_defer_json_summary(&mut self, text: &str) -> std::io::Result<()> {
        if self.is_json && self.writer.is_some() {
            self.deferred
                .push(DeferredOutput::Stderr(format!("{text}\n")));
            Ok(())
        } else {
            self.try_summary(text)
        }
    }

    /// Defers a detail fragment until the selected destination is flushed.
    pub fn try_defer_print(&mut self, text: &str) -> std::io::Result<()> {
        if self.is_json {
            if self.writer.is_some() {
                self.deferred.push(DeferredOutput::Stderr(text.to_string()));
            } else {
                eprint!("{}", text);
            }
        } else {
            self.write_detail(text, false)?;
            if !self.is_token_efficient {
                if self.writer.is_some() {
                    self.deferred
                        .push(DeferredOutput::StdoutFragment(text.to_string()));
                } else {
                    print!("{}", text);
                }
            }
        }
        Ok(())
    }

    /// Defers a detail line until the selected destination is flushed.
    pub fn try_defer_println(&mut self, text: &str) -> std::io::Result<()> {
        if self.is_json {
            return self.try_defer_json_summary(text);
        }
        self.write_detail(text, true)?;
        if !self.is_token_efficient {
            self.defer_stdout_line(text);
        }
        Ok(())
    }

    /// Writes a text summary and defers its display until destination flush succeeds.
    pub fn try_defer_text_summary(&mut self, text: &str) -> std::io::Result<()> {
        if self.is_json {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "text summary requested in JSON mode",
            ));
        }
        self.write_detail(text, true)?;
        let formatted = if self.is_token_efficient {
            format!("[TOKEN-EFFICIENT] {}", text)
        } else {
            text.to_string()
        };
        self.defer_stdout_line(&formatted);
        Ok(())
    }

    pub fn try_summary(&mut self, text: &str) -> std::io::Result<()> {
        if self.is_json {
            if self.deferred.is_empty() {
                eprintln!("{}", text);
            } else {
                self.deferred
                    .push(DeferredOutput::Stderr(format!("{text}\n")));
            }
        } else {
            let formatted = if self.is_token_efficient {
                format!("[TOKEN-EFFICIENT] {}", text)
            } else {
                text.to_string()
            };
            self.write_detail(text, true)?;
            self.try_finish()?;
            println!("{}", formatted);
        }
        Ok(())
    }

    pub fn try_finish(&mut self) -> std::io::Result<()> {
        if let Some(writer) = &mut self.writer {
            writer.flush()?;
        }
        for event in self.deferred.drain(..) {
            match event {
                DeferredOutput::StdoutLine(text) => println!("{}", text),
                DeferredOutput::StdoutFragment(text) => print!("{}", text),
                DeferredOutput::Stderr(text) => eprint!("{}", text),
            }
        }
        Ok(())
    }

    pub fn is_json(&self) -> bool {
        self.is_json
    }

    pub fn is_token_efficient(&self) -> bool {
        self.is_token_efficient
    }
}

#[cfg(test)]
mod output_error_tests {
    use super::{DeferredOutput, OutputManager};
    use std::io::{self, Write};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    #[cfg(any(unix, windows))]
    fn protected_output_uses_held_identity_before_truncation() {
        struct OwnedDir(std::path::PathBuf);
        impl Drop for OwnedDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tmp");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(format!("held-identity-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        let work = OwnedDir(path);
        let original = work.0.join("input");
        let renamed = work.0.join("renamed");
        let output = work.0.join("unrelated");
        std::fs::write(&original, b"original input").unwrap();
        std::fs::write(&output, b"existing output").unwrap();
        let input = std::fs::File::open(&original).unwrap();
        std::fs::rename(&original, &renamed).unwrap();
        std::fs::write(&original, b"replacement input").unwrap();
        let error =
            super::create_protected_output(renamed.to_str().unwrap(), &[&input]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(std::fs::read(&renamed).unwrap(), b"original input");
        assert_eq!(std::fs::read(&original).unwrap(), b"replacement input");
        let mut writer =
            super::create_protected_output(output.to_str().unwrap(), &[&input]).unwrap();
        writer.write_all(b"new report").unwrap();
        writer.flush().unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), b"new report");
        #[cfg(unix)]
        let non_regular = std::fs::File::open(&work.0).unwrap();
        #[cfg(windows)]
        let non_regular = std::fs::File::open("NUL").unwrap();
        assert!(super::create_protected_output(output.to_str().unwrap(), &[&non_regular]).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"new report");
    }

    #[test]
    fn automatic_allocation_preserves_existing_and_concurrent_reports() {
        struct OwnedDir(std::path::PathBuf);
        impl Drop for OwnedDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tmp");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "automatic-allocation-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        let work = OwnedDir(path);
        let (empty, empty_path) = super::create_automatic_output(&work.0, "empty", 42).unwrap();
        drop(empty);
        assert_eq!(empty_path.file_name().unwrap(), "empty_42.txt");
        for name in ["verify_42.txt", "verify_42_1.txt"] {
            std::fs::write(work.0.join(name), b"sentinel").unwrap();
        }
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|index| {
                let root = work.0.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let (mut file, path) =
                        super::create_automatic_output(&root, "verify", 42).unwrap();
                    let bytes = format!("report-{index}");
                    file.write_all(bytes.as_bytes()).unwrap();
                    file.flush().unwrap();
                    (path, bytes)
                })
            })
            .collect();
        let mut paths = std::collections::HashSet::new();
        for handle in handles {
            let (path, expected) = handle.join().unwrap();
            assert!(paths.insert(path.clone()));
            assert_eq!(std::fs::read(path).unwrap(), expected.as_bytes());
        }
        assert_eq!(paths.len(), 8);
        for name in ["verify_42.txt", "verify_42_1.txt"] {
            assert_eq!(std::fs::read(work.0.join(name)).unwrap(), b"sentinel");
        }
        assert!(super::create_automatic_output(&work.0.join("missing"), "verify", 42).is_err());
    }

    #[test]
    fn output_delivery_child() {
        let Ok(mode) = std::env::var("D2R_TEST_OUTPUT_CHILD") else {
            return;
        };
        let (mut output, _, _) = manager(true, mode == "write", mode == "flush");
        output.is_token_efficient = mode == "token";
        if mode == "no_writer" {
            output.writer = None;
        }
        if let Err(error) = output.try_json("REPORT_ONE") {
            eprintln!("OUTPUT_ERROR:{error}");
            return;
        }
        output.try_json("REPORT_TWO").unwrap();
        output.try_print("DIAGNOSTIC_PART").unwrap();
        output.try_println("_LINE").unwrap();
        output.try_summary("DIAGNOSTIC_AFTER_REPORT").unwrap();
        if mode == "no_writer" || mode == "early" {
            return;
        }
        if let Err(error) = output.try_finish() {
            eprintln!("OUTPUT_ERROR:{error}");
            return;
        }
        output.try_finish().unwrap();
    }

    #[test]
    fn checked_json_delivery_is_visible_only_after_successful_finish() {
        use std::process::Command;

        for mode in ["success", "flush", "write", "no_writer", "token", "early"] {
            let run = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "verify::output_error_tests::output_delivery_child",
                    "--nocapture",
                ])
                .env("D2R_TEST_OUTPUT_CHILD", mode)
                .output()
                .unwrap();
            assert!(run.status.success(), "child failed in {mode}: {run:?}");
            let stdout = String::from_utf8(run.stdout).unwrap();
            let stderr = String::from_utf8(run.stderr).unwrap();
            let expected = usize::from(matches!(mode, "success" | "no_writer"));
            assert_eq!(stdout.matches("REPORT_ONE").count(), expected, "{mode}");
            assert_eq!(stdout.matches("REPORT_TWO").count(), expected, "{mode}");
            if expected == 1 {
                assert!(
                    stdout.find("REPORT_ONE") < stdout.find("REPORT_TWO"),
                    "{mode}"
                );
            }
            if mode == "success" || mode == "no_writer" || mode == "token" {
                assert!(
                    stderr.contains("DIAGNOSTIC_PART_LINE\nDIAGNOSTIC_AFTER_REPORT\n"),
                    "{mode}"
                );
            } else {
                assert!(!stderr.contains("DIAGNOSTIC_AFTER_REPORT"), "{mode}");
                assert!(!stderr.contains("DIAGNOSTIC_PART"), "{mode}");
            }
            if mode == "write" || mode == "flush" {
                assert!(stderr.contains("OUTPUT_ERROR:"), "{mode}");
            }
        }
    }

    #[test]
    fn deferred_summary_child() {
        let Ok(mode) = std::env::var("D2R_TEST_DEFERRED_SUMMARY") else {
            return;
        };
        let (mut output, _, _) = manager(
            true,
            mode == "write",
            mode == "flush" || mode == "token_flush",
        );
        output.is_token_efficient = mode == "token" || mode == "token_flush";
        if mode == "no_writer" || mode == "no_writer_early" {
            output.writer = None;
        }
        output
            .try_defer_json_summary("SUMMARY_BEFORE_REPORT")
            .unwrap();
        if mode == "early" || mode == "no_writer_early" {
            return;
        }
        if let Err(error) = output.try_json("JSON_REPORT") {
            eprintln!("OUTPUT_ERROR:{error}");
            return;
        }
        if let Err(error) = output.try_finish() {
            eprintln!("OUTPUT_ERROR:{error}");
        }
    }

    #[test]
    fn deferred_summary_requires_successful_json_delivery() {
        use std::process::Command;

        for mode in [
            "success",
            "flush",
            "write",
            "token",
            "token_flush",
            "no_writer",
            "early",
            "no_writer_early",
        ] {
            let run = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "verify::output_error_tests::deferred_summary_child",
                    "--nocapture",
                ])
                .env("D2R_TEST_DEFERRED_SUMMARY", mode)
                .output()
                .unwrap();
            assert!(run.status.success(), "child failed in {mode}: {run:?}");
            let stdout = String::from_utf8(run.stdout).unwrap();
            let stderr = String::from_utf8(run.stderr).unwrap();
            let summary_expected =
                matches!(mode, "success" | "token" | "no_writer" | "no_writer_early");
            let report_expected = matches!(mode, "success" | "no_writer");
            assert_eq!(
                stderr.contains("SUMMARY_BEFORE_REPORT"),
                summary_expected,
                "{mode}"
            );
            assert_eq!(stdout.contains("JSON_REPORT"), report_expected, "{mode}");
            if matches!(mode, "write" | "flush" | "token_flush") {
                assert!(stderr.contains("OUTPUT_ERROR:"), "{mode}");
            }
        }
    }

    struct ProbeWriter {
        writes: Arc<AtomicUsize>,
        flushes: Arc<AtomicUsize>,
        fail_write: bool,
        fail_flush: bool,
    }

    impl Write for ProbeWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            if self.fail_write {
                Err(io::Error::other("write failed"))
            } else {
                Ok(bytes.len())
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes.fetch_add(1, Ordering::SeqCst);
            if self.fail_flush {
                Err(io::Error::other("flush failed"))
            } else {
                Ok(())
            }
        }
    }

    fn manager(
        json: bool,
        fail_write: bool,
        fail_flush: bool,
    ) -> (OutputManager, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let writes = Arc::new(AtomicUsize::new(0));
        let flushes = Arc::new(AtomicUsize::new(0));
        let writer = ProbeWriter {
            writes: writes.clone(),
            flushes: flushes.clone(),
            fail_write,
            fail_flush,
        };
        (
            OutputManager {
                writer: Some(Box::new(writer)),
                is_token_efficient: true,
                is_json: json,
                deferred: Vec::new(),
            },
            writes,
            flushes,
        )
    }

    #[test]
    fn checked_writes_return_file_errors() {
        for operation in 0..4 {
            let (mut output, writes, _) = manager(operation == 2, true, false);
            let result = match operation {
                0 => output.try_print("text"),
                1 => output.try_println("text"),
                2 => output.try_json("{}"),
                _ => output.try_summary("text"),
            };
            assert_eq!(result.unwrap_err().to_string(), "write failed");
            assert!(writes.load(Ordering::SeqCst) > 0);
        }
    }

    #[test]
    fn checked_finish_returns_flush_error() {
        let (mut output, writes, flushes) = manager(true, false, true);
        output.try_json("{}").expect("write succeeds");
        assert_eq!(output.try_finish().unwrap_err().to_string(), "flush failed");
        assert!(writes.load(Ordering::SeqCst) > 0);
        assert_eq!(flushes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn checked_text_summary_returns_flush_error() {
        let (mut output, _, flushes) = manager(false, false, true);
        assert_eq!(
            output.try_summary("summary").unwrap_err().to_string(),
            "flush failed"
        );
        assert_eq!(flushes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn deferred_stdout_stays_queued_on_delivery_failure() {
        for (fail_write, fail_flush) in [(true, false), (false, true)] {
            let (mut output, _, _) = manager(false, fail_write, fail_flush);
            output.defer_stdout_line("visual generated");
            assert!(output.try_summary("visual generated").is_err());
            assert!(
                matches!(output.deferred.as_slice(), [DeferredOutput::StdoutLine(line)] if line == "visual generated")
            );
        }
        let (mut output, _, _) = manager(false, false, false);
        output.defer_stdout_line("visual generated");
        output.try_summary("visual generated").unwrap();
        assert!(output.deferred.is_empty());
    }

    #[test]
    fn deferred_text_summaries_wait_for_successful_flush() {
        let (mut output, _, _) = manager(false, false, true);
        output.try_defer_text_summary("first").unwrap();
        output.try_defer_text_summary("second").unwrap();
        assert!(
            matches!(output.deferred.as_slice(), [DeferredOutput::StdoutLine(first), DeferredOutput::StdoutLine(second)] if first == "[TOKEN-EFFICIENT] first" && second == "[TOKEN-EFFICIENT] second")
        );
        assert_eq!(output.try_finish().unwrap_err().to_string(), "flush failed");
        assert_eq!(output.deferred.len(), 2);
        let (mut output, _, _) = manager(false, true, false);
        assert_eq!(
            output
                .try_defer_text_summary("first")
                .unwrap_err()
                .to_string(),
            "write failed"
        );
        assert!(output.deferred.is_empty());
        let (mut output, _, _) = manager(true, false, false);
        assert_eq!(
            output.try_defer_text_summary("bad").unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn deferred_detail_lines_preserve_modes_and_errors() {
        let (mut text, _, _) = manager(false, false, true);
        text.is_token_efficient = false;
        text.try_defer_println("first").unwrap();
        assert!(
            matches!(text.deferred.as_slice(), [DeferredOutput::StdoutLine(line)] if line == "first")
        );
        assert!(text.try_finish().is_err());
        assert_eq!(text.deferred.len(), 1);
        let (mut quiet, _, _) = manager(false, false, false);
        quiet.try_defer_println("quiet").unwrap();
        assert!(quiet.deferred.is_empty());
        let (mut json, _, _) = manager(true, false, false);
        json.is_token_efficient = false;
        json.try_defer_println("diagnostic").unwrap();
        json.try_json("{}").unwrap();
        assert!(
            matches!(json.deferred.as_slice(), [DeferredOutput::Stderr(line), DeferredOutput::StdoutLine(report)] if line == "diagnostic\n" && report == "{}")
        );
        let (mut failing, _, _) = manager(false, true, false);
        assert!(failing.try_defer_println("failure").is_err());
        assert!(failing.deferred.is_empty());
    }

    #[test]
    fn deferred_fragments_preserve_order_and_failures() {
        let (mut output, _, _) = manager(false, false, true);
        output.is_token_efficient = false;
        output.try_defer_print("fragment").unwrap();
        output.try_defer_println("line").unwrap();
        assert!(
            matches!(output.deferred.as_slice(), [DeferredOutput::StdoutFragment(a), DeferredOutput::StdoutLine(b)] if a == "fragment" && b == "line")
        );
        assert!(output.try_finish().is_err());
        assert_eq!(output.deferred.len(), 2);
        let (mut failed, _, _) = manager(false, true, false);
        assert!(failed.try_defer_print("fragment").is_err());
        assert!(failed.deferred.is_empty());
        let (mut json, _, _) = manager(true, false, false);
        json.try_defer_print("part").unwrap();
        json.try_defer_println("end").unwrap();
        assert!(
            matches!(json.deferred.as_slice(), [DeferredOutput::Stderr(a), DeferredOutput::Stderr(b)] if a == "part" && b == "end\n")
        );
    }

    #[test]
    fn checked_finish_without_writer_succeeds() {
        let mut output = OutputManager {
            writer: None,
            is_token_efficient: false,
            is_json: false,
            deferred: Vec::new(),
        };
        output.try_finish().expect("no file to flush");
    }

    #[test]
    fn checked_writes_deliver_complete_text() {
        struct Capture(Arc<std::sync::Mutex<Vec<u8>>>);
        impl Write for Capture {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut output = OutputManager {
            writer: Some(Box::new(Capture(bytes.clone()))),
            is_token_efficient: true,
            is_json: false,
            deferred: Vec::new(),
        };
        output.try_print("a").unwrap();
        output.try_println("b").unwrap();
        output.try_json("{}").unwrap();
        output.try_summary("summary").unwrap();
        output.try_finish().unwrap();
        assert_eq!(&*bytes.lock().unwrap(), b"ab\n{}\nsummary\n");
    }

    #[test]
    fn legacy_methods_continue_after_file_errors() {
        let (mut output, writes, _) = manager(false, true, false);
        output.print("first");
        output.println("second");
        output.json("{}");
        output.summary("summary");
        assert!(writes.load(Ordering::SeqCst) >= 4);
    }
}
