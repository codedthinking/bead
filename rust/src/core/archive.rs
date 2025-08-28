use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::{Read, Write};
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
    zipfile: OnceCell<ZipArchive<File>>,
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

    /// Extract data to a directory
    pub fn unpack_data_to(&self, target_dir: impl AsRef<Path>) -> Result<()> {
        let mut zip = self.get_zipfile()?;
        
        // Extract all files in data/ directory
        for i in 0..zip.len() {
            let mut file = zip.by_index(i)?;
            let name = file.name();
            
            if name.starts_with("data/") {
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
    use tempfile::{TempDir, NamedTempFile};

    fn create_test_archive() -> (NamedTempFile, Archive) {
        let temp_file = NamedTempFile::new().unwrap();
        
        // Create a minimal valid ZIP archive
        {
            let file = File::create(temp_file.path()).unwrap();
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
        
        let archive = Archive::open(temp_file.path(), "test-box").unwrap();
        (temp_file, archive)
    }

    #[test]
    fn test_archive_open() {
        let (_temp_file, archive) = create_test_archive();
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
        let (_temp_file, archive) = create_test_archive();
        
        let meta = archive.meta().unwrap();
        assert_eq!(meta.kind, "test-kind");
        assert_eq!(meta.freeze_name, Some("test-bead".to_string()));
        assert_eq!(meta.freeze_time, Some("20240115T120000000000+0000".to_string()));
    }

    #[test]
    fn test_archive_kind() {
        let (_temp_file, archive) = create_test_archive();
        assert_eq!(archive.kind().unwrap(), "test-kind");
    }

    #[test]
    fn test_archive_inputs() {
        let (_temp_file, archive) = create_test_archive();
        let inputs = archive.inputs();
        assert!(inputs.is_empty());
    }

    #[test]
    fn test_archive_validate() {
        let (_temp_file, archive) = create_test_archive();
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
        
        let archive_file = NamedTempFile::new().unwrap();
        let archive_path = archive_file.path().with_extension("zip");
        
        let freeze_time = "20240115T120000000000+0000".to_string();
        let comment = "Test archive comment";
        
        let archive = Archive::create(&archive_path, &workspace, freeze_time, comment).unwrap();
        
        assert_eq!(archive.name().as_str(), "test-workspace");
        assert!(archive_path.exists());
    }
}