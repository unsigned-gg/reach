//! Unit tests for Docker abstraction types — sandbox config, resolution
//! parsing, labels, status mapping.

use reach_cli::docker::*;

// ═══════════════════════════════════════════════════════════
// Resolution parsing
// ═══════════════════════════════════════════════════════════

#[test]
fn resolution_parses_standard_formats() {
    let r = Resolution::parse("1280x720").unwrap();
    assert_eq!(r.width, 1280);
    assert_eq!(r.height, 720);

    let r = Resolution::parse("1920x1080").unwrap();
    assert_eq!(r.width, 1920);
    assert_eq!(r.height, 1080);
}

#[test]
fn resolution_display_roundtrips() {
    let r = Resolution::parse("1280x720").unwrap();
    assert_eq!(r.to_string(), "1280x720");
}

#[test]
fn resolution_rejects_garbage() {
    assert!(Resolution::parse("not-a-resolution").is_err());
    assert!(Resolution::parse("1280").is_err());
    assert!(Resolution::parse("x720").is_err());
    assert!(Resolution::parse("").is_err());
}

#[test]
fn resolution_rejects_non_numeric() {
    assert!(Resolution::parse("widexhigh").is_err());
}

// ═══════════════════════════════════════════════════════════
// Sandbox config defaults
// ═══════════════════════════════════════════════════════════

#[test]
fn sandbox_config_default_image_is_reach_latest() {
    let config = SandboxConfig::default();
    assert_eq!(config.image, "reach:latest");
}

#[test]
fn sandbox_config_default_resolution_is_720p() {
    let config = SandboxConfig::default();
    assert_eq!(config.resolution.width, 1280);
    assert_eq!(config.resolution.height, 720);
}

#[test]
fn sandbox_config_default_shm_is_2gb() {
    let config = SandboxConfig::default();
    assert_eq!(config.shm_size, 2 * 1024 * 1024 * 1024);
}

#[test]
fn sandbox_config_default_ports() {
    let config = SandboxConfig::default();
    assert_eq!(config.ports.vnc, 5900);
    assert_eq!(config.ports.novnc, 6080);
    assert_eq!(config.ports.health, 8400);
}

// ═══════════════════════════════════════════════════════════
// Container labels
// ═══════════════════════════════════════════════════════════

#[test]
fn labels_for_sandbox_includes_all_required_keys() {
    let config = SandboxConfig::default();
    let labels = Labels::for_sandbox(&config);

    assert_eq!(labels.get(Labels::MANAGED), Some(&"true".to_string()));
    assert_eq!(labels.get(Labels::NAME), Some(&config.name));
    assert!(labels.contains_key(Labels::CREATED));
    assert!(labels.contains_key(Labels::RESOLUTION));
}

#[test]
fn labels_filter_targets_managed_containers() {
    let filter = Labels::filter();
    let label_filters = filter.get("label").unwrap();
    assert!(
        label_filters
            .iter()
            .any(|f| f.contains("reach.sandbox=true"))
    );
}

// ═══════════════════════════════════════════════════════════
// Sandbox status mapping
// ═══════════════════════════════════════════════════════════

#[test]
fn sandbox_status_from_docker_state() {
    assert_eq!(SandboxStatus::from("running"), SandboxStatus::Running);
    assert_eq!(SandboxStatus::from("created"), SandboxStatus::Starting);
    assert_eq!(SandboxStatus::from("restarting"), SandboxStatus::Starting);
    assert_eq!(SandboxStatus::from("exited"), SandboxStatus::Stopped);
    assert_eq!(SandboxStatus::from("dead"), SandboxStatus::Stopped);
    assert_eq!(SandboxStatus::from("paused"), SandboxStatus::Unknown);
    assert_eq!(SandboxStatus::from("banana"), SandboxStatus::Unknown);
}

// ═══════════════════════════════════════════════════════════
// Sandbox serialization
// ═══════════════════════════════════════════════════════════

#[test]
fn sandbox_serializes_to_json() {
    let sandbox = Sandbox {
        name: "test".into(),
        container_id: "abc123".into(),
        status: SandboxStatus::Running,
        image: "reach:latest".into(),
        ports: SandboxPortMapping {
            vnc: Some(5900),
            novnc: Some(6080),
            health: Some(8400),
            browserd: Some(8401),
            extra: Vec::new(),
        },
        created_at: "2026-04-02T00:00:00Z".into(),
    };

    let json = serde_json::to_value(&sandbox).unwrap();
    assert_eq!(json["name"], "test");
    assert_eq!(json["status"], "running");
    assert_eq!(json["ports"]["vnc"], 5900);
}

// ═══════════════════════════════════════════════════════════
// ExecOutput
// ═══════════════════════════════════════════════════════════

#[test]
fn exec_output_serializes() {
    let output = ExecOutput {
        exit_code: 0,
        stdout: "hello\n".into(),
        stderr: "".into(),
    };
    let json = serde_json::to_value(&output).unwrap();
    assert_eq!(json["exit_code"], 0);
    assert_eq!(json["stdout"], "hello\n");
}

// ═══════════════════════════════════════════════════════════
// Profile mounts
// ═══════════════════════════════════════════════════════════

#[test]
fn profile_mount_container_path_uses_stable_root() {
    assert_eq!(
        ProfileMount::container_path_for("threads"),
        "/home/sandbox/.config/google-chrome-profiles/threads"
    );
}

#[test]
fn profile_mount_host_path_joins_under_base() {
    let base = std::path::Path::new("/var/lib/reach/profiles");
    let host = ProfileMount::host_path_for(base, "threads");
    assert_eq!(
        host,
        std::path::PathBuf::from("/var/lib/reach/profiles/threads")
    );
}

#[test]
fn labels_for_sandbox_includes_profile_label_when_set() {
    let config = SandboxConfig {
        profile: Some(ProfileMount {
            name: "threads".into(),
            host_path: std::path::PathBuf::from("/tmp/x"),
            container_path: ProfileMount::container_path_for("threads"),
        }),
        ..SandboxConfig::default()
    };
    let labels = Labels::for_sandbox(&config);
    assert_eq!(labels.get(Labels::PROFILE), Some(&"threads".to_string()));
}

#[test]
fn labels_for_sandbox_omits_profile_label_when_unset() {
    let config = SandboxConfig::default();
    let labels = Labels::for_sandbox(&config);
    assert!(!labels.contains_key(Labels::PROFILE));
}

#[test]
fn novnc_url_uses_localhost_pattern() {
    assert_eq!(
        novnc_url("localhost", 6080),
        "http://localhost:6080/vnc.html?autoconnect=1&resize=remote"
    );
}
