extern crate proc_macro;

use proc_macro::TokenStream;
use syn::parse::Parser;
use quote::quote;

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
            "Hashing verification requires post-eval byte alignment constraints.\nSelf-Healing Hint: Hashing verification requires post-eval byte alignment constraints. When 'align' is set to false, you cannot specify a 'checksum' parameter. To resolve this, either remove 'checksum' or set 'align = true'."
        );
        return err.to_compile_error().into();
    }

    // Constraint 2: If seed is some and checksum is none, return compile error
    if seed.is_some() && checksum.is_none() {
        let err = syn::Error::new(
            proc_macro2::Span::call_site(),
            "Checksum verification seed requires an explicitly bound algorithm parameter.\nSelf-Healing Hint: Checksum verification seed requires an explicitly bound algorithm parameter. If you specify a 'seed' parameter, you must also specify a valid 'checksum' algorithm parameter (e.g. checksum = \"Xor8\")."
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
                "Attribute #[serialization_symmetry] can only be applied to structs, enums, or unions."
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
pub fn rhythm_alignment(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Custom attribute for Category C forensic sensing pipeline governance.
#[proc_macro_attribute]
pub fn forensic_sensor(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2 = proc_macro2::TokenStream::from(attr);
    
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
            "#[forensic_sensor] requires a 'target' parameter."
        ).to_compile_error().into();
    }

    if let Some(t) = &trigger {
        if t != "on_desync" && t != "always" {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "#[forensic_sensor] 'trigger' must be either \"on_desync\" or \"always\"."
            ).to_compile_error().into();
        }
    } else {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[forensic_sensor] requires a 'trigger' parameter."
        ).to_compile_error().into();
    }

    item
}
