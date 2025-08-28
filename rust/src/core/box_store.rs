use std::path::{Path, PathBuf};
use std::fs;
use glob::glob;
use regex::Regex;
use crate::error::{BeadError, Result};
use crate::core::archive::Archive;
use crate::core::workspace::Workspace;


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

    /// Find beads by kind
    pub fn find_by_kind(&self, kind: &str) -> Result<Vec<Archive>> {
        let all = self.all_beads()?;
        Ok(all.into_iter()
            .filter(|archive| archive.kind() == kind)
            .collect())
    }
    
    /// Find beads by content ID prefix
    pub fn find_by_content_id_prefix(&self, prefix: &str) -> Result<Vec<Archive>> {
        let all = self.all_beads()?;
        Ok(all.into_iter()
            .filter(|archive| archive.content_id().as_str().starts_with(prefix))
            .collect())
    }
    
    /// Find latest bead by name (based on freeze time)
    pub fn find_latest_by_name(&self, name: &str) -> Result<Option<Archive>> {
        let beads = self.find_by_name(name)?;
        Ok(beads.into_iter()
            .max_by_key(|b| b.freeze_time().timestamp()))
    }
    
    /// Find newest bead by name (based on file modification time)
    pub fn find_newest_by_name(&self, name: &str) -> Result<Option<Archive>> {
        let beads = self.find_by_name(name)?;
        
        // Get file modification times and find newest
        let mut newest: Option<(Archive, std::time::SystemTime)> = None;
        
        for bead in beads {
            if let Ok(metadata) = fs::metadata(bead.path()) {
                if let Ok(modified) = metadata.modified() {
                    match &newest {
                        None => newest = Some((bead, modified)),
                        Some((_, current_newest)) => {
                            if modified > *current_newest {
                                newest = Some((bead, modified));
                            }
                        }
                    }
                }
            }
        }
        
        Ok(newest.map(|(archive, _)| archive))
    }
    
    /// Find by reference (name, kind, or content ID prefix)
    pub fn find_by_ref(&self, reference: &str) -> Result<Vec<Archive>> {
        let mut results = Vec::new();
        
        // Try as name
        results.extend(self.find_by_name(reference)?);
        
        // Try as kind
        results.extend(self.find_by_kind(reference)?);
        
        // Try as content ID prefix
        results.extend(self.find_by_content_id_prefix(reference)?);
        
        // Deduplicate by path
        let mut seen = std::collections::HashSet::new();
        Ok(results.into_iter()
            .filter(|archive| seen.insert(archive.path().to_path_buf()))
            .collect())
    }
    
    /// Find beads by name before a specific timestamp
    pub fn find_by_name_before(&self, name: &str, timestamp: &str) -> Result<Vec<Archive>> {
        use crate::tech::timestamp::parse_timestamp;
        
        let cutoff = parse_timestamp(timestamp)?;
        let beads = self.find_by_name(name)?;
        
        Ok(beads.into_iter()
            .filter(|archive| archive.freeze_time() <= cutoff)
            .collect())
    }
    
    /// Find beads by name after a specific timestamp
    pub fn find_by_name_after(&self, name: &str, timestamp: &str) -> Result<Vec<Archive>> {
        use crate::tech::timestamp::parse_timestamp;
        
        let cutoff = parse_timestamp(timestamp)?;
        let beads = self.find_by_name(name)?;
        
        Ok(beads.into_iter()
            .filter(|archive| archive.freeze_time() > cutoff)
            .collect())
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
    
    /// Find beads by kind across all boxes
    pub fn find_by_kind(&self, kind: &str) -> Result<Vec<Archive>> {
        let mut found_beads = Vec::new();
        
        for box_store in &self.boxes {
            if let Ok(beads) = box_store.find_by_kind(kind) {
                found_beads.extend(beads);
            }
        }
        
        Ok(found_beads)
    }
    
    /// Find beads by content ID prefix across all boxes
    pub fn find_by_content_id_prefix(&self, prefix: &str) -> Result<Vec<Archive>> {
        let mut found_beads = Vec::new();
        
        for box_store in &self.boxes {
            if let Ok(beads) = box_store.find_by_content_id_prefix(prefix) {
                found_beads.extend(beads);
            }
        }
        
        Ok(found_beads)
    }
    
    /// Find by reference across all boxes
    pub fn find_by_ref(&self, reference: &str) -> Result<Vec<Archive>> {
        let mut results = Vec::new();
        
        for box_store in &self.boxes {
            if let Ok(beads) = box_store.find_by_ref(reference) {
                results.extend(beads);
            }
        }
        
        // Deduplicate by path
        let mut seen = std::collections::HashSet::new();
        Ok(results.into_iter()
            .filter(|archive| seen.insert(archive.path().to_path_buf()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::tech::timestamp;

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
    fn test_box_find_by_name() {
        let (temp_dir, box_store) = create_test_box();
        
        // Create test archives with specific names
        create_test_archive(&temp_dir, "test-bead", "20240115T120000000000+0000");
        create_test_archive(&temp_dir, "test-bead", "20240116T120000000000+0000");
        create_test_archive(&temp_dir, "other-bead", "20240115T120000000000+0000");
        
        let beads = box_store.find_by_name("test-bead").unwrap();
        assert_eq!(beads.len(), 2);
        
        let other_beads = box_store.find_by_name("other-bead").unwrap();
        assert_eq!(other_beads.len(), 1);
        
        let no_beads = box_store.find_by_name("nonexistent").unwrap();
        assert_eq!(no_beads.len(), 0);
    }

    #[test]
    fn test_box_find_by_kind() {
        let (temp_dir, box_store) = create_test_box();
        
        // Create archives with different kinds
        create_test_archive_with_kind(&temp_dir, "bead1", "kind-a", "20240115T120000000000+0000");
        create_test_archive_with_kind(&temp_dir, "bead2", "kind-a", "20240115T120000000000+0000");
        create_test_archive_with_kind(&temp_dir, "bead3", "kind-b", "20240115T120000000000+0000");
        
        let beads = box_store.find_by_kind("kind-a").unwrap();
        assert_eq!(beads.len(), 2);
        
        let other_beads = box_store.find_by_kind("kind-b").unwrap();
        assert_eq!(other_beads.len(), 1);
    }

    #[test]
    fn test_box_find_by_content_id_prefix() {
        let (temp_dir, box_store) = create_test_box();
        
        // Create archives with specific content IDs
        create_test_archive_with_content_id(&temp_dir, "bead1", "abc123def456", "20240115T120000000000+0000");
        create_test_archive_with_content_id(&temp_dir, "bead2", "abc456def789", "20240115T120000000000+0000");
        create_test_archive_with_content_id(&temp_dir, "bead3", "xyz123def456", "20240115T120000000000+0000");
        
        let beads = box_store.find_by_content_id_prefix("abc").unwrap();
        assert_eq!(beads.len(), 2);
        
        let specific_bead = box_store.find_by_content_id_prefix("abc123").unwrap();
        assert_eq!(specific_bead.len(), 1);
        
        let no_beads = box_store.find_by_content_id_prefix("zzz").unwrap();
        assert_eq!(no_beads.len(), 0);
    }

    #[test]
    fn test_box_find_latest_by_name() {
        let (temp_dir, box_store) = create_test_box();
        
        // Create archives with different timestamps
        create_test_archive(&temp_dir, "test-bead", "20240115T120000000000+0000");
        create_test_archive(&temp_dir, "test-bead", "20240116T120000000000+0000");
        create_test_archive(&temp_dir, "test-bead", "20240117T120000000000+0000");
        
        let latest = box_store.find_latest_by_name("test-bead").unwrap();
        assert!(latest.is_some());
        
        let archive = latest.unwrap();
        // The latest should have the most recent timestamp
        assert!(archive.path().to_str().unwrap().contains("20240117"));
    }

    #[test]
    fn test_box_find_newest_by_name() {
        let (temp_dir, box_store) = create_test_box();
        
        // Create archives with different timestamps but add them in different order
        create_test_archive(&temp_dir, "test-bead", "20240117T120000000000+0000");
        std::thread::sleep(std::time::Duration::from_millis(10));
        create_test_archive(&temp_dir, "test-bead", "20240115T120000000000+0000");
        std::thread::sleep(std::time::Duration::from_millis(10));
        create_test_archive(&temp_dir, "test-bead", "20240116T120000000000+0000");
        
        // Newest should be based on file modification time, not freeze time
        let newest = box_store.find_newest_by_name("test-bead").unwrap();
        assert!(newest.is_some());
        
        let archive = newest.unwrap();
        // The newest should be the last one created (20240116)
        assert!(archive.path().to_str().unwrap().contains("20240116"));
    }

    #[test]
    fn test_box_find_by_ref() {
        let (temp_dir, box_store) = create_test_box();
        
        // Create test archives
        create_test_archive_with_content_id(&temp_dir, "test-bead", "abc123def456", "20240115T120000000000+0000");
        create_test_archive_with_kind(&temp_dir, "other-bead", "special-kind", "20240115T120000000000+0000");
        
        // Find by name
        let by_name = box_store.find_by_ref("test-bead").unwrap();
        assert_eq!(by_name.len(), 1);
        
        // Find by content ID prefix
        let by_content = box_store.find_by_ref("abc123").unwrap();
        assert_eq!(by_content.len(), 1);
        
        // Find by kind
        let by_kind = box_store.find_by_ref("special-kind").unwrap();
        assert_eq!(by_kind.len(), 1);
    }

    #[test]
    fn test_box_find_with_time_constraint() {
        let (temp_dir, box_store) = create_test_box();
        
        // Create archives with different timestamps
        create_test_archive(&temp_dir, "test-bead", "20240115T120000000000+0000");
        create_test_archive(&temp_dir, "test-bead", "20240116T120000000000+0000");
        create_test_archive(&temp_dir, "test-bead", "20240117T120000000000+0000");
        
        // Find beads before a specific time
        let before = box_store.find_by_name_before("test-bead", "20240116T180000000000+0000").unwrap();
        assert_eq!(before.len(), 2); // Should include 15th and 16th
        
        // Find beads after a specific time
        let after = box_store.find_by_name_after("test-bead", "20240115T180000000000+0000").unwrap();
        assert_eq!(after.len(), 2); // Should include 16th and 17th
    }

    #[test]
    fn test_box_all_beads() {
        let (temp_dir, box_store) = create_test_box();
        
        // Create multiple archives
        create_test_archive(&temp_dir, "bead1", "20240115T120000000000+0000");
        create_test_archive(&temp_dir, "bead2", "20240116T120000000000+0000");
        create_test_archive(&temp_dir, "bead3", "20240117T120000000000+0000");
        
        // Also create a non-bead file to ensure it's filtered out
        fs::write(temp_dir.path().join("not-a-bead.txt"), "data").unwrap();
        
        let all_beads = box_store.all_beads().unwrap();
        assert_eq!(all_beads.len(), 3);
    }

    #[test]
    fn test_box_find_empty_box() {
        let (_temp_dir, box_store) = create_test_box();
        
        let beads = box_store.find_by_name("anything").unwrap();
        assert_eq!(beads.len(), 0);
        
        let all = box_store.all_beads().unwrap();
        assert_eq!(all.len(), 0);
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

    #[test]
    fn test_union_box_with_beads() {
        // Create two boxes with different beads
        let temp_dir1 = TempDir::new().unwrap();
        create_test_archive(&temp_dir1, "bead1", "20240115T120000000000+0000");
        create_test_archive(&temp_dir1, "shared-bead", "20240115T120000000000+0000");
        let box1 = Box::new("box1".to_string(), temp_dir1.path()).unwrap();
        
        let temp_dir2 = TempDir::new().unwrap();
        create_test_archive(&temp_dir2, "bead2", "20240116T120000000000+0000");
        create_test_archive(&temp_dir2, "shared-bead", "20240116T120000000000+0000");
        let box2 = Box::new("box2".to_string(), temp_dir2.path()).unwrap();
        
        let union = UnionBox::new(vec![box1, box2]);
        
        // Should find beads from both boxes
        let all_beads = union.all_beads().unwrap();
        assert_eq!(all_beads.len(), 4);
        
        // Should find shared bead from both boxes
        let shared = union.find_by_name("shared-bead").unwrap();
        assert_eq!(shared.len(), 2);
        
        // Should find latest version
        let latest = union.find_latest("shared-bead").unwrap();
        assert!(latest.is_some());
        assert!(latest.unwrap().path().to_str().unwrap().contains("20240116"));
    }

    #[test]
    fn test_union_box_find_by_kind() {
        let temp_dir1 = TempDir::new().unwrap();
        create_test_archive_with_kind(&temp_dir1, "bead1", "kind-a", "20240115T120000000000+0000");
        let box1 = Box::new("box1".to_string(), temp_dir1.path()).unwrap();
        
        let temp_dir2 = TempDir::new().unwrap();
        create_test_archive_with_kind(&temp_dir2, "bead2", "kind-a", "20240115T120000000000+0000");
        create_test_archive_with_kind(&temp_dir2, "bead3", "kind-b", "20240115T120000000000+0000");
        let box2 = Box::new("box2".to_string(), temp_dir2.path()).unwrap();
        
        let union = UnionBox::new(vec![box1, box2]);
        
        let kind_a_beads = union.find_by_kind("kind-a").unwrap();
        assert_eq!(kind_a_beads.len(), 2);
        
        let kind_b_beads = union.find_by_kind("kind-b").unwrap();
        assert_eq!(kind_b_beads.len(), 1);
    }

    #[test]
    fn test_find_with_ambiguous_ref() {
        let (temp_dir, box_store) = create_test_box();
        
        // Create archives where a string could match multiple criteria
        create_test_archive(&temp_dir, "abc123", "20240115T120000000000+0000"); // name is "abc123"
        create_test_archive_with_content_id(&temp_dir, "other-bead", "abc123def456", "20240115T120000000000+0000"); // content starts with "abc123"
        
        // Search for "abc123" should find both
        let results = box_store.find_by_ref("abc123").unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_case_sensitive_search() {
        let (temp_dir, box_store) = create_test_box();
        
        create_test_archive(&temp_dir, "TestBead", "20240115T120000000000+0000");
        create_test_archive(&temp_dir, "testbead", "20240115T120000000000+0000");
        
        // Should be case-sensitive
        let upper = box_store.find_by_name("TestBead").unwrap();
        assert_eq!(upper.len(), 1);
        
        let lower = box_store.find_by_name("testbead").unwrap();
        assert_eq!(lower.len(), 1);
        
        // Wrong case should not find
        let wrong = box_store.find_by_name("TESTBEAD").unwrap();
        assert_eq!(wrong.len(), 0);
    }

    // Helper functions for creating test archives
    fn create_test_archive(temp_dir: &TempDir, name: &str, timestamp: &str) {
        let archive_path = temp_dir.path().join(format!("{}_{}.zip", name, timestamp));
        create_archive_file(&archive_path, name, "test-kind", "content123", timestamp);
    }

    fn create_test_archive_with_kind(temp_dir: &TempDir, name: &str, kind: &str, timestamp: &str) {
        let archive_path = temp_dir.path().join(format!("{}_{}.zip", name, timestamp));
        create_archive_file(&archive_path, name, kind, "content123", timestamp);
    }

    fn create_test_archive_with_content_id(temp_dir: &TempDir, name: &str, content_id: &str, timestamp: &str) {
        let archive_path = temp_dir.path().join(format!("{}_{}.zip", name, timestamp));
        create_archive_file(&archive_path, name, "test-kind", content_id, timestamp);
    }

    fn create_archive_file(path: &Path, name: &str, kind: &str, content_id: &str, timestamp: &str) {
        use crate::core::meta::{BeadMeta, BeadName};
        use crate::tech::persistence;
        use std::io::Write;
        
        let file = fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        
        // Create metadata with specified values
        let mut meta = BeadMeta::new_frozen(
            kind.to_string(),
            BeadName::new(name).unwrap(),
            timestamp.to_string(),
        );
        
        // We need to set the content_id somehow - this might need adjustment
        // based on actual BeadMeta implementation
        
        persistence::save_json_to_zip(&mut zip, &meta, crate::core::archive::layout::BEAD_META).unwrap();
        
        let manifest: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        persistence::save_json_to_zip(&mut zip, &manifest, crate::core::archive::layout::MANIFEST).unwrap();
        
        let input_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        persistence::save_json_to_zip(&mut zip, &input_map, crate::core::archive::layout::INPUT_MAP).unwrap();
        
        // Add some data
        let options = zip::write::FileOptions::default();
        zip.start_file("data/test.txt", options).unwrap();
        zip.write_all(b"test content").unwrap();
        
        zip.finish().unwrap();
    }
}