use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashMap;
use crate::error::{BeadError, Result};
use crate::core::meta::{BeadMeta, InputSpec};
use crate::tech::persistence;

/// Layout constants for workspace directories
pub mod layout {
    
    pub const INPUT: &str = "input";
    pub const OUTPUT: &str = "output";
    pub const TEMP: &str = "temp";
    pub const META_DIR: &str = ".bead-meta";
    pub const BEAD_META: &str = ".bead-meta/bead";
    pub const INPUT_MAP: &str = ".bead-meta/input.map";
}

/// Represents a bead workspace (working directory)
#[derive(Debug, Clone)]
pub struct Workspace {
    pub directory: PathBuf,
    pub meta: BeadMeta,
    pub input_map: HashMap<String, String>,
}

impl Workspace {
    /// Open an existing workspace
    pub fn open(directory: impl AsRef<Path>) -> Result<Self> {
        let directory = directory.as_ref().to_path_buf();
        
        if !Self::is_valid(&directory) {
            return Err(BeadError::InvalidWorkspace(
                format!("Not a valid workspace: {}", directory.display())
            ));
        }

        let meta = persistence::load_json(&directory.join(layout::BEAD_META))?;
        let input_map = if directory.join(layout::INPUT_MAP).exists() {
            persistence::load_json(&directory.join(layout::INPUT_MAP))?
        } else {
            HashMap::new()
        };

        Ok(Workspace {
            directory,
            meta,
            input_map,
        })
    }

    /// Create a new workspace
    pub fn create(directory: impl AsRef<Path>, kind: String) -> Result<Self> {
        let directory = directory.as_ref().to_path_buf();
        
        if directory.exists() {
            return Err(BeadError::AlreadyExists(
                format!("Directory already exists: {}", directory.display())
            ));
        }

        // Create directory structure
        fs::create_dir_all(&directory)?;
        fs::create_dir(directory.join(layout::INPUT))?;
        fs::create_dir(directory.join(layout::OUTPUT))?;
        fs::create_dir(directory.join(layout::TEMP))?;
        fs::create_dir(directory.join(layout::META_DIR))?;

        // Create metadata
        let meta = BeadMeta::new_workspace(kind);
        persistence::save_json(&meta, &directory.join(layout::BEAD_META))?;

        // Make input directory read-only
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(directory.join(layout::INPUT))?.permissions();
            perms.set_mode(0o555); // r-xr-xr-x
            fs::set_permissions(directory.join(layout::INPUT), perms)?;
        }

        Ok(Workspace {
            directory,
            meta,
            input_map: HashMap::new(),
        })
    }

    /// Check if a directory is a valid workspace
    pub fn is_valid(directory: impl AsRef<Path>) -> bool {
        let dir = directory.as_ref();
        dir.join(layout::INPUT).is_dir()
            && dir.join(layout::OUTPUT).is_dir()
            && dir.join(layout::TEMP).is_dir()
            && dir.join(layout::BEAD_META).is_file()
    }

    /// Get the workspace name (directory name)
    pub fn name(&self) -> Option<&str> {
        self.directory.file_name()?.to_str()
    }

    /// Get the workspace kind
    pub fn kind(&self) -> &str {
        &self.meta.kind
    }

    /// Get all inputs
    pub fn inputs(&self) -> Vec<(&String, &InputSpec)> {
        self.meta.inputs.iter().collect()
    }

    /// Check if an input exists
    pub fn has_input(&self, name: &str) -> bool {
        self.meta.has_input(name)
    }

    /// Check if an input is loaded (data present in input directory)
    pub fn is_loaded(&self, name: &str) -> bool {
        self.directory.join(layout::INPUT).join(name).is_dir()
    }

    /// Add a new input
    pub fn add_input(&mut self, name: String, spec: InputSpec) -> Result<()> {
        if self.has_input(&name) {
            return Err(BeadError::AlreadyExists(
                format!("Input '{}' already exists", name)
            ));
        }

        self.meta.add_input(name, spec);
        self.save_meta()?;
        Ok(())
    }

    /// Remove an input
    pub fn delete_input(&mut self, name: &str) -> Result<()> {
        if !self.has_input(name) {
            return Err(BeadError::InvalidInput(
                format!("Input '{}' does not exist", name)
            ));
        }

        // Unload if loaded
        if self.is_loaded(name) {
            self.unload_input(name)?;
        }

        self.meta.remove_input(name);
        self.input_map.remove(name);
        self.save_meta()?;
        self.save_input_map()?;
        Ok(())
    }

    /// Unload input data (remove from filesystem but keep metadata)
    pub fn unload_input(&self, name: &str) -> Result<()> {
        let input_path = self.directory.join(layout::INPUT).join(name);
        if input_path.exists() {
            // Make directory writable before deletion
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&input_path)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&input_path, perms)?;
            }
            fs::remove_dir_all(input_path)?;
        }
        Ok(())
    }

    /// Get the bead name mapping for an input
    pub fn get_input_bead_name(&self, input_name: &str) -> Option<&String> {
        self.input_map.get(input_name)
    }

    /// Set the bead name mapping for an input
    pub fn set_input_bead_name(&mut self, input_name: String, bead_name: String) -> Result<()> {
        if !self.has_input(&input_name) {
            return Err(BeadError::InvalidInput(
                format!("Input '{}' does not exist", input_name)
            ));
        }

        self.input_map.insert(input_name, bead_name);
        self.save_input_map()?;
        Ok(())
    }

    /// Save metadata to disk
    fn save_meta(&self) -> Result<()> {
        persistence::save_json(&self.meta, &self.directory.join(layout::BEAD_META))
    }

    /// Save input map to disk
    fn save_input_map(&self) -> Result<()> {
        if !self.input_map.is_empty() {
            persistence::save_json(&self.input_map, &self.directory.join(layout::INPUT_MAP))
        } else {
            // Remove file if map is empty
            let path = self.directory.join(layout::INPUT_MAP);
            if path.exists() {
                fs::remove_file(path)?;
            }
            Ok(())
        }
    }

    /// Delete the workspace from disk
    pub fn delete(self) -> Result<()> {
        // Make input directory writable before deletion
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let input_dir = self.directory.join(layout::INPUT);
            if input_dir.exists() {
                let mut perms = fs::metadata(&input_dir)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&input_dir, perms)?;
            }
        }

        fs::remove_dir_all(&self.directory)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_workspace() -> (TempDir, Workspace) {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path().join("test-workspace");
        let workspace = Workspace::create(&workspace_path, "test-kind".to_string()).unwrap();
        (temp_dir, workspace)
    }

    #[test]
    fn test_workspace_creation() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path().join("test-workspace");
        
        let workspace = Workspace::create(&workspace_path, "test-kind".to_string()).unwrap();
        
        assert!(workspace_path.exists());
        assert!(workspace_path.join(layout::INPUT).is_dir());
        assert!(workspace_path.join(layout::OUTPUT).is_dir());
        assert!(workspace_path.join(layout::TEMP).is_dir());
        assert!(workspace_path.join(layout::BEAD_META).is_file());
        assert_eq!(workspace.kind(), "test-kind");
    }

    #[test]
    fn test_workspace_already_exists() {
        let (_temp_dir, workspace) = create_test_workspace();
        
        let result = Workspace::create(&workspace.directory, "another-kind".to_string());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), BeadError::AlreadyExists(_)));
    }

    #[test]
    fn test_workspace_open() {
        let (_temp_dir, workspace) = create_test_workspace();
        let workspace_path = workspace.directory.clone();
        
        let opened = Workspace::open(&workspace_path).unwrap();
        assert_eq!(opened.kind(), "test-kind");
        assert_eq!(opened.directory, workspace_path);
    }

    #[test]
    fn test_workspace_invalid() {
        let temp_dir = TempDir::new().unwrap();
        let invalid_path = temp_dir.path().join("invalid");
        
        let result = Workspace::open(&invalid_path);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), BeadError::InvalidWorkspace(_)));
    }

    #[test]
    fn test_workspace_is_valid() {
        let (_temp_dir, workspace) = create_test_workspace();
        assert!(Workspace::is_valid(&workspace.directory));
        
        let temp_dir = TempDir::new().unwrap();
        assert!(!Workspace::is_valid(temp_dir.path()));
    }

    #[test]
    fn test_workspace_name() {
        let (_temp_dir, workspace) = create_test_workspace();
        assert_eq!(workspace.name(), Some("test-workspace"));
    }

    #[test]
    fn test_workspace_inputs() {
        let (_temp_dir, mut workspace) = create_test_workspace();
        
        // Initially no inputs
        assert!(workspace.inputs().is_empty());
        
        // Add an input
        let spec = InputSpec::new(
            "input-kind".to_string(),
            "content123".to_string(),
            "20240115T120000000000+0000".to_string(),
        );
        workspace.add_input("input1".to_string(), spec.clone()).unwrap();
        
        // Check input exists
        assert!(workspace.has_input("input1"));
        assert_eq!(workspace.inputs().len(), 1);
        
        // Check input is not loaded
        assert!(!workspace.is_loaded("input1"));
    }

    #[test]
    fn test_workspace_add_duplicate_input() {
        let (_temp_dir, mut workspace) = create_test_workspace();
        
        let spec = InputSpec::new(
            "input-kind".to_string(),
            "content123".to_string(),
            "20240115T120000000000+0000".to_string(),
        );
        
        workspace.add_input("input1".to_string(), spec.clone()).unwrap();
        
        // Try to add duplicate
        let result = workspace.add_input("input1".to_string(), spec);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), BeadError::AlreadyExists(_)));
    }

    #[test]
    fn test_workspace_delete_input() {
        let (_temp_dir, mut workspace) = create_test_workspace();
        
        let spec = InputSpec::new(
            "input-kind".to_string(),
            "content123".to_string(),
            "20240115T120000000000+0000".to_string(),
        );
        
        workspace.add_input("input1".to_string(), spec).unwrap();
        assert!(workspace.has_input("input1"));
        
        workspace.delete_input("input1").unwrap();
        assert!(!workspace.has_input("input1"));
    }

    #[test]
    fn test_workspace_delete_nonexistent_input() {
        let (_temp_dir, mut workspace) = create_test_workspace();
        
        let result = workspace.delete_input("nonexistent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), BeadError::InvalidInput(_)));
    }

    #[test]
    fn test_workspace_input_bead_name_mapping() {
        let (_temp_dir, mut workspace) = create_test_workspace();
        
        let spec = InputSpec::new(
            "input-kind".to_string(),
            "content123".to_string(),
            "20240115T120000000000+0000".to_string(),
        );
        
        workspace.add_input("input1".to_string(), spec).unwrap();
        
        // Initially no mapping
        assert_eq!(workspace.get_input_bead_name("input1"), None);
        
        // Set mapping
        workspace.set_input_bead_name("input1".to_string(), "bead-name".to_string()).unwrap();
        assert_eq!(workspace.get_input_bead_name("input1"), Some(&"bead-name".to_string()));
    }

    #[test]
    fn test_workspace_set_mapping_for_nonexistent_input() {
        let (_temp_dir, mut workspace) = create_test_workspace();
        
        let result = workspace.set_input_bead_name("nonexistent".to_string(), "bead".to_string());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), BeadError::InvalidInput(_)));
    }

    #[test]
    fn test_workspace_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path().join("test-workspace");
        
        // Create and modify workspace
        {
            let mut workspace = Workspace::create(&workspace_path, "test-kind".to_string()).unwrap();
            
            let spec = InputSpec::new(
                "input-kind".to_string(),
                "content123".to_string(),
                "20240115T120000000000+0000".to_string(),
            );
            
            workspace.add_input("input1".to_string(), spec).unwrap();
            workspace.set_input_bead_name("input1".to_string(), "bead-name".to_string()).unwrap();
        }
        
        // Open again and verify
        {
            let workspace = Workspace::open(&workspace_path).unwrap();
            assert_eq!(workspace.kind(), "test-kind");
            assert!(workspace.has_input("input1"));
            assert_eq!(workspace.get_input_bead_name("input1"), Some(&"bead-name".to_string()));
        }
    }

    #[test]
    fn test_workspace_delete() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path().join("test-workspace");
        
        let workspace = Workspace::create(&workspace_path, "test-kind".to_string()).unwrap();
        assert!(workspace_path.exists());
        
        workspace.delete().unwrap();
        assert!(!workspace_path.exists());
    }
}