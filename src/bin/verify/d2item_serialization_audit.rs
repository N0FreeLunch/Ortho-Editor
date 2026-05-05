use d2r_core::verify::args::{ArgParser, ArgSpec, ArgError};
use d2r_core::verify::symmetry::calculate_symmetry_diff;
use std::env;
use std::fs;

fn main() {
    let mut parser = ArgParser::new("d2item_serialization_audit");
    parser.add_spec(ArgSpec::positional("save_file", "Path to save file"));
    parser.add_spec(ArgSpec::flag("json", None, Some("json"), "Output results in JSON format"));
    parser.add_spec(ArgSpec::flag("diff-visual", None, Some("diff-visual"), "Show visual bitstream alignment for mismatches"));

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
                println!("{:>5} | {:<10} | {:>8} | {:>8} | {:>5} | {:<5}", "Idx", "Code", "OrigLen", "SerLen", "Match", "Fid");
                println!("{:-<90}", "");
                
                for (i, item) in report.items.iter().enumerate() {
                    println!("{:5} | {:10} | {:8} | {:8} | {:5} | {:.2}",
                        i, item.code, item.original_len, item.target_len,
                        if item.is_match { "OK" } else { "FAIL" },
                        item.fidelity_score
                    );
                    if !item.is_match || item.fidelity_score < 1.0 {
                        if let Some(m_type) = &item.mismatch_type {
                            println!("      [REASON] {}", m_type);
                        }
                        if let Some(seg) = &item.segment {
                            println!("      [SEGMENT] {}", seg);
                        }
                        if let Some(offset) = item.first_mismatch_offset {
                            println!("      [OFFSET] bit {}", offset);
                        }
                        if item.fidelity_score < 1.0 {
                            println!("      [FORENSIC RATIONALE]");
                            for finding in &item.forensic_audit.findings {
                                if finding.confidence < d2r_core::domain::item::axiom_meta::Confidence::VerifiedTruth {
                                    println!("        - [{:?}] {}", finding.confidence, finding.rationale);
                                }
                            }
                        }
                        if use_visual {
                            if let (Some(orig), Some(target)) = (&item.orig_bits, &item.target_bits) {
                                println!("      [BITSTREAM ALIGNMENT]");
                                print_visual_diff(orig, target);
                            }
                        }
                    }
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

fn print_visual_diff(orig: &str, target: &str) {
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

    let chunk_size = 80;
    for k in (0..o_out.len()).step_by(chunk_size) {
        let end = (k + chunk_size).min(o_out.len());
        println!("      Orig:   {}", &o_out[k..end]);
        println!("      Target: {}", &t_out[k..end]);
        println!("      Diff:   {}", &m_out[k..end]);
        println!();
    }
}
