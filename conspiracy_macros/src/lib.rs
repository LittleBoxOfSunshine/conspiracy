//! [![github]](https://github.com/LittleBoxOfSunshine/conspiracy)&ensp;[![crates-io]](https://crates.io/crates/conspiracy_macros)&ensp;[![docs-rs]](https://docs.rs/conspiracy_macros)
//!
//! [github]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
//! [crates-io]: https://img.shields.io/badge/crates.io-fc8d62?style=for-the-badge&labelColor=555555&logo=rust
//! [docs-rs]: https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs
//!
//! <br>
//!
//! This crate is not intended to be consumed directly! It contains the proc macros re-exported by
//! [`conspiracy`](https://crates.io/crates/conspiracy). This is an artifact of that proc macros
//! have to be in a separate crate. To use these macros, take a dependency on the re-exports.
//!
//! The re-exports provide the rustdoc for each macro.

use proc_macro::{self, TokenStream};

mod config;
mod feature_control;

#[proc_macro]
pub fn config_struct(item: TokenStream) -> TokenStream {
    config::config_struct(item)
}

#[proc_macro]
pub fn define_features(item: TokenStream) -> TokenStream {
    feature_control::define_features(item)
}

#[proc_macro]
pub fn feature_enabled(item: TokenStream) -> TokenStream {
    feature_control::feature_enabled(item)
}

#[proc_macro]
pub fn feature_enabled_or(item: TokenStream) -> TokenStream {
    feature_control::feature_enabled_or(item)
}

#[proc_macro]
pub fn feature_enabled_or_default(item: TokenStream) -> TokenStream {
    feature_control::feature_enabled_or_default(item)
}

#[proc_macro]
pub fn try_feature_enabled(item: TokenStream) -> TokenStream {
    feature_control::try_feature_enabled(item)
}
