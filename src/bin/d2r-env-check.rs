use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone)]
struct PathCheck {
    name: &'static str,
    configured: PathBuf,
    canonical: PathBuf,
}

use d2r_core::verify::args::ArgParser;

fn canonicalize_path(path: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(path).map_err(|err| err.to_string())
}

fn read_required_var(name: &'static str) -> Result<PathBuf, String> {
    env::var(name)
        .map(PathBuf::from)
        .map_err(|_| format!("{name} is not set"))
}

fn inspect_required_path(name: &'static str) -> Result<PathCheck, String> {
    let configured = read_required_var(name)?;
    if !configured.exists() {
        return Err(format!(
            "{name} points to a missing path: {}",
            configured.display()
        ));
    }

    let canonical = canonicalize_path(&configured)?;
    Ok(PathCheck {
        name,
        configured,
        canonical,
    })
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn path_is_under(child: &Path, parent: &Path) -> bool {
    child.starts_with(parent)
}

fn report_ok(label: &str, value: &str) {
    println!("[ok] {label}: {value}");
}

fn report_issue(label: &str, detail: &str) {
    println!("[issue] {label}: {detail}");
}

fn main() {
    let parser = ArgParser::new("d2r-env-check");
    let _ = parser.parse(env::args_os().skip(1).collect());

    let mut issues = 0usize;

    let core = match inspect_required_path("D2R_CORE_PATH") {
        Ok(check) => check,
        Err(err) => {
            report_issue("D2R_CORE_PATH", &err);
            std::process::exit(1);
        }
    };

    let spec = match inspect_required_path("D2R_SPEC_PATH") {
        Ok(check) => check,
        Err(err) => {
            report_issue("D2R_SPEC_PATH", &err);
            std::process::exit(1);
        }
    };

    let data = match inspect_required_path("D2R_DATA_PATH") {
        Ok(check) => check,
        Err(err) => {
            report_issue("D2R_DATA_PATH", &err);
            std::process::exit(1);
        }
    };

    let json_dir = match inspect_required_path("D2DATA_JSON_DIR") {
        Ok(check) => check,
        Err(err) => {
            report_issue("D2DATA_JSON_DIR", &err);
            std::process::exit(1);
        }
    };

    println!("Configured environment paths:");
    for check in [&core, &spec, &data, &json_dir] {
        println!(
            "- {}:\n  configured: {}\n  canonical:  {}",
            check.name,
            display_path(&check.configured),
            display_path(&check.canonical)
        );
    }

    let manifest_dir = canonicalize_path(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|_| core.canonical.clone());
    let repo_parent = manifest_dir.parent().map(Path::to_path_buf);

    if core.canonical != manifest_dir {
        issues += 1;
        report_issue(
            "D2R_CORE_PATH",
            &format!(
                "expected canonical core path {}, got {}",
                display_path(&manifest_dir),
                display_path(&core.canonical)
            ),
        );
    } else {
        report_ok("D2R_CORE_PATH", "matches current repository root");
    }

    let Some(repo_parent) = repo_parent else {
        report_issue(
            "repo-parent",
            "could not derive sibling repository root from CARGO_MANIFEST_DIR",
        );
        std::process::exit(1);
    };

    let expected_spec = repo_parent.join("d2r-spec");
    let expected_data = repo_parent.join("d2r-data");
    let expected_json = expected_spec.join("references").join("d2data").join("json");

    let expected_spec_canonical =
        canonicalize_path(&expected_spec).unwrap_or(expected_spec.clone());
    let expected_data_canonical =
        canonicalize_path(&expected_data).unwrap_or(expected_data.clone());
    let expected_json_canonical =
        canonicalize_path(&expected_json).unwrap_or(expected_json.clone());

    for (label, check, expected) in [
        ("D2R_SPEC_PATH", &spec, &expected_spec_canonical),
        ("D2R_DATA_PATH", &data, &expected_data_canonical),
    ] {
        if check.canonical != *expected {
            issues += 1;
            report_issue(
                label,
                &format!(
                    "expected sibling repo path {}, got {}",
                    display_path(expected),
                    display_path(&check.canonical)
                ),
            );
        } else {
            report_ok(label, "canonical path matches sibling repository root");
        }

        if path_is_under(&check.configured, &core.configured) {
            issues += 1;
            report_issue(
                label,
                &format!(
                    "configured path points under D2R_CORE_PATH ({}) and is likely using a junction convenience path; prefer {}",
                    display_path(&check.configured),
                    display_path(expected)
                ),
            );
        }
    }

    if json_dir.canonical != expected_json_canonical {
        issues += 1;
        report_issue(
            "D2DATA_JSON_DIR",
            &format!(
                "expected {}, got {}",
                display_path(&expected_json_canonical),
                display_path(&json_dir.canonical)
            ),
        );
    } else {
        report_ok(
            "D2DATA_JSON_DIR",
            "matches the expected JSON reference directory",
        );
    }

    if !path_is_under(&json_dir.canonical, &spec.canonical) {
        issues += 1;
        report_issue(
            "D2DATA_JSON_DIR",
            "canonical JSON directory is not inside D2R_SPEC_PATH",
        );
    }

    if issues > 0 {
        println!("\nResult: FAILED ({issues} issue(s))");
        println!("Recommended fix:");
        println!(
            "- Set D2R_SPEC_PATH and D2R_DATA_PATH to real sibling repository roots, not junction paths under d2r-core."
        );
        println!("- Set D2DATA_JSON_DIR under the real D2R_SPEC_PATH reference tree.");
        std::process::exit(1);
    }

    println!("\nResult: OK");
}
