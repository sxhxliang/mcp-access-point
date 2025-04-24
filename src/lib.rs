//! This module contains the core logic of the MCP Access Point API gateway.
//!
//! It defines the main modules for configuration, proxying, and service management.

pub mod admin;
pub mod config;
pub mod logging;
pub mod plugin;
pub mod proxy;
pub mod service;
pub mod utils;
pub mod mcp;
pub mod sse_event;
pub mod jsonrpc;
pub mod openapi;
pub mod types;
