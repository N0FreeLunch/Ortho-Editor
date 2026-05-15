use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use std::fs;
use syn::visit::{self, Visit};

#[derive(Debug, Serialize, Deserialize)]
struct Span {
    start_line: usize,
    end_line: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct SymbolEntry {
    repo: String,
    path: String,
    language: String,
    symbol: String,
    kind: String,
    span: Span,
    #[serde(skip_serializing_if = "Option::is_none")]
    container: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
}

struct RustSymbolVisitor {
    repo: String,
    file_path: String,
    entries: Vec<SymbolEntry>,
}

impl<'a> Visit<'a> for RustSymbolVisitor {
    fn visit_item_fn(&mut self, i: &'a syn::ItemFn) {
        let span = i.sig.ident.span();
        let start = span.start().line;
        let end = span.end().line;

        self.entries.push(SymbolEntry {
            repo: self.repo.clone(),
            path: self.file_path.clone(),
            language: "rust".to_string(),
            symbol: i.sig.ident.to_string(),
            kind: "function".to_string(),
            span: Span {
                start_line: start,
                end_line: end,
            },
            container: None,
            summary: None,
        });
        visit::visit_item_fn(self, i);
    }

    fn visit_item_struct(&mut self, i: &'a syn::ItemStruct) {
        let span = i.ident.span();
        self.entries.push(SymbolEntry {
            repo: self.repo.clone(),
            path: self.file_path.clone(),
            language: "rust".to_string(),
            symbol: i.ident.to_string(),
            kind: "struct".to_string(),
            span: Span {
                start_line: span.start().line,
                end_line: span.end().line,
            },
            container: None,
            summary: None,
        });
        visit::visit_item_struct(self, i);
    }

    fn visit_item_enum(&mut self, i: &'a syn::ItemEnum) {
        let span = i.ident.span();
        self.entries.push(SymbolEntry {
            repo: self.repo.clone(),
            path: self.file_path.clone(),
            language: "rust".to_string(),
            symbol: i.ident.to_string(),
            kind: "enum".to_string(),
            span: Span {
                start_line: span.start().line,
                end_line: span.end().line,
            },
            container: None,
            summary: None,
        });
        visit::visit_item_enum(self, i);
    }

    fn visit_item_impl(&mut self, i: &'a syn::ItemImpl) {
        // For impl, we use the type name as the symbol
        if let syn::Type::Path(tp) = &*i.self_ty {
            if let Some(last) = tp.path.segments.last() {
                let span = last.ident.span();
                self.entries.push(SymbolEntry {
                    repo: self.repo.clone(),
                    path: self.file_path.clone(),
                    language: "rust".to_string(),
                    symbol: format!("impl {}", last.ident),
                    kind: "impl".to_string(),
                    span: Span {
                        start_line: span.start().line,
                        end_line: span.end().line,
                    },
                    container: None,
                    summary: None,
                });
            }
        }
        visit::visit_item_impl(self, i);
    }

    fn visit_item_trait(&mut self, i: &'a syn::ItemTrait) {
        let span = i.ident.span();
        self.entries.push(SymbolEntry {
            repo: self.repo.clone(),
            path: self.file_path.clone(),
            language: "rust".to_string(),
            symbol: i.ident.to_string(),
            kind: "trait".to_string(),
            span: Span {
                start_line: span.start().line,
                end_line: span.end().line,
            },
            container: None,
            summary: None,
        });
        visit::visit_item_trait(self, i);
    }

    fn visit_item_mod(&mut self, i: &'a syn::ItemMod) {
        let span = i.ident.span();
        self.entries.push(SymbolEntry {
            repo: self.repo.clone(),
            path: self.file_path.clone(),
            language: "rust".to_string(),
            symbol: i.ident.to_string(),
            kind: "mod".to_string(),
            span: Span {
                start_line: span.start().line,
                end_line: span.end().line,
            },
            container: None,
            summary: None,
        });
        visit::visit_item_mod(self, i);
    }
}

fn extract_rust_symbols(repo: &str, file_path: &str, content: &str) -> Vec<SymbolEntry> {
    let file = syn::parse_file(content).expect("Unable to parse Rust file");
    let mut visitor = RustSymbolVisitor {
        repo: repo.to_string(),
        file_path: file_path.to_string(),
        entries: Vec::new(),
    };
    visitor.visit_file(&file);
    visitor.entries
}

fn extract_markdown_symbols(repo: &str, file_path: &str, content: &str) -> Vec<SymbolEntry> {
    let parser = Parser::new(content);
    let mut entries = Vec::new();
    let mut current_heading = String::new();
    let mut is_in_heading = false;

    // We need line numbers. pulldown-cmark doesn't give direct line numbers easily per event,
    // so we'll do a simple approximation or just capture the text for now.
    // For a better implementation, we could track offset to line mapping.
    let line_offsets: Vec<usize> = content.match_indices('\n').map(|(i, _)| i).collect();
    let get_line = |byte_offset: usize| -> usize {
        match line_offsets.binary_search(&byte_offset) {
            Ok(idx) => idx + 1,
            Err(idx) => idx + 1,
        }
    };

    let mut start_line = 0;

    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                is_in_heading = true;
                current_heading.clear();
                start_line = get_line(range.start);
            }
            Event::Text(text) if is_in_heading => {
                current_heading.push_str(&text);
            }
            Event::End(TagEnd::Heading(..)) => {
                entries.push(SymbolEntry {
                    repo: repo.to_string(),
                    path: file_path.to_string(),
                    language: "markdown".to_string(),
                    symbol: current_heading.clone(),
                    kind: "heading".to_string(),
                    span: Span {
                        start_line,
                        end_line: get_line(range.end),
                    },
                    container: None,
                    summary: None,
                });
                is_in_heading = false;
            }
            _ => {}
        }
    }
    entries
}

fn main() {
    let targets = vec![
        ("d2r-core", "d2r-core/src/item.rs"),
        ("d2r-spec", "d2r-spec/NAVIGATOR.md"),
        ("d2r-core", "d2r-core/src/lib.rs"),
    ];

    let mut all_entries = Vec::new();

    for (repo, rel_path) in targets {
        let path = if rel_path.starts_with("d2r-core/") {
            rel_path.strip_prefix("d2r-core/").unwrap().to_string()
        } else {
            format!("../{}", rel_path)
        };

        if let Ok(content) = fs::read_to_string(&path) {
            let entries = if path.ends_with(".rs") {
                extract_rust_symbols(repo, rel_path, &content)
            } else if path.ends_with(".md") {
                extract_markdown_symbols(repo, rel_path, &content)
            } else {
                Vec::new()
            };
            all_entries.extend(entries);
        } else {
            eprintln!("Failed to read file: {}", path);
        }
    }

    let json = serde_json::to_string_pretty(&all_entries).unwrap();
    println!("{}", json);
    fs::create_dir_all("tmp").expect("Unable to create tmp directory");
    fs::write("tmp/navigation-map.json", json).expect("Unable to write tmp/navigation-map.json");
}
