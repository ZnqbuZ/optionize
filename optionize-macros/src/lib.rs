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
