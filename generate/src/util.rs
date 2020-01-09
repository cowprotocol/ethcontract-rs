use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;

pub fn ident(name: &str) -> Ident {
    Ident::new(name, Span::call_site())
}

pub fn expand_doc(s: &str) -> TokenStream {
    let doc = Literal::string(s);
    quote! {
        #[doc = #doc]
    }
}
