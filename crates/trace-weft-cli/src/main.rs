use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the local development server and UI
    Dev {
        /// Port to run the UI and API server on
        #[arg(short, long, default_value_t = 3000)]
        port: u16,

        /// Path to the local sqlite database
        #[arg(short, long, default_value = "./.trace-weft/traces.sqlite")]
        db_path: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match &cli.command {
        Commands::Dev { port, db_path } => {
            println!("Starting TraceWeft local server on port {}...", port);
            println!("Reading traces from: {}", db_path.display());

            let blob_dir = PathBuf::from("./.trace-weft/blobs");
            trace_weft_server::start_server(&db_path.to_string_lossy(), *port, blob_dir).await?;
        }
    }

    Ok(())
}
