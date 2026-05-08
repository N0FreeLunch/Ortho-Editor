use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::symmetry::{calculate_symmetry_diff, SymmetryOptions, ItemDiff};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
enum FailureFamily {
    Geometry,
    RWSet,
    Stat,
    Nudge,
    Unknown,
}

impl FailureFamily {
    fn as_tag(&self) -> String {
        format!("[{}]", match self {
            Self::Geometry => "Geometry",
            Self::RWSet => "RW/Set",
            Self::Stat => "Stat",
            Self::Nudge => "Nudge",
            Self::Unknown => "Unknown",
        })
    }

    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "geometry" => Some(Self::Geometry),
            "rwset" | "rw" | "set" => Some(Self::RWSet),
            "stat" => Some(Self::Stat),
            "nudge" => Some(Self::Nudge),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Serialize)]
struct MismatchRow {
    item_label: String,
    code: String,
    mismatch_type: String,
    segment: String,
    first_mismatch_offset: Option<usize>,
}

#[derive(Serialize)]
struct AuditResult {
    status: String,
    filename: String,
    item_count: usize,
    avg_fidelity: f32,
    hint: String,
    family: Option<FailureFamily>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    mismatch_rows: Vec<MismatchRow>,
}

#[derive(Serialize)]
struct GlobalAuditReport {
    target_dir: String,
    total_files: usize,
    total_pass: usize,
    total_fail: usize,
    total_items: usize,
    global_avg_fidelity: f32,
    failure_breakdown: HashMap<String, usize>,
    results: Vec<AuditResult>,
}

fn classify_failure(diff: &ItemDiff) -> FailureFamily {
    let mismatch_type = diff.mismatch_type.as_deref().unwrap_or("");
    let offset = diff.first_mismatch_offset.unwrap_or(0);
    let version = diff.version;
    let flags = diff.flags;

    // Alpha v105 specific RW/Shadow check (approximation)
    let is_rw_or_shadow = if version == 5 || version == 1 {
        let is_shadow = (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0;
        let is_rw = !is_shadow && ((flags & (1 << 11)) != 0 || (flags & (1 << 12)) != 0 || (flags & (1 << 13)) != 0 || (flags & 0x800) != 0);
        is_rw || is_shadow
    } else {
        (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0
    };

    if mismatch_type == "Length" {
        let diff_bits = (diff.original_len as i64 - diff.target_len as i64).abs();
        if offset < 100 {
            FailureFamily::Geometry
        } else if diff_bits <= 7 {
            FailureFamily::Nudge
        } else {
            FailureFamily::Geometry
        }
    } else if mismatch_type.contains("Gap") {
        FailureFamily::Geometry
    } else if mismatch_type == "Content" {
        if is_rw_or_shadow {
            FailureFamily::RWSet
        } else if offset >= 100 {
            FailureFamily::Stat
        } else {
            FailureFamily::Geometry
        }
    } else {
        FailureFamily::Unknown
    }
}

fn generate_markdown_report(report: &GlobalAuditReport) -> String {
    let mut md = String::new();
    md.push_str(&format!("# Global Item Symmetry Audit: {}\n\n", report.target_dir));
    
    md.push_str("## SUMMARY\n\n");
    md.push_str("| Metric | Value |\n");
    md.push_str("| :--- | :--- |\n");
    md.push_str(&format!("| Total Files | {} |\n", report.total_files));
    md.push_str(&format!("| Total Pass | {} |\n", report.total_pass));
    md.push_str(&format!("| Total Fail | {} |\n", report.total_fail));
    md.push_str(&format!("| Total Items | {} |\n", report.total_items));
    md.push_str(&format!("| Global Fidelity | {:.2}% |\n\n", report.global_avg_fidelity));

    if !report.failure_breakdown.is_empty() {
        md.push_str("## FAILURE BREAKDOWN\n\n");
        md.push_str("| Family | Count |\n");
        md.push_str("| :--- | :--- |\n");
        let mut families: Vec<_> = report.failure_breakdown.keys().collect();
        families.sort();
        for family in families {
            md.push_str(&format!("| {} | {} |\n", family, report.failure_breakdown[family]));
        }
        md.push_str("\n");
    }

    md.push_str("## DETAILED RESULTS\n\n");
    md.push_str("| Status | Filename | Items | Fidelity | Hint |\n");
    md.push_str("| :--- | :--- | :--- | :--- | :--- |\n");
    for res in &report.results {
        md.push_str(&format!(
            "| {} | {} | {} | {:.2}% | {} |\n",
            res.status, res.filename, res.item_count, res.avg_fidelity, res.hint
        ));
    }
    
    md
}

struct Args {
    target_dir: String,
    filter_family: Option<FailureFamily>,
    summary_only: bool,
    detailed: bool,
    output_json: bool,
    output_path: Option<String>,
    output_html: Option<String>,
}

fn process_file(
    args: &Args,
    file_path: &Path,
    failure_breakdown: &mut HashMap<FailureFamily, usize>,
) -> AuditResult {
    let file_name = file_path.file_name().unwrap().to_string_lossy().into_owned();

    let bytes = match fs::read(file_path) {
        Ok(b) => b,
        Err(e) => {
            return AuditResult {
                status: "[ERROR]".to_string(),
                filename: file_name,
                item_count: 0,
                avg_fidelity: 0.0,
                hint: format!("Read error: {}", e),
                family: Some(FailureFamily::Unknown),
                mismatch_rows: Vec::new(),
            };
        }
    };

    let options = SymmetryOptions {
        roundtrip: true,
        target_index: None,
        fail_fast: !args.detailed,
    };

    match calculate_symmetry_diff(&bytes, None, options) {
        Ok(report) => {
            let status = if report.success { "[PASS]" } else { "[FAIL]" };
            let item_count = report.items.len();
            
            let avg_fidelity = if item_count > 0 {
                let sum: f32 = report.items.iter().map(|it| it.fidelity_score).sum();
                sum / item_count as f32
            } else {
                100.0
            };

            let mut mismatch_rows = Vec::new();
            let mut first_fail_family = None;

            let hint = if !report.success {
                if args.detailed {
                    for (i, it) in report.items.iter().enumerate() {
                        if !it.is_match {
                            let family = classify_failure(it);
                            if first_fail_family.is_none() {
                                first_fail_family = Some(family);
                            }
                            *failure_breakdown.entry(family).or_insert(0) += 1;
                            mismatch_rows.push(MismatchRow {
                                item_label: format!("Item {}", i),
                                code: it.code.clone(),
                                mismatch_type: it.mismatch_type.clone().unwrap_or_default(),
                                segment: it.segment.clone().unwrap_or_default(),
                                first_mismatch_offset: it.first_mismatch_offset.map(|o| o as usize),
                            });
                        }
                    }
                    format!("{} failures detected", mismatch_rows.len())
                } else if let Some(first_fail) = report.items.iter().find(|it| !it.is_match) {
                    let family = classify_failure(first_fail);
                    first_fail_family = Some(family);
                    *failure_breakdown.entry(family).or_insert(0) += 1;
                    format!("{} {}", family.as_tag(), first_fail.mismatch_type.as_deref().unwrap_or("Mismatch"))
                } else {
                    "Unknown failure".to_string()
                }
            } else {
                "".to_string()
            };

            AuditResult {
                status: status.to_string(),
                filename: file_name,
                item_count,
                avg_fidelity,
                hint,
                family: first_fail_family,
                mismatch_rows,
            }
        }
        Err(e) => {
            *failure_breakdown.entry(FailureFamily::Unknown).or_insert(0) += 1;
            AuditResult {
                status: "[FAIL]".to_string(),
                filename: file_name,
                item_count: 0,
                avg_fidelity: 0.0,
                hint: format!("Audit error: {}", e),
                family: Some(FailureFamily::Unknown),
                mismatch_rows: Vec::new(),
            }
        }
    }
}

#[derive(Serialize, Default)]
struct DashboardGroup {
    total: usize,
    pass: usize,
    fidelity_sum: f32,
}

#[derive(Serialize, Default)]
struct StabilityDashboard {
    target_dir: String,
    by_act: HashMap<String, DashboardGroup>,
    by_class: HashMap<String, DashboardGroup>,
    global: DashboardGroup,
}

fn extract_metadata(path: &Path) -> (String, String) {
    let path_str = path.to_string_lossy().to_lowercase();
    
    let act = if path_str.contains("act1") {
        "Act 1"
    } else if path_str.contains("act2") {
        "Act 2"
    } else if path_str.contains("act3") {
        "Act 3"
    } else if path_str.contains("act4") {
        "Act 4"
    } else if path_str.contains("act5") {
        "Act 5"
    } else {
        "Unknown"
    };

    let class = if path_str.contains("amazon") || path_str.contains("ama") {
        "Amazon"
    } else if path_str.contains("sorceress") || path_str.contains("sor") {
        "Sorceress"
    } else if path_str.contains("necromancer") || path_str.contains("nec") {
        "Necromancer"
    } else if path_str.contains("paladin") || path_str.contains("pal") {
        "Paladin"
    } else if path_str.contains("barbarian") || path_str.contains("bar") {
        "Barbarian"
    } else if path_str.contains("druid") || path_str.contains("dru") {
        "Druid"
    } else if path_str.contains("assassin") || path_str.contains("asn") {
        "Assassin"
    } else {
        "Unknown"
    };

    (act.to_string(), class.to_string())
}

fn generate_html_report(dashboard: &StabilityDashboard) -> String {
    let mut html = String::new();
    html.push_str("<!DOCTYPE html>
<html lang=\"en\">
<head>
    <meta charset=\"UTF-8\">
    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">
    <title>Alpha v105 Forensic Dashboard</title>
    <style>
        body { font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; background-color: #1a1a1a; color: #e0e0e0; margin: 20px; }
        h1, h2 { color: #4fc3f7; }
        .container { max-width: 1000px; margin: auto; }
        .summary-card { background: #2d2d2d; padding: 20px; border-radius: 8px; margin-bottom: 20px; box-shadow: 0 4px 6px rgba(0,0,0,0.3); }
        .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 20px; }
        table { width: 100%; border-collapse: collapse; background: #2d2d2d; border-radius: 8px; overflow: hidden; }
        th, td { padding: 12px; text-align: left; border-bottom: 1px solid #444; }
        th { background-color: #333; color: #4fc3f7; }
        tr:hover { background-color: #383838; }
        .progress-bar { background: #444; border-radius: 4px; height: 10px; width: 100%; margin-top: 4px; }
        .progress-fill { height: 100%; border-radius: 4px; }
        .high { background-color: #4caf50; }
        .medium { background-color: #ffc107; }
        .low { background-color: #f44336; }
        .stats { font-size: 0.9em; color: #aaa; }
    </style>
</head>
<body>
<div class=\"container\">
    <h1>Alpha v105 Forensic Dashboard</h1>
");

    html.push_str(&format!("
    <div class=\"summary-card\">
        <h2>Global Stability</h2>
        <p>Target Directory: <code>{}</code></p>
        <div class=\"stats\">Total Files: {} | Pass: {} | Success Rate: {:.1}% | Avg Fidelity: {:.2}%</div>
    </div>
", 
        dashboard.target_dir, 
        dashboard.global.total, 
        dashboard.global.pass,
        if dashboard.global.total > 0 { dashboard.global.pass as f32 / dashboard.global.total as f32 * 100.0 } else { 0.0 },
        if dashboard.global.total > 0 { dashboard.global.fidelity_sum / dashboard.global.total as f32 } else { 0.0 }
    ));

    html.push_str("<div class=\"grid\">");

    // Act Stability
    html.push_str("<div><h2>By Act</h2><table><thead><tr><th>Act</th><th>Stability</th><th>Rate</th></tr></thead><tbody>");
    let mut acts: Vec<_> = dashboard.by_act.keys().collect();
    acts.sort();
    for act in acts {
        let group = &dashboard.by_act[act];
        let rate = if group.total > 0 { group.pass as f32 / group.total as f32 * 100.0 } else { 0.0 };
        let fidelity = if group.total > 0 { group.fidelity_sum / group.total as f32 } else { 0.0 };
        let color_class = if fidelity >= 95.0 { "high" } else if fidelity >= 80.0 { "medium" } else { "low" };
        
        html.push_str(&format!("<tr><td>{}</td><td><div class=\"progress-bar\"><div class=\"progress-fill {}\" style=\"width: {:.1}%\"></div></div><div class=\"stats\">Fidelity: {:.1}%</div></td><td>{:.1}%</td></tr>", 
            act, color_class, fidelity, fidelity, rate));
    }
    html.push_str("</tbody></table></div>");

    // Class Stability
    html.push_str("<div><h2>By Class</h2><table><thead><tr><th>Class</th><th>Stability</th><th>Rate</th></tr></thead><tbody>");
    let mut classes: Vec<_> = dashboard.by_class.keys().collect();
    classes.sort();
    for class in classes {
        let group = &dashboard.by_class[class];
        let rate = if group.total > 0 { group.pass as f32 / group.total as f32 * 100.0 } else { 0.0 };
        let fidelity = if group.total > 0 { group.fidelity_sum / group.total as f32 } else { 0.0 };
        let color_class = if fidelity >= 95.0 { "high" } else if fidelity >= 80.0 { "medium" } else { "low" };
        
        html.push_str(&format!("<tr><td>{}</td><td><div class=\"progress-bar\"><div class=\"progress-fill {}\" style=\"width: {:.1}%\"></div></div><div class=\"stats\">Fidelity: {:.1}%</div></td><td>{:.1}%</td></tr>", 
            class, color_class, fidelity, fidelity, rate));
    }
    html.push_str("</tbody></table></div>");

    html.push_str("</div>
</div>
</body>
</html>");
    html
}

fn find_d2s_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    if let Ok(read_dir) = fs::read_dir(dir) {
        for entry in read_dir.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                find_d2s_files(&path, files);
            } else if path.is_file() && path.extension().map_or(false, |ext| ext == "d2s") {
                files.push(path);
            }
        }
    }
}

fn main() {
    let mut parser = ArgParser::new("d2item_global_audit");
    parser.add_spec(ArgSpec::positional("target_dir", "Directory containing .d2s files").optional());
    parser.add_spec(ArgSpec::option("filter", None, Some("filter"), "Filter failures by family (Geometry, RWSet, Stat, Nudge, Unknown)"));
    parser.add_spec(ArgSpec::flag("summary-only", None, Some("summary-only"), "Show only the summary block"));
    parser.add_spec(ArgSpec::flag("detailed", Some('d'), Some("detailed"), "Report all mismatches in a file, not just the first one"));
    parser.add_spec(ArgSpec::flag("json", None, Some("json"), "Output results in JSON format"));
    parser.add_spec(ArgSpec::option("output", Some('o'), Some("output"), "Save execution output to a file"));
    parser.add_spec(ArgSpec::option("html", None, Some("html"), "Save HTML dashboard report to a file"));
    
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

    let args = Args {
        target_dir: parsed
            .get("target_dir")
            .map(|s| s.as_str())
            .unwrap_or("tests/fixtures/savegames/original")
            .to_string(),
        filter_family: parsed.get("filter").and_then(|s| FailureFamily::from_str(s)),
        summary_only: parsed.is_set("summary-only"),
        detailed: parsed.is_set("detailed"),
        output_json: parsed.is_set("json"),
        output_path: parsed.get("output").cloned(),
        output_html: parsed.get("html").cloned(),
    };

    let path = Path::new(&args.target_dir);
    if !path.is_dir() {
        eprintln!("Error: target path '{}' is not a directory.", args.target_dir);
        std::process::exit(1);
    }

    let mut file_paths = Vec::new();
    find_d2s_files(path, &mut file_paths);

    // Deterministic sort by filename
    file_paths.sort();

    if file_paths.is_empty() {
        println!("No .d2s files found in {}", args.target_dir);
        return;
    }

    let mut total_files = 0;
    let mut total_pass = 0;
    let mut total_fail = 0;
    let mut cumulative_fidelity = 0.0;
    let mut total_items = 0;
    let mut failure_breakdown: HashMap<FailureFamily, usize> = HashMap::new();
    let mut results: Vec<AuditResult> = Vec::new();
    let mut dashboard = StabilityDashboard {
        target_dir: args.target_dir.clone(),
        ..Default::default()
    };

    if args.output_path.is_none() && !args.output_json && !args.summary_only {
        println!("Global Item Symmetry Audit: {}", args.target_dir);
        println!("{:-<100}", "");
        println!(
            "{:<8} | {:<40} | {:>8} | {:>10} | {:<20}",
            "Status", "Filename", "Items", "Fidelity", "Hint"
        );
        println!("{:-<100}", "");
    }

    for path in file_paths {
        total_files += 1;
        let res = process_file(&args, &path, &mut failure_breakdown);
        let (act, class) = extract_metadata(&path);

        // Update dashboard
        {
            let is_pass = res.status == "[PASS]";
            let act_group = dashboard.by_act.entry(act).or_default();
            act_group.total += 1;
            if is_pass { act_group.pass += 1; }
            act_group.fidelity_sum += res.avg_fidelity;

            let class_group = dashboard.by_class.entry(class).or_default();
            class_group.total += 1;
            if is_pass { class_group.pass += 1; }
            class_group.fidelity_sum += res.avg_fidelity;

            dashboard.global.total += 1;
            if is_pass { dashboard.global.pass += 1; }
            dashboard.global.fidelity_sum += res.avg_fidelity;
        }

        // Filter logic
        if let Some(f) = args.filter_family {
            if res.status == "[PASS]" || res.family != Some(f) {
                continue;
            }
        }

        if res.status == "[PASS]" {
            total_pass += 1;
        } else {
            total_fail += 1;
        }

        total_items += res.item_count;
        cumulative_fidelity += res.avg_fidelity;

        if args.output_path.is_none() && !args.output_json && !args.summary_only {
            println!(
                "{:<8} | {:<40} | {:>8} | {:>9.2}% | {:<20}",
                res.status, res.filename, res.item_count, res.avg_fidelity, res.hint
            );
        }
        results.push(res);
    }

    let global_avg_fidelity = if total_files > 0 {
        cumulative_fidelity / total_files as f32
    } else {
        0.0
    };

    let mut breakdown_str = HashMap::new();
    for (f, count) in failure_breakdown.iter() {
        breakdown_str.insert(format!("{:?}", f), *count);
    }

    let global_report = GlobalAuditReport {
        target_dir: args.target_dir.clone(),
        total_files,
        total_pass,
        total_fail,
        total_items,
        global_avg_fidelity,
        failure_breakdown: breakdown_str,
        results,
    };

    if let Some(out) = &args.output_html {
        let content = generate_html_report(&dashboard);
        if let Some(parent) = Path::new(out).parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                fs::create_dir_all(parent).expect("Failed to create output directory");
            }
        }
        fs::write(out, content).expect("Failed to write HTML report");
        println!("Dashboard HTML written to: {}", out);
    }

    if let Some(out) = &args.output_path {
        let content = if out.ends_with(".json") || args.output_json {
            serde_json::to_string_pretty(&global_report).unwrap()
        } else {
            generate_markdown_report(&global_report)
        };
        
        if let Some(parent) = Path::new(out).parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                fs::create_dir_all(parent).expect("Failed to create output directory");
            }
        }
        fs::write(out, content).expect("Failed to write output file");
        println!("Report saved to: {}", out);
    } else if args.output_json {
        println!("{}", serde_json::to_string_pretty(&global_report).unwrap());
    } else {
        println!("{:-<100}", "");
        println!("SUMMARY:");
        println!("  Total Files:       {}", total_files);
        println!("  Total Pass:        {}", total_pass);
        println!("  Total Fail:        {}", total_fail);
        println!("  Total Items:       {}", total_items);
        println!("  Global Fidelity:   {:.2}%", global_avg_fidelity);
        
        if !failure_breakdown.is_empty() {
            println!("\nFAILURE BREAKDOWN:");
            let mut families: Vec<_> = failure_breakdown.keys().collect();
            families.sort_by_key(|f| f.as_tag());
            for family in families {
                println!("  {:<12}: {}", family.as_tag(), failure_breakdown[family]);
            }
        }
        println!("{:-<100}", "");
    }

    if total_fail > 0 {
        std::process::exit(1);
    }
}

