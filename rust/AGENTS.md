# Bead Rust Codebase Guide

## Build/Test/Lint Commands
- **Build:** `cargo build` (debug) or `cargo build --release` (optimized)
- **Test all:** `cargo test`
- **Test single:** `cargo test test_name` or `cargo test --test integration_test_name`
- **Lint:** `cargo clippy -- -D warnings`
- **Format:** `cargo fmt` (check: `cargo fmt -- --check`)
- **Check:** `cargo check` (fast type checking without building)

## Code Style Guidelines
- **Rust edition:** 2021 (Cargo.toml specified)
- **Imports:** Group std, external crates, then local modules; use explicit paths
- **Error handling:** Use `thiserror` for custom errors, return `Result<T, BeadError>`
- **Module structure:** `mod.rs` for module roots, organize by domain (core/, tech/)
- **Naming:** snake_case for functions/variables, PascalCase for types, SCREAMING_SNAKE for constants
- **Traits:** Define common interfaces (e.g., `Bead` trait for workspaces/archives)
- **Testing:** Use `#[cfg(test)]` modules within source files, mock implementations for traits
- **Dependencies:** Prefer well-maintained crates (serde, clap, chrono, anyhow, thiserror)
- **Documentation:** Use /// for public items, //! for module-level docs
- **Type annotations:** Explicit for public APIs, infer for local variables when clear