# Repository Guidelines

## Project Structure & Module Organization
- `src/` core Rust code: `mcp/` (MCP bridge), `proxy/` (Pingora integration, routing, TLS), `service/` (MCP/HTTP endpoints, admin), `config/` (YAML + etcd), `plugin/` (auth, gzip), `utils/`, `logging/`, `main.rs` (binary), `lib.rs`.
- `config/` sample OpenAPI specs; `config.yaml` quick-start server config.
- `examples/` runnable examples and OpenAPI snippets; `demo/`, `experience/`, `web/` contain supporting assets.
- Unit tests live next to modules inside `src/`.

## Build, Test, and Development Commands
- `cargo build` / `cargo build --release` — compile (debug/release).
- `cargo run -- -c config.yaml` — start the gateway with local config.
- `RUST_LOG=debug cargo run -- -c config.yaml` — verbose logs.
- `cargo test` — run all unit tests.
- `cargo fmt --all -- --check` — verify formatting; use `cargo fmt --all` to fix.
- `cargo clippy --all-targets -- -D warnings` — lint (MSRV 1.85 per `clippy.toml`).

## Coding Style & Naming Conventions
- Rust 2021; 4‑space indent; format with `rustfmt`.
- Naming: snake_case for files/modules/functions; CamelCase types; SCREAMING_SNAKE consts.
- Prefer `?` for error propagation; avoid `unwrap()`/`expect()` outside tests (allowed in tests via `clippy.toml`).
- Keep modules focused; follow existing folder layout when adding features.

## Testing Guidelines
- Co‑locate tests with code using `#[cfg(test)] mod tests { ... }`.
- Use descriptive names (e.g., `should_parse_path_params`).
- Keep tests deterministic and offline; mock inputs/outputs where possible.
- Run subsets with `cargo test path::to::module::test_name`.

## Commit & Pull Request Guidelines
- Use Conventional Commits: `feat:`, `fix:`, `docs:`, `refactor:`, `perf:`, `build(deps):` (see `git log`).
- PRs must include: what/why, how to test (commands + config used), and linked issues.
- Ensure `cargo fmt` and `cargo clippy` pass; update `README.md` and config examples when behavior or config changes.

## Security & Configuration Tips
- Do not commit secrets; sanitize `config.yaml` (API keys, tokens) in examples.
- Prefer mounting configs in Docker over embedding secrets; use `port` env var to set the listen port.
- Control log verbosity with `RUST_LOG=info,pingora_core=warn` during local runs.

