use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, parse_macro_input};

#[proc_macro_derive(Payload)]
pub fn derive_payload(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    let variants = match &ast.data {
        Data::Enum(data_enum) => &data_enum.variants,
        _ => {
            return Error::new_spanned(ast, "#[derive(Payload)] can only be used with enums")
                .to_compile_error()
                .into();
        }
    };

    if !variants.iter().any(|v| v.ident == "Created") {
        return Error::new_spanned(
            name,
            "The enum must have a variant named 'Created' to derive Payload",
        )
        .to_compile_error()
        .into();
    }

    let created_variant = variants
        .iter()
        .find(|v| v.ident == "Created")
        .expect("the created variant to exist");

    // Determine the correct pattern based on the field type
    use syn::Fields::*;
    let pattern = match &created_variant.fields {
        Named(_) => quote! { #name::Created { .. } }, // Struct-like
        Unnamed(_) => quote! { #name::Created(..) },  // Tuple-like
        Unit => quote! { #name::Created },            // Unit-like
    };

    let expanded = quote! {
        impl crate::store::universal::Payload for #name {
            fn creates_entity(&self) -> bool {
                matches!(self, #pattern)
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(Projection)]
pub fn derive_projection(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    let expanded = quote! {
        impl crate::store::universal::Projection for #name {}
    };

    TokenStream::from(expanded)
}
