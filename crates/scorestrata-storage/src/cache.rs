//! Content-addressed cache for expensive artifacts.

use std::path::Path;

use scorestrata_core::error::ScorestrataError;

/// Compute a SHA-256 hash of the given data.
#[must_use]
pub fn content_hash(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(data);
    hex::encode(hash)
}

/// Get the cache path for a given hash and category.
#[must_use]
pub fn cache_path(cache_dir: &Path, category: &str, hash: &str) -> std::path::PathBuf {
    cache_dir
        .join("objects")
        .join(category)
        .join(format!("{hash}.json"))
}

/// Check if a cached object exists.
#[must_use]
pub fn cache_hit(cache_dir: &Path, category: &str, hash: &str) -> bool {
    cache_path(cache_dir, category, hash).exists()
}

/// Store data in the cache.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] if the cache directory cannot be created
/// or the file cannot be written.
pub fn store_in_cache(
    cache_dir: &Path,
    category: &str,
    data: &[u8],
) -> Result<String, ScorestrataError> {
    let hash = content_hash(data);
    let path = cache_path(cache_dir, category, &hash);
    std::fs::create_dir_all(path.parent().unwrap_or(cache_dir)).map_err(|e| {
        ScorestrataError::Io {
            path: path.display().to_string(),
            source: e,
        }
    })?;
    std::fs::write(&path, data).map_err(|e| ScorestrataError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    Ok(hash)
}

/// Load data from the cache.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] if the cached file cannot be read.
pub fn load_from_cache(
    cache_dir: &Path,
    category: &str,
    hash: &str,
) -> Result<Vec<u8>, ScorestrataError> {
    let path = cache_path(cache_dir, category, hash);
    std::fs::read(&path).map_err(|e| ScorestrataError::Io {
        path: path.display().to_string(),
        source: e,
    })
}
