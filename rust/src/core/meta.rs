use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use crate::error::{BeadError, Result};

/// Metadata version for compatibility
pub const META_VERSION: &str = "aaa947a6-1f7a-11e6-ba3a-0021cc73492e";

/// Type-safe wrapper for bead names
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BeadName(String);

impl BeadName {
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if Self::is_valid(&name) {
            Ok(BeadName(name))
        } else {
            Err(BeadError::InvalidBeadName(name))
        }
    }

    pub fn is_valid(name: &str) -> bool {
        !name.is_empty() 
            && name != "." 
            && name != ".."
            && !name.contains('/')
            && !name.contains("__")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BeadName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Type-safe wrapper for input names
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InputName(String);

impl InputName {
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if BeadName::is_valid(&name) {
            Ok(InputName(name))
        } else {
            Err(BeadError::InvalidInput(format!("Invalid input name: {}", name)))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Type-safe wrapper for content IDs
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentId(String);

impl ContentId {
    pub fn new(id: impl Into<String>) -> Self {
        ContentId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ContentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Input specification - describes a dependency
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputSpec {
    pub kind: String,
    pub content_id: String,
    pub freeze_time: String,
}

impl InputSpec {
    pub fn new(kind: String, content_id: String, freeze_time: String) -> Self {
        InputSpec {
            kind,
            content_id,
            freeze_time,
        }
    }

    pub fn freeze_time_datetime(&self) -> Result<DateTime<Utc>> {
        crate::tech::timestamp::parse_timestamp(&self.freeze_time)
    }
}

/// Main metadata structure for beads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeadMeta {
    pub meta_version: String,
    pub kind: String,
    pub inputs: HashMap<String, InputSpec>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freeze_time: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freeze_name: Option<String>,
}

impl BeadMeta {
    /// Create a new workspace metadata
    pub fn new_workspace(kind: String) -> Self {
        BeadMeta {
            meta_version: META_VERSION.to_string(),
            kind,
            inputs: HashMap::new(),
            freeze_time: None,
            freeze_name: None,
        }
    }

    /// Create metadata for a frozen bead
    pub fn new_frozen(kind: String, name: BeadName, freeze_time: String) -> Self {
        BeadMeta {
            meta_version: META_VERSION.to_string(),
            kind,
            inputs: HashMap::new(),
            freeze_time: Some(freeze_time),
            freeze_name: Some(name.to_string()),
        }
    }

    /// Add an input dependency
    pub fn add_input(&mut self, name: String, spec: InputSpec) {
        self.inputs.insert(name, spec);
    }

    /// Remove an input dependency
    pub fn remove_input(&mut self, name: &str) -> Option<InputSpec> {
        self.inputs.remove(name)
    }

    /// Check if an input exists
    pub fn has_input(&self, name: &str) -> bool {
        self.inputs.contains_key(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bead_name_validation() {
        // Valid names
        assert!(BeadName::new("valid-name").is_ok());
        assert!(BeadName::new("test_bead").is_ok());
        assert!(BeadName::new("bead123").is_ok());
        assert!(BeadName::new("my.bead.v2").is_ok());

        // Invalid names
        assert!(BeadName::new("").is_err());
        assert!(BeadName::new(".").is_err());
        assert!(BeadName::new("..").is_err());
        assert!(BeadName::new("path/to/bead").is_err());
        assert!(BeadName::new("bead__private").is_err());
    }

    #[test]
    fn test_input_name_validation() {
        // Valid input names
        assert!(InputName::new("input1").is_ok());
        assert!(InputName::new("my-input").is_ok());

        // Invalid input names
        assert!(InputName::new("").is_err());
        assert!(InputName::new("../parent").is_err());
    }

    #[test]
    fn test_content_id_creation() {
        let id = ContentId::new("abc123def456");
        assert_eq!(id.as_str(), "abc123def456");
    }

    #[test]
    fn test_input_spec_creation() {
        let spec = InputSpec::new(
            "some-kind".to_string(),
            "content123".to_string(),
            "20240115T120000000000+0000".to_string(),
        );
        assert_eq!(spec.kind, "some-kind");
        assert_eq!(spec.content_id, "content123");
        assert_eq!(spec.freeze_time, "20240115T120000000000+0000");
    }

    #[test]
    fn test_bead_meta_workspace() {
        let meta = BeadMeta::new_workspace("test-kind".to_string());
        assert_eq!(meta.meta_version, META_VERSION);
        assert_eq!(meta.kind, "test-kind");
        assert!(meta.inputs.is_empty());
        assert!(meta.freeze_time.is_none());
        assert!(meta.freeze_name.is_none());
    }

    #[test]
    fn test_bead_meta_frozen() {
        let name = BeadName::new("test-bead").unwrap();
        let freeze_time = "20240115T120000000000+0000".to_string();
        let meta = BeadMeta::new_frozen("test-kind".to_string(), name, freeze_time.clone());
        
        assert_eq!(meta.meta_version, META_VERSION);
        assert_eq!(meta.kind, "test-kind");
        assert_eq!(meta.freeze_time, Some(freeze_time));
        assert_eq!(meta.freeze_name, Some("test-bead".to_string()));
    }

    #[test]
    fn test_bead_meta_inputs() {
        let mut meta = BeadMeta::new_workspace("test-kind".to_string());
        
        let spec = InputSpec::new(
            "input-kind".to_string(),
            "content456".to_string(),
            "20240115T120000000000+0000".to_string(),
        );
        
        // Add input
        meta.add_input("input1".to_string(), spec.clone());
        assert!(meta.has_input("input1"));
        assert_eq!(meta.inputs.len(), 1);
        
        // Remove input
        let removed = meta.remove_input("input1");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap(), spec);
        assert!(!meta.has_input("input1"));
        assert!(meta.inputs.is_empty());
    }

    #[test]
    fn test_bead_meta_serialization() {
        let mut meta = BeadMeta::new_workspace("test-kind".to_string());
        meta.add_input(
            "input1".to_string(),
            InputSpec::new(
                "input-kind".to_string(),
                "content123".to_string(),
                "20240115T120000000000+0000".to_string(),
            ),
        );

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&meta).unwrap();
        assert!(json.contains("test-kind"));
        assert!(json.contains("input1"));

        // Deserialize back
        let deserialized: BeadMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.kind, meta.kind);
        assert_eq!(deserialized.inputs.len(), meta.inputs.len());
    }
}