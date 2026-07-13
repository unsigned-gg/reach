use clap::Args;
use colored::Colorize;
use reach_cli::docker::DockerClient;

#[derive(Args)]
pub struct VncArgs {
    /// Sandbox name or container ID
    pub target: String,
}

pub async fn run(args: VncArgs) -> anyhow::Result<()> {
    let docker = DockerClient::new()?;
    let sandbox = docker.find(&args.target).await?;

    let port = sandbox
        .ports
        .novnc
        .ok_or_else(|| anyhow::anyhow!("noVNC port not mapped"))?;

    let url = format!("http://localhost:{port}/vnc.html?autoconnect=true");
    println!("{} {}", "Opening...".dimmed(), url.cyan());
    open::that(&url)?;
    Ok(())
}
