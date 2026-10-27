//! `#[fiex_sdk::filesystem]`: turn one `FileSystemProvider` impl into a
//! capability declaration plus a generated `fs.*` dispatcher.
//!
//! Rust cannot observe which trait default methods an impl overrides, so the
//! attribute macro reads the impl block instead. Only the methods written by
//! the plugin become capabilities; a default `unsupported` body is never
//! advertised.
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, spanned::Spanned, ImplItem, ItemImpl, Type};

/// (trait method, `FsOp` variant, `FsProduct` variant, returns `()`)
const OPS: &[(&str, &str, &str, bool)] = &[
    ("list", "List", "Listing", false),
    ("stat", "Stat", "Entry", false),
    ("read", "Read", "Read", false),
    ("write", "Write", "Write", false),
    ("mkdir", "Mkdir", "Entry", false),
    ("rename", "Rename", "Entry", false),
    ("remove", "Remove", "Done", true),
    ("search", "Search", "Listing", false),
];

#[proc_macro_attribute]
pub fn filesystem(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    let block = parse_macro_input!(item as ItemImpl);
    match expand(block) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand(block: ItemImpl) -> syn::Result<proc_macro2::TokenStream> {
    if let Some((_, path, _)) = &block.trait_ {
        let implemented = path.segments.last().map(|segment| segment.ident.to_string());
        if implemented.as_deref() != Some("FileSystemProvider") {
            return Err(syn::Error::new(
                path.span(),
                "#[fiex_sdk::filesystem] must be applied to `impl FileSystemProvider for ..`",
            ));
        }
    } else {
        return Err(syn::Error::new(
            block.self_ty.span(),
            "#[fiex_sdk::filesystem] requires a trait impl block",
        ));
    }

    let mut implemented = Vec::new();
    for item in &block.items {
        let ImplItem::Fn(function) = item else {
            continue;
        };
        let name = function.sig.ident.to_string();
        let Some(op) = OPS.iter().find(|(method, ..)| *method == name) else {
            return Err(syn::Error::new(
                function.sig.ident.span(),
                format!(
                    "`{name}` is not part of FileSystemProvider; \
                     the capability set is derived from the impl block, so unknown \
                     methods are rejected instead of silently ignored"
                ),
            ));
        };
        implemented.push(*op);
    }

    let self_ty = &block.self_ty;
    if matches!(**self_ty, Type::Infer(_)) {
        return Err(syn::Error::new(self_ty.span(), "invalid self type"));
    }
    let (impl_generics, _, where_clause) = block.generics.split_for_impl();

    let capabilities = implemented.iter().map(|(_, variant, ..)| {
        let variant = format_ident!("{variant}");
        quote!(::fiex_plugin_sdk::filesystem::FsOp::#variant)
    });
    let arms = implemented.iter().map(|(method, variant, product, unit)| {
        let method = format_ident!("{method}");
        let variant = format_ident!("{variant}");
        let call = quote!(::fiex_plugin_sdk::filesystem::FileSystemProvider::#method(self, request)?);
        let body = match (*product, *unit) {
            (_, true) => quote!({ #call; ::core::result::Result::Ok(::fiex_plugin_sdk::filesystem::FsProduct::Done) }),
            (product, false) => {
                let product = format_ident!("{product}");
                quote!(::core::result::Result::Ok(::fiex_plugin_sdk::filesystem::FsProduct::#product(#call)))
            }
        };
        quote!(::fiex_plugin_sdk::filesystem::FsOp::#variant => #body,)
    });

    Ok(quote! {
        #block

        impl #impl_generics ::fiex_plugin_sdk::filesystem::FsDispatch for #self_ty #where_clause {
            fn operations(&self) -> &'static [::fiex_plugin_sdk::filesystem::FsOp] {
                &[#(#capabilities),*]
            }

            fn invoke(
                &self,
                request: &::fiex_plugin_sdk::filesystem::FsRequest<'_>,
            ) -> ::core::result::Result<
                ::fiex_plugin_sdk::filesystem::FsProduct,
                ::fiex_plugin_sdk::RpcError,
            > {
                match request.op() {
                    #(#arms)*
                    other => ::core::result::Result::Err(
                        ::fiex_plugin_sdk::RpcError::method_not_found(other.as_str()),
                    ),
                }
            }
        }
    })
}
