// Copyright 2018-2023 the Deno authors. All rights reserved. MIT license.
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::Error;
use syn::Ident;
use syn::Result;
use syn::Token;

#[derive(Clone, Debug, Default)]
pub struct Attributes {
  pub is_v8: bool,
  pub deferred: bool,
  pub is_wasm: bool,
}

impl Parse for Attributes {
  fn parse(input: ParseStream) -> Result<Self> {
    let mut self_ = Self::default();
    while let Ok(v) = input.parse::<Ident>() {
      match v.to_string().as_str() {
        "v8" => self_.is_v8 = true,
        "deferred" => self_.deferred = true,
        "wasm" => self_.is_wasm = true,
        _ => {
          return Err(Error::new(
             input.span(),
            "invalid attribute, expected one of: v8, deferred, wasm",
            ));
        }
      };
      let _ = input.parse::<Token![,]>();
    }

    Ok(self_)
  }
}
