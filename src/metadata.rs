use std::fs;
use std::path::{Path, PathBuf};

use crate::overlay::ExtraOverlayMount;

const METADATA_FILENAME: &str = ".sketch-metadata";

pub struct SessionMetadata {
    pub id: String,
    pub pid: Option<i32>,
    pub session_dir: String,
    pub name: Option<String>,
    pub created: u64,
    pub command: String,
    pub upper_dir: String,
    pub work_dir: String,
    pub merged_dir: String,
    pub extra_mounts: Vec<ExtraOverlayMount>,
}

impl SessionMetadata {
    pub fn new(
        id: &str,
        session_dir: &str,
        name: Option<String>,
        created: u64,
        command: &str,
        upper_dir: &str,
        work_dir: &str,
        merged_dir: &str,
        extra_mounts: Vec<ExtraOverlayMount>,
    ) -> Self {
        Self {
            id: id.into(),
            pid: None,
            session_dir: session_dir.into(),
            name,
            created,
            command: command.into(),
            upper_dir: upper_dir.into(),
            work_dir: work_dir.into(),
            merged_dir: merged_dir.into(),
            extra_mounts,
        }
    }

    pub fn save(&self, session_dir: &Path) -> Result<(), String> {
        let path = session_dir.join(METADATA_FILENAME);
        let mut lines = Vec::new();

        lines.push(format!("id={}", self.id));
        if let Some(pid) = self.pid {
            lines.push(format!("pid={}", pid));
        }
        lines.push(format!("session_dir={}", self.session_dir));
        if let Some(ref name) = self.name {
            lines.push(format!("name={}", name));
        }
        lines.push(format!("created={}", self.created));
        lines.push(format!("command={}", self.command));
        lines.push(format!("upper_dir={}", self.upper_dir));
        lines.push(format!("work_dir={}", self.work_dir));
        lines.push(format!("merged_dir={}", self.merged_dir));

        for mount in &self.extra_mounts {
            lines.push(format!(
                "extra_mount={}|{}|{}|{}",
                mount.lowerdir,
                mount.upperdir.display(),
                mount.workdir.display(),
                mount.target.display()
            ));
        }

        let content = lines.join("\n") + "\n";
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write metadata to {}: {}", path.display(), e))?;
        Ok(())
    }

    pub fn load(session_dir: &Path) -> Result<Self, String> {
        let path = session_dir.join(METADATA_FILENAME);
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read metadata from {}: {}", path.display(), e))?;

        let mut id = String::new();
        let mut pid = None;
        let mut session_dir_val = String::new();
        let mut name = None;
        let mut created = 0u64;
        let mut command = String::new();
        let mut upper_dir = String::new();
        let mut work_dir = String::new();
        let mut merged_dir = String::new();
        let mut extra_mounts = Vec::new();

        for line in content.lines() {
            if let Some(eq_pos) = line.find('=') {
                let key = &line[..eq_pos];
                let value = &line[eq_pos + 1..];

                match key {
                    "id" => id = value.to_string(),
                    "pid" => {
                        pid = Some(
                            value
                                .parse()
                                .map_err(|_| format!("Failed to parse pid: {}", value))?,
                        )
                    }
                    "session_dir" => session_dir_val = value.to_string(),
                    "name" => name = Some(value.to_string()),
                    "created" => {
                        created = value
                            .parse()
                            .map_err(|_| format!("Failed to parse created: {}", value))?
                    }
                    "command" => command = value.to_string(),
                    "upper_dir" => upper_dir = value.to_string(),
                    "work_dir" => work_dir = value.to_string(),
                    "merged_dir" => merged_dir = value.to_string(),
                    "extra_mount" => {
                        let parts: Vec<&str> = value.split('|').collect();
                        if parts.len() == 4 {
                            extra_mounts.push(ExtraOverlayMount {
                                lowerdir: parts[0].to_string(),
                                upperdir: PathBuf::from(parts[1]),
                                workdir: PathBuf::from(parts[2]),
                                target: PathBuf::from(parts[3]),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(Self {
            id,
            pid,
            session_dir: session_dir_val,
            name,
            created,
            command,
            upper_dir,
            work_dir,
            merged_dir,
            extra_mounts,
        })
    }

    pub fn takeover(&mut self, pid: i32, session_dir: &Path) -> Result<(), String> {
        self.pid = Some(pid);
        self.save(session_dir)?;
        Ok(())
    }

    pub fn is_alive(&self) -> bool {
        let name_path = Path::new("/proc")
            .join(self.pid.map_or_else(|| "0".into(), |p| p.to_string()))
            .join("comm");
        if !name_path.exists() {
            return false;
        } else {
            if let Ok(name) = fs::read_to_string(name_path) {
                return name.trim() == "sketch";
            } else {
                return false;
            }
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
