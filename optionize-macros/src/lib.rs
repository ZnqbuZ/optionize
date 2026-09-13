use proc_macro::TokenStream;
use quote::quote;

mod optionize;

#[proc_macro_attribute]
pub fn optionized(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = input.into();
    optionize::proc(args.into(), &input)
        .unwrap_or_else(|e| {
            let e = e.write_errors();
            quote! {
                #input
                #e
            }
        })
        .into()
}

#[proc_macro_derive(Optionize, attributes(optionize))]
pub fn derive(_: TokenStream) -> TokenStream {
    Default::default()
}

// Derive inputs have already been processed by rustc's cfg expansion. Discard
// the intermediate declaration and re-emit that input for the attribute macro,
// which may still need to remove skipped fields from an existing object.
#[doc(hidden)]
#[proc_macro_derive(Prepare, attributes(optionize))]
pub fn prepare(input: TokenStream) -> TokenStream {
    let mut input = syn::parse_macro_input!(input as syn::DeriveInput);
    input.attrs.remove(0); // The discard attribute inserted by optionized.
    quote!(#input).into()
}

#[doc(hidden)]
#[proc_macro_attribute]
pub fn discard(_: TokenStream, _: TokenStream) -> TokenStream {
    TokenStream::new()
}

#[doc(hidden)]
#[proc_macro_attribute]
pub fn expand(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = input.into();
    optionize::expand(args.into(), &input)
        .unwrap_or_else(|error| error.write_errors())
        .into()
}
