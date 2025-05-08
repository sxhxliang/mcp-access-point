//! This module contains the core logic of the MCP Access Point API gateway.
//!
//! It defines the main modules for configuration, proxying, and service management.

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
pub(crate) mod utils;
