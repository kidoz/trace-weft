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
    /// Start the local API server used by the web UI
    Dev {
        /// Port to run the API server on
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
            // `dev` is the local-first command: the auth bypass defaults on when
            // no keys are configured (set TRACE_WEFT_API_KEYS to enforce auth).
            trace_weft_server::start_dev_server(&db_path.to_string_lossy(), *port, blob_dir)
                .await?;
        }
    }

    Ok(())
}
