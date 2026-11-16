//! Helpers shared by unit tests.

use std::fs;
use std::path::{Path, PathBuf};

/// A fresh, unique directory under the system temp dir.
pub fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("screenmate_{tag}_{}_{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Writes `content` to `path`, creating parent folders.
pub fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// Minimal PNG header (signature + IHDR width/height) for texture-size checks.
pub fn png_header(width: u32, height: u32) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend_from_slice(&width.to_be_bytes());
    png.extend_from_slice(&height.to_be_bytes());
    png
}
