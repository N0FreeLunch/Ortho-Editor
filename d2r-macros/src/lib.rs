extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::Parser;

/// Custom attribute for Category B bitstream serialization symmetry governance.
#[proc_macro_attribute]
pub fn serialization_symmetry(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2 = proc_macro2::TokenStream::from(attr);
    let item2 = proc_macro2::TokenStream::from(item);

    let input: syn::Item = match syn::parse2(item2.clone()) {
        Ok(parsed) => parsed,
        Err(err) => return err.to_compile_error().into(),
    };

    let mut align = None;
    let mut checksum = None;
    let mut seed = None;
    let mut tentative = None;
    let mut confidence = None;

    {
        let args_parser = syn::meta::parser(|meta| {
            if meta.path.is_ident("align") {
                let value: syn::LitBool = meta.value()?.parse()?;
                align = Some(value.value);
            } else if meta.path.is_ident("checksum") {
                let value: syn::LitStr = meta.value()?.parse()?;
                checksum = Some(value.value());
            } else if meta.path.is_ident("seed") {
                let value: syn::LitInt = meta.value()?.parse()?;
                let num: u8 = value.base10_parse()?;
                seed = Some(num);
            } else if meta.path.is_ident("tentative") {
                let value: syn::LitBool = meta.value()?.parse()?;
                tentative = Some(value.value);
            } else if meta.path.is_ident("confidence") {
                let value: syn::LitStr = meta.value()?.parse()?;
                confidence = Some(value.value());
            } else {
                return Err(meta.error("unsupported attribute property"));
            }
            Ok(())
        });

        if !attr2.is_empty() {
            if let Err(err) = args_parser.parse2(attr2) {
                return err.to_compile_error().into();
            }
        }
    }

    let align_val = align.unwrap_or(false);

    // Constraint 1: If align == false and checksum is some, return compile error
    if !align_val && checksum.is_some() {
        let err = syn::Error::new(
            proc_macro2::Span::call_site(),
            "Hashing verification requires post-eval byte alignment constraints.\nSelf-Healing Hint: Hashing verification requires post-eval byte alignment constraints. When 'align' is set to false, you cannot specify a 'checksum' parameter. To resolve this, either remove 'checksum' or set 'align = true'.",
        );
        return err.to_compile_error().into();
    }

    // Constraint 2: If seed is some and checksum is none, return compile error
    if seed.is_some() && checksum.is_none() {
        let err = syn::Error::new(
            proc_macro2::Span::call_site(),
            "Checksum verification seed requires an explicitly bound algorithm parameter.\nSelf-Healing Hint: Checksum verification seed requires an explicitly bound algorithm parameter. If you specify a 'seed' parameter, you must also specify a valid 'checksum' algorithm parameter (e.g. checksum = \"Xor8\").",
        );
        return err.to_compile_error().into();
    }

    // Extract item identifier for impl block injection
    let name = match &input {
        syn::Item::Struct(item_struct) => &item_struct.ident,
        syn::Item::Enum(item_enum) => &item_enum.ident,
        syn::Item::Union(item_union) => &item_union.ident,
        _ => {
            let err = syn::Error::new(
                proc_macro2::Span::call_site(),
                "Attribute #[serialization_symmetry] can only be applied to structs, enums, or unions.",
            );
            return err.to_compile_error().into();
        }
    };

    let checksum_expr = match checksum {
        Some(algo) => quote! { Some(#algo) },
        None => quote! { None },
    };
    let seed_expr = match seed {
        Some(s) => quote! { Some(#s) },
        None => quote! { None },
    };
    let tentative_val = tentative.unwrap_or(false);
    let confidence_expr = match confidence {
        Some(conf) => quote! { Some(#conf) },
        None => quote! { None },
    };

    let expanded = quote! {
        #item2

        impl #name {
            pub const SER_ALIGN: bool = #align_val;
            pub const SER_CHECKSUM: Option<&'static str> = #checksum_expr;
            pub const SER_SEED: Option<u8> = #seed_expr;
            pub const SER_TENTATIVE: bool = #tentative_val;
            pub const SER_CONFIDENCE: Option<&'static str> = #confidence_expr;

            pub fn align_required() -> bool {
                #align_val
            }
            pub fn checksum_algorithm() -> Option<&'static str> {
                #checksum_expr
            }
            pub fn checksum_seed() -> Option<u8> {
                #seed_expr
            }
            pub fn is_tentative() -> bool {
                #tentative_val
            }
            pub fn confidence_level() -> Option<&'static str> {
                #confidence_expr
            }
        }
    };

    TokenStream::from(expanded)
}

/// Custom attribute for Category A bitstream slot geometry governance.
#[proc_macro_attribute]
pub fn rhythm_alignment(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2 = proc_macro2::TokenStream::from(attr);
    let item2 = proc_macro2::TokenStream::from(item);

    let input: syn::Item = match syn::parse2(item2.clone()) {
        Ok(parsed) => parsed,
        Err(err) => return err.to_compile_error().into(),
    };

    let mut width = None;
    let mut gap = None;
    let mut versions = Vec::new();

    let args_parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("width") {
            let value: syn::LitInt = meta.value()?.parse()?;
            let num: u32 = value.base10_parse()?;
            width = Some(num);
        } else if meta.path.is_ident("gap") {
            let value: syn::LitStr = meta.value()?.parse()?;
            gap = Some(value.value());
        } else if meta.path.is_ident("versions") {
            let value: syn::Expr = meta.value()?.parse()?;
            if let syn::Expr::Array(expr_array) = value {
                for elem in expr_array.elems {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Int(lit_int),
                        ..
                    }) = elem
                    {
                        let num: u32 = lit_int.base10_parse()?;
                        versions.push(num);
                    } else {
                        return Err(meta.error("versions array must contain only integer literals"));
                    }
                }
            } else {
                return Err(
                    meta.error("versions must be an array of integers, e.g. versions = [0, 1, 5]")
                );
            }
        } else {
            return Err(meta.error("unsupported rhythm_alignment property"));
        }
        Ok(())
    });

    if let Err(err) = args_parser.parse2(attr2) {
        return err.to_compile_error().into();
    }

    if width.is_none() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[rhythm_alignment] requires a 'width' parameter.",
        )
        .to_compile_error()
        .into();
    }

    if let Some(w) = width {
        if w == 0 {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "#[rhythm_alignment] 'width' must be greater than 0.",
            )
            .to_compile_error()
            .into();
        }
        if w % 2 != 0 {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "#[rhythm_alignment] 'width' must be an even number.",
            )
            .to_compile_error()
            .into();
        }
    }

    if let syn::Item::Struct(ref item_struct) = input {
        let name = &item_struct.ident;
        let width_val = width.unwrap();

        let gap_expr = match gap {
            Some(g) => quote! { Some(#g) },
            None => quote! { None },
        };

        let versions_expr = quote! { &[#(#versions),*] };

        let expanded = quote! {
            #item2

            impl #name {
                pub const RHYTHM_WIDTH: u32 = #width_val;
                pub const RHYTHM_GAP: Option<&'static str> = #gap_expr;
                pub const RHYTHM_VERSIONS: &'static [u32] = #versions_expr;

                pub fn slot_width() -> u32 {
                    #width_val
                }
                pub fn alignment_gap() -> Option<&'static str> {
                    #gap_expr
                }
                pub fn supported_versions() -> &'static [u32] {
                    #versions_expr
                }
            }
        };
        return TokenStream::from(expanded);
    }

    TokenStream::from(item2)
}

/// Custom attribute for Category C forensic sensing pipeline governance.
#[proc_macro_attribute]
pub fn forensic_sensor(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2 = proc_macro2::TokenStream::from(attr);
    let item2 = proc_macro2::TokenStream::from(item);

    let input: syn::Item = match syn::parse2(item2.clone()) {
        Ok(parsed) => parsed,
        Err(err) => return err.to_compile_error().into(),
    };

    let mut target = None;
    let mut trigger = None;
    let mut _bit_offset = None;
    let mut _label = None;

    let args_parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("target") {
            let value: syn::LitStr = meta.value()?.parse()?;
            target = Some(value.value());
        } else if meta.path.is_ident("trigger") {
            let value: syn::LitStr = meta.value()?.parse()?;
            trigger = Some(value.value());
        } else if meta.path.is_ident("bit_offset") {
            let value: syn::LitInt = meta.value()?.parse()?;
            let num: u64 = value.base10_parse()?;
            _bit_offset = Some(num);
        } else if meta.path.is_ident("label") {
            let value: syn::LitStr = meta.value()?.parse()?;
            _label = Some(value.value());
        } else {
            return Err(meta.error("unsupported forensic_sensor property"));
        }
        Ok(())
    });

    if let Err(err) = args_parser.parse2(attr2) {
        return err.to_compile_error().into();
    }

    if target.is_none() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[forensic_sensor] requires a 'target' parameter.",
        )
        .to_compile_error()
        .into();
    }

    if let Some(t) = &trigger {
        if t != "on_desync" && t != "always" {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "#[forensic_sensor] 'trigger' must be either \"on_desync\" or \"always\".",
            )
            .to_compile_error()
            .into();
        }
    } else {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[forensic_sensor] requires a 'trigger' parameter.",
        )
        .to_compile_error()
        .into();
    }

    if let syn::Item::Struct(ref item_struct) = input {
        let name = &item_struct.ident;
        let target_str = target.clone().unwrap();
        let target_ident = syn::Ident::new(&target_str, proc_macro2::Span::call_site());

        let trigger_expr = match &trigger {
            Some(t) => quote! { Some(#t) },
            None => quote! { None },
        };

        let label_expr = match &_label {
            Some(l) => quote! { Some(#l) },
            None => quote! { None },
        };

        let expanded = quote! {
            #item2

            impl #name {
                pub fn sensor_dump(&self) {
                    if std::env::var("D2R_FORENSIC").is_ok() {
                        let trigger_val = #trigger_expr;
                        let mut should_log = true;
                        if trigger_val == Some("on_desync") {
                            should_log = std::env::var("D2R_DESYNC").is_ok();
                        }
                        if should_log {
                            let label_str = #label_expr.unwrap_or("generic");
                            eprintln!("[FORENSIC-SENSOR] label: {}, target: {}, value: {:?}", label_str, #target_str, self.#target_ident);
                        }
                    }
                }
            }
        };
        return TokenStream::from(expanded);
    }

    TokenStream::from(item2)
}

/// Custom attribute for declared item parsing fidelity obligations.
#[proc_macro_attribute]
pub fn fidelity_contract(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2 = proc_macro2::TokenStream::from(attr);
    let item2 = proc_macro2::TokenStream::from(item);

    let input: syn::Item = match syn::parse2(item2.clone()) {
        Ok(parsed) => parsed,
        Err(err) => return err.to_compile_error().into(),
    };

    let mut metric_version = None;
    let mut format_family = None;
    let mut section = None;
    let mut preservation = None;
    let mut semantic_coverage = None;
    let mut owner = None;
    let mut required_proof = None;

    let args_parser = syn::meta::parser(|meta| {
        let (slot, key) = if meta.path.is_ident("metric_version") {
            (&mut metric_version, "metric_version")
        } else if meta.path.is_ident("format_family") {
            (&mut format_family, "format_family")
        } else if meta.path.is_ident("section") {
            (&mut section, "section")
        } else if meta.path.is_ident("preservation") {
            (&mut preservation, "preservation")
        } else if meta.path.is_ident("semantic_coverage") {
            (&mut semantic_coverage, "semantic_coverage")
        } else if meta.path.is_ident("owner") {
            (&mut owner, "owner")
        } else if meta.path.is_ident("required_proof") {
            (&mut required_proof, "required_proof")
        } else {
            return Err(meta.error("unsupported fidelity_contract property; use only the documented contract keys"));
        };

        if slot.is_some() {
            return Err(meta.error(format!(
                "duplicate fidelity_contract property '{key}'; specify each required key exactly once"
            )));
        }

        let value: syn::LitStr = meta.value()?.parse()?;
        *slot = Some(value.value());
        Ok(())
    });

    if let Err(err) = args_parser.parse2(attr2) {
        return err.to_compile_error().into();
    }

    let metric_version = match metric_version {
        Some(value) if !value.is_empty() => value,
        _ => return syn::Error::new(proc_macro2::Span::call_site(), "#[fidelity_contract] requires non-empty 'metric_version'. Self-Healing Hint: declare a stable version such as \"item_fidelity_v1\".").to_compile_error().into(),
    };
    let format_family = match format_family {
        Some(value) if !value.is_empty() => value,
        _ => return syn::Error::new(proc_macro2::Span::call_site(), "#[fidelity_contract] requires non-empty 'format_family'. Self-Healing Hint: declare the bounded save format family.").to_compile_error().into(),
    };
    let section = match section {
        Some(value) if !value.is_empty() => value,
        _ => return syn::Error::new(proc_macro2::Span::call_site(), "#[fidelity_contract] requires non-empty 'section'. Self-Healing Hint: name the stable parsing seam.").to_compile_error().into(),
    };
    let preservation = match preservation {
        Some(value) if matches!(value.as_str(), "exact" | "opaque" | "partial") => value,
        _ => return syn::Error::new(proc_macro2::Span::call_site(), "#[fidelity_contract] 'preservation' must be \"exact\", \"opaque\", or \"partial\". Self-Healing Hint: choose the declared preservation obligation, not an observed score.").to_compile_error().into(),
    };
    let semantic_coverage = match semantic_coverage {
        Some(value) if matches!(value.as_str(), "none" | "partial" | "expected") => value,
        _ => return syn::Error::new(proc_macro2::Span::call_site(), "#[fidelity_contract] 'semantic_coverage' must be \"none\", \"partial\", or \"expected\". Self-Healing Hint: declare the semantic obligation separately from preservation.").to_compile_error().into(),
    };
    let owner = match owner {
        Some(value) if !value.is_empty() => value,
        _ => return syn::Error::new(proc_macro2::Span::call_site(), "#[fidelity_contract] requires non-empty 'owner'. Self-Healing Hint: name the stable parser or marker owner.").to_compile_error().into(),
    };
    let required_proof = match required_proof {
        Some(value) if matches!(value.as_str(), "targeted_fixture" | "authority_fixture" | "corpus_regression") => value,
        _ => return syn::Error::new(proc_macro2::Span::call_site(), "#[fidelity_contract] 'required_proof' must be \"targeted_fixture\", \"authority_fixture\", or \"corpus_regression\". Self-Healing Hint: select the next required runtime proof.").to_compile_error().into(),
    };

    let name = match &input {
        syn::Item::Struct(item_struct) => &item_struct.ident,
        syn::Item::Enum(item_enum) => &item_enum.ident,
        syn::Item::Union(item_union) => &item_union.ident,
        _ => return syn::Error::new(proc_macro2::Span::call_site(), "Attribute #[fidelity_contract] can only be applied to structs, enums, or unions.").to_compile_error().into(),
    };

    TokenStream::from(quote! {
        #item2

        impl #name {
            pub const FIDELITY_METRIC_VERSION: &'static str = #metric_version;
            pub const FIDELITY_FORMAT_FAMILY: &'static str = #format_family;
            pub const FIDELITY_SECTION: &'static str = #section;
            pub const FIDELITY_PRESERVATION: &'static str = #preservation;
            pub const FIDELITY_SEMANTIC_COVERAGE: &'static str = #semantic_coverage;
            pub const FIDELITY_OWNER: &'static str = #owner;
            pub const FIDELITY_REQUIRED_PROOF: &'static str = #required_proof;
        }
    })
}
