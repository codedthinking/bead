# Bead CLI Tool - Rust Implementation Specifications

## Overview
Bead is a data versioning and computational reproducibility tool that captures frozen computations in the form `output = function(*inputs)`. It manages dependencies between computational artifacts, tracks versions, and enables reproducible workflows.

## Core Concepts

### 1. Bead
A frozen computation unit containing:
- **Output data**: Result files from computation
- **Code**: Source files that produced the output
- **Inputs**: References to other beads (dependencies)
- **Metadata**: Kind, freeze time, content ID, name

### 2. Workspace
A working directory with structure:
```
workspace/
├── input/        # Read-only mounted input beads
├── output/       # Output data files
├── temp/         # Temporary files
├── .bead-meta/   # Metadata directory
│   ├── bead      # Main metadata file (JSON)
│   └── input.map # Input name mappings
└── [code files]  # User's source code
```

### 3. Box
A storage location (directory) containing bead archives (.zip files)

## Architecture Design

### Module Structure
```rust
bead/
├── src/
│   ├── main.rs           # CLI entry point
│   ├── lib.rs            # Library exports
│   ├── cli/
│   │   ├── mod.rs        # CLI command definitions
│   │   ├── workspace.rs  # Workspace commands
│   │   ├── input.rs      # Input management
│   │   ├── box_cmd.rs    # Box management
│   │   └── web.rs        # Visualization commands
│   ├── core/
│   │   ├── mod.rs
│   │   ├── bead.rs       # Bead data structures
│   │   ├── workspace.rs  # Workspace operations
│   │   ├── archive.rs    # Archive handling
│   │   ├── box_store.rs  # Box storage
│   │   └── meta.rs       # Metadata structures
│   ├── tech/
│   │   ├── mod.rs
│   │   ├── securehash.rs # Content hashing
│   │   ├── persistence.rs # JSON serialization
│   │   ├── timestamp.rs  # Timestamp handling
│   │   ├── fs.rs         # Filesystem operations
│   │   └── identifier.rs # UUID generation
│   └── web/
│       ├── mod.rs
│       ├── graph.rs      # Dependency graph
│       └── visualize.rs  # Graph visualization
```

## Data Structures

### Core Types
```rust
use std::collections::HashMap;
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

// Meta structures
#[derive(Serialize, Deserialize, Debug, Clone)]
struct BeadMeta {
    meta_version: String,
    kind: String,
    inputs: HashMap<String, InputSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    freeze_time: Option<String>,  // Only in archives
    #[serde(skip_serializing_if = "Option::is_none")]
    freeze_name: Option<String>,  // Only in archives
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct InputSpec {
    kind: String,
    content_id: String,
    freeze_time: String,
}

// Core structures
struct Bead {
    name: BeadName,
    kind: String,
    inputs: Vec<InputSpec>,
    content_id: String,
    freeze_time: DateTime<Utc>,
    box_name: String,
}

struct Workspace {
    directory: PathBuf,
    meta: BeadMeta,
    input_map: HashMap<String, String>,
}

struct Archive {
    path: PathBuf,
    cache: HashMap<String, serde_json::Value>,
    zipfile: Option<zip::ZipArchive<std::fs::File>>,
}

struct Box {
    name: String,
    location: PathBuf,
}

// Type safety wrappers
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BeadName(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ContentId(String);

impl BeadName {
    fn is_valid(&self) -> bool {
        let s = &self.0;
        !s.is_empty() 
            && s != "." 
            && s != ".."
            && !s.contains('/')
            && !s.contains("__")
    }
}
```

## Command Specifications

### Main Commands

#### `bead new <workspace>`
- Create new workspace directory
- Initialize with UUID kind (using uuid v4)
- Create directory structure
- Write initial metadata
- Implementation:
  ```rust
  fn cmd_new(workspace: &str) -> Result<()> {
      let path = PathBuf::from(workspace);
      if path.exists() {
          return Err(BeadError::AlreadyExists);
      }
      let kind = uuid::Uuid::new_v4().to_string();
      create_workspace(&path, &kind)?;
      println!("Created \"{}\"", workspace);
      Ok(())
  }
  ```

#### `bead develop <bead-ref> [workspace]`
- Find bead in boxes by name/kind/content-id prefix
- Extract code to workspace
- Set up metadata and input references
- Optionally extract output with `-x/--extract-output`
- Support `--time` for version selection

#### `bead save [box-name]`
- Pack workspace to ZIP archive
- Calculate content hash using SHA-512
- Store in specified box (or default if only one)
- Generate filename: `{name}_{timestamp}.zip`
- Add archive comment with bead description

#### `bead status [workspace]`
- Show workspace info (kind, name, path)
- List inputs and their load status
- Display timestamps and content IDs with `-v/--verbose`
- Check for outdated inputs

#### `bead zap <workspace>`
- Delete workspace completely
- Safety check for uncommitted changes
- Remove all files and directories

#### `bead version`
- Display version info
- Show git commit if development build
- Include Python equivalent version for compatibility

### Input Commands

#### `bead input add <nick> <bead-ref>`
- Define new input dependency
- Load data into `input/<nick>` directory
- Update workspace metadata
- Validate bead exists in boxes

#### `bead input update [nick] [bead-ref]`
- Update to newest version by default
- Support `--time LATEST|NEWEST|<timestamp>` for specific versions
- Support `--next` and `--prev` for relative updates
- Update all inputs if no nick specified
- Preserve loaded state

#### `bead input load <nick>`
- Load already defined input
- Extract data from archive to `input/<nick>`
- Make directory read-only
- Verify content hash

#### `bead input unload <nick>`
- Remove input data from filesystem
- Keep metadata reference intact
- Free disk space while preserving dependency info

#### `bead input delete <nick>`
- Remove input completely
- Delete both data and metadata
- Unload first if loaded

#### `bead input map <nick> <bead-name>`
- Change bead name mapping for updates
- Update `.bead-meta/input.map` file
- Enable switching between bead branches

### Box Commands

#### `bead box add <name> <location>`
- Register new box in user config
- Validate location exists and is directory
- Store in platform-specific config dir

#### `bead box list`
- Show all configured boxes
- Display names and locations
- Check accessibility

#### `bead box forget <name>`
- Remove box from configuration
- Don't delete actual box directory

#### `bead box rewire <box-name>`
- Fix broken input references
- Scan for matching beads by content
- Update input maps in archives

### Web/Visualization Commands

#### `bead web ...`
Pipeline-based graph operations with subcommands:

- `load <file.web>` - Load saved graph
- `save <file.web>` - Save current graph
- `/ <sources> .. <sinks> /` - Filter by connections
- `png <file.png>` - Generate PNG visualization
- `svg <file.svg>` - Generate SVG visualization
- `color` - Add freshness coloring
- `auto-rewire` - Auto-fix broken links
- `rewire-options <file.json>` - Generate rewiring options
- `rewire <file.json>` - Apply rewiring
- `heads` - Show only latest versions
- `view <file>` - Open in browser

## Technical Implementation Details

### Content Hashing
```rust
// SHA-512 with netstring-like format
fn calculate_content_id(files: &[(PathBuf, Vec<u8>)]) -> String {
    use sha2::{Sha512, Digest};
    
    let mut hasher = Sha512::new();
    for (path, content) in files {
        let size = content.len();
        hasher.update(format!("{}:", size).as_bytes());
        hasher.update(content);
        hasher.update(format!(";{}", size).as_bytes());
    }
    format!("{:x}", hasher.finalize())
}
```

### Archive Format
Standard ZIP file structure:
```
archive.zip
├── meta/
│   ├── bead      # JSON metadata
│   ├── manifest  # File hashes {path: content_id}
│   └── input.map # Input name mappings
├── code/         # Source files (all non-ignored files)
└── data/         # Output files (from output/ directory)
```

ZIP comment format:
```
This file is a BEAD zip archive.

It is a normal zip file that stores a discrete computation of the form

    output = code(*inputs)

...
```

### Persistence
- JSON for all metadata with specific formatting:
  - UTF-8 encoding
  - 4-space indentation
  - Sorted keys
  - ASCII-only output
- Example:
  ```rust
  fn save_json<T: Serialize>(data: &T, path: &Path) -> Result<()> {
      let file = std::fs::File::create(path)?;
      serde_json::to_writer_pretty(file, data)?;
      Ok(())
  }
  ```

### Timestamp Format
- ISO 8601 with microsecond precision
- Format: `YYYYMMDDTHHMMSSNNNNNN±ZZZZ`
- Example: `20240115T143022123456+0100`
- Implementation:
  ```rust
  fn timestamp() -> String {
      let now = chrono::Utc::now();
      now.format("%Y%m%dT%H%M%S%6f%z").to_string()
  }
  ```

### File Naming Convention
- Archive: `{bead-name}_{timestamp}.zip`
- Cache: `{archive-name}.xmeta`
- Error logs: `error_{timestamp}.txt`
- Parsing regex: `^(.+?)_(\d{8}T[\d+-]+)\.zip$`

## Configuration

### Environment Setup
```rust
use directories::ProjectDirs;

fn get_config_dir() -> PathBuf {
    ProjectDirs::from("", "", "bead")
        .expect("Could not determine config directory")
        .config_dir()
        .to_path_buf()
}

struct Config {
    boxes: Vec<Box>,
}
```

### Dependencies (Cargo.toml)
```toml
[package]
name = "bead"
version = "0.8.2"
edition = "2021"
authors = ["Your Name"]
license = "MIT OR Apache-2.0"
description = "Linked frozen computations - Rust implementation"

[dependencies]
clap = { version = "4", features = ["derive", "env", "cargo"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = "0.4"
uuid = { version = "1", features = ["v4", "serde"] }
sha2 = "0.10"
zip = "0.6"
directories = "5"
anyhow = "1"
thiserror = "1"
cached = "0.46"
glob = "0.3"
regex = "1"
tempfile = "3"
walkdir = "2"
rayon = "1.7"  # For parallel operations

[dev-dependencies]
tempdir = "0.3"
assert_cmd = "2"
predicates = "3"
```

## Error Handling

### Error Types
```rust
use thiserror::Error;

#[derive(Error, Debug)]
enum BeadError {
    #[error("Invalid workspace: {0}")]
    InvalidWorkspace(String),
    
    #[error("Box not found: {0}")]
    BoxNotFound(String),
    
    #[error("Bead not found: {0}")]
    BeadNotFound(String),
    
    #[error("Invalid archive: {0}")]
    InvalidArchive(String),
    
    #[error("Already exists: {0}")]
    AlreadyExists(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    
    #[error("ZIP error: {0}")]
    ZipError(#[from] zip::result::ZipError),
}

type Result<T> = std::result::Result<T, BeadError>;
```

### User-Friendly Error Messages
```rust
fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        if let Ok("1") = std::env::var("RUST_BACKTRACE").as_deref() {
            eprintln!("Backtrace:\n{:?}", e);
        }
        std::process::exit(1);
    }
}
```

## Performance Optimizations

### 1. Lazy Loading
```rust
struct Archive {
    path: PathBuf,
    cache: HashMap<String, Value>,
    zipfile: OnceCell<ZipArchive<File>>,  // Load only when needed
}
```

### 2. Metadata Caching
- Cache metadata in `.xmeta` files alongside archives
- Validate cache against archive modification time
- Store content_id, inputs, kind, freeze_time

### 3. Parallel Operations
```rust
use rayon::prelude::*;

fn hash_files(paths: Vec<PathBuf>) -> Vec<(PathBuf, String)> {
    paths.par_iter()
        .map(|path| {
            let hash = calculate_file_hash(path)?;
            Ok((path.clone(), hash))
        })
        .collect()
}
```

### 4. Memory-Mapped Files
Consider for large file operations:
```rust
use memmap2::MmapOptions;

fn hash_large_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mmap = unsafe { MmapOptions::new().map(&file)? };
    calculate_hash(&mmap[..])
}
```

## Platform Support

### Cross-Platform Path Handling
```rust
fn normalize_path(path: &Path) -> PathBuf {
    // Convert to forward slashes for archives
    let path_str = path.to_string_lossy().replace('\\', "/");
    PathBuf::from(path_str)
}
```

### Platform-Specific Features
- **Unix/Linux**: Set read-only permissions with mode 0o444
- **macOS**: Handle case-insensitive filesystem
- **Windows**: Long path support (>260 chars)

## Testing Strategy

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bead_name_validation() {
        assert!(BeadName("valid-name".into()).is_valid());
        assert!(!BeadName("".into()).is_valid());
        assert!(!BeadName("../parent".into()).is_valid());
    }
    
    #[test]
    fn test_content_hash() {
        let files = vec![
            (PathBuf::from("test.txt"), b"content".to_vec())
        ];
        let hash = calculate_content_id(&files);
        assert_eq!(hash.len(), 128);  // SHA-512 hex length
    }
}
```

### Integration Tests
```rust
// tests/integration.rs
use assert_cmd::Command;
use tempdir::TempDir;

#[test]
fn test_new_workspace() {
    let temp = TempDir::new("test").unwrap();
    let workspace = temp.path().join("test-bead");
    
    Command::cargo_bin("bead")
        .arg("new")
        .arg(&workspace)
        .assert()
        .success();
    
    assert!(workspace.exists());
    assert!(workspace.join("input").exists());
}
```

### Filesystem Tests
```rust
use tempfile::tempdir;

#[test]
fn test_workspace_creation() {
    let dir = tempdir().unwrap();
    let workspace_path = dir.path().join("workspace");
    
    create_workspace(&workspace_path, "test-kind").unwrap();
    
    assert!(workspace_path.join(".bead-meta/bead").exists());
    assert!(workspace_path.join("input").is_dir());
}
```

## Migration Path

### Python Compatibility
1. **Metadata Format**: Exact JSON structure match
2. **Hash Algorithm**: Same SHA-512 netstring format
3. **Archive Layout**: Identical ZIP structure
4. **Timestamp Format**: Compatible ISO 8601 format

### Migration Tool
```rust
fn verify_python_compatibility(archive: &Path) -> Result<()> {
    let mut zip = ZipArchive::new(File::open(archive)?)?;
    
    // Check for required files
    assert!(zip.by_name("meta/bead").is_ok());
    assert!(zip.by_name("meta/manifest").is_ok());
    
    // Verify metadata structure
    let meta: BeadMeta = read_json_from_zip(&mut zip, "meta/bead")?;
    validate_meta_version(&meta.meta_version)?;
    
    Ok(())
}
```

## Security Considerations

### Path Traversal Prevention
```rust
fn safe_extract_path(base: &Path, relative: &Path) -> Result<PathBuf> {
    let path = base.join(relative);
    let canonical = path.canonicalize()?;
    
    if !canonical.starts_with(base.canonicalize()?) {
        return Err(BeadError::InvalidPath("Path traversal detected".into()));
    }
    
    Ok(canonical)
}
```

### Archive Validation
```rust
fn validate_archive(archive: &mut ZipArchive<File>) -> Result<()> {
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        if file.name().contains("..") {
            return Err(BeadError::InvalidArchive("Unsafe path in archive".into()));
        }
    }
    Ok(())
}
```

### Permission Management
```rust
#[cfg(unix)]
fn make_readonly(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = path.metadata()?.permissions();
    perms.set_mode(0o444);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}
```

## Future Enhancements

### 1. Remote Box Support
```rust
enum BoxLocation {
    Local(PathBuf),
    S3(S3Config),
    Http(url::Url),
}
```

### 2. Streaming Operations
```rust
async fn stream_large_bead(url: &str) -> Result<impl Stream<Item = Result<Bytes>>> {
    // Stream download for large beads
}
```

### 3. Index Files
```rust
struct BoxIndex {
    beads: Vec<BeadSummary>,
    updated: DateTime<Utc>,
}
```

### 4. Incremental Updates
```rust
fn update_incremental(workspace: &Workspace, input: &str) -> Result<()> {
    // Only transfer changed files
}
```

## CLI Interface (using clap)

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(name = "bead")]
#[clap(version = env!("CARGO_PKG_VERSION"))]
#[clap(about = "Linked frozen computations")]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create and initialize new workspace
    New {
        /// Workspace directory to create
        workspace: PathBuf,
    },
    
    /// Create workspace from specified bead
    Develop {
        /// Bead reference (name, kind, or content-id prefix)
        bead_ref: String,
        
        /// Workspace directory (defaults to bead name)
        workspace: Option<PathBuf>,
        
        /// Extract output data as well
        #[clap(short = 'x', long)]
        extract_output: bool,
        
        /// Time specification (LATEST, NEWEST, or timestamp)
        #[clap(long, default_value = "LATEST")]
        time: String,
    },
    
    /// Save workspace in a box
    Save {
        /// Box name (uses default if only one box exists)
        box_name: Option<String>,
        
        /// Workspace directory
        #[clap(short, long, default_value = ".")]
        workspace: PathBuf,
    },
    
    /// Manage inputs
    #[clap(subcommand)]
    Input(InputCommands),
    
    /// Manage boxes
    #[clap(subcommand)]
    Box(BoxCommands),
    
    // ... other commands
}

#[derive(Subcommand)]
enum InputCommands {
    /// Add new input dependency
    Add {
        /// Input nickname
        nick: String,
        /// Bead reference
        bead_ref: Option<String>,
    },
    // ... other input commands
}
```

## Compatibility Notes

### Differences from Python Implementation
1. **Performance**: Faster hashing and parallel operations
2. **Memory Usage**: Lower memory footprint with streaming
3. **Type Safety**: Compile-time validation of data structures
4. **Error Handling**: More explicit error types

### Maintaining Compatibility
1. **File Formats**: Exact binary compatibility
2. **JSON Schema**: Identical field names and types
3. **CLI Interface**: Same command structure and flags
4. **Box Layout**: Same directory and naming conventions

## Development Workflow

### Building
```bash
cargo build --release
```

### Testing
```bash
cargo test
cargo test --integration
```

### Benchmarking
```bash
cargo bench
```

### Cross-Compilation
```bash
# For Windows from Linux/Mac
cargo build --target x86_64-pc-windows-gnu

# For Linux from Mac
cargo build --target x86_64-unknown-linux-gnu
```

## Documentation

### API Documentation
```bash
cargo doc --open
```

### User Guide
- Command reference with examples
- Tutorial for common workflows
- Migration guide from Python version

## Release Process

1. Update version in Cargo.toml
2. Run full test suite
3. Build for all platforms
4. Create GitHub release
5. Publish to crates.io

```bash
cargo publish --dry-run
cargo publish
```

## Conclusion

This specification provides a complete blueprint for reimplementing the Bead CLI tool in Rust. The implementation maintains full compatibility with the existing Python version while offering improved performance, better error handling, and stronger type safety. The modular architecture allows for future enhancements while preserving the core functionality and file formats.