use sha2::{Sha512, Digest};
use std::fs::File;
use std::io::{Read, BufReader};
use std::path::Path;
use crate::error::Result;

const READ_BLOCK_SIZE: usize = 1024 * 1024; // 1MB

/// Calculate SHA-512 hash with netstring-like format for bytes
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha512::new();
    
    // Add prefix
    let size = bytes.len();
    hasher.update(format!("{}:", size).as_bytes());
    
    // Add content
    hasher.update(bytes);
    
    // Add suffix
    hasher.update(format!(";{}", size).as_bytes());
    
    format!("{:x}", hasher.finalize())
}

/// Calculate SHA-512 hash with netstring-like format for a file
pub fn hash_file(path: impl AsRef<Path>) -> Result<String> {
    let file = File::open(path)?;
    let file_size = file.metadata()?.len();
    let mut reader = BufReader::new(file);
    let mut hasher = Sha512::new();
    
    // Add prefix
    hasher.update(format!("{}:", file_size).as_bytes());
    
    // Read and hash file in blocks
    let mut buffer = vec![0; READ_BLOCK_SIZE];
    let mut bytes_read = 0u64;
    
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        bytes_read += n as u64;
    }
    
    assert_eq!(bytes_read, file_size);
    
    // Add suffix
    hasher.update(format!(";{}", file_size).as_bytes());
    
    Ok(format!("{:x}", hasher.finalize()))
}

/// Calculate content ID from multiple file hashes
pub fn calculate_content_id(file_hashes: &[(String, String)]) -> String {
    let mut hasher = Sha512::new();
    
    for (path, hash) in file_hashes {
        hasher.update(path.as_bytes());
        hasher.update(b":");
        hasher.update(hash.as_bytes());
        hasher.update(b"\n");
    }
    
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_hash_bytes() {
        let data = b"test content";
        let hash = hash_bytes(data);
        
        // Should be a valid SHA-512 hex string (128 chars)
        assert_eq!(hash.len(), 128);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_bytes_consistency() {
        let data = b"test content";
        let hash1 = hash_bytes(data);
        let hash2 = hash_bytes(data);
        
        // Same data should produce same hash
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_bytes_different() {
        let data1 = b"content1";
        let data2 = b"content2";
        let hash1 = hash_bytes(data1);
        let hash2 = hash_bytes(data2);
        
        // Different data should produce different hashes
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let content = b"file content for testing";
        temp_file.write_all(content).unwrap();
        temp_file.flush().unwrap();
        
        let hash = hash_file(temp_file.path()).unwrap();
        
        // Should be a valid SHA-512 hex string
        assert_eq!(hash.len(), 128);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_file_consistency() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let content = b"file content for testing";
        temp_file.write_all(content).unwrap();
        temp_file.flush().unwrap();
        
        let hash1 = hash_file(temp_file.path()).unwrap();
        let hash2 = hash_file(temp_file.path()).unwrap();
        
        // Same file should produce same hash
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_calculate_content_id() {
        let file_hashes = vec![
            ("file1.txt".to_string(), "hash1".to_string()),
            ("file2.txt".to_string(), "hash2".to_string()),
        ];
        
        let content_id = calculate_content_id(&file_hashes);
        
        // Should be a valid SHA-512 hex string
        assert_eq!(content_id.len(), 128);
        assert!(content_id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_calculate_content_id_order_matters() {
        let hashes1 = vec![
            ("file1.txt".to_string(), "hash1".to_string()),
            ("file2.txt".to_string(), "hash2".to_string()),
        ];
        
        let hashes2 = vec![
            ("file2.txt".to_string(), "hash2".to_string()),
            ("file1.txt".to_string(), "hash1".to_string()),
        ];
        
        let id1 = calculate_content_id(&hashes1);
        let id2 = calculate_content_id(&hashes2);
        
        // Order should matter
        assert_ne!(id1, id2);
    }
}