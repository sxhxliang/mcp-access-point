//! This module contains the core logic of the MCP Access Point API gateway.
//!
//! It defines the main modules for configuration, proxying, and service management.
#![allow(unknown_lints, bare_trait_objects, deprecated, unused_variables)]
// Ignored clippy and clippy_pedantic lints
#![allow(
    // clippy bug: https://github.com/rust-lang/rust-clippy/issues/5704
    clippy::unnested_or_patterns,
    // clippy bug: https://github.com/rust-lang/rust-clippy/issues/7768
    clippy::semicolon_if_nothing_returned,
    // not available in our oldest supported compiler
    clippy::empty_enum,
    clippy::type_repetition_in_bounds, // https://github.com/rust-lang/rust-clippy/issues/8772
    // integer and float ser/de requires these sorts of casts
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    // things are often more readable this way
    clippy::cast_lossless,
    clippy::module_name_repetitions,
    clippy::single_match_else,
    clippy::type_complexity,
    clippy::use_self,
    clippy::zero_prefixed_literal,
    // correctly used
    clippy::derive_partial_eq_without_eq,
    clippy::enum_glob_use,
    clippy::explicit_auto_deref,
    clippy::incompatible_msrv,
    clippy::let_underscore_untyped,
    clippy::map_err_ignore,
    clippy::new_without_default,
    clippy::result_unit_err,
    clippy::wildcard_imports,
    // not practical
    clippy::needless_pass_by_value,
    clippy::similar_names,
    clippy::too_many_lines,
    // preference
    clippy::doc_markdown,
    clippy::elidable_lifetime_names,
    clippy::needless_lifetimes,
    clippy::unseparated_literal_suffix,
    // false positive
    clippy::needless_doctest_main,
    // noisy
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::enum_variant_names,
    clippy::inherent_to_string,
    clippy::enum_variant_names,
)]
// Restrictions
// #![deny(clippy::question_mark_used)]
// Rustc lints.
// #![deny(missing_docs, unused_imports)]
////////////////////////////////////////////////////////////////////////////////
/// admin control
pub mod admin;
pub mod config;
pub(crate) mod jsonrpc;
pub mod logging;
pub(crate) mod mcp;
pub mod openapi;
pub(crate) mod plugin;
pub mod proxy;
pub mod service;
pub(crate) mod sse_event;
pub(crate) mod types;
pub mod utils;
