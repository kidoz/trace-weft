// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use tokio::runtime::Runtime;

fn main() {
    tracing_subscriber::fmt::init();

    // Determine paths for the local database and blobs
    // For MVP, we just use the current directory's .trace-weft folder,
    // but in a real desktop app, we'd use tauri's app_data_dir().
    let db_path = PathBuf::from("./.trace-weft/traces.sqlite");
    let blob_dir = PathBuf::from("./.trace-weft/blobs");
    let port = 3000; // Same port our React app expects via proxy in dev, or directly in prod.

    // We must spawn the axum server in a background thread because Tauri blocks the main thread.
    std::thread::spawn(move || {
        let rt = Runtime::new().expect("Failed to create tokio runtime for axum");
        rt.block_on(async {
            tracing::info!("Starting Embedded TraceWeft Server on port {}", port);
            if let Err(e) =
                trace_weft_server::start_server(&db_path.to_string_lossy(), port, blob_dir).await
            {
                tracing::error!("Embedded server failed: {}", e);
            }
        });
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
