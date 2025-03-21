//! [![github]](https://github.com/LittleBoxOfSunshine/conspiracy)&ensp;[![crates-io]](https://crates.io/crates/conspiracy)&ensp;[![docs-rs]](https://docs.rs/conspiracy)
//!
//! [github]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
//! [crates-io]: https://img.shields.io/badge/crates.io-fc8d62?style=for-the-badge&labelColor=555555&logo=rust
//! [docs-rs]: https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs
//!
//! <br>
//!
//! Conspiracy is an opinionated, extensible configuration crate that applies the "rust ethos" to
//! configuration. It shifts as much validation to compile time as possible, ensures state changes
//! are consistent, and guarantees constructed states are valid through:
//!
//! - Static typing
//! - Atomic updates
//! - Facilities for composition (depending on only a sub-config without resorting to dynamic typing)
//! - Abstractions for high-performance, lock-free updates
//!
//! # Concepts, Usage, and Examples
//!
//! See the module documentation for each concept:
//!
//! - Configuration: [`config`]
//! - Feature Control: [`feature_control`]
//!
//! # Future Work
//!
//! These crates are still experimental. Most updates should expect breaking changes.
//!
//! Planned features:
//!
//! - A universal configuration fetcher implementation for runtime configuration updates supporting
//!     - Layers
//!     - Serde inputs
//! - Dynamic evaluation of configuration based on environment context with "Flighting" DSL.
//! - Enable universal feature tracker to track against a config input, enabling dynamic values + reboot required support.
//! - Support factoring a config struct into multiple partial definitions.

pub mod config;
pub mod feature_control;
