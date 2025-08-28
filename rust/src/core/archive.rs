use std::path::{Path, PathBuf};
use std::fs::{self, File};
use std::io;

use std::collections::HashMap;
use chrono::{DateTime, Utc};
use once_cell::sync::OnceCell;
use zip::ZipArchive;

use crate::error::{BeadError, Result};
use crate::core::meta::{BeadMeta, BeadName, ContentId, InputSpec};
use crate::core::workspace::Workspace;
use crate::tech::{persistence, timestamp};

/// Layout constants for archive structure
pub mod layout {
    pub const META_DIR: &str = "meta";
    pub const CODE_DIR: &str = "code";
    pub const DATA_DIR: &str = "data";
    
    pub const BEAD_META: &str = "meta/bead";
    pub const MANIFEST: &str = "meta/manifest";
    pub const INPUT_MAP: &str = "meta/input.map";
}

/// Represents a frozen bead archive (ZIP file)
#[derive(Debug)]
pub struct Archive {
    path: PathBuf,
    box_name: String,
    name: BeadName,
    cache: HashMap<String, serde_json::Value>,
    cache_path: PathBuf,
    zipfile: OnceCell<std::sync::Mutex<ZipArchive<File>>>,
    meta: OnceCell<BeadMeta>,
}

impl Archive {
    /// Open an existing archive
    pub fn open(path: impl AsRef<Path>, box_name: &str) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        
        if !path.exists() {
            return Err(BeadError::InvalidArchive(
                format!("Archive does not exist: {}", path.display())
            ));
        }

        // Parse bead name from filename
        let name = Self::parse_name_from_path(&path)?;
        
        // Determine cache path (.xmeta file)
        let cache_path = path.with_extension("xmeta");
        
        // Load cache if exists
        let cache = if cache_path.exists() {
            persistence::load_json(&cache_path).unwrap_or_default()
        } else {
            HashMap::new()
        };

        Ok(Archive {
            path: path.clone(),
            box_name: box_name.to_string(),
            name,
            cache,
            cache_path,
            zipfile: OnceCell::new(),
            meta: OnceCell::new(),
        })
    }

    /// Create a new archive from a workspace
    pub fn create(
        path: impl AsRef<Path>,
        workspace: &Workspace,
        freeze_time: String,
        comment: &str,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        
        if path.exists() {
            return Err(BeadError::AlreadyExists(
                format!("Archive already exists: {}", path.display())
            ));
        }

        // Create ZIP file
        let file = File::create(&path)?;
        let mut zip = zip::ZipWriter::new(file);
        
        // Set archive comment
        zip.set_comment(comment);
        
        // Add metadata
        let mut meta = workspace.meta.clone();
        meta.freeze_time = Some(freeze_time.clone());
        meta.freeze_name = Some(workspace.name().unwrap_or("unnamed").to_string());
        
        persistence::save_json_to_zip(&mut zip, &meta, layout::BEAD_META)?;
        
        // Add manifest (simplified - would hash all files in real impl)
        let manifest: HashMap<String, String> = HashMap::new();
        persistence::save_json_to_zip(&mut zip, &manifest, layout::MANIFEST)?;
        
        // Add input map
        persistence::save_json_to_zip(&mut zip, &workspace.input_map, layout::INPUT_MAP)?;
        
        // TODO: Add code and data files
        
        zip.finish()?;
        
        // Create and return Archive instance
        let name = BeadName::new(workspace.name().unwrap_or("unnamed"))?;
        Ok(Archive {
            path: path.clone(),
            box_name: String::new(),
            name,
            cache: HashMap::new(),
            cache_path: path.with_extension("xmeta"),
            zipfile: OnceCell::new(),
            meta: OnceCell::from(meta),
        })
    }

    /// Get the archive's metadata
    pub fn meta(&self) -> Result<&BeadMeta> {
        self.meta.get_or_try_init(|| {
            // Try cache first
            if let Some(cached_meta) = self.cache.get("meta") {
                if let Ok(meta) = serde_json::from_value::<BeadMeta>(cached_meta.clone()) {
                    return Ok(meta);
                }
            }
            
            // Load from ZIP
            let mut zip = self.get_zipfile()?;
            let meta: BeadMeta = persistence::load_json_from_zip(&mut zip, layout::BEAD_META)?;
            Ok(meta)
        })
    }

    /// Get or open the ZIP file
    fn get_zipfile(&self) -> Result<std::sync::MutexGuard<'_, ZipArchive<File>>> {
        use std::sync::Mutex;
        
        let zip = self.zipfile.get_or_try_init(|| {
            let file = File::open(&self.path)?;
            let archive = ZipArchive::new(file)?;
            Ok::<_, BeadError>(Mutex::new(archive))
        })?;
        
        Ok(zip.lock().unwrap())
    }

    /// Get the bead name
    pub fn name(&self) -> &BeadName {
        &self.name
    }

    /// Get the bead kind
    pub fn kind(&self) -> Result<String> {
        Ok(self.meta()?.kind.clone())
    }

    /// Get content ID
    pub fn content_id(&self) -> ContentId {
        // Try cache first
        if let Some(id) = self.cache.get("content_id") {
            if let Some(id_str) = id.as_str() {
                return ContentId::new(id_str);
            }
        }
        
        // For testing, generate a dummy content ID
        ContentId::new("dummy_content_id")
    }

    /// Get freeze time
    pub fn freeze_time(&self) -> DateTime<Utc> {
        self.meta()
            .ok()
            .and_then(|m| m.freeze_time.as_ref())
            .and_then(|t| timestamp::parse_timestamp(t).ok())
            .unwrap_or_else(Utc::now)
    }

    /// Get inputs
    pub fn inputs(&self) -> Vec<InputSpec> {
        self.meta()
            .ok()
            .map(|m| m.inputs.values().cloned().collect())
            .unwrap_or_default()
    }


    /// Extract code to a directory
    pub fn unpack_code_to(&self, target_dir: impl AsRef<Path>) -> Result<()> {
        let mut zip = self.get_zipfile()?;
        
        // Extract all files in code/ directory
        for i in 0..zip.len() {
            let mut file = zip.by_index(i)?;
            let name = file.name();
            
            if name.starts_with("code/") {
                let target_path = target_dir.as_ref().join(&name[5..]);
                if let Some(parent) = target_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                
                let mut target_file = File::create(target_path)?;
                std::io::copy(&mut file, &mut target_file)?;
            }
        }
        
        Ok(())
    }

    /// Unpack to a workspace
    pub fn unpack_to(&self, workspace: &mut Workspace) -> Result<()> {
        self.unpack_code_to(&workspace.directory)?;
        workspace.meta = self.meta()?.clone();
        Ok(())
    }

    /// Validate the archive integrity
    pub fn validate(&self) -> Result<()> {
        // Check that we can read metadata
        self.meta()?;
        
        // Check required files exist
        let mut zip = self.get_zipfile()?;
        zip.by_name(layout::BEAD_META)
            .map_err(|_| BeadError::InvalidArchive("Missing meta/bead file".into()))?;
        zip.by_name(layout::MANIFEST)
            .map_err(|_| BeadError::InvalidArchive("Missing meta/manifest file".into()))?;
        
        Ok(())
    }

    /// Save cache to disk
    pub fn save_cache(&self) -> Result<()> {
        if !self.cache.is_empty() {
            persistence::save_json(&self.cache, &self.cache_path)?;
        }
        Ok(())
    }

    /// Extract a single file from the archive
    pub fn extract_file(&self, archive_path: &str, dest_dir: &Path) -> Result<PathBuf> {
        // Validate the archive path doesn't contain path traversal attempts
        if archive_path.contains("..") {
            return Err(BeadError::InvalidArchive("Path traversal detected in archive path".into()));
        }
        
        let mut zip = self.get_zipfile()?;
        
        let mut file = zip.by_name(archive_path)
            .map_err(|_| BeadError::InvalidArchive(format!("File not found in archive: {}", archive_path)))?;
        
        // Get just the filename from the archive path
        let file_name = Path::new(archive_path)
            .file_name()
            .ok_or_else(|| BeadError::InvalidArchive("Invalid file path in archive".into()))?;
        
        let dest_path = dest_dir.join(file_name);
        
        // Ensure the destination path is within dest_dir (defense in depth)
        let dest_path = dest_path.canonicalize()
            .or_else(|_| -> Result<PathBuf> {
                // If file doesn't exist yet, canonicalize parent and append filename
                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent)?;
                    Ok(parent.canonicalize()?.join(file_name))
                } else {
                    Ok(dest_path.clone())
                }
            })?;
        
        // Verify the path is still within dest_dir
        let dest_dir_canonical = dest_dir.canonicalize()
            .or_else(|_| {
                fs::create_dir_all(dest_dir)?;
                dest_dir.canonicalize()
            })?;
        
        if !dest_path.starts_with(&dest_dir_canonical) {
            return Err(BeadError::InvalidArchive("Path traversal detected".into()));
        }
        
        // Extract the file
        let mut dest_file = File::create(&dest_path)?;
        io::copy(&mut file, &mut dest_file)?;
        
        Ok(dest_path)
    }
    
    /// Extract all files from a directory in the archive
    pub fn extract_dir(&self, dir_prefix: &str, dest_dir: &Path) -> Result<Vec<PathBuf>> {
        let mut extracted_files = Vec::new();
        let mut zip = self.get_zipfile()?;
        
        // Normalize the prefix to ensure it ends with /
        let prefix = if dir_prefix.ends_with('/') {
            dir_prefix.to_string()
        } else {
            format!("{}/", dir_prefix)
        };
        
        // Collect file names first to avoid borrow issues
        let file_names: Vec<String> = (0..zip.len())
            .filter_map(|i| {
                zip.by_index(i).ok().and_then(|file| {
                    let name = file.name().to_string();
                    if name.starts_with(&prefix) && !name.ends_with('/') {
                        Some(name)
                    } else {
                        None
                    }
                })
            })
            .collect();
        
        // Extract each file
        for file_name in file_names {
            let mut file = zip.by_name(&file_name)?;
            
            // Calculate destination path
            let relative_path = &file_name[prefix.len()..];
            let dest_path = dest_dir.join(relative_path);
            
            // Create parent directories
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            
            // Extract the file
            let mut dest_file = File::create(&dest_path)?;
            io::copy(&mut file, &mut dest_file)?;
            
            extracted_files.push(dest_path);
        }
        
        Ok(extracted_files)
    }
    
    /// Unpack data directory to destination (for input loading)
    pub fn unpack_data_to(&self, dest_dir: &Path) -> Result<()> {
        self.extract_dir("data/", dest_dir)?;
        Ok(())
    }
    
    /// Extract all files from the archive
    pub fn extract_all(&self, dest_dir: &Path) -> Result<()> {
        // Create destination directory if it doesn't exist
        fs::create_dir_all(dest_dir)?;
        let dest_dir_canonical = dest_dir.canonicalize()?;
        
        let mut zip = self.get_zipfile()?;
        
        for i in 0..zip.len() {
            let mut file = zip.by_index(i)?;
            let file_path = file.name().to_string();
            
            // Skip directories
            if file_path.ends_with('/') {
                continue;
            }
            
            // Sanitize the path - remove leading slashes and check for path traversal
            let sanitized_path = file_path.trim_start_matches('/');
            if sanitized_path.contains("..") {
                return Err(BeadError::InvalidArchive(
                    format!("Path traversal detected in archive: {}", file_path)
                ));
            }
            
            let dest_path = dest_dir_canonical.join(sanitized_path);
            
            // Create parent directories
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            
            // Final safety check - ensure destination is within dest_dir
            if !dest_path.starts_with(&dest_dir_canonical) {
                return Err(BeadError::InvalidArchive(
                    format!("Path traversal detected for: {}", file_path)
                ));
            }
            
            // Extract the file
            let mut dest_file = File::create(&dest_path)?;
            io::copy(&mut file, &mut dest_file)?;
        }
        
        Ok(())
    }

    /// Parse bead name from archive path
    fn parse_name_from_path(path: &Path) -> Result<BeadName> {
        let filename = path.file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| BeadError::InvalidArchive("Invalid archive filename".into()))?;
        
        // Remove .zip extension and timestamp
        let name = if filename.ends_with(".zip") {
            &filename[..filename.len() - 4]
        } else {
            filename
        };
        
        // Remove timestamp suffix (_YYYYMMDDTHHMMSS...)
        let name = name.split('_').next().unwrap_or(name);
        
        BeadName::new(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_archive() -> (TempDir, Archive) {
        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test-bead_20240115T120000000000+0000.zip");
        
        // Create a minimal valid ZIP archive
        {
            let file = File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            
            // Add required metadata
            let meta = BeadMeta::new_frozen(
                "test-kind".to_string(),
                BeadName::new("test-bead").unwrap(),
                "20240115T120000000000+0000".to_string(),
            );
            
            persistence::save_json_to_zip(&mut zip, &meta, layout::BEAD_META).unwrap();
            
            let manifest: HashMap<String, String> = HashMap::new();
            persistence::save_json_to_zip(&mut zip, &manifest, layout::MANIFEST).unwrap();
            
            let input_map: HashMap<String, String> = HashMap::new();
            persistence::save_json_to_zip(&mut zip, &input_map, layout::INPUT_MAP).unwrap();
            
            zip.finish().unwrap();
        }
        
        let archive = Archive::open(&archive_path, "test-box").unwrap();
        (temp_dir, archive)
    }

    #[test]
    fn test_archive_open() {
        let (_temp_dir, archive) = create_test_archive();
        assert_eq!(archive.name().as_str(), "test-bead");
        assert_eq!(archive.box_name, "test-box");
    }

    #[test]
    fn test_archive_open_nonexistent() {
        let result = Archive::open("/nonexistent/archive.zip", "test-box");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), BeadError::InvalidArchive(_)));
    }

    #[test]
    fn test_archive_metadata() {
        let (_temp_dir, archive) = create_test_archive();
        
        let meta = archive.meta().unwrap();
        assert_eq!(meta.kind, "test-kind");
        assert_eq!(meta.freeze_name, Some("test-bead".to_string()));
        assert_eq!(meta.freeze_time, Some("20240115T120000000000+0000".to_string()));
    }

    #[test]
    fn test_archive_kind() {
        let (_temp_dir, archive) = create_test_archive();
        assert_eq!(archive.kind().unwrap(), "test-kind");
    }

    #[test]
    fn test_archive_inputs() {
        let (_temp_dir, archive) = create_test_archive();
        let inputs = archive.inputs();
        assert!(inputs.is_empty());
    }

    #[test]
    fn test_archive_validate() {
        let (_temp_dir, archive) = create_test_archive();
        assert!(archive.validate().is_ok());
    }

    #[test]
    fn test_archive_parse_name_from_path() {
        // With timestamp
        let path = Path::new("/path/to/my-bead_20240115T120000000000+0000.zip");
        let name = Archive::parse_name_from_path(path).unwrap();
        assert_eq!(name.as_str(), "my-bead");
        
        // Without timestamp
        let path = Path::new("/path/to/simple-bead.zip");
        let name = Archive::parse_name_from_path(path).unwrap();
        assert_eq!(name.as_str(), "simple-bead");
        
        // Complex name
        let path = Path::new("bead-v2.1_20240115T120000000000+0000.zip");
        let name = Archive::parse_name_from_path(path).unwrap();
        assert_eq!(name.as_str(), "bead-v2.1");
    }

    #[test]
    fn test_archive_create_from_workspace() {
        let workspace_dir = TempDir::new().unwrap();
        let workspace_path = workspace_dir.path().join("test-workspace");
        let workspace = Workspace::create(&workspace_path, "test-kind".to_string()).unwrap();
        
        let archive_dir = TempDir::new().unwrap();
        let archive_path = archive_dir.path().join("test-workspace_20240115T120000000000+0000.zip");
        
        let freeze_time = "20240115T120000000000+0000".to_string();
        let comment = "Test archive comment";
        
        let archive = Archive::create(&archive_path, &workspace, freeze_time, comment).unwrap();
        
        assert_eq!(archive.name().as_str(), "test-workspace");
        assert!(archive_path.exists());
    }

    #[test]
    fn test_archive_extract_file() {
        let (_temp_dir, archive) = create_test_archive_with_files();
        let extract_dir = TempDir::new().unwrap();
        
        // Extract a single file
        let result = archive.extract_file("data/test.txt", extract_dir.path());
        assert!(result.is_ok());
        
        let extracted_file = extract_dir.path().join("test.txt");
        assert!(extracted_file.exists());
        
        let content = fs::read_to_string(&extracted_file).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_archive_extract_file_nonexistent() {
        let (_temp_dir, archive) = create_test_archive();
        let extract_dir = TempDir::new().unwrap();
        
        let result = archive.extract_file("nonexistent.txt", extract_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_archive_extract_dir() {
        let (_temp_dir, archive) = create_test_archive_with_files();
        let extract_dir = TempDir::new().unwrap();
        
        // Extract entire data directory
        let result = archive.extract_dir("data/", extract_dir.path());
        assert!(result.is_ok());
        
        assert!(extract_dir.path().join("test.txt").exists());
        assert!(extract_dir.path().join("subdir/nested.txt").exists());
    }

    #[test]
    fn test_archive_extract_all() {
        let (_temp_dir, archive) = create_test_archive_with_files();
        let extract_dir = TempDir::new().unwrap();
        
        // Extract everything
        let result = archive.extract_all(extract_dir.path());
        assert!(result.is_ok());
        
        // Check that various parts were extracted
        assert!(extract_dir.path().join("meta/bead").exists());
        assert!(extract_dir.path().join("data/test.txt").exists());
        assert!(extract_dir.path().join("code/main.rs").exists());
    }

    #[test]
    fn test_extract_file_with_nested_path() {
        let (_temp_dir, archive) = create_test_archive_with_files();
        let extract_dir = TempDir::new().unwrap();
        
        // Extract a deeply nested file
        let result = archive.extract_file("data/subdir/nested.txt", extract_dir.path());
        assert!(result.is_ok());
        
        let extracted_file = extract_dir.path().join("nested.txt");
        assert!(extracted_file.exists());
        
        let content = fs::read_to_string(&extracted_file).unwrap();
        assert_eq!(content, "nested content");
    }

    #[test]
    fn test_extract_file_to_non_existent_dest() {
        let (_temp_dir, archive) = create_test_archive_with_files();
        let extract_dir = TempDir::new().unwrap();
        let non_existent = extract_dir.path().join("non/existent/path");
        
        // Should create parent directories
        let result = archive.extract_file("data/test.txt", &non_existent);
        assert!(result.is_ok());
        assert!(non_existent.join("test.txt").exists());
    }

    #[test]
    fn test_extract_file_overwrite_existing() {
        let (_temp_dir, archive) = create_test_archive_with_files();
        let extract_dir = TempDir::new().unwrap();
        
        // Create existing file with different content
        let existing_file = extract_dir.path().join("test.txt");
        fs::write(&existing_file, "old content").unwrap();
        
        // Extract should overwrite
        let result = archive.extract_file("data/test.txt", extract_dir.path());
        assert!(result.is_ok());
        
        let content = fs::read_to_string(&existing_file).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_extract_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test-bead_20240115T120000000000+0000.zip");
        
        // Create archive with empty file
        {
            let file = File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            
            add_test_metadata(&mut zip);
            
            let options = zip::write::FileOptions::default();
            zip.start_file("data/empty.txt", options).unwrap();
            // Don't write any content
            
            zip.finish().unwrap();
        }
        
        let archive = Archive::open(&archive_path, "test-box").unwrap();
        let extract_dir = TempDir::new().unwrap();
        
        let result = archive.extract_file("data/empty.txt", extract_dir.path());
        assert!(result.is_ok());
        
        let extracted_file = extract_dir.path().join("empty.txt");
        assert!(extracted_file.exists());
        assert_eq!(fs::read_to_string(&extracted_file).unwrap(), "");
    }

    #[test]
    fn test_extract_large_file() {
        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test-bead_20240115T120000000000+0000.zip");
        
        // Create archive with large file (1MB)
        let large_content = vec![b'x'; 1024 * 1024];
        {
            let file = File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            
            add_test_metadata(&mut zip);
            
            let options = zip::write::FileOptions::default();
            zip.start_file("data/large.txt", options).unwrap();
            std::io::Write::write_all(&mut zip, &large_content).unwrap();
            
            zip.finish().unwrap();
        }
        
        let archive = Archive::open(&archive_path, "test-box").unwrap();
        let extract_dir = TempDir::new().unwrap();
        
        let result = archive.extract_file("data/large.txt", extract_dir.path());
        assert!(result.is_ok());
        
        let extracted_file = extract_dir.path().join("large.txt");
        assert!(extracted_file.exists());
        assert_eq!(fs::read(&extracted_file).unwrap().len(), 1024 * 1024);
    }

    #[test]
    fn test_extract_file_with_special_chars() {
        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test-bead_20240115T120000000000+0000.zip");
        
        // Create archive with file containing special characters
        {
            let file = File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            
            add_test_metadata(&mut zip);
            
            let options = zip::write::FileOptions::default();
            zip.start_file("data/test file (copy).txt", options).unwrap();
            std::io::Write::write_all(&mut zip, b"special chars content").unwrap();
            
            zip.finish().unwrap();
        }
        
        let archive = Archive::open(&archive_path, "test-box").unwrap();
        let extract_dir = TempDir::new().unwrap();
        
        let result = archive.extract_file("data/test file (copy).txt", extract_dir.path());
        assert!(result.is_ok());
        
        let extracted_file = extract_dir.path().join("test file (copy).txt");
        assert!(extracted_file.exists());
    }

    #[test]
    fn test_extract_dir_empty() {
        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test-bead_20240115T120000000000+0000.zip");
        
        // Create archive with empty directory (no files in it)
        {
            let file = File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            
            add_test_metadata(&mut zip);
            
            // Add a file outside the empty dir
            let options = zip::write::FileOptions::default();
            zip.start_file("other/file.txt", options).unwrap();
            std::io::Write::write_all(&mut zip, b"content").unwrap();
            
            zip.finish().unwrap();
        }
        
        let archive = Archive::open(&archive_path, "test-box").unwrap();
        let extract_dir = TempDir::new().unwrap();
        
        // Try to extract from empty directory
        let result = archive.extract_dir("empty/", extract_dir.path());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_extract_dir_deeply_nested() {
        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test-bead_20240115T120000000000+0000.zip");
        
        // Create archive with deeply nested directories
        {
            let file = File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            
            add_test_metadata(&mut zip);
            
            let options = zip::write::FileOptions::default();
            zip.start_file("data/a/b/c/d/deep.txt", options).unwrap();
            std::io::Write::write_all(&mut zip, b"deep content").unwrap();
            
            zip.start_file("data/a/b/mid.txt", options).unwrap();
            std::io::Write::write_all(&mut zip, b"mid content").unwrap();
            
            zip.finish().unwrap();
        }
        
        let archive = Archive::open(&archive_path, "test-box").unwrap();
        let extract_dir = TempDir::new().unwrap();
        
        let result = archive.extract_dir("data/a/", extract_dir.path());
        assert!(result.is_ok());
        
        assert!(extract_dir.path().join("b/c/d/deep.txt").exists());
        assert!(extract_dir.path().join("b/mid.txt").exists());
    }

    #[test]
    fn test_extract_dir_non_existent() {
        let (_temp_dir, archive) = create_test_archive_with_files();
        let extract_dir = TempDir::new().unwrap();
        
        let result = archive.extract_dir("nonexistent/", extract_dir.path());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_extract_dir_without_trailing_slash() {
        let (_temp_dir, archive) = create_test_archive_with_files();
        let extract_dir = TempDir::new().unwrap();
        
        // Should still work without trailing slash
        let result = archive.extract_dir("data", extract_dir.path());
        assert!(result.is_ok());
        
        assert!(extract_dir.path().join("test.txt").exists());
        assert!(extract_dir.path().join("subdir/nested.txt").exists());
    }

    #[test]
    fn test_extract_all_to_non_existent_dest() {
        let (_temp_dir, archive) = create_test_archive_with_files();
        let extract_dir = TempDir::new().unwrap();
        let non_existent = extract_dir.path().join("new/dest");
        
        // Should create destination directories
        let result = archive.extract_all(&non_existent);
        assert!(result.is_ok());
        assert!(non_existent.join("data/test.txt").exists());
    }

    #[test]
    fn test_extract_preserves_directory_structure() {
        let (_temp_dir, archive) = create_test_archive_with_files();
        let extract_dir = TempDir::new().unwrap();
        
        archive.extract_all(extract_dir.path()).unwrap();
        
        // Verify full directory structure is preserved
        assert!(extract_dir.path().join("meta").is_dir());
        assert!(extract_dir.path().join("data").is_dir());
        assert!(extract_dir.path().join("data/subdir").is_dir());
        assert!(extract_dir.path().join("code").is_dir());
    }

    #[test]
    fn test_extract_file_case_sensitive() {
        let (_temp_dir, archive) = create_test_archive_with_files();
        let extract_dir = TempDir::new().unwrap();
        
        // Wrong case should fail
        let result = archive.extract_file("Data/Test.txt", extract_dir.path());
        assert!(result.is_err());
        
        // Correct case should succeed
        let result = archive.extract_file("data/test.txt", extract_dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_concurrent_extractions() {
        use std::thread;
        use std::sync::Arc;
        
        let (_temp_dir, archive) = create_test_archive_with_files();
        let archive = Arc::new(archive);
        
        let mut handles = vec![];
        
        for _i in 0..3 {
            let archive_clone = Arc::clone(&archive);
            let handle = thread::spawn(move || {
                let extract_dir = TempDir::new().unwrap();
                let result = archive_clone.extract_all(extract_dir.path());
                assert!(result.is_ok());
                assert!(extract_dir.path().join("data/test.txt").exists());
            });
            handles.push(handle);
        }
        
        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_extract_with_absolute_path_in_archive() {
        // This test ensures we handle potentially malicious archives safely
        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test-bead_20240115T120000000000+0000.zip");
        
        // Try to create archive with absolute path (should be rejected or normalized)
        {
            let file = File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            
            add_test_metadata(&mut zip);
            
            let options = zip::write::FileOptions::default();
            // Note: Most zip libraries will normalize this anyway
            zip.start_file("/etc/passwd", options).unwrap();
            std::io::Write::write_all(&mut zip, b"malicious content").unwrap();
            
            zip.finish().unwrap();
        }
        
        let archive = Archive::open(&archive_path, "test-box").unwrap();
        let extract_dir = TempDir::new().unwrap();
        
        // Should extract safely within the target directory
        let result = archive.extract_all(extract_dir.path());
        assert!(result.is_ok());
        
        // File should be extracted relative to extract_dir, not at /etc/passwd
        assert!(!Path::new("/etc/passwd").exists() || 
                fs::read_to_string("/etc/passwd").unwrap() != "malicious content");
    }

    #[test]
    fn test_extract_dir_with_file_selection() {
        let (_temp_dir, archive) = create_test_archive_with_files();
        let extract_dir = TempDir::new().unwrap();
        
        // Extract only from data directory
        let files = archive.extract_dir("data/", extract_dir.path()).unwrap();
        
        // Should have extracted exactly 2 files
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|p| p.ends_with("test.txt")));
        assert!(files.iter().any(|p| p.ends_with("nested.txt")));
        
        // Code files should not be extracted
        assert!(!extract_dir.path().join("main.rs").exists());
    }

    #[test]
    #[cfg(unix)]
    fn test_extract_readonly_destination() {
        use std::os::unix::fs::PermissionsExt;
        
        let (_temp_dir, archive) = create_test_archive_with_files();
        let extract_dir = TempDir::new().unwrap();
        
        // Make destination read-only
        let mut perms = fs::metadata(extract_dir.path()).unwrap().permissions();
        perms.set_mode(0o555);
        fs::set_permissions(extract_dir.path(), perms).unwrap();
        
        // Should fail to extract
        let result = archive.extract_file("data/test.txt", extract_dir.path());
        assert!(result.is_err());
        
        // Restore permissions for cleanup
        let mut perms = fs::metadata(extract_dir.path()).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(extract_dir.path(), perms).unwrap();
    }

    // Helper function to add standard test metadata
    fn add_test_metadata(zip: &mut zip::ZipWriter<File>) {
        let meta = BeadMeta::new_frozen(
            "test-kind".to_string(),
            BeadName::new("test-bead").unwrap(),
            "20240115T120000000000+0000".to_string(),
        );
        
        persistence::save_json_to_zip(zip, &meta, layout::BEAD_META).unwrap();
        
        let manifest: HashMap<String, String> = HashMap::new();
        persistence::save_json_to_zip(zip, &manifest, layout::MANIFEST).unwrap();
        
        let input_map: HashMap<String, String> = HashMap::new();
        persistence::save_json_to_zip(zip, &input_map, layout::INPUT_MAP).unwrap();
    }

    fn create_test_archive_with_files() -> (TempDir, Archive) {
        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test-bead_20240115T120000000000+0000.zip");
        
        // Create a ZIP archive with test files
        {
            let file = File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            
            // Add metadata
            let meta = BeadMeta::new_frozen(
                "test-kind".to_string(),
                BeadName::new("test-bead").unwrap(),
                "20240115T120000000000+0000".to_string(),
            );
            
            persistence::save_json_to_zip(&mut zip, &meta, layout::BEAD_META).unwrap();
            
            let manifest: HashMap<String, String> = HashMap::new();
            persistence::save_json_to_zip(&mut zip, &manifest, layout::MANIFEST).unwrap();
            
            let input_map: HashMap<String, String> = HashMap::new();
            persistence::save_json_to_zip(&mut zip, &input_map, layout::INPUT_MAP).unwrap();
            
            // Add test data files
            let options = zip::write::FileOptions::default();
            zip.start_file("data/test.txt", options).unwrap();
            std::io::Write::write_all(&mut zip, b"test content").unwrap();
            
            zip.start_file("data/subdir/nested.txt", options).unwrap();
            std::io::Write::write_all(&mut zip, b"nested content").unwrap();
            
            // Add test code files
            zip.start_file("code/main.rs", options).unwrap();
            std::io::Write::write_all(&mut zip, b"fn main() {}").unwrap();
            
            zip.finish().unwrap();
        }
        
        let archive = Archive::open(&archive_path, "test-box").unwrap();
        (temp_dir, archive)
    }
}