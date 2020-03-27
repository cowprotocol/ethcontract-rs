use anyhow::{anyhow, Result};
use curl::easy::Easy;
use ethcontract_common::Address;
use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;
use syn::Ident as SynIdent;

/// Expands a identifier string into an token.
pub fn ident(name: &str) -> Ident {
    Ident::new(name, Span::call_site())
}

/// Expands an identifier string into a token and appending `_` if the
/// identifier is for a reserved keyword.
///
/// Parsing keywords like `self` can fail, in this case we add an underscore.
pub fn safe_ident(name: &str) -> Ident {
    syn::parse_str::<SynIdent>(name).unwrap_or_else(|_| ident(&format!("{}_", name)))
}

/// Expands a doc string into an attribute token stream.
pub fn expand_doc(s: &str) -> TokenStream {
    let doc = Literal::string(s);
    quote! {
        #[doc = #doc]
    }
}

/// Parses the given address string
pub fn parse_address<S>(address_str: S) -> Result<Address>
where
    S: AsRef<str>,
{
    let address_str = address_str.as_ref();
    if !address_str.starts_with("0x") {
        return Err(anyhow!("address must start with '0x'"));
    }
    Ok(address_str[2..].parse()?)
}

/// Perform an HTTP GET request and return the contents of the response.
pub fn http_get(url: &str) -> Result<String> {
    let mut buffer = Vec::new();
    let mut handle = Easy::new();
    handle.url(url)?;
    {
        let mut transfer = handle.transfer();
        transfer.write_function(|data| {
            buffer.extend_from_slice(data);
            Ok(data.len())
        })?;
        transfer.perform()?;
    }

    let buffer = String::from_utf8(buffer)?;
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_address_missing_prefix() {
        if parse_address("0000000000000000000000000000000000000000").is_ok() {
            panic!("parsing address not starting with 0x should fail");
        }
    }

    #[test]
    fn parse_address_address_too_short() {
        if parse_address("0x00000000000000").is_ok() {
            panic!("parsing address not starting with 0x should fail");
        }
    }

    #[test]
    fn parse_address_ok() {
        let expected = Address::from([
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19,
        ]);
        assert_eq!(
            parse_address("0x000102030405060708090a0b0c0d0e0f10111213").unwrap(),
            expected
        );
    }
}
