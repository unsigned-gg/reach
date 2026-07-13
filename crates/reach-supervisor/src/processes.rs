use anyhow::{Context, Result};
use nix::sys::signal::Signal;
use nix::unistd::Pid;
use std::path::Path;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::time;

// ═══════════════════════════════════════════════════════════
// Process specification — what to run and how to manage it
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub name: &'static str,
    pub command: &'static str,
    pub args: Vec<String>,
    pub env: Vec<(&'static str, String)>,
    pub restart: RestartPolicy,
    pub depends_on: Vec<&'static str>,
    pub ready_check: ReadyCheck,
}

#[derive(Debug, Clone)]
pub enum RestartPolicy {
    Always {
        max_restarts: u32,
        backoff: Duration,
    },
}

#[derive(Debug, Clone)]
pub enum ReadyCheck {
    /// Process is ready when file exists (e.g., X11 socket)
    FileExists(String),
    /// Process is ready when TCP port accepts connections
    TcpPort(u16),
    /// Process is ready immediately after spawn
    Immediate,
}

// ═══════════════════════════════════════════════════════════
// Process state — runtime status of a managed process
// ═══════════════════════════════════════════════════════════

#[derive(Debug)]
pub struct ManagedProcess {
    pub spec: ProcessSpec,
    pub state: ProcessState,
}

#[derive(Debug)]
pub enum ProcessState {
    Stopped,
    Starting,
    Running {
        child: Child,
        pid: u32,
        started_at: std::time::Instant,
        restart_count: u32,
    },
    Failed {
        exit_code: Option<i32>,
        restart_count: u32,
        last_error: String,
    },
}

impl ProcessState {
    pub fn is_running(&self) -> bool {
        matches!(self, ProcessState::Running { .. })
    }

    pub fn pid(&self) -> Option<u32> {
        match self {
            ProcessState::Running { pid, .. } => Some(*pid),
            _ => None,
        }
    }

    pub fn uptime(&self) -> Option<Duration> {
        match self {
            ProcessState::Running { started_at, .. } => Some(started_at.elapsed()),
            _ => None,
        }
    }

    pub fn restart_count(&self) -> u32 {
        match self {
            ProcessState::Running { restart_count, .. } => *restart_count,
            ProcessState::Failed { restart_count, .. } => *restart_count,
            _ => 0,
        }
    }

    pub fn exit_code(&self) -> Option<i32> {
        match self {
            ProcessState::Failed { exit_code, .. } => *exit_code,
            _ => None,
        }
    }

    pub fn last_error(&self) -> Option<&str> {
        match self {
            ProcessState::Failed { last_error, .. } => Some(last_error),
            _ => None,
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Health — reported per-process and aggregate
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProcessHealth {
    pub name: String,
    pub status: ProcessStatus,
    pub pid: Option<u32>,
    pub uptime_secs: Option<f64>,
    pub restart_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProcessStatus {
    Running,
    Starting,
    Stopped,
    Failed,
}

// ═══════════════════════════════════════════════════════════
// Supervisor — owns and manages all processes
// ═══════════════════════════════════════════════════════════

pub struct Supervisor {
    processes: Vec<ManagedProcess>,
    display: u32,
    width: u32,
    height: u32,
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl Supervisor {
    pub fn new() -> Self {
        Self {
            processes: Vec::new(),
            display: 99,
            width: 1280,
            height: 720,
        }
    }

    pub fn from_env() -> Self {
        let display = std::env::var("DISPLAY_NUM")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(99);
        let width = std::env::var("WIDTH")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1280);
        let height = std::env::var("HEIGHT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(720);

        Self {
            processes: Vec::new(),
            display,
            width,
            height,
        }
    }

    /// Build the process table — defines what runs and in what order.
    fn process_table(&self) -> Vec<ProcessSpec> {
        let display = format!(":{}", self.display);
        let resolution = format!("{}x{}x24", self.width, self.height);

        vec![
            ProcessSpec {
                name: "xvfb",
                command: "Xvfb",
                args: vec![
                    display.clone(),
                    "-screen".into(),
                    "0".into(),
                    resolution,
                    "-nolisten".into(),
                    "tcp".into(),
                ],
                env: vec![],
                restart: RestartPolicy::Always {
                    max_restarts: 5,
                    backoff: Duration::from_secs(1),
                },
                depends_on: vec![],
                ready_check: ReadyCheck::FileExists(format!("/tmp/.X11-unix/X{}", self.display)),
            },
            ProcessSpec {
                name: "openbox",
                command: "openbox",
                args: vec!["--sm-disable".into()],
                env: vec![("DISPLAY", display.clone())],
                restart: RestartPolicy::Always {
                    max_restarts: 5,
                    backoff: Duration::from_secs(1),
                },
                depends_on: vec!["xvfb"],
                ready_check: ReadyCheck::Immediate,
            },
            ProcessSpec {
                name: "x11vnc",
                command: "x11vnc",
                args: vec![
                    "-display".into(),
                    display.clone(),
                    "-forever".into(),
                    "-nopw".into(),
                    "-rfbport".into(),
                    "5900".into(),
                    "-shared".into(),
                ],
                env: vec![],
                restart: RestartPolicy::Always {
                    max_restarts: 10,
                    backoff: Duration::from_millis(500),
                },
                depends_on: vec!["xvfb"],
                ready_check: ReadyCheck::TcpPort(5900),
            },
            ProcessSpec {
                name: "novnc",
                command: "websockify",
                args: vec![
                    "--web".into(),
                    "/opt/noVNC".into(),
                    "6080".into(),
                    "localhost:5900".into(),
                ],
                env: vec![],
                restart: RestartPolicy::Always {
                    max_restarts: 5,
                    backoff: Duration::from_secs(1),
                },
                depends_on: vec!["x11vnc"],
                ready_check: ReadyCheck::TcpPort(6080),
            },
            ProcessSpec {
                name: "chrome",
                command: "google-chrome",
                args: vec![
                    "--remote-debugging-port=9222".into(),
                    "--remote-debugging-address=127.0.0.1".into(),
                    "--no-sandbox".into(),
                    "--disable-dev-shm-usage".into(),
                    // Enable software-rendered WebGL via SwiftShader. Without
                    // a working WebGL context the sandbox itself looks like a
                    // bot regardless of what stealth shims claim about
                    // VENDOR/RENDERER strings. `--enable-unsafe-swiftshader`
                    // is required from Chrome 120+ to opt into the fallback.
                    "--use-gl=angle".into(),
                    "--use-angle=swiftshader".into(),
                    "--enable-unsafe-swiftshader".into(),
                    "--user-data-dir=/tmp/chrome-profile".into(),
                    "--window-position=0,0".into(),
                    format!("--window-size={},{}", self.width, self.height),
                    "about:blank".into(),
                ],
                env: vec![("DISPLAY", display.clone())],
                restart: RestartPolicy::Always {
                    max_restarts: 10,
                    backoff: Duration::from_millis(500),
                },
                depends_on: vec!["xvfb"],
                ready_check: ReadyCheck::TcpPort(9222),
            },
            ProcessSpec {
                name: "reach-browserd",
                command: "reach-browserd",
                args: vec![],
                env: vec![],
                restart: RestartPolicy::Always {
                    max_restarts: 10,
                    backoff: Duration::from_millis(500),
                },
                depends_on: vec!["chrome"],
                ready_check: ReadyCheck::Immediate,
            },
        ]
    }

    /// Start all processes in dependency order.
    pub async fn start_all(&mut self) -> Result<()> {
        let specs = self.process_table();
        for spec in specs {
            self.spawn_process(spec).await?;
        }
        tracing::info!("all processes started");
        Ok(())
    }

    async fn spawn_process(&mut self, spec: ProcessSpec) -> Result<()> {
        tracing::info!(name = spec.name, cmd = spec.command, "starting process");

        let mut cmd = Command::new(spec.command);
        cmd.args(&spec.args);
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }

        // Push as Starting before spawn
        let idx = self.processes.len();
        self.processes.push(ManagedProcess {
            spec: spec.clone(),
            state: ProcessState::Starting,
        });

        let child = cmd
            .spawn()
            .with_context(|| format!("failed to spawn {}", spec.name))?;

        let pid = child.id().unwrap_or(0);

        // Wait for ready check
        self.wait_ready(&spec.ready_check).await?;

        tracing::info!(name = spec.name, pid, "process ready");

        // Transition to Running
        self.processes[idx] = ManagedProcess {
            spec,
            state: ProcessState::Running {
                child,
                pid,
                started_at: std::time::Instant::now(),
                restart_count: 0,
            },
        };

        Ok(())
    }

    async fn wait_ready(&self, check: &ReadyCheck) -> Result<()> {
        match check {
            ReadyCheck::Immediate => Ok(()),
            ReadyCheck::FileExists(path) => {
                for _ in 0..50 {
                    if Path::new(path).exists() {
                        return Ok(());
                    }
                    time::sleep(Duration::from_millis(100)).await;
                }
                anyhow::bail!("timeout waiting for {path}")
            }
            ReadyCheck::TcpPort(port) => {
                let addr = format!("127.0.0.1:{port}");
                for _ in 0..50 {
                    if tokio::net::TcpStream::connect(&addr).await.is_ok() {
                        return Ok(());
                    }
                    time::sleep(Duration::from_millis(100)).await;
                }
                anyhow::bail!("timeout waiting for port {port}")
            }
        }
    }

    /// Graceful shutdown — SIGTERM each child, wait, then SIGKILL stragglers.
    pub async fn stop_all(&mut self) -> Result<()> {
        for proc in self.processes.iter_mut().rev() {
            if let ProcessState::Running { child, pid, .. } = &mut proc.state {
                tracing::info!(name = proc.spec.name, pid, "sending SIGTERM");
                let nix_pid = Pid::from_raw(*pid as i32);
                let _ = nix::sys::signal::kill(nix_pid, Signal::SIGTERM);

                match time::timeout(Duration::from_secs(5), child.wait()).await {
                    Ok(Ok(status)) => {
                        tracing::info!(
                            name = proc.spec.name,
                            code = ?status.code(),
                            "exited cleanly"
                        );
                    }
                    _ => {
                        tracing::warn!(name = proc.spec.name, "SIGKILL after timeout");
                        let _ = child.kill().await;
                    }
                }
            }
            proc.state = ProcessState::Stopped;
        }
        Ok(())
    }

    /// Check all processes and restart any that have exited unexpectedly.
    /// Returns the number of processes restarted.
    pub async fn check_and_restart(&mut self) -> Result<usize> {
        let mut restarted = 0;

        // Snapshot which processes are running before mutable iteration
        let running_names: Vec<&'static str> = self
            .processes
            .iter()
            .filter(|p| p.state.is_running())
            .map(|p| p.spec.name)
            .collect();

        for proc in &mut self.processes {
            if let ProcessState::Running {
                child,
                restart_count,
                ..
            } = &mut proc.state
            {
                // Non-blocking check if process exited
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let code = status.code();
                        let count = *restart_count;

                        tracing::warn!(
                            name = proc.spec.name,
                            exit_code = ?code,
                            restart_count = count,
                            "process exited unexpectedly"
                        );

                        // Check restart policy
                        match &proc.spec.restart {
                            RestartPolicy::Always {
                                max_restarts,
                                backoff,
                            } => {
                                if count >= *max_restarts {
                                    tracing::error!(
                                        name = proc.spec.name,
                                        "max restarts ({max_restarts}) exceeded"
                                    );
                                    proc.state = ProcessState::Failed {
                                        exit_code: code,
                                        restart_count: count,
                                        last_error: format!(
                                            "max restarts exceeded (exit code: {code:?})"
                                        ),
                                    };
                                    continue;
                                }

                                // Backoff before restart
                                time::sleep(*backoff).await;

                                // Check dependencies are still running
                                let deps_ok = proc
                                    .spec
                                    .depends_on
                                    .iter()
                                    .all(|dep| running_names.contains(dep));

                                if !deps_ok {
                                    tracing::warn!(
                                        name = proc.spec.name,
                                        "dependency not running, marking failed"
                                    );
                                    proc.state = ProcessState::Failed {
                                        exit_code: code,
                                        restart_count: count,
                                        last_error: "dependency not running".into(),
                                    };
                                    continue;
                                }

                                // Restart
                                tracing::info!(
                                    name = proc.spec.name,
                                    attempt = count + 1,
                                    "restarting process"
                                );

                                let mut cmd = Command::new(proc.spec.command);
                                cmd.args(&proc.spec.args);
                                for (k, v) in &proc.spec.env {
                                    cmd.env(k, v);
                                }

                                match cmd.spawn() {
                                    Ok(new_child) => {
                                        let pid = new_child.id().unwrap_or(0);
                                        proc.state = ProcessState::Running {
                                            child: new_child,
                                            pid,
                                            started_at: std::time::Instant::now(),
                                            restart_count: count + 1,
                                        };
                                        restarted += 1;
                                    }
                                    Err(e) => {
                                        proc.state = ProcessState::Failed {
                                            exit_code: code,
                                            restart_count: count,
                                            last_error: e.to_string(),
                                        };
                                    }
                                }
                            }
                        }
                    }
                    Ok(None) => {} // still running
                    Err(e) => {
                        tracing::error!(name = proc.spec.name, error = %e, "failed to check process");
                    }
                }
            }
        }

        Ok(restarted)
    }

    /// Collect health status for all managed processes.
    pub fn health(&self) -> Vec<ProcessHealth> {
        self.processes
            .iter()
            .map(|p| ProcessHealth {
                name: p.spec.name.to_string(),
                status: match &p.state {
                    ProcessState::Running { .. } => ProcessStatus::Running,
                    ProcessState::Starting => ProcessStatus::Starting,
                    ProcessState::Stopped => ProcessStatus::Stopped,
                    ProcessState::Failed { .. } => ProcessStatus::Failed,
                },
                pid: p.state.pid(),
                uptime_secs: p.state.uptime().map(|d| d.as_secs_f64()),
                restart_count: p.state.restart_count(),
                exit_code: p.state.exit_code(),
                last_error: p.state.last_error().map(String::from),
            })
            .collect()
    }

    /// Check if all processes are running.
    pub fn all_healthy(&self) -> bool {
        self.processes.iter().all(|p| p.state.is_running())
    }
}

/// Remove stale X11 lock files that prevent Xvfb from starting.
pub fn clean_x11_locks() -> Result<()> {
    {
        let display = 99;
        let lock = format!("/tmp/.X{display}-lock");
        let path = Path::new(&lock);
        if path.exists() {
            std::fs::remove_file(path)?;
            tracing::info!("removed stale X11 lock: {lock}");
        }
    }
    Ok(())
}
