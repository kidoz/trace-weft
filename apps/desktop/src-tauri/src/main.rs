// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use tauri::Manager;
use tokio::runtime::Runtime;

fn main() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Determine paths for the local database and blobs using tauri's app_data_dir()
            // or an environment variable override for local development.
            let app_data_dir = if let Ok(dev_dir) = std::env::var("TRACE_WEFT_DEV_DIR") {
                PathBuf::from(dev_dir)
            } else {
                app.path()
                    .app_data_dir()
                    .unwrap_or_else(|_| PathBuf::from("./.trace-weft"))
            };
            let db_path = app_data_dir.join("traces.sqlite");
            let blob_dir = app_data_dir.join("blobs");
            let port = 3000; // Same port our React app expects via proxy in dev, or directly in prod.

            // We must spawn the axum server in a background thread because Tauri blocks the main thread.
            std::thread::spawn(move || {
                let rt = Runtime::new().expect("Failed to create tokio runtime for axum");
                rt.block_on(async {
                    tracing::info!("Starting Embedded TraceWeft Server on port {}", port);
                    if let Err(e) =
                        trace_weft_server::start_server(&db_path.to_string_lossy(), port, blob_dir)
                            .await
                    {
                        tracing::error!("Embedded server failed: {}", e);
                    }
                });
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
