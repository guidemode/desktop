//! SHA256 hashing utilities for deduplication.
//!
//! Provides hash functions for both file and in-memory content.

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

/// Calculate SHA256 hash of file content (for v2 upload deduplication)
pub fn calculate_file_hash_sha256(file_path: &PathBuf) -> Result<String, String> {
    let mut file = File::open(file_path)
        .map_err(|e| format!("Failed to open file for hashing: {}", e))?;

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .map_err(|e| format!("Failed to read file for hashing: {}", e))?;

    // Calculate SHA256 hash
    let mut hasher = Sha256::new();
    hasher.update(&buffer);
    let result = hasher.finalize();

    // Convert to hex string
    Ok(format!("{:x}", result))
}

/// Calculate SHA256 hash of content in memory (for v2 upload deduplication)
pub fn calculate_content_hash_sha256(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_calculate_content_hash() {
        let content = "test content";
        let hash = calculate_content_hash_sha256(content);
        
        // SHA256 hash should be 64 characters (hex)
        assert_eq!(hash.len(), 64);
        
        // Same content should produce same hash
        let hash2 = calculate_content_hash_sha256(content);
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_calculate_file_hash() {
        // Create temp file
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"test file content").unwrap();
        temp_file.flush().unwrap();
        
        let path = PathBuf::from(temp_file.path());
        let hash = calculate_file_hash_sha256(&path).unwrap();
        
        // SHA256 hash should be 64 characters (hex)
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_different_content_different_hash() {
        let hash1 = calculate_content_hash_sha256("content1");
        let hash2 = calculate_content_hash_sha256("content2");
        
        assert_ne!(hash1, hash2);
    }
}
