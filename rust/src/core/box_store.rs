use std::path::{Path, PathBuf};
use std::fs;
use glob::glob;
use regex::Regex;
use crate::error::{BeadError, Result};
use crate::core::archive::Archive;
use crate::core::workspace::Workspace;
use crate::core::meta::BeadName;
use crate::tech::timestamp;

/// A Box is a storage location for bead archives
#[derive(Debug, Clone)]
pub struct Box {
    pub name: String,
    pub location: PathBuf,
}

impl Box {
    /// Create a new Box
    pub fn new(name: String, location: impl AsRef<Path>) -> Result<Self> {
        let location = location.as_ref().to_path_buf();
        
        if !location.exists() {
            return Err(BeadError::BoxNotFound(
                format!("Box location does not exist: {}", location.display())
            ));
        }
        
        if !location.is_dir() {
            return Err(BeadError::BoxNotFound(
                format!("Box location is not a directory: {}", location.display())
            ));
        }
        
        Ok(Box { name, location })
    }

    /// Store a workspace as a bead archive in this box
    pub fn store(&self, workspace: &Workspace, freeze_time: String) -> Result<PathBuf> {
        let bead_name = workspace.name()
            .ok_or_else(|| BeadError::InvalidWorkspace("Cannot determine workspace name".into()))?;
        
        // Generate archive filename
        let archive_name = format!("{}_{}.zip", bead_name, freeze_time);
        let archive_path = self.location.join(&archive_name);
        
        if archive_path.exists() {
            return Err(BeadError::AlreadyExists(
                format!("Archive already exists: {}", archive_path.display())
            ));
        }

        // Create archive (simplified - actual implementation would pack the workspace)
        // This is a placeholder - real implementation would create a proper ZIP
        fs::File::create(&archive_path)?;
        
        Ok(archive_path)
    }

    /// Find all beads in this box
    pub fn all_beads(&self) -> Result<Vec<Archive>> {
        let pattern = self.location.join("*.zip");
        let mut beads = Vec::new();
        
        for entry in glob(pattern.to_str().unwrap()).map_err(|e| {
            BeadError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e))
        })? {
            match entry {
                Ok(path) => {
                    if let Ok(archive) = Archive::open(&path, &self.name) {
                        beads.push(archive);
                    }
                }
                Err(_) => continue,
            }
        }
        
        Ok(beads)
    }

    /// Find beads by name
    pub fn find_by_name(&self, name: &str) -> Result<Vec<Archive>> {
        let pattern = self.location.join(format!("{}_*.zip", name));
        let mut beads = Vec::new();
        
        for entry in glob(pattern.to_str().unwrap()).map_err(|e| {
            BeadError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e))
        })? {
            match entry {
                Ok(path) => {
                    if let Ok(archive) = Archive::open(&path, &self.name) {
                        beads.push(archive);
                    }
                }
                Err(_) => continue,
            }
        }
        
        Ok(beads)
    }

    /// Find a specific bead by name and content ID
    pub fn find_bead(&self, name: &str, content_id: &str) -> Result<Option<Archive>> {
        let beads = self.find_by_name(name)?;
        
        for bead in beads {
            if bead.content_id().starts_with(content_id) {
                return Ok(Some(bead));
            }
        }
        
        Ok(None)
    }

    /// Check if the box is accessible
    pub fn is_accessible(&self) -> bool {
        self.location.exists() && self.location.is_dir()
    }

    /// Parse bead name from archive filename
    pub fn parse_bead_name_from_path(path: &Path) -> Option<String> {
        let filename = path.file_name()?.to_str()?;
        
        // Pattern: name_YYYYMMDDTHHMMSSNNNNNN±ZZZZ.zip
        let re = Regex::new(r"^(.+?)_\d{8}T[\d+-]+\.zip$").ok()?;
        
        if let Some(captures) = re.captures(filename) {
            captures.get(1).map(|m| m.as_str().to_string())
        } else {
            // Try without timestamp
            if filename.ends_with(".zip") {
                Some(filename[..filename.len() - 4].to_string())
            } else {
                None
            }
        }
    }
}

/// Union of multiple boxes for searching
pub struct UnionBox {
    boxes: Vec<Box>,
}

impl UnionBox {
    /// Create a union of boxes
    pub fn new(boxes: Vec<Box>) -> Self {
        UnionBox { boxes }
    }

    /// Find all beads across all boxes
    pub fn all_beads(&self) -> Result<Vec<Archive>> {
        let mut all_beads = Vec::new();
        
        for box_store in &self.boxes {
            if let Ok(beads) = box_store.all_beads() {
                all_beads.extend(beads);
            }
        }
        
        Ok(all_beads)
    }

    /// Find beads by name across all boxes
    pub fn find_by_name(&self, name: &str) -> Result<Vec<Archive>> {
        let mut found_beads = Vec::new();
        
        for box_store in &self.boxes {
            if let Ok(beads) = box_store.find_by_name(name) {
                found_beads.extend(beads);
            }
        }
        
        Ok(found_beads)
    }

    /// Find latest bead by name
    pub fn find_latest(&self, name: &str) -> Result<Option<Archive>> {
        let beads = self.find_by_name(name)?;
        
        if beads.is_empty() {
            return Ok(None);
        }
        
        // Sort by freeze time and return the latest
        let latest = beads.into_iter()
            .max_by_key(|b| b.freeze_time().timestamp());
        
        Ok(latest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_box() -> (TempDir, Box) {
        let temp_dir = TempDir::new().unwrap();
        let box_store = Box::new("test-box".to_string(), temp_dir.path()).unwrap();
        (temp_dir, box_store)
    }

    #[test]
    fn test_box_creation() {
        let temp_dir = TempDir::new().unwrap();
        let box_store = Box::new("test-box".to_string(), temp_dir.path()).unwrap();
        
        assert_eq!(box_store.name, "test-box");
        assert_eq!(box_store.location, temp_dir.path());
        assert!(box_store.is_accessible());
    }

    #[test]
    fn test_box_nonexistent_location() {
        let result = Box::new("test-box".to_string(), "/nonexistent/path");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), BeadError::BoxNotFound(_)));
    }

    #[test]
    fn test_box_not_directory() {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let result = Box::new("test-box".to_string(), temp_file.path());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), BeadError::BoxNotFound(_)));
    }

    #[test]
    fn test_parse_bead_name_from_path() {
        // With timestamp
        let path = Path::new("my-bead_20240115T143022123456+0100.zip");
        assert_eq!(
            Box::parse_bead_name_from_path(path),
            Some("my-bead".to_string())
        );
        
        // Without timestamp
        let path = Path::new("simple-bead.zip");
        assert_eq!(
            Box::parse_bead_name_from_path(path),
            Some("simple-bead".to_string())
        );
        
        // Not a zip file
        let path = Path::new("not-a-bead.txt");
        assert_eq!(Box::parse_bead_name_from_path(path), None);
        
        // Complex name with timestamp
        let path = Path::new("bead-2015v3_20150923T010203012345-0200.zip");
        assert_eq!(
            Box::parse_bead_name_from_path(path),
            Some("bead-2015v3".to_string())
        );
    }

    #[test]
    fn test_box_store_workspace() {
        let (_temp_dir, box_store) = create_test_box();
        
        // Create a test workspace
        let workspace_dir = TempDir::new().unwrap();
        let workspace_path = workspace_dir.path().join("test-workspace");
        let workspace = Workspace::create(&workspace_path, "test-kind".to_string()).unwrap();
        
        // Store the workspace
        let freeze_time = timestamp::timestamp();
        let archive_path = box_store.store(&workspace, freeze_time.clone()).unwrap();
        
        assert!(archive_path.exists());
        assert!(archive_path.file_name().unwrap().to_str().unwrap()
            .starts_with("test-workspace_"));
    }

    #[test]
    fn test_box_store_duplicate() {
        let (_temp_dir, box_store) = create_test_box();
        
        // Create a test workspace
        let workspace_dir = TempDir::new().unwrap();
        let workspace_path = workspace_dir.path().join("test-workspace");
        let workspace = Workspace::create(&workspace_path, "test-kind".to_string()).unwrap();
        
        // Store the workspace
        let freeze_time = "20240115T120000000000+0000".to_string();
        box_store.store(&workspace, freeze_time.clone()).unwrap();
        
        // Try to store again with same timestamp
        let result = box_store.store(&workspace, freeze_time);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), BeadError::AlreadyExists(_)));
    }

    #[test]
    fn test_union_box() {
        // Create two boxes
        let temp_dir1 = TempDir::new().unwrap();
        let box1 = Box::new("box1".to_string(), temp_dir1.path()).unwrap();
        
        let temp_dir2 = TempDir::new().unwrap();
        let box2 = Box::new("box2".to_string(), temp_dir2.path()).unwrap();
        
        // Create union
        let union = UnionBox::new(vec![box1, box2]);
        
        // Test operations (they should work even with empty boxes)
        let all_beads = union.all_beads().unwrap();
        assert!(all_beads.is_empty());
        
        let found = union.find_by_name("nonexistent").unwrap();
        assert!(found.is_empty());
        
        let latest = union.find_latest("nonexistent").unwrap();
        assert!(latest.is_none());
    }
}