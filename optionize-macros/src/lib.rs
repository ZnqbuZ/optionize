use proc_macro::TokenStream;

mod optionize;

#[proc_macro_attribute]
pub fn optionized(args: TokenStream, input: TokenStream) -> TokenStream {
    optionize::proc(args.into(), input.clone().into())
        .unwrap_or_else(|e| {
            let mut e = e.write_errors();
            e.extend(proc_macro2::TokenStream::from(input));
            e
        })
        .into()
}

#[proc_macro_derive(Optionize, attributes(optionize))]
pub fn derive(input: TokenStream) -> TokenStream {
    optionize::derive(input.into())
        .unwrap_or_else(|e| e.write_errors())
        .into()
}
