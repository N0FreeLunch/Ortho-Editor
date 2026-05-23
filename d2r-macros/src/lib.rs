extern crate proc_macro;

use proc_macro::TokenStream;
use syn::parse::Parser;

/// Custom attribute for Category B bitstream serialization symmetry governance.
#[proc_macro_attribute]
pub fn serialization_symmetry(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2 = proc_macro2::TokenStream::from(attr);
    
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

    item
}

/// Custom attribute for Category A bitstream slot geometry governance.
#[proc_macro_attribute]
pub fn rhythm_alignment(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Custom attribute for Category C forensic sensing pipeline governance.
#[proc_macro_attribute]
pub fn forensic_sensor(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
