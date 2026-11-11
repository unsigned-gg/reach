mod commands;

use clap::Parser;

#[derive(Parser)]
#[command(name = "reach", about = "AI-drivable containerized desktop sandbox")]
#[command(version, propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: commands::Command,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "reach=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    commands::run(cli.command).await
}
