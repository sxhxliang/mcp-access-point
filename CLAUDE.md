# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

### Build and Development
- `cargo build` - Compile the project (debug mode)
- `cargo build --release` - Compile optimized release build
- `cargo run -- -c config.yaml` - Run the gateway with local configuration
- `RUST_LOG=debug cargo run -- -c config.yaml` - Run with verbose debug logging
- `RUST_LOG=info,pingora_core=warn cargo run -- -c config.yaml` - Run with selective logging

### Testing and Quality
- `cargo test` - Run all unit tests
- `cargo test path::to::module::test_name` - Run specific test
- `cargo fmt --all` - Format code according to rustfmt rules
- `cargo fmt --all -- --check` - Check formatting without applying changes
- `cargo clippy --all-targets -- -D warnings` - Run linter with strict warning treatment

### Docker
- `docker build -t liangshihua/mcp-access-point:latest .` - Build Docker image
- `docker run -d --name mcp-access-point --rm -p 8080:8080 -e port=8080 -v /path/to/config.yaml:/app/config/config.yaml ghcr.io/sxhxliang/mcp-access-point:main` - Run container

### Debugging
- `npx @modelcontextprotocol/inspector node build/index.js` - Use MCP Inspector for debugging (after starting service)

## Architecture

### Core Components

**MCP Access Point** is a lightweight protocol conversion gateway built on Pingora that bridges HTTP services with MCP (Model Context Protocol) clients. The architecture consists of:

1. **Proxy Layer** (`src/proxy/`):
   - `mcp.rs` - Core MCP protocol proxy logic
   - `service.rs` - Service registration and management
   - `route.rs` - Request routing and path matching
   - `upstream.rs` - Backend service connection management
   - `ssl.rs` - TLS/SSL certificate handling
   - `discovery.rs` - Service discovery mechanisms

2. **MCP Bridge** (`src/mcp/`):
   - `tools.rs` - MCP tools interface implementation
   - `resources.rs` - Resource management and exposure
   - `prompts.rs` - Prompt handling and templating
   - `notifications.rs` - Event notification system
   - `sampling.rs` - Request sampling and metrics

3. **Service Layer** (`src/service/`):
   - `mcp.rs` - MCP service endpoint handlers
   - `endpoint.rs` - HTTP endpoint management

4. **Configuration** (`src/config/`):
   - `mcp.rs` - MCP service configuration
   - `upstream.rs` - Backend service configuration
   - `route.rs` - Routing rule configuration
   - `etcd.rs` - Distributed configuration via etcd
   - `control.rs` - Configuration control and validation

5. **Plugin System** (`src/plugin/`):
   - Authentication: `jwt_auth.rs`, `key_auth.rs`
   - Compression: `gzip.rs`, `brotli.rs`
   - Traffic: `limit_count.rs`, `ip_restriction.rs`
   - Observability: `prometheus.rs`, `file_logger.rs`
   - Protocol: `grpc_web.rs`, `cors.rs`

### Protocol Support

The gateway supports dual transport protocols:
- **SSE (Server-Sent Events)** - Access via `ip:port/sse` or `ip:port/api/{service_id}/sse`
- **Streamable HTTP** - Access via `ip:port/mcp` or `ip:port/api/{service_id}/mcp`

### Multi-tenancy

Services are configured in `config.yaml` with unique IDs enabling isolated access:
- All services: `0.0.0.0:8080/mcp` or `0.0.0.0:8080/sse`
- Individual services: `/api/{mcp-service-id}/mcp` or `/api/{mcp-service-id}/sse`

### Configuration Structure

The system uses YAML configuration with two main sections:
- `mcps[]` - Array of MCP service definitions with OpenAPI specs
- `upstreams[]` - Backend service connection details and load balancing

### Key Design Patterns

- **Zero-Intrusive Integration**: Existing HTTP services require no modifications
- **Protocol Conversion**: Seamless HTTP ↔ MCP protocol translation
- **OpenAPI-Driven**: Service capabilities auto-discovered from OpenAPI specifications
- **Pingora-Based**: Built on Cloudflare's high-performance proxy framework
- **Plugin Architecture**: Extensible middleware system for cross-cutting concerns

### Development Notes

- MSRV: Rust 1.85+ (specified in `clippy.toml`)
- Uses conventional commit messages (`feat:`, `fix:`, `docs:`, etc.)
- Tests are co-located with code using `#[cfg(test)]` modules
- Prefer `?` for error handling, avoid `unwrap()`/`expect()` outside tests
- 4-space indentation, snake_case naming conventions