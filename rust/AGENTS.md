# Bead Rust Codebase Guide

## Build/Test/Lint Commands
- Build: `cargo build` (debug) or `cargo build --release` (optimized)
- Test all: `cargo test`
- Test single: `cargo test test_name` or `cargo test --test integration_test_name`
- Lint: `cargo clippy -- -D warnings`
- Format: `cargo fmt` (check: `cargo fmt -- --check`)
- Check: `cargo check` (fast type checking without building)

## Project status (local memory)
- Test count: 112 total (102 passing, 10 failing)
- Major features implemented:
  - Archive extraction: extract_file, extract_dir, extract_all with path traversal protection
  - Workspace input management: add_input, load_input, unload_input, update_input, delete_input
  - BoxStore search: find_by_name, find_by_kind, find_by_content_id_prefix, find_by_ref,
    find_latest_by_name, find_newest_by_name, time filters; UnionBox variants
- Known issues:
  - Some workspace tests failing on macOS due to permission handling and directory recreation
  - Content ID in test archives is not persisted in metadata; tests simulate by matching prefixes
  - Kind comparison currently returns String; consider caching or OnceCell for meta fields

## Immediate TDD todo plan
1) Fix failing Workspace tests (permissions, parent dir recreation)
   - Ensure input/ is recreated when missing (before load)
   - Normalize permission handling on macOS; avoid brittle mode checks; guard with cfg(unix)
   - Make unload_input robust to readonly bits and hidden files
2) Harden BoxStore tests and impl
   - Ensure archive.kind() comparisons are strings; handle unknown
   - Store content_id in metadata cache when creating test archives for deterministic matches
3) Implement Archive::create from Workspace (next TDD milestone)
   - Write failing tests: creates proper ZIP with meta, manifest, input.map, code/, data/
   - Include comment, correct filename pattern, and cache file
   - Compute content hash; persist to cache and/or meta
4) CLI scaffolding (later)
   - clap-based skeleton; wire commands: new, save, input add/load/update, develop

## Design notes
- Prefer OnceCell and Mutex for lazy, thread-safe ZIP access
- Use chrono Timelike/Datelike traits explicitly in modules with time ops
- Path traversal defense: sanitize inputs and verify canonicalized paths remain under dest root
- File naming convention: `{name}_{YYYYMMDDTHHMMSSNNNNNN±ZZZZ}.zip`

## Testing strategy
- Unit tests live near code with #[cfg(test)]
- Edge cases emphasized: empty/large files, special chars, nested dirs, concurrency, read-only dirs
- Avoid reliance on platform-specific permission semantics where flaky

## Next steps snapshot
- Implement Archive::create via TDD (manifests, content hashing)
- Resolve remaining 10 failing tests
- Add BoxStore index cache (optional) for performance
