use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PackageManager {
    Apt,
    Dnf,
    Yum,
    Pacman,
    Zypper,
    Apk,
}

impl PackageManager {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Apt => "apt",
            Self::Dnf => "dnf",
            Self::Yum => "yum",
            Self::Pacman => "pacman",
            Self::Zypper => "zypper",
            Self::Apk => "apk",
        }
    }

    pub fn install_args(&self, packages: &[&str]) -> Vec<String> {
        match self {
            Self::Apt => {
                let mut args = vec![
                    "apt-get".into(),
                    "install".into(),
                    "-y".into(),
                ];
                args.extend(packages.iter().map(|p| p.to_string()));
                args
            }
            Self::Dnf => {
                let mut args = vec!["dnf".into(), "install".into(), "-y".into()];
                args.extend(packages.iter().map(|p| p.to_string()));
                args
            }
            Self::Yum => {
                let mut args = vec!["yum".into(), "install".into(), "-y".into()];
                args.extend(packages.iter().map(|p| p.to_string()));
                args
            }
            Self::Pacman => {
                let mut args = vec!["pacman".into(), "-S".into(), "--noconfirm".into()];
                args.extend(packages.iter().map(|p| p.to_string()));
                args
            }
            Self::Zypper => {
                let mut args = vec!["zypper".into(), "install".into(), "-y".into()];
                args.extend(packages.iter().map(|p| p.to_string()));
                args
            }
            Self::Apk => {
                let mut args = vec!["apk".into(), "add".into()];
                args.extend(packages.iter().map(|p| p.to_string()));
                args
            }
        }
    }

    pub fn remove_args(&self, packages: &[&str]) -> Vec<String> {
        match self {
            Self::Apt => {
                let mut args = vec!["apt-get".into(), "remove".into(), "-y".into()];
                args.extend(packages.iter().map(|p| p.to_string()));
                args
            }
            Self::Dnf => {
                let mut args = vec!["dnf".into(), "remove".into(), "-y".into()];
                args.extend(packages.iter().map(|p| p.to_string()));
                args
            }
            Self::Yum => {
                let mut args = vec!["yum".into(), "remove".into(), "-y".into()];
                args.extend(packages.iter().map(|p| p.to_string()));
                args
            }
            Self::Pacman => {
                let mut args = vec!["pacman".into(), "-R".into(), "--noconfirm".into()];
                args.extend(packages.iter().map(|p| p.to_string()));
                args
            }
            Self::Zypper => {
                let mut args = vec!["zypper".into(), "remove".into(), "-y".into()];
                args.extend(packages.iter().map(|p| p.to_string()));
                args
            }
            Self::Apk => {
                let mut args = vec!["apk".into(), "del".into()];
                args.extend(packages.iter().map(|p| p.to_string()));
                args
            }
        }
    }

    pub fn update_args(&self) -> Vec<String> {
        match self {
            Self::Apt => vec!["apt-get".into(), "update".into()],
            Self::Dnf => vec!["dnf".into(), "check-update".into()],
            Self::Yum => vec!["yum".into(), "check-update".into()],
            Self::Pacman => vec!["pacman".into(), "-Sy".into()],
            Self::Zypper => vec!["zypper".into(), "refresh".into()],
            Self::Apk => vec!["apk".into(), "update".into()],
        }
    }

    pub fn cache_dirs(&self) -> Vec<&'static str> {
        match self {
            Self::Apt => vec!["/var/cache/apt/archives"],
            Self::Dnf => vec!["/var/cache/dnf"],
            Self::Yum => vec!["/var/cache/yum"],
            Self::Pacman => vec!["/var/cache/pacman/pkg"],
            Self::Zypper => vec!["/var/cache/zypp"],
            Self::Apk => vec!["/var/cache/apk"],
        }
    }

    /// Directories that the package manager writes state to (DB, locks, lists).
    /// These must be writable for install/remove/update operations.
    pub fn state_dirs(&self) -> Vec<&'static str> {
        match self {
            Self::Apt => vec![
                "/var/lib/dpkg",
                "/var/lib/apt/lists",
                "/var/cache/apt/archives/partial",
                "/var/log/apt",
            ],
            Self::Dnf => vec![
                "/var/lib/dnf",
                "/var/lib/rpm",
                "/var/cache/dnf",
                "/var/log/dnf.log",
            ],
            Self::Yum => vec![
                "/var/lib/yum",
                "/var/lib/rpm",
                "/var/cache/yum",
            ],
            Self::Pacman => vec![
                "/var/lib/pacman",
                "/var/cache/pacman/pkg",
            ],
            Self::Zypper => vec![
                "/var/lib/zypp",
                "/var/cache/zypp",
            ],
            Self::Apk => vec![
                "/var/lib/apk",
                "/var/cache/apk",
                "/etc/apk",
            ],
        }
    }

    /// Environment variables to set for non-interactive package operations.
    pub fn env_vars(&self) -> Vec<(&'static str, &'static str)> {
        match self {
            Self::Apt => vec![
                ("DEBIAN_FRONTEND", "noninteractive"),
                ("DEBCONF_NONINTERACTIVE_SEEN", "true"),
            ],
            _ => vec![],
        }
    }
}

/// Detect the system's package manager by checking for known binaries.
pub fn detect_package_manager() -> Option<PackageManager> {
    let checks = [
        ("/usr/bin/apt-get", PackageManager::Apt),
        ("/usr/bin/dnf", PackageManager::Dnf),
        ("/usr/bin/yum", PackageManager::Yum),
        ("/usr/bin/pacman", PackageManager::Pacman),
        ("/usr/bin/zypper", PackageManager::Zypper),
        ("/sbin/apk", PackageManager::Apk),
    ];

    for (path, pm) in &checks {
        if Path::new(path).exists() {
            return Some(*pm);
        }
    }

    None
}

/// Detect available user-level package managers (pip, npm, cargo, gem, etc.)
/// These work within the overlay without special setup since they install to
/// user-writable paths that the overlay captures automatically.
pub fn detect_user_package_managers() -> Vec<&'static str> {
    let candidates = [
        ("/usr/bin/pip3", "pip3"),
        ("/usr/bin/pip", "pip"),
        ("/usr/bin/npm", "npm"),
        ("/usr/bin/yarn", "yarn"),
        ("/usr/bin/cargo", "cargo"),
        ("/usr/bin/gem", "gem"),
        ("/usr/bin/go", "go"),
        ("/usr/local/bin/pip3", "pip3"),
        ("/usr/local/bin/npm", "npm"),
        ("/usr/local/bin/cargo", "cargo"),
    ];

    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (path, name) in &candidates {
        if Path::new(path).exists() && seen.insert(*name) {
            found.push(*name);
        }
    }
    found
}

/// Run a package manager command, returning its exit code.
pub fn run_package_command(args: &[String]) -> Result<i32, String> {
    if args.is_empty() {
        return Err("No command to run".into());
    }

    let status = Command::new(&args[0])
        .args(&args[1..])
        .status()
        .map_err(|e| format!("Failed to run {}: {}", args[0], e))?;

    Ok(status.code().unwrap_or(1))
}

/// Clean package manager caches to reclaim space in the overlay.
pub fn clean_package_cache(pm: PackageManager) -> Result<(), String> {
    let args = match pm {
        PackageManager::Apt => vec!["apt-get".into(), "clean".into()],
        PackageManager::Dnf => vec!["dnf".into(), "clean".into(), "all".into()],
        PackageManager::Yum => vec!["yum".into(), "clean".into(), "all".into()],
        PackageManager::Pacman => vec!["pacman".into(), "-Sc".into(), "--noconfirm".into()],
        PackageManager::Zypper => vec!["zypper".into(), "clean".into()],
        PackageManager::Apk => vec!["rm".into(), "-rf".into(), "/var/cache/apk/*".into()],
    };

    let code = run_package_command(&args)?;
    if code != 0 {
        return Err(format!("Cache cleanup exited with code {}", code));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================
    // PackageManager::name tests
    // ============================================================

    #[test]
    fn package_manager_names() {
        assert_eq!(PackageManager::Apt.name(), "apt");
        assert_eq!(PackageManager::Dnf.name(), "dnf");
        assert_eq!(PackageManager::Yum.name(), "yum");
        assert_eq!(PackageManager::Pacman.name(), "pacman");
        assert_eq!(PackageManager::Zypper.name(), "zypper");
        assert_eq!(PackageManager::Apk.name(), "apk");
    }

    // ============================================================
    // install_args tests
    // ============================================================

    #[test]
    fn apt_install_args() {
        let args = PackageManager::Apt.install_args(&["curl", "wget"]);
        assert_eq!(args[0], "apt-get");
        assert_eq!(args[1], "install");
        assert_eq!(args[2], "-y");
        assert!(args.contains(&"curl".into()));
        assert!(args.contains(&"wget".into()));
    }

    #[test]
    fn dnf_install_args() {
        let args = PackageManager::Dnf.install_args(&["vim"]);
        assert_eq!(args[0], "dnf");
        assert_eq!(args[1], "install");
        assert!(args.contains(&"vim".into()));
    }

    #[test]
    fn pacman_install_args() {
        let args = PackageManager::Pacman.install_args(&["git"]);
        assert_eq!(args[0], "pacman");
        assert_eq!(args[1], "-S");
        assert_eq!(args[2], "--noconfirm");
        assert!(args.contains(&"git".into()));
    }

    #[test]
    fn apk_install_args() {
        let args = PackageManager::Apk.install_args(&["bash"]);
        assert_eq!(args[0], "apk");
        assert_eq!(args[1], "add");
        assert!(args.contains(&"bash".into()));
    }

    #[test]
    fn install_args_empty_packages() {
        let args = PackageManager::Apt.install_args(&[]);
        assert_eq!(args.len(), 3); // just "apt-get install -y"
    }

    #[test]
    fn install_args_multiple_packages() {
        let args = PackageManager::Apt.install_args(&["a", "b", "c"]);
        assert_eq!(args.len(), 6); // apt-get install -y a b c
    }

    // ============================================================
    // remove_args tests
    // ============================================================

    #[test]
    fn apt_remove_args() {
        let args = PackageManager::Apt.remove_args(&["curl"]);
        assert_eq!(args[0], "apt-get");
        assert_eq!(args[1], "remove");
        assert!(args.contains(&"curl".into()));
    }

    #[test]
    fn pacman_remove_args() {
        let args = PackageManager::Pacman.remove_args(&["git"]);
        assert_eq!(args[0], "pacman");
        assert_eq!(args[1], "-R");
        assert_eq!(args[2], "--noconfirm");
    }

    #[test]
    fn apk_remove_args() {
        let args = PackageManager::Apk.remove_args(&["bash"]);
        assert_eq!(args[0], "apk");
        assert_eq!(args[1], "del");
    }

    // ============================================================
    // update_args tests
    // ============================================================

    #[test]
    fn apt_update_args() {
        let args = PackageManager::Apt.update_args();
        assert_eq!(args, vec!["apt-get".to_string(), "update".to_string()]);
    }

    #[test]
    fn pacman_update_args() {
        let args = PackageManager::Pacman.update_args();
        assert_eq!(args, vec!["pacman".to_string(), "-Sy".to_string()]);
    }

    #[test]
    fn all_managers_have_update_args() {
        let managers = [
            PackageManager::Apt, PackageManager::Dnf, PackageManager::Yum,
            PackageManager::Pacman, PackageManager::Zypper, PackageManager::Apk,
        ];
        for pm in &managers {
            let args = pm.update_args();
            assert!(!args.is_empty(), "{:?} should have update args", pm);
        }
    }

    // ============================================================
    // cache_dirs tests
    // ============================================================

    #[test]
    fn cache_dirs_non_empty_and_absolute() {
        let managers = [
            PackageManager::Apt, PackageManager::Dnf, PackageManager::Yum,
            PackageManager::Pacman, PackageManager::Zypper, PackageManager::Apk,
        ];
        for pm in &managers {
            let dirs = pm.cache_dirs();
            assert!(!dirs.is_empty(), "{:?} should have cache dirs", pm);
            for d in &dirs {
                assert!(d.starts_with('/'), "cache dir should be absolute: {}", d);
            }
        }
    }

    // ============================================================
    // state_dirs tests
    // ============================================================

    #[test]
    fn state_dirs_non_empty_and_absolute() {
        let managers = [
            PackageManager::Apt, PackageManager::Dnf, PackageManager::Yum,
            PackageManager::Pacman, PackageManager::Zypper, PackageManager::Apk,
        ];
        for pm in &managers {
            let dirs = pm.state_dirs();
            assert!(!dirs.is_empty(), "{:?} should have state dirs", pm);
            for d in &dirs {
                assert!(d.starts_with('/'), "state dir should be absolute: {}", d);
            }
        }
    }

    // ============================================================
    // env_vars tests
    // ============================================================

    #[test]
    fn apt_has_env_vars() {
        let vars = PackageManager::Apt.env_vars();
        assert!(!vars.is_empty());
        assert!(vars.iter().any(|(k, _)| *k == "DEBIAN_FRONTEND"));
    }

    #[test]
    fn non_apt_env_vars_empty() {
        for pm in &[PackageManager::Dnf, PackageManager::Yum, PackageManager::Pacman] {
            assert!(pm.env_vars().is_empty(), "{:?} should have empty env_vars", pm);
        }
    }

    // ============================================================
    // detect functions
    // ============================================================

    #[test]
    fn detect_package_manager_does_not_panic() {
        let _result = detect_package_manager();
    }

    #[test]
    fn detect_user_package_managers_does_not_panic() {
        let _result = detect_user_package_managers();
    }

    #[test]
    fn detect_user_package_managers_no_duplicates() {
        let managers = detect_user_package_managers();
        let mut seen = std::collections::HashSet::new();
        for m in &managers {
            assert!(seen.insert(*m), "duplicate user package manager: {}", m);
        }
    }

    #[test]
    fn package_manager_equality() {
        assert_eq!(PackageManager::Apt, PackageManager::Apt);
        assert_ne!(PackageManager::Apt, PackageManager::Dnf);
    }

    #[test]
    fn package_manager_clone() {
        let pm = PackageManager::Apt;
        let pm2 = pm;
        assert_eq!(pm, pm2);
    }

    // ============================================================
    // run_package_command tests
    // ============================================================

    #[test]
    fn run_package_command_empty_args_fails() {
        let result = run_package_command(&[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No command"));
    }

    #[test]
    fn run_package_command_nonexistent_binary() {
        let result = run_package_command(&["nonexistent_binary_xyz_12345".into()]);
        assert!(result.is_err());
    }

    #[test]
    fn run_package_command_true_succeeds() {
        let result = run_package_command(&["true".into()]);
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn run_package_command_false_returns_nonzero() {
        let result = run_package_command(&["false".into()]);
        assert_ne!(result.unwrap(), 0);
    }

    #[test]
    fn run_package_command_with_args() {
        let result = run_package_command(&["echo".into(), "hello".into()]);
        assert_eq!(result.unwrap(), 0);
    }
}
