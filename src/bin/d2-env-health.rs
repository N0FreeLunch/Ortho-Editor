use d2r_core::verify::args::ArgParser;
use d2r_core::verify::{Report, ReportMetadata, ReportStatus};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Serialize, Clone)]
struct EnvVariable {
    name: String,
    value: String,
    exists: bool,
    is_sanitized: bool,
    canonical: Option<String>,
}

#[derive(Serialize, Clone)]
struct FileHygiene {
    env_found: bool,
    has_bom: bool,
    is_utf8: bool,
}

#[derive(Serialize, Clone)]
struct HealthData {
    hygiene: FileHygiene,
    variables: Vec<EnvVariable>,
}

fn check_hygiene() -> FileHygiene {
    let env_path = Path::new(".env");
    if !env_path.exists() {
        return FileHygiene {
            env_found: false,
            has_bom: false,
            is_utf8: false,
        };
    }

    let bytes = fs::read(env_path).unwrap_or_default();
    let has_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
    let is_utf8 = std::str::from_utf8(&bytes).is_ok();

    FileHygiene {
        env_found: true,
        has_bom,
        is_utf8,
    }
}

fn check_variable(name: &str) -> Option<EnvVariable> {
    let value = env::var(name).ok()?;
    let path = Path::new(&value);
    let exists = path.exists();
    let is_sanitized = !value.contains('\\');

    let canonical = if exists {
        fs::canonicalize(path).ok().map(|p| p.display().to_string())
    } else {
        None
    };

    Some(EnvVariable {
        name: name.to_string(),
        value,
        exists,
        is_sanitized,
        canonical,
    })
}

fn main() {
    let parser = ArgParser::new("d2-env-health")
        .description("Diagnoses D2R Workspace Environment & .env hygiene");

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(err) => match err {
            d2r_core::verify::args::ArgError::Help(h) => {
                println!("{}", h);
                std::process::exit(0);
            }
            d2r_core::verify::args::ArgError::Error(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
    };

    let hygiene = check_hygiene();
    let mut variables = Vec::new();

    let required_vars = [
        "D2R_CORE_PATH",
        "D2R_SPEC_PATH",
        "D2R_DATA_PATH",
        "D2DATA_JSON_DIR",
        "D2R_PATH",
        "D2R_SAVE_DIR",
    ];

    for var in required_vars {
        if let Some(info) = check_variable(var) {
            variables.push(info);
        }
    }

    let health = HealthData {
        hygiene: hygiene.clone(),
        variables: variables.clone(),
    };

    if parsed.is_json() {
        let meta = ReportMetadata::new("d2-env-health", ".env", "0.1.0");
        let report = Report::new(meta, ReportStatus::Ok).with_results(health);
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("D2R Environment Health Report");
        println!("============================");

        if hygiene.env_found {
            print!("- .env file: FOUND");
            if hygiene.has_bom {
                print!(" (Warning: HAS BOM)");
            }
            if !hygiene.is_utf8 {
                print!(" (Error: NOT UTF-8)");
            }
            println!();
        } else {
            println!(
                "- .env file: NOT FOUND in CWD ({})",
                env::current_dir().unwrap().display()
            );
        }

        println!("\nVariables:");
        for var in variables {
            let status = if var.exists { "OK" } else { "MISSING" };
            let sanitized = if var.is_sanitized {
                "SANITY: OK"
            } else {
                "WARNING: Contains backslashes"
            };
            println!(
                "  [{}] {} = {} ({})",
                status, var.name, var.value, sanitized
            );
            if let Some(can) = var.canonical {
                println!("    -> Canonical: {}", can);
            }
        }

        let all_ok = hygiene.env_found
            && !hygiene.has_bom
            && hygiene.is_utf8
            && health.variables.iter().all(|v| v.exists && v.is_sanitized);

        if all_ok {
            println!("\nResult: HEALTHY");
        } else {
            println!("\nResult: UNHEALTHY (Check warnings/errors above)");
            std::process::exit(1);
        }
    }
}
