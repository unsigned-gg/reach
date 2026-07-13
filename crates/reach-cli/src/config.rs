use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════
// CLI configuration — loaded from ~/.config/reach/config.toml
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct ReachConfig {
    pub sandbox: SandboxDefaults,
    pub server: ServerConfig,
    pub docker: DockerConfig,
    pub scraper: ScraperConfig,
}

/// Configuration for the host-side scraper integration (`reach-scraper`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ScraperConfig {
    /// Override path for the AdaptiveMemory SQLite database.
    ///
    /// `None` means use the platform default, which today is
    /// `$XDG_DATA_HOME/reach/adaptive.sqlite` (or
    /// `~/.local/share/reach/adaptive.sqlite`).
    #[serde(default)]
    pub memory_path: Option<PathBuf>,
}

impl ScraperConfig {
    /// Resolve the path used for the AdaptiveMemory SQLite database.
    pub fn resolved_memory_path(&self) -> PathBuf {
        self.memory_path
            .clone()
            .unwrap_or_else(default_adaptive_memory_path)
    }
}

/// Platform default for the AdaptiveMemory SQLite database path.
pub fn default_adaptive_memory_path() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".local").join("share")
        });
    base.join("reach").join("adaptive.sqlite")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SandboxDefaults {
    /// Default Docker image
    pub image: String,
    /// Default display resolution
    pub resolution: String,
    /// Shared memory size in bytes
    pub shm_size: u64,
    /// Default VNC port
    pub vnc_port: u16,
    /// Default noVNC port
    pub novnc_port: u16,
    /// Default health API port
    pub health_port: u16,
    /// Default browserd port
    pub browserd_port: u16,
    /// Root directory for persistent Chrome profiles on the host.
    ///
    /// Each `--persist-profile <name>` is materialised as a subdirectory
    /// under this path. `None` means use the platform default
    /// (`~/.local/share/reach/profiles`).
    #[serde(default)]
    pub profile_dir: Option<PathBuf>,
}

impl SandboxDefaults {
    /// Resolve the directory used to store persistent Chrome profiles.
    ///
    /// Falls back to `$XDG_DATA_HOME/reach/profiles` (or
    /// `~/.local/share/reach/profiles`) when `profile_dir` is unset.
    pub fn resolved_profile_dir(&self) -> PathBuf {
        self.profile_dir.clone().unwrap_or_else(default_profile_dir)
    }
}

/// Platform default for the persistent Chrome profile root.
pub fn default_profile_dir() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".local").join("share")
        });
    base.join("reach").join("profiles")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// MCP SSE server port
    pub port: u16,
    /// Bind address
    pub host: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct DockerConfig {
    /// Docker socket path (empty = auto-detect)
    pub socket: String,
}

// ═══════════════════════════════════════════════════════════
// Defaults
// ═══════════════════════════════════════════════════════════

impl Default for SandboxDefaults {
    fn default() -> Self {
        Self {
            image: "reach:latest".into(),
            resolution: "1280x720".into(),
            shm_size: 2 * 1024 * 1024 * 1024,
            vnc_port: 5900,
            novnc_port: 6080,
            health_port: 8400,
            browserd_port: 8401,
            profile_dir: None,
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 4200,
            host: "127.0.0.1".into(),
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Loading
// ═══════════════════════════════════════════════════════════

impl ReachConfig {
    pub fn config_path() -> PathBuf {
        dirs().join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            toml::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }
}

fn dirs() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config")
        });
    base.join("reach")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_profile_dir_uses_explicit_override() {
        let defaults = SandboxDefaults {
            profile_dir: Some(PathBuf::from("/tmp/custom/profiles")),
            ..SandboxDefaults::default()
        };
        assert_eq!(
            defaults.resolved_profile_dir(),
            PathBuf::from("/tmp/custom/profiles")
        );
    }

    #[test]
    fn resolved_profile_dir_falls_back_to_default() {
        let defaults = SandboxDefaults::default();
        let resolved = defaults.resolved_profile_dir();
        assert!(resolved.ends_with("reach/profiles"));
    }

    #[test]
    fn default_profile_dir_contains_reach_segment() {
        let dir = default_profile_dir();
        assert!(dir.to_string_lossy().contains("reach"));
    }

    #[test]
    fn resolved_memory_path_uses_explicit_override() {
        let cfg = ScraperConfig {
            memory_path: Some(PathBuf::from("/tmp/custom/adaptive.sqlite")),
        };
        assert_eq!(
            cfg.resolved_memory_path(),
            PathBuf::from("/tmp/custom/adaptive.sqlite")
        );
    }

    #[test]
    fn default_memory_path_lives_under_reach_share() {
        let path = default_adaptive_memory_path();
        let s = path.to_string_lossy();
        assert!(s.ends_with("adaptive.sqlite"));
        assert!(s.contains("reach"));
    }
}
