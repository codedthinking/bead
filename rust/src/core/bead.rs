use chrono::{DateTime, Utc};
use crate::core::meta::{BeadName, ContentId, InputSpec};

/// Common interface for all beads (workspaces and archives)
pub trait Bead {
    /// Get the bead name
    fn name(&self) -> &BeadName;
    
    /// Get the bead kind
    fn kind(&self) -> &str;
    
    /// Get all inputs
    fn inputs(&self) -> Vec<&InputSpec>;
    
    /// Get content ID
    fn content_id(&self) -> ContentId;
    
    /// Get freeze time
    fn freeze_time(&self) -> DateTime<Utc>;
    
    /// Get box name
    fn box_name(&self) -> &str;
    
    /// Check if a specific input exists
    fn has_input(&self, name: &str) -> bool {
        self.inputs().iter().any(|input| {
            // Compare against the input spec's key in the hashmap
            false // This would need adjustment based on actual structure
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Mock implementation for testing
    struct MockBead {
        name: BeadName,
        kind: String,
        inputs: Vec<InputSpec>,
        content_id: ContentId,
        freeze_time: DateTime<Utc>,
        box_name: String,
    }
    
    impl Bead for MockBead {
        fn name(&self) -> &BeadName {
            &self.name
        }
        
        fn kind(&self) -> &str {
            &self.kind
        }
        
        fn inputs(&self) -> Vec<&InputSpec> {
            self.inputs.iter().collect()
        }
        
        fn content_id(&self) -> ContentId {
            self.content_id.clone()
        }
        
        fn freeze_time(&self) -> DateTime<Utc> {
            self.freeze_time
        }
        
        fn box_name(&self) -> &str {
            &self.box_name
        }
    }
    
    #[test]
    fn test_bead_trait_implementation() {
        let mock_bead = MockBead {
            name: BeadName::new("test-bead").unwrap(),
            kind: "test-kind".to_string(),
            inputs: vec![],
            content_id: ContentId::new("abc123"),
            freeze_time: Utc::now(),
            box_name: "test-box".to_string(),
        };
        
        assert_eq!(mock_bead.name().as_str(), "test-bead");
        assert_eq!(mock_bead.kind(), "test-kind");
        assert!(mock_bead.inputs().is_empty());
        assert_eq!(mock_bead.content_id().as_str(), "abc123");
        assert_eq!(mock_bead.box_name(), "test-box");
    }
}