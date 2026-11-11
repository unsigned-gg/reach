//! Docker endpoint resolution.
//!
//! Order (first that applies wins):
//! 1. `DOCKER_HOST`
//! 2. the current docker context (`config.json` `currentContext`, then
//!    `contexts/meta/<sha256(name)>/meta.json` `Endpoints.docker.Host`)
//! 3. `$XDG_RUNTIME_DIR/docker.sock`
//! 4. `/var/run/docker.sock`

use anyhow::{Result, bail};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Inputs to resolution, injectable for tests.
#[derive(Debug, Default, Clone)]
pub struct EndpointEnv {
    pub docker_host: Option<String>,
    pub docker_config: Option<PathBuf>,
    pub xdg_runtime_dir: Option<PathBuf>,
    pub default_socket: PathBuf,
}

impl EndpointEnv {
    pub fn from_process() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let docker_config = std::env::var_os("DOCKER_CONFIG")
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join(".docker")));
        Self {
            docker_host: std::env::var("DOCKER_HOST").ok().filter(|s| !s.is_empty()),
            docker_config,
            xdg_runtime_dir: std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
            default_socket: PathBuf::from("/var/run/docker.sock"),
        }
    }
}

/// Resolve to an endpoint string (`unix://...`, `tcp://...`, ...).
pub fn resolve(env: &EndpointEnv) -> Result<String> {
    let mut tried: Vec<String> = Vec::new();

    if let Some(h) = &env.docker_host {
        return Ok(h.clone());
    }
    tried.push("DOCKER_HOST (unset)".into());

    match context_endpoint(env) {
        Ok(Some(h)) => return Ok(h),
        Ok(None) => tried.push("docker context (none selected or default)".into()),
        Err(why) => tried.push(format!("docker context ({why})")),
    }

    if let Some(x) = &env.xdg_runtime_dir {
        let p = x.join("docker.sock");
        if p.exists() {
            return Ok(format!("unix://{}", p.display()));
        }
        tried.push(p.display().to_string());
    } else {
        tried.push("$XDG_RUNTIME_DIR/docker.sock (XDG_RUNTIME_DIR unset)".into());
    }

    if env.default_socket.exists() {
        return Ok(format!("unix://{}", env.default_socket.display()));
    }
    tried.push(env.default_socket.display().to_string());

    bail!("no docker endpoint found; tried: {}", tried.join(", "))
}

fn context_endpoint(env: &EndpointEnv) -> std::result::Result<Option<String>, String> {
    let Some(dir) = &env.docker_config else {
        return Ok(None);
    };
    let cfg = dir.join("config.json");
    let Ok(raw) = std::fs::read_to_string(&cfg) else {
        return Ok(None);
    };
    let v: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", cfg.display()))?;
    let Some(name) = v.get("currentContext").and_then(|n| n.as_str()) else {
        return Ok(None);
    };
    if name.is_empty() || name == "default" {
        return Ok(None);
    }
    let meta = context_meta_path(dir, name);
    let raw = std::fs::read_to_string(&meta).map_err(|e| format!("{}: {e}", meta.display()))?;
    let m: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", meta.display()))?;
    m.pointer("/Endpoints/docker/Host")
        .and_then(|h| h.as_str())
        .map(|s| Some(s.to_string()))
        .ok_or_else(|| format!("{}: no Endpoints.docker.Host", meta.display()))
}

fn context_meta_path(config_dir: &Path, name: &str) -> PathBuf {
    let digest = Sha256::digest(name.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    config_dir.join("contexts/meta").join(hex).join("meta.json")
}

/// Connect bollard to the resolved endpoint.
pub fn connect() -> Result<bollard::Docker> {
    let ep = resolve(&EndpointEnv::from_process())?;
    let timeout = 120;
    let api = bollard::API_DEFAULT_VERSION;
    let client = if let Some(path) = ep.strip_prefix("unix://") {
        bollard::Docker::connect_with_unix(path, timeout, api)?
    } else if ep.starts_with("tcp://") || ep.starts_with("http://") {
        bollard::Docker::connect_with_http(&ep, timeout, api)?
    } else {
        bail!("unsupported docker endpoint scheme: {ep}");
    };
    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(dir: &Path, name: &str, host: &str) {
        std::fs::write(
            dir.join("config.json"),
            format!(r#"{{"currentContext":"{name}"}}"#),
        )
        .unwrap();
        let meta = context_meta_path(dir, name);
        std::fs::create_dir_all(meta.parent().unwrap()).unwrap();
        std::fs::write(
            meta,
            format!(r#"{{"Name":"{name}","Endpoints":{{"docker":{{"Host":"{host}"}}}}}}"#),
        )
        .unwrap();
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("reach-ep-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn sha256_dir_matches_docker() {
        // `printf rootless | sha256sum`, and the dir docker created on this host
        let p = context_meta_path(Path::new("/c"), "rootless");
        assert_eq!(
            p,
            PathBuf::from(
                "/c/contexts/meta/12b961af5feb3e9d39f93b2cefb9a1a944f18d02cca0cac2f04f5a982240605f/meta.json"
            )
        );
    }

    #[test]
    fn docker_host_wins() {
        let d = tmp("host");
        ctx(&d, "c", "unix:///ctx.sock");
        let env = EndpointEnv {
            docker_host: Some("tcp://h:1".into()),
            docker_config: Some(d.clone()),
            xdg_runtime_dir: Some(d.clone()),
            default_socket: d.join("none"),
        };
        assert_eq!(resolve(&env).unwrap(), "tcp://h:1");
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn context_beats_xdg_and_default() {
        let d = tmp("ctx");
        ctx(&d, "rootless", "unix:///run/user/1000/docker.sock");
        std::fs::write(d.join("docker.sock"), "").unwrap();
        let env = EndpointEnv {
            docker_config: Some(d.clone()),
            xdg_runtime_dir: Some(d.clone()),
            default_socket: d.join("docker.sock"),
            ..Default::default()
        };
        assert_eq!(resolve(&env).unwrap(), "unix:///run/user/1000/docker.sock");
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn xdg_beats_default() {
        let d = tmp("xdg");
        std::fs::write(d.join("docker.sock"), "").unwrap();
        let def = d.join("default.sock");
        std::fs::write(&def, "").unwrap();
        let env = EndpointEnv {
            xdg_runtime_dir: Some(d.clone()),
            default_socket: def,
            ..Default::default()
        };
        assert_eq!(
            resolve(&env).unwrap(),
            format!("unix://{}", d.join("docker.sock").display())
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn default_context_falls_through_to_default_socket() {
        let d = tmp("def");
        std::fs::write(d.join("config.json"), r#"{"currentContext":"default"}"#).unwrap();
        let def = d.join("default.sock");
        std::fs::write(&def, "").unwrap();
        let env = EndpointEnv {
            docker_config: Some(d.clone()),
            default_socket: def.clone(),
            ..Default::default()
        };
        assert_eq!(resolve(&env).unwrap(), format!("unix://{}", def.display()));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn error_names_every_path() {
        let d = tmp("err");
        let env = EndpointEnv {
            xdg_runtime_dir: Some(d.clone()),
            default_socket: d.join("var-run-docker.sock"),
            ..Default::default()
        };
        let e = resolve(&env).unwrap_err().to_string();
        assert!(e.contains("DOCKER_HOST"), "{e}");
        assert!(e.contains("docker context"), "{e}");
        assert!(
            e.contains(&d.join("docker.sock").display().to_string()),
            "{e}"
        );
        assert!(e.contains("var-run-docker.sock"), "{e}");
        std::fs::remove_dir_all(d).unwrap();
    }
}
