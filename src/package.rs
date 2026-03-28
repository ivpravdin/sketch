use std::path::Path;

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

}
