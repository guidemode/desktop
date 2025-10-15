use std::path::Path;

/// Check if a file should be filtered out (hidden files, temp files)
pub fn should_skip_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

/// Get file size safely
pub fn get_file_size(path: &Path) -> Result<u64, std::io::Error> {
    let metadata = std::fs::metadata(path)?;
    Ok(metadata.len())
}

/// Check if file matches extension
pub fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e == ext)
        .unwrap_or(false)
}

/// Extract session ID from filename (stem without extension)
pub fn extract_session_id_from_filename(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_should_skip_file() {
        assert!(should_skip_file(Path::new(".hidden")));
        assert!(should_skip_file(Path::new("/path/.hidden")));
        assert!(!should_skip_file(Path::new("visible.txt")));
        assert!(!should_skip_file(Path::new("/path/visible.txt")));
    }

    #[test]
    fn test_has_extension() {
        assert!(has_extension(Path::new("file.json"), "json"));
        assert!(has_extension(Path::new("/path/file.json"), "json"));
        assert!(!has_extension(Path::new("file.txt"), "json"));
        assert!(!has_extension(Path::new("file"), "json"));
    }

    #[test]
    fn test_extract_session_id() {
        assert_eq!(
            extract_session_id_from_filename(Path::new("session123.json")),
            "session123"
        );
        assert_eq!(
            extract_session_id_from_filename(Path::new("/path/to/abc-def.json")),
            "abc-def"
        );
        assert_eq!(
            extract_session_id_from_filename(Path::new("nosuffix")),
            "nosuffix"
        );
    }
}
