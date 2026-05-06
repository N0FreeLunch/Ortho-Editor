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

    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to read file {}: {}", path, e);
            std::process::exit(1);
        }
    };

    match calculate_symmetry_diff(&bytes, None, true) {
        Ok(report) => {
            if use_json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                println!("Serialization Audit for: {}", path);
                println!("{:-<80}", "");
                println!(
                    "{:>5} | {:<10} | {:>8} | {:>8} | {:>5} | {:<5}",
                    "Idx", "Code", "OrigLen", "SerLen", "Match", "Fid"
                );
                println!("{:-<90}", "");

                for (i, item) in report.items.iter().enumerate() {
                    print_item_diff(Some(i), item, 0, use_visual);
                }
                println!("{:-<90}", "");

                if report.success {
                    println!("MATCH: 100% fidelity");
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
                println!("{:indent$}      ORIG:   {}", "", &orig[start..end], indent = indent.len());
                println!("{:indent$}      TARG:   {}", "", &target[start..end], indent = indent.len());
                let mut markers = String::new();
                for i in start..end {
                    if i == offset as usize { markers.push('^'); }
                    else { markers.push(' '); }
                }
                println!("{:indent$}              {}", "", markers, indent = indent.len());
            }
        }
        if let Some(gap) = item.alpha_header_gap {
            println!("{:indent$}      [HEADER GAP] 0x{:X}", "", gap, indent = indent.len());
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
                print_visual_diff(orig, target, indent_level);
            }
        }
    }

    for child in &item.children {
        print_item_diff(None, child, indent_level + 1, use_visual);
    }
}

fn print_visual_diff(orig: &str, target: &str, indent_level: usize) {
    let indent = "  ".repeat(indent_level);
    let mut i = 0;
    let mut j = 0;
    let mut o_out = String::new();
    let mut t_out = String::new();
    let mut m_out = String::new();

    let o_chars: Vec<char> = orig.chars().collect();
    let t_chars: Vec<char> = target.chars().collect();

    while i < o_chars.len() || j < t_chars.len() {
        if i < o_chars.len() && j < t_chars.len() && o_chars[i] == t_chars[j] {
            o_out.push(o_chars[i]);
            t_out.push(t_chars[j]);
            m_out.push(' ');
            i += 1;
            j += 1;
        } else if i < o_chars.len() && j < t_chars.len() {
            let mut found_sync = false;
            for nudge in 1..17 {
                if j + nudge < t_chars.len() && o_chars[i] == t_chars[j + nudge] {
                    for _ in 0..nudge {
                        o_out.push('-');
                        t_out.push(t_chars[j]);
                        m_out.push('^');
                        j += 1;
                    }
                    found_sync = true;
                    break;
                }
                if i + nudge < o_chars.len() && o_chars[i + nudge] == t_chars[j] {
                    for _ in 0..nudge {
                        o_out.push(o_chars[i]);
                        t_out.push('-');
                        m_out.push('v');
                        i += 1;
                    }
                    found_sync = true;
                    break;
                }
            }
            if !found_sync {
                o_out.push(o_chars[i]);
                t_out.push(t_chars[j]);
                m_out.push('X');
                i += 1;
                j += 1;
            }
        } else if i < o_chars.len() {
            o_out.push(o_chars[i]);
            t_out.push('-');
            m_out.push('v');
            i += 1;
        } else {
            o_out.push('-');
            t_out.push(t_chars[j]);
            m_out.push('^');
            j += 1;
        }
    }

    let chunk_size = 80usize.saturating_sub(indent_level.saturating_mul(2)).max(24);
    for k in (0..o_out.len()).step_by(chunk_size) {
        let end = (k + chunk_size).min(o_out.len());
        println!("{:indent$}      Orig:   {}", "", &o_out[k..end], indent = indent.len());
        println!("{:indent$}      Target: {}", "", &t_out[k..end], indent = indent.len());
        println!("{:indent$}      Diff:   {}", "", &m_out[k..end], indent = indent.len());
        println!();
    }
}

