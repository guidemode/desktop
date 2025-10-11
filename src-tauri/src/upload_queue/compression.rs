//! Gzip compression utilities for upload optimization.
//!
//! Reduces payload size for network transfer.

use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;

/// Compress file content using gzip (for v2 upload optimization)
pub fn compress_file_content(content: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(content)
        .map_err(|e| format!("Failed to compress content: {}", e))?;
    encoder
        .finish()
        .map_err(|e| format!("Failed to finalize compression: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_file_content() {
        let content = b"test content that should be compressed";
        let compressed = compress_file_content(content).unwrap();

        // Compressed should be smaller for repetitive content
        // (though very small content might not compress much)
        assert!(!compressed.is_empty());
    }

    #[test]
    fn test_compress_empty_content() {
        let content = b"";
        let compressed = compress_file_content(content).unwrap();

        // Even empty content produces gzip headers
        assert!(!compressed.is_empty());
    }

    #[test]
    fn test_compress_large_content() {
        // Create repetitive content that should compress well
        let content = "test ".repeat(1000);
        let compressed = compress_file_content(content.as_bytes()).unwrap();

        // Repetitive content should compress significantly
        assert!(compressed.len() < content.len());
    }
}
