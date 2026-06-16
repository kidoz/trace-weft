// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Note: `edition = "2024"` in Cargo.toml is intentional and matches the
// workspace edition; it is not a typo for 2021.

use std::path::PathBuf;
use tauri::Manager;
use tokio::runtime::Runtime;

/// Connection details for the embedded server, surfaced to the web UI over IPC
/// via the [`server_info`] command so the frontend can discover the API port
/// and on-disk paths without hard-coding them.
#[derive(Clone, serde::Serialize)]
struct ServerInfo {
    port: u16,
    db_path: String,
    blob_dir: String,
}

#[tauri::command]
fn server_info(info: tauri::State<'_, ServerInfo>) -> ServerInfo {
    info.inner().clone()
}

fn main() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![server_info])
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

            // Expose the resolved port/paths to the frontend over IPC.
            app.manage(ServerInfo {
                port,
                db_path: db_path.to_string_lossy().into_owned(),
                blob_dir: blob_dir.to_string_lossy().into_owned(),
            });

            // We must spawn the axum server in a background thread because Tauri blocks the main thread.
            // The embedded server is local-first: with no API keys configured it enables the
            // dev bypass so the bundled UI works without auth (see
            // trace_weft_server::auth::AuthConfig::from_env_local_first). The thread is tied to
            // the process and the OS reclaims it on app exit.
            let server_db_path = db_path.clone();
            std::thread::spawn(move || {
                let rt = Runtime::new().expect("Failed to create tokio runtime for axum");
                rt.block_on(async {
                    tracing::info!("Starting Embedded TraceWeft Server on port {}", port);
                    if let Err(e) = trace_weft_server::start_server(
                        &server_db_path.to_string_lossy(),
                        port,
                        blob_dir,
                    )
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
