//! File commit functionality for sketch sessions.
//!
//! Allows files modified in a session to be persisted to the host filesystem.
//! This feature is only available inside an active session.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Check if we're currently inside a sketch session.
fn in_session() -> bool {
    std::env::var("SKETCH_SESSION").is_ok()
}

/// Get the session directory path from environment.
/// Returns the path to the session's merged root directory.
fn get_session_dir() -> Option<PathBuf> {
    std::env::var("SKETCH_SESSION_DIR")
        .ok()
        .map(PathBuf::from)
}

/// Commit (persist) files from the overlay to the host filesystem.
///
/// # Arguments
/// * `files` - List of file paths to commit (relative to overlay root)
/// * `force` - If true, overwrite existing files without prompting
/// * `verbose` - If true, show detailed output
///
/// # Returns
/// * `Ok(count)` - Number of files successfully committed
/// * `Err(msg)` - Error message if operation failed
pub fn commit_files(files: &[String], force: bool, verbose: bool) -> Result<usize, String> {
    if !in_session() {
        return Err(
            "sketch: 'commit' can only be used inside a sketch session.\n\
             (Set SKETCH_SESSION_DIR to override for testing)"
                .into(),
        );
    }

    if files.is_empty() {
        return Err("sketch: no files specified for commit".into());
    }

    let mut committed = 0;

    for file_path in files {
        match commit_single_file(file_path, force, verbose) {
            Ok(true) => committed += 1,
            Ok(false) => {
                // User declined to overwrite
                if verbose {
                    eprintln!("sketch: skipped {}", file_path);
                }
            }
            Err(e) => {
                eprintln!("sketch: failed to commit {}: {}", file_path, e);
            }
        }
    }

    Ok(committed)
}

/// Commit a single file.
///
/// # Returns
/// * `Ok(true)` - File was committed
/// * `Ok(false)` - File was skipped (user declined overwrite)
/// * `Err(msg)` - Error occurred
fn commit_single_file(file_path: &str, force: bool, verbose: bool) -> Result<bool, String> {
    let normalized_path = normalize_path(file_path);

    // Get the current working directory inside the session
    let cwd = std::env::current_dir().ok();

    // Resolve the file path (handle relative vs absolute)
    let file_path_resolved = if Path::new(&normalized_path).is_absolute() {
        normalized_path.clone()
    } else {
        if let Some(cwd) = cwd {
            cwd.join(&normalized_path)
                .to_string_lossy()
                .to_string()
        } else {
            normalized_path.clone()
        }
    };

    // Check that the file exists in the current (merged) view
    if !Path::new(&file_path_resolved).exists() {
        return Err(format!("file not found: {}", file_path_resolved));
    }

    // Warn if file exists on the host (user might overwrite unintentionally)
    if Path::new(&normalized_path).exists() && !force {
        eprintln!(
            "sketch: warning: {} already exists on the host filesystem",
            normalized_path
        );
        eprintln!("sketch: use 'sketch commit --force {}' to overwrite", file_path);
        return Ok(false);
    }

    // Copy the file to the host
    // Since we're in the merged view after pivot_root, we can just read and write
    match fs::read(&file_path_resolved) {
        Ok(content) => {
            // Create parent directories if needed
            if let Some(parent) = Path::new(&normalized_path).parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create parent directories: {}", e))?;
                }
            }

            // Write the file
            fs::write(&normalized_path, content)
                .map_err(|e| format!("failed to write file: {}", e))?;

            if verbose {
                eprintln!("sketch: committed {} -> {}", file_path_resolved, normalized_path);
            }

            Ok(true)
        }
        Err(e) => Err(format!("failed to read file: {}", e)),
    }
}

/// Normalize a file path (remove redundant . and .. components).
fn normalize_path(path: &str) -> String {
    use std::path::Component;

    let path = Path::new(path);
    path.components()
        .fold(PathBuf::new(), |mut path, component| {
            match component {
                Component::ParentDir => {
                    path.pop();
                }
                Component::CurDir => {}
                _ => {
                    path.push(component);
                }
            }
            path
        })
        .to_string_lossy()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_removes_dots() {
        assert_eq!(normalize_path("/home/./user"), "/home/user");
    }

    #[test]
    fn test_normalize_path_handles_parent_dir() {
        assert_eq!(normalize_path("/home/user/../admin"), "/home/admin");
    }

    #[test]
    fn test_normalize_path_absolute() {
        assert_eq!(normalize_path("/etc/config"), "/etc/config");
    }

    #[test]
    fn test_not_in_session_error() {
        // Outside a session, commit should fail
        if !in_session() {
            let result = commit_files(&["test.txt".into()], false, false);
            assert!(result.is_err());
        }
    }
}
