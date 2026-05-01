extern crate alloc;

use proc_macro::TokenStream;

mod optionize;

#[proc_macro_attribute]
pub fn optionize(args: TokenStream, input: TokenStream) -> TokenStream {
    optionize::proc(args, input)
}
