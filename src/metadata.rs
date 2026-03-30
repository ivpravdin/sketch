use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const METADATA_FILENAME: &str = ".sketch-metadata.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: String,
    pub name: Option<String>,
    pub created: u64,
    pub pid: u32,
    pub command: String,
    pub username: String,
    pub overlay_path: String,
}

impl SessionMetadata {
    pub fn new(id: &str, name: Option<String>, command: &str, overlay_path: &Path) -> Self {
        let created = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let username = std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .unwrap_or_else(|_| "unknown".into());

        Self {
            id: id.into(),
            name,
            created,
            pid: std::process::id(),
            command: command.into(),
            username,
            overlay_path: overlay_path.display().to_string(),
        }
    }

    pub fn save(&self, session_dir: &Path) -> Result<(), String> {
        let path = session_dir.join(METADATA_FILENAME);
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
        fs::write(&path, json)
            .map_err(|e| format!("Failed to write metadata to {}: {}", path.display(), e))?;
        Ok(())
    }

    pub fn load(session_dir: &Path) -> Result<Self, String> {
        let path = session_dir.join(METADATA_FILENAME);
        let json = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read metadata from {}: {}", path.display(), e))?;
        serde_json::from_str(&json).map_err(|e| format!("Failed to parse metadata: {}", e))
    }

    pub fn is_alive(&self) -> bool {
        Path::new(&format!("/proc/{}", self.pid)).exists()
    }

    pub fn age_secs(&self) -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs().saturating_sub(self.created))
            .unwrap_or(0)
    }

    pub fn format_age(&self) -> String {
        let secs = self.age_secs();
        if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else if secs < 86400 {
            format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
        } else {
            format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
        }
    }
}

/// Scan /tmp for active sketch sessions and return their metadata.
pub fn list_sessions() -> Vec<SessionMetadata> {
    let tmp = Path::new("/tmp");
    let mut sessions = Vec::new();

    if let Ok(entries) = fs::read_dir(tmp) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("sketch-") {
                if let Ok(meta) = SessionMetadata::load(&entry.path()) {
                    sessions.push(meta);
                }
            }
        }
    }

    sessions.sort_by(|a, b| a.created.cmp(&b.created));
    sessions
}

/// Calculate the size of a session's upper directory in bytes.
pub fn session_size(session_dir: &Path) -> u64 {
    let upper = session_dir.join("upper");
    dir_size(&upper)
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                total += dir_size(&path);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

/// Format bytes as human-readable size.
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.0}K", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}G", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Get overlay directory size for a session by its path.
pub fn get_session_dir(id: &str) -> Option<PathBuf> {
    let path = PathBuf::from(format!("/tmp/sketch-{}", id));
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================
    // SessionMetadata::new
    // ============================================================

    #[test]
    fn new_sets_id_and_command() {
        let meta = SessionMetadata::new("test-123", None, "echo hi", Path::new("/tmp/test"));
        assert_eq!(meta.id, "test-123");
        assert_eq!(meta.command, "echo hi");
        assert!(meta.name.is_none());
    }

    #[test]
    fn new_with_name() {
        let meta = SessionMetadata::new("id", Some("myname".into()), "cmd", Path::new("/tmp"));
        assert_eq!(meta.name.as_deref(), Some("myname"));
    }

    #[test]
    fn new_sets_current_pid() {
        let meta = SessionMetadata::new("id", None, "cmd", Path::new("/tmp"));
        assert_eq!(meta.pid, std::process::id());
    }

    #[test]
    fn new_sets_created_timestamp() {
        let before = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let meta = SessionMetadata::new("id", None, "cmd", Path::new("/tmp"));
        let after = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(meta.created >= before && meta.created <= after);
    }

    #[test]
    fn new_sets_overlay_path() {
        let meta = SessionMetadata::new("id", None, "cmd", Path::new("/tmp/sketch-abc"));
        assert_eq!(meta.overlay_path, "/tmp/sketch-abc");
    }

    // ============================================================
    // save / load roundtrip
    // ============================================================

    #[test]
    fn load_nonexistent_fails() {
        let result = SessionMetadata::load(Path::new("/tmp/nonexistent_sketch_test_xyz"));
        assert!(result.is_err());
    }

    // ============================================================
    // is_alive
    // ============================================================

    #[test]
    fn is_alive_for_current_process() {
        let meta = SessionMetadata::new("id", None, "cmd", Path::new("/tmp"));
        assert!(meta.is_alive(), "current process should be alive");
    }

    #[test]
    fn is_alive_false_for_nonexistent_pid() {
        let mut meta = SessionMetadata::new("id", None, "cmd", Path::new("/tmp"));
        meta.pid = 999_999_999; // Very unlikely to exist
        assert!(!meta.is_alive());
    }

    // ============================================================
    // age_secs / format_age
    // ============================================================

    #[test]
    fn age_secs_for_just_created() {
        let meta = SessionMetadata::new("id", None, "cmd", Path::new("/tmp"));
        assert!(
            meta.age_secs() < 2,
            "just-created session should have age < 2s"
        );
    }

    #[test]
    fn format_age_seconds() {
        let mut meta = SessionMetadata::new("id", None, "cmd", Path::new("/tmp"));
        meta.created = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 30;
        let age = meta.format_age();
        assert!(age.ends_with('s'), "age should be in seconds: {}", age);
        assert!(!age.contains('m'), "should not have minutes: {}", age);
    }

    #[test]
    fn format_age_minutes() {
        let mut meta = SessionMetadata::new("id", None, "cmd", Path::new("/tmp"));
        meta.created = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 150; // 2m 30s
        let age = meta.format_age();
        assert!(age.contains('m'), "age should contain minutes: {}", age);
    }

    #[test]
    fn format_age_hours() {
        let mut meta = SessionMetadata::new("id", None, "cmd", Path::new("/tmp"));
        meta.created = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 7200; // 2h
        let age = meta.format_age();
        assert!(age.contains('h'), "age should contain hours: {}", age);
    }

    #[test]
    fn format_age_days() {
        let mut meta = SessionMetadata::new("id", None, "cmd", Path::new("/tmp"));
        meta.created = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 100_000; // ~1.15 days
        let age = meta.format_age();
        assert!(age.contains('d'), "age should contain days: {}", age);
    }

    // ============================================================
    // format_size
    // ============================================================

    #[test]
    fn format_size_bytes() {
        assert_eq!(format_size(0), "0B");
        assert_eq!(format_size(512), "512B");
        assert_eq!(format_size(1023), "1023B");
    }

    #[test]
    fn format_size_kilobytes() {
        assert_eq!(format_size(1024), "1K");
        assert_eq!(format_size(2048), "2K");
    }

    #[test]
    fn format_size_megabytes() {
        assert_eq!(format_size(1024 * 1024), "1.0M");
        assert_eq!(format_size(10 * 1024 * 1024), "10.0M");
    }

    #[test]
    fn format_size_gigabytes() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0G");
    }

    // ============================================================
    // get_session_dir
    // ============================================================

    #[test]
    fn get_session_dir_nonexistent() {
        assert!(get_session_dir("nonexistent-uuid-xyz-12345").is_none());
    }

    // ============================================================
    // list_sessions (basic)
    // ============================================================

    #[test]
    fn list_sessions_does_not_panic() {
        // Just verify it doesn't panic on the current system
        let _sessions = list_sessions();
    }

    // ============================================================
    // Serialization roundtrip via serde_json
    // ============================================================

    #[test]
    fn json_serialization_roundtrip() {
        let meta = SessionMetadata {
            id: "test-id".into(),
            name: Some("named".into()),
            created: 1700000000,
            pid: 12345,
            command: "echo test".into(),
            username: "testuser".into(),
            overlay_path: "/tmp/sketch-test".into(),
        };

        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: SessionMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "test-id");
        assert_eq!(deserialized.name.as_deref(), Some("named"));
        assert_eq!(deserialized.created, 1700000000);
        assert_eq!(deserialized.pid, 12345);
        assert_eq!(deserialized.command, "echo test");
        assert_eq!(deserialized.username, "testuser");
    }

    #[test]
    fn json_with_null_name() {
        let meta = SessionMetadata {
            id: "id".into(),
            name: None,
            created: 0,
            pid: 1,
            command: "cmd".into(),
            username: "user".into(),
            overlay_path: "/tmp".into(),
        };

        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("null"));
        let deserialized: SessionMetadata = serde_json::from_str(&json).unwrap();
        assert!(deserialized.name.is_none());
    }
}
