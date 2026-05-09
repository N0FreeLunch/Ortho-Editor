use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::verify::symmetry::{calculate_symmetry_diff, ItemDiff};
use std::env;
use std::fs;

fn main() {
    let mut parser = ArgParser::new("d2item_serialization_audit");
    parser.add_spec(ArgSpec::positional("save_file", "Path to save file"));
    parser.add_spec(ArgSpec::flag(
        "json",
        None,
        Some("json"),
        "Output results in JSON format",
    ));
    parser.add_spec(ArgSpec::flag(
        "diff-visual",
        None,
        Some("diff-visual"),
        "Show visual bitstream alignment for mismatches",
    ));
    parser.add_spec(ArgSpec::option(
        "visual",
        None,
        Some("visual"),
        "Generate a high-resolution SVG/HTML bit-diff report",
    ));
    parser.add_spec(ArgSpec::option(
        "target",
        Some('t'),
        Some("target"),
        "Only audit a specific item index",
    ));

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

    let path = parsed.get("save_file").unwrap();
    let use_json = parsed.is_set("json");
    let use_visual = parsed.is_set("diff-visual");
    let visual_out = parsed.get("visual").cloned();
    let target_idx = parsed
        .get("target")
        .and_then(|s| s.parse::<usize>().ok());
    let fail_fast = parsed.is_set("fail-fast");

    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to read file {}: {}", path, e);
            std::process::exit(1);
        }
    };

    let options = d2r_core::verify::symmetry::SymmetryOptions {
        roundtrip: true,
        target_index: target_idx,
        fail_fast,
    };

    match calculate_symmetry_diff(&bytes, None, options) {
        Ok(report) => {
            if use_json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else if let Some(out_path) = visual_out {
                generate_html_visual_report(&report, path, &out_path);
            } else {
                println!("Serialization Audit for: {}", path);
                if let Some(target) = target_idx {
                    println!("Targeting item index: {}", target);
                }
                println!("{:-<80}", "");
                println!(
                    "{:>5} | {:<10} | {:>8} | {:>8} | {:>5} | {:<5}",
                    "Idx", "Code", "OrigLen", "SerLen", "Match", "Fid"
                );
                println!("{:-<90}", "");

                for item in &report.items {
                    // When targeting, the index in the report might not be the actual index if we filtered.
                    // But our implementation currently preserves the "Item N" label.
                    let idx_from_label = item
                        .label
                        .strip_prefix("Item ")
                        .and_then(|s| s.parse::<usize>().ok());

                    print_item_diff(idx_from_label, item, 0, use_visual);
                }
                println!("{:-<90}", "");

                if report.success {
                    if target_idx.is_some() {
                        println!("MATCH: Target item matches with 100% fidelity");
                    } else {
                        println!("MATCH: 100% fidelity");
                    }
                } else {
                    println!("FAIL: Mismatches detected.");
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Error during symmetry audit: {}", e);
            std::process::exit(1);
        }
    }
}

fn generate_html_visual_report(report: &d2r_core::verify::symmetry::DiffReport, file_path: &str, out_path: &str) {
    let mut html = String::new();
    html.push_str(&"<!DOCTYPE html>
<html lang=\"en\">
<head>
    <meta charset=\"UTF-8\">
    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">
    <title>Alpha v105 Bit-Diff Visualizer</title>
    <style>
        :root {
            --bg-color: #0f172a;
            --card-bg: #1e293b;
            --text-primary: #f8fafc;
            --text-secondary: #94a3b8;
            --accent: #38bdf8;
            --success: #22c55e;
            --warning: #eab308;
            --danger: #ef4444;
            --border: #334155;
            --bit-match: #15803d;
            --bit-mismatch: #b91c1c;
            --bit-nudge: #a16207;
            --bit-empty: #334155;
        }
        body { font-family: 'Inter', -apple-system, sans-serif; background-color: var(--bg-color); color: var(--text-primary); margin: 0; padding: 40px 20px; line-height: 1.5; }
        .container { max-width: 1200px; margin: auto; }
        header { margin-bottom: 40px; text-align: center; }
        h1 { font-size: 2rem; font-weight: 800; margin-bottom: 8px; color: var(--accent); }
        .subtitle { color: var(--text-secondary); font-size: 0.9rem; }
        .legend { display: flex; gap: 24px; margin-bottom: 32px; padding: 16px 24px; background: var(--card-bg); border-radius: 12px; border: 1px solid var(--border); justify-content: center; }
        .legend-item { display: flex; align-items: center; gap: 8px; font-size: 0.875rem; font-weight: 500; }
        .box { width: 14px; height: 14px; border-radius: 3px; }
        .match { background-color: var(--bit-match); }
        .mismatch { background-color: var(--bit-mismatch); }
        .nudge { background-color: var(--bit-nudge); }
        .empty { background-color: var(--bit-empty); }
        .item-card { background: var(--card-bg); border-radius: 16px; border: 1px solid var(--border); overflow: hidden; margin-bottom: 32px; }
        .item-header { padding: 20px 24px; border-bottom: 1px solid var(--border); background: rgba(255,255,255,0.02); }
        .item-header h2 { margin: 0; font-size: 1.25rem; font-weight: 600; display: flex; justify-content: space-between; align-items: center; }
        .fidelity-badge { font-size: 0.75rem; font-weight: 700; padding: 4px 12px; border-radius: 9999px; background: rgba(56, 189, 248, 0.1); color: var(--accent); }
        .item-meta { padding: 16px 24px; font-size: 0.875rem; color: var(--text-secondary); border-bottom: 1px solid var(--border); }
        .grid-container { padding: 24px; overflow-x: auto; }
        .diff-grid { display: flex; flex-direction: column; gap: 12px; font-family: 'JetBrains Mono', 'Fira Code', monospace; }
        .bit-row { display: flex; align-items: center; }
        .row-label { width: 100px; font-size: 0.75rem; font-weight: 600; text-transform: uppercase; color: var(--text-secondary); flex-shrink: 0; }
        .bits-wrapper { display: flex; gap: 1px; flex-wrap: wrap; }
        .bit { width: 12px; height: 18px; font-size: 9px; display: flex; align-items: center; justify-content: center; color: rgba(255,255,255,0.7); }
        .bit.byte-gap { margin-right: 4px; }
        .perfect-msg { text-align: center; padding: 60px; color: var(--text-secondary); }
    </style>
</head>
<body>
<div class=\"container\">
    <header>
        <h1>Bitstream Visual Diff</h1>
        <p class=\"subtitle\">File: <code>{file_path}</code></p>
    </header>

    <div class=\"legend\">
        <div class=\"legend-item\"><div class=\"box match\"></div> Match</div>
        <div class=\"legend-item\"><div class=\"box mismatch\"></div> Mismatch</div>
        <div class=\"legend-item\"><div class=\"box nudge\"></div> Nudge (Shift)</div>
        <div class=\"legend-item\"><div class=\"box empty\"></div> Gap / Empty</div>
    </div>"
    .replace("{file_path}", file_path));


    let mut found_issues = false;
    for item in &report.items {
        if !item.is_match || item.fidelity_score < 1.0 {
            found_issues = true;
            render_item_diff_html(&mut html, item, 0);
        }
    }

    if !found_issues {
        html.push_str("<div class=\"perfect-msg\"><h2>All items match perfectly! No differences detected in this save file.</h2></div>");
    }

    html.push_str("</div>\n</body>\n</html>");

    if let Some(parent) = std::path::Path::new(out_path).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            let _ = fs::create_dir_all(parent);
        }
    }

    if let Err(e) = fs::write(out_path, html) {
        eprintln!("Failed to write HTML report: {}", e);
    } else {
        println!("Visual diff report generated at {}", out_path);
    }
}

fn render_item_diff_html(html: &mut String, item: &d2r_core::verify::symmetry::ItemDiff, depth: usize) {
    let margin = depth * 24;
    html.push_str(&format!("<div class=\"item-card\" style=\"margin-left: {}px;\">
        <div class=\"item-header\">
            <h2>
                <span>{} (Code: {})</span>
                <span class=\"fidelity-badge\">Fidelity: {:.2}%</span>
            </h2>
        </div>
        <div class=\"item-meta\">", 
        margin, item.label, item.code, item.fidelity_score * 100.0));
    
    if let Some(m_type) = &item.mismatch_type {
        html.push_str(&format!("<div><strong>Reason:</strong> {}</div>", m_type));
    }
    if let Some(offset) = item.first_mismatch_offset {
        html.push_str(&format!("<div><strong>First Mismatch:</strong> Bit {}</div>", offset));
    }
    if let Some(seg) = &item.segment {
        html.push_str(&format!("<div><strong>Segment:</strong> {}</div>", seg));
    }
    html.push_str("</div>");

    if let (Some(orig), Some(target)) = (&item.orig_bits, &item.target_bits) {
        let o_chars: Vec<char> = orig.chars().collect();
        let t_chars: Vec<char> = target.chars().collect();
        
        html.push_str("<div class=\"grid-container\"><div class=\"diff-grid\">");
        
        let mut i = 0;
        let mut j = 0;
        let mut o_row = String::new();
        let mut t_row = String::new();
        
        while i < o_chars.len() || j < t_chars.len() {
            let is_byte_end = (i.max(j) + 1) % 8 == 0;
            let gap_class = if is_byte_end { " byte-gap" } else { "" };
            
            if i < o_chars.len() && j < t_chars.len() && o_chars[i] == t_chars[j] {
                o_row.push_str(&format!("<div class=\"bit match{}\">{}</div>", gap_class, o_chars[i]));
                t_row.push_str(&format!("<div class=\"bit match{}\">{}</div>", gap_class, t_chars[j]));
                i += 1;
                j += 1;
            } else if i < o_chars.len() && j < t_chars.len() {
                // Lookahead for nudge
                let mut found_sync = false;
                for nudge in 1..49 {
                    if j + nudge < t_chars.len() && o_chars[i] == t_chars[j + nudge] {
                        for _ in 0..nudge {
                            let g_class = if (j + 1) % 8 == 0 { " byte-gap" } else { "" };
                            o_row.push_str(&format!("<div class=\"bit empty{}\">-</div>", g_class));
                            t_row.push_str(&format!("<div class=\"bit nudge{}\">{}</div>", g_class, t_chars[j]));
                            j += 1;
                        }
                        found_sync = true;
                        break;
                    }
                    if i + nudge < o_chars.len() && o_chars[i + nudge] == t_chars[j] {
                        for _ in 0..nudge {
                            let g_class = if (i + 1) % 8 == 0 { " byte-gap" } else { "" };
                            o_row.push_str(&format!("<div class=\"bit nudge{}\">{}</div>", g_class, o_chars[i]));
                            t_row.push_str(&format!("<div class=\"bit empty{}\">-</div>", g_class));
                            i += 1;
                        }
                        found_sync = true;
                        break;
                    }
                }
                
                if !found_sync {
                    o_row.push_str(&format!("<div class=\"bit mismatch{}\">{}</div>", gap_class, o_chars[i]));
                    t_row.push_str(&format!("<div class=\"bit mismatch{}\">{}</div>", gap_class, t_chars[j]));
                    i += 1;
                    j += 1;
                }
            } else if i < o_chars.len() {
                o_row.push_str(&format!("<div class=\"bit mismatch{}\">{}</div>", gap_class, o_chars[i]));
                t_row.push_str(&format!("<div class=\"bit empty{}\">-</div>", gap_class));
                i += 1;
            } else {
                o_row.push_str(&format!("<div class=\"bit empty{}\">-</div>", gap_class));
                t_row.push_str(&format!("<div class=\"bit mismatch{}\">{}</div>", gap_class, t_chars[j]));
                j += 1;
            }
        }
        
        html.push_str("<div class=\"bit-row\"><div class=\"row-label\">Original</div><div class=\"bits-wrapper\">");
        html.push_str(&o_row);
        html.push_str("</div></div>");
        
        html.push_str("<div class=\"bit-row\"><div class=\"row-label\">Target</div><div class=\"bits-wrapper\">");
        html.push_str(&t_row);
        html.push_str("</div></div>");
        
        html.push_str("</div></div>");
    }

    for child in &item.children {
        render_item_diff_html(html, child, depth + 1);
    }

    html.push_str("</div>");
}

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD_RED: &str = "\x1b[31;1m";
const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_BLUE: &str = "\x1b[34m";
const ANSI_MAGENTA: &str = "\x1b[35m";
const ANSI_CYAN: &str = "\x1b[36m";
const ANSI_WHITE: &str = "\x1b[37m";

fn get_segment_color(segment: Option<&str>) -> &'static str {
    match segment {
        Some(s) => {
            let s = s.to_lowercase();
            if s.contains("header") {
                ANSI_GREEN
            } else if s.contains("id") {
                ANSI_YELLOW
            } else if s.contains("value") {
                ANSI_CYAN
            } else if s.contains("padding") {
                ANSI_MAGENTA
            } else if s.contains("gap") {
                ANSI_BLUE
            } else {
                ANSI_WHITE
            }
        }
        None => ANSI_WHITE,
    }
}

fn print_item_diff(idx: Option<usize>, item: &ItemDiff, indent_level: usize, use_visual: bool) {
    let indent = "  ".repeat(indent_level);
    let prefix = if indent_level > 0 { "|-- " } else { "" };
    let idx_str = idx.map(|i| i.to_string()).unwrap_or_default();

    println!(
        "{:indent$}{:<5} | {:<10} | {:>8} | {:>8} | {:>5} | {:.2} | V:{} F:0x{:08X}",
        prefix,
        idx_str,
        item.code,
        item.original_len,
        item.target_len,
        if item.is_match { "OK" } else { "FAIL" },
        item.fidelity_score,
        item.version,
        item.flags,
        indent = indent.len()
    );

    if !item.is_match || item.fidelity_score < 1.0 {
        if let Some(m_type) = &item.mismatch_type {
            println!("{:indent$}      [REASON] {}", "", m_type, indent = indent.len());
        }
        if let Some(seg) = &item.segment {
            println!("{:indent$}      [SEGMENT] {}", "", seg, indent = indent.len());
        }
        if let Some(offset) = item.first_mismatch_offset {
            println!("{:indent$}      [OFFSET] bit {}", "", offset, indent = indent.len());
            if let (Some(orig), Some(target)) = (&item.orig_bits, &item.target_bits) {
                let start = (offset as usize).saturating_sub(10);
                let end = (offset as usize + 20).min(orig.len()).min(target.len());
                println!("{:indent$}      [BITS]  ...", "", indent = indent.len());
                
                let seg_color = get_segment_color(item.segment.as_deref());
                let mut o_bits = String::new();
                let mut t_bits = String::new();
                
                let o_chars: Vec<char> = orig.chars().collect();
                let t_chars: Vec<char> = target.chars().collect();

                for i in start..end {
                    let color = if i == offset as usize { ANSI_BOLD_RED } else { seg_color };
                    o_bits.push_str(&format!("{}{}{}", color, o_chars[i], ANSI_RESET));
                    t_bits.push_str(&format!("{}{}{}", color, t_chars[i], ANSI_RESET));
                }

                println!("{:indent$}      ORIG:   {}", "", o_bits, indent = indent.len());
                println!("{:indent$}      TARG:   {}", "", t_bits, indent = indent.len());
                let mut markers = String::new();
                for i in start..end {
                    if i == offset as usize { markers.push_str(&format!("{}^{}", ANSI_BOLD_RED, ANSI_RESET)); }
                    else { markers.push(' '); }
                }
                println!("{:indent$}              {}", "", markers, indent = indent.len());
            }
        }
        if let Some(gap) = item.discovered_alpha_header_gap {
            println!("{:indent$}      [DISC GAP] {} bits", "", gap, indent = indent.len());
        }
        if let Some(gap) = item.parsed_alpha_header_gap {
            println!("{:indent$}      [PARS GAP] {} bits", "", gap, indent = indent.len());
        }
        if item.alpha_alignment_padding_len > 0 {
            println!("{:indent$}      [ALIGNMENT PAD] {} bits", "", item.alpha_alignment_padding_len, indent = indent.len());
        }
        if item.alpha_body_gap_len > 0 {
            println!("{:indent$}      [BODY GAP] {} bits", "", item.alpha_body_gap_len, indent = indent.len());
        }
        if item.fidelity_score < 1.0 {
            println!("{:indent$}      [FORENSIC RATIONALE]", "", indent = indent.len());
            for finding in &item.forensic_audit.findings {
                if finding.confidence
                    < d2r_core::domain::item::axiom_meta::Confidence::VerifiedTruth
                {
                    println!(
                        "{:indent$}        - [{:?}] {}",
                        "",
                        finding.confidence, finding.rationale,
                        indent = indent.len()
                    );
                }
            }
        }
        if use_visual {
            if let (Some(orig), Some(target)) = (&item.orig_bits, &item.target_bits) {
                println!("{:indent$}      [BITSTREAM ALIGNMENT]", "", indent = indent.len());
                print_visual_diff(orig, target, indent_level, item.segment.as_deref());
            }
        }
    }

    for child in &item.children {
        print_item_diff(None, child, indent_level + 1, use_visual);
    }
}

fn print_visual_diff(orig: &str, target: &str, indent_level: usize, segment: Option<&str>) {
    let indent = "  ".repeat(indent_level);
    let mut i = 0;
    let mut j = 0;
    
    let mut o_out = Vec::new();
    let mut t_out = Vec::new();
    let mut m_out = Vec::new();

    let o_chars: Vec<char> = orig.chars().collect();
    let t_chars: Vec<char> = target.chars().collect();

    let seg_color = get_segment_color(segment);

    while i < o_chars.len() || j < t_chars.len() {
        if i < o_chars.len() && j < t_chars.len() && o_chars[i] == t_chars[j] {
            o_out.push(format!("{}{}{}", seg_color, o_chars[i], ANSI_RESET));
            t_out.push(format!("{}{}{}", seg_color, t_chars[j], ANSI_RESET));
            m_out.push(" ".to_string());
            i += 1;
            j += 1;
        } else if i < o_chars.len() && j < t_chars.len() {
            let mut found_sync = false;
            // Range 1..49 to cover up to 3x 16-bit gaps
            for nudge in 1..49 {
                let is_gap_candidate = nudge % 16 == 0;
                
                if j + nudge < t_chars.len() && o_chars[i] == t_chars[j + nudge] {
                    // Check if all skipped bits are '0' for a Gap
                    let all_zeros = t_chars[j..j+nudge].iter().all(|&c| c == '0');
                    let color = if is_gap_candidate && all_zeros { ANSI_BLUE } else { ANSI_BOLD_RED };
                    let marker = if is_gap_candidate && all_zeros { "G" } else { "^" };

                    for _ in 0..nudge {
                        o_out.push(format!("{}{}{}", color, "-", ANSI_RESET));
                        t_out.push(format!("{}{}{}", color, t_chars[j], ANSI_RESET));
                        m_out.push(format!("{}{}{}", color, marker, ANSI_RESET));
                        j += 1;
                    }
                    found_sync = true;
                    break;
                }
                if i + nudge < o_chars.len() && o_chars[i + nudge] == t_chars[j] {
                    // Check if all skipped bits are '0' for a Gap
                    let all_zeros = o_chars[i..i+nudge].iter().all(|&c| c == '0');
                    let color = if is_gap_candidate && all_zeros { ANSI_BLUE } else { ANSI_BOLD_RED };
                    let v_marker = if is_gap_candidate && all_zeros { "G" } else { "v" };

                    for _ in 0..nudge {
                        o_out.push(format!("{}{}{}", color, o_chars[i], ANSI_RESET));
                        t_out.push(format!("{}{}{}", color, "-", ANSI_RESET));
                        m_out.push(format!("{}{}{}", color, v_marker, ANSI_RESET));
                        i += 1;
                    }
                    found_sync = true;
                    break;
                }
            }
            if !found_sync {
                o_out.push(format!("{}{}{}", ANSI_BOLD_RED, o_chars[i], ANSI_RESET));
                t_out.push(format!("{}{}{}", ANSI_BOLD_RED, t_chars[j], ANSI_RESET));
                m_out.push(format!("{}{}{}", ANSI_BOLD_RED, "X", ANSI_RESET));
                i += 1;
                j += 1;
            }
        } else if i < o_chars.len() {
            o_out.push(format!("{}{}{}", ANSI_BOLD_RED, o_chars[i], ANSI_RESET));
            t_out.push(format!("{}{}{}", ANSI_BOLD_RED, "-", ANSI_RESET));
            m_out.push(format!("{}{}{}", ANSI_BOLD_RED, "v", ANSI_RESET));
            i += 1;
        } else {
            o_out.push(format!("{}{}{}", ANSI_BOLD_RED, "-", ANSI_RESET));
            t_out.push(format!("{}{}{}", ANSI_BOLD_RED, t_chars[j], ANSI_RESET));
            m_out.push(format!("{}{}{}", ANSI_BOLD_RED, "^", ANSI_RESET));
            j += 1;
        }
    }

    let chunk_size = 80usize.saturating_sub(indent_level.saturating_mul(2)).max(24);
    for k in (0..o_out.len()).step_by(chunk_size) {
        let end = (k + chunk_size).min(o_out.len());
        println!("{:indent$}      Orig:   {}", "", o_out[k..end].join(""), indent = indent.len());
        println!("{:indent$}      Target: {}", "", t_out[k..end].join(""), indent = indent.len());
        println!("{:indent$}      Diff:   {}", "", m_out[k..end].join(""), indent = indent.len());
        println!();
    }
}

