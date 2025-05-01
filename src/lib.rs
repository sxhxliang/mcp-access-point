//! This module contains the core logic of the MCP Access Point API gateway.
//!
//! It defines the main modules for configuration, proxying, and service management.

pub mod admin;
pub mod config;
pub mod logging;
pub mod plugin;
pub mod proxy;
pub mod service;
pub(crate) mod utils;
pub(crate) mod mcp;
pub(crate) mod sse_event;
pub(crate) mod jsonrpc;
pub mod openapi;
pub(crate) mod types;
