use std::path::{Path, PathBuf};
use std::fs;
use crate::error::Result;
use crate::core::meta::{InputSpec, InputName};
use crate::core::archive::Archive;

/// Represents an input dependency in a workspace
#[derive(Debug, Clone)]
pub struct Input {
    pub name: InputName,
    pub spec: InputSpec,
    pub loaded: bool,
    pub path: PathBuf,
}

impl Input {
    /// Create a new input
    pub fn new(name: InputName, spec: InputSpec, workspace_dir: &Path) -> Self {
        let path = workspace_dir.join("input").join(name.as_str());
        let loaded = path.exists();
        
        Input {
            name,
            spec,
            loaded,
            path,
        }
    }

    /// Load input data from an archive
    pub fn load(&mut self, archive: &Archive) -> Result<()> {
        if self.loaded {
            return Ok(());
        }

        // Create input directory
        fs::create_dir_all(&self.path)?;
        
        // Extract data from archive
        archive.unpack_data_to(&self.path)?;
        
        // Make directory read-only
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&self.path)?.permissions();
            perms.set_mode(0o555); // r-xr-xr-x
            fs::set_permissions(&self.path, perms)?;
        }
        
        self.loaded = true;
        Ok(())
    }

    /// Unload input data (remove from filesystem)
    pub fn unload(&mut self) -> Result<()> {
        if !self.loaded {
            return Ok(());
        }

        // Make directory writable before deletion
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&self.path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&self.path, perms)?;
        }

        fs::remove_dir_all(&self.path)?;
        self.loaded = false;
        Ok(())
    }

    /// Update input to a newer version
    pub fn update(&mut self, new_spec: InputSpec, archive: &Archive) -> Result<()> {
        // Unload old version if loaded
        if self.loaded {
            self.unload()?;
        }
        
        // Update spec
        self.spec = new_spec;
        
        // Load new version
        self.load(archive)?;
        
        Ok(())
    }

    /// Check if this input matches the given spec
    pub fn matches_spec(&self, spec: &InputSpec) -> bool {
        self.spec.content_id == spec.content_id
    }

    /// Check if an update is available
    pub fn needs_update(&self, latest_spec: &InputSpec) -> bool {
        self.spec.freeze_time != latest_spec.freeze_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_input() -> (TempDir, Input) {
        let temp_dir = TempDir::new().unwrap();
        let name = InputName::new("test-input").unwrap();
        let spec = InputSpec::new(
            "input-kind".to_string(),
            "content123".to_string(),
            "20240115T120000000000+0000".to_string(),
        );
        
        let input = Input::new(name, spec, temp_dir.path());
        (temp_dir, input)
    }

    #[test]
    fn test_input_creation() {
        let (_temp_dir, input) = create_test_input();
        
        assert_eq!(input.name.as_str(), "test-input");
        assert_eq!(input.spec.kind, "input-kind");
        assert_eq!(input.spec.content_id, "content123");
        assert!(!input.loaded);
    }

    #[test]
    fn test_input_matches_spec() {
        let (_temp_dir, input) = create_test_input();
        
        let matching_spec = InputSpec::new(
            "input-kind".to_string(),
            "content123".to_string(),
            "20240115T120000000000+0000".to_string(),
        );
        assert!(input.matches_spec(&matching_spec));
        
        let different_spec = InputSpec::new(
            "input-kind".to_string(),
            "different123".to_string(),
            "20240115T120000000000+0000".to_string(),
        );
        assert!(!input.matches_spec(&different_spec));
    }

    #[test]
    fn test_input_needs_update() {
        let (_temp_dir, input) = create_test_input();
        
        let newer_spec = InputSpec::new(
            "input-kind".to_string(),
            "content123".to_string(),
            "20240116T120000000000+0000".to_string(), // Different timestamp
        );
        assert!(input.needs_update(&newer_spec));
        
        let same_spec = InputSpec::new(
            "input-kind".to_string(),
            "content123".to_string(),
            "20240115T120000000000+0000".to_string(), // Same timestamp
        );
        assert!(!input.needs_update(&same_spec));
    }
}