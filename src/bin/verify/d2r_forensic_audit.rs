use std::fs;
use std::path::{Path, PathBuf};
use syn::{visit::{self, Visit}, ItemMacro, ItemImpl, Expr, Lit, parse2};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::Token;
use serde::Serialize;
use anyhow::Result;

#[derive(Debug, Serialize, Clone)]
struct AxiomInfo {
    name: String,
    confidence: String,
    intentionality: String,
    rationale: String,
    file: String,
    module: String,
}

struct MacroArgs {
    args: Punctuated<Expr, Token![,]>,
}

impl Parse for MacroArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(MacroArgs {
            args: Punctuated::parse_terminated(input)?,
        })
    }
}

struct AxiomVisitor {
    found_axioms: Vec<AxiomInfo>,
    current_file: String,
    current_module: String,
}

impl<'ast> Visit<'ast> for AxiomVisitor {
    fn visit_item_macro(&mut self, i: &'ast ItemMacro) {
        let path = &i.mac.path;
        let is_target = path.is_ident("impl_forensic_axiom") || 
           (path.segments.len() == 2 && path.segments[0].ident == "crate" && path.segments[1].ident == "impl_forensic_axiom");

        if is_target {
            let tokens = i.mac.tokens.clone();
            if let Ok(m_args) = parse2::<MacroArgs>(tokens) {
                let args = m_args.args;
                if args.len() >= 4 {
                    let name = expr_to_string(&args[0]);
                    let confidence = expr_to_string(&args[1]);
                    let intentionality = expr_to_string(&args[2]);
                    let rationale = expr_to_string(&args[3]);

                    self.found_axioms.push(AxiomInfo {
                        name,
                        confidence,
                        intentionality,
                        rationale: rationale.trim_matches('"').to_string(),
                        file: self.current_file.clone(),
                        module: self.current_module.clone(),
                    });
                }
            }
        }
        visit::visit_item_macro(self, i);
    }

    fn visit_item_impl(&mut self, i: &'ast ItemImpl) {
        if let Some((_, path, _)) = &i.trait_ {
            if path.segments.iter().any(|s| s.ident == "ForensicAxiom") {
                let name = match &*i.self_ty {
                    syn::Type::Path(tp) => tp.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default(),
                    _ => "unknown".to_string(),
                };

                // Try to find metadata() function and extract values from ForensicMetadata::new(...)
                let mut confidence = "Manual".to_string();
                let mut intentionality = "Manual".to_string();
                let mut rationale = "Complex or Dynamic Implementation".to_string();

                for item in &i.items {
                    if let syn::ImplItem::Fn(m) = item {
                        if m.sig.ident == "metadata" {
                            // Basic search for ForensicMetadata::new pattern in the function body
                            let body_str = quote::quote!(#m.block).to_string();
                            if body_str.contains("ForensicMetadata :: new") || body_str.contains("new") {
                                // For MVP, we'll mark as Manual implementation if it's not a simple macro
                                rationale = "Manual ForensicAxiom implementation found (Check source for details)".to_string();
                            }
                        }
                    }
                }

                self.found_axioms.push(AxiomInfo {
                    name,
                    confidence,
                    intentionality,
                    rationale,
                    file: self.current_file.clone(),
                    module: self.current_module.clone(),
                });
            }
        }
        visit::visit_item_impl(self, i);
    }
}

fn expr_to_string(expr: &Expr) -> String {
    match expr {
        Expr::Path(p) => {
            p.path.segments.iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::")
        },
        Expr::Lit(l) => {
            match &l.lit {
                Lit::Str(s) => s.value(),
                _ => quote::quote!(#l).to_string(),
            }
        },
        _ => quote::quote!(#expr).to_string(),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let is_json = args.contains(&"--json".to_string());

    let mut visitor = AxiomVisitor {
        found_axioms: Vec::new(),
        current_file: String::new(),
        current_module: String::new(),
    };

    let src_dir = Path::new("src/domain");
    if !src_dir.exists() {
        eprintln!("Warning: src/domain not found, searching from current directory.");
    }

    let mut files = Vec::new();
    visit_dirs(src_dir, &mut files)?;

    for path in files {
        let path_str = path.to_string_lossy().to_string();
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(file) = syn::parse_file(&content) {
                visitor.current_file = path_str.clone();
                visitor.current_module = extract_module_name(&path_str);
                visitor.visit_file(&file);
            }
        }
    }

    if is_json {
        println!("{}", serde_json::to_string_pretty(&visitor.found_axioms)?);
    } else {
        print_knowledge_map(&visitor.found_axioms);
    }

    Ok(())
}

fn visit_dirs(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path, files)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
    Ok(())
}

fn extract_module_name(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let parts: Vec<_> = normalized.split('/').collect();
    if parts.len() >= 3 {
        // e.g. "src/domain/forensic/v105/axioms.rs" -> "forensic/v105"
        if parts[2] == "forensic" && parts.len() >= 5 {
             format!("{}/{}", parts[2], parts[3])
        } else {
             parts[2].to_string()
        }
    } else {
        "unknown".to_string()
    }
}

fn print_knowledge_map(axioms: &[AxiomInfo]) {
    println!("\n=== d2r Forensic Knowledge Map ===");
    println!("{:<20} | {:<25} | {:<15} | {}", "Module", "Axiom Name", "Confidence", "Rationale");
    println!("{:-<130}", "");

    for ax in axioms {
        println!("{:<20} | {:<25} | {:<15} | {}", 
            ax.module, 
            ax.name, 
            ax.confidence.replace("Confidence::", ""), 
            ax.rationale
        );
    }
    println!("{:-<130}", "");
    println!("Total Axioms Found: {}\n", axioms.len());
}
