// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Note: `edition = "2024"` in Cargo.toml is intentional and matches the
// workspace edition; it is not a typo for 2021.

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Manager, RunEvent, State};
use tokio::runtime::Runtime;
use tokio::sync::oneshot;

/// Connection details for the embedded server, surfaced to the web UI over IPC
/// via [`server_info`] so the frontend can discover the API port and on-disk
/// paths without hard-coding them.
#[derive(Clone, serde::Serialize)]
struct ServerInfo {
    port: u16,
    db_path: String,
    blob_dir: String,
}

/// Owns the lifecycle of the embedded `trace-weft-server`. Each run lives on its
/// own thread + Tokio runtime and is shut down gracefully by resolving a
/// oneshot channel; `shutdown` is `Some` exactly while the server is running.
struct ServerControl {
    info: ServerInfo,
    shutdown: Mutex<Option<oneshot::Sender<()>>>,
}

impl ServerControl {
    fn new(info: ServerInfo) -> Self {
        Self {
            info,
            shutdown: Mutex::new(None),
        }
    }

    /// Start the server if it isn't already running.
    fn start(&self) -> Result<(), String> {
        let mut guard = self.shutdown.lock().unwrap();
        if guard.is_some() {
            return Err("server already running".into());
        }
        let (tx, rx) = oneshot::channel::<()>();
        let info = self.info.clone();
        std::thread::spawn(move || {
            let rt = Runtime::new().expect("Failed to create tokio runtime for axum");
            rt.block_on(async move {
                tracing::info!("Starting Embedded TraceWeft Server on port {}", info.port);
                let shutdown = async move {
                    let _ = rx.await;
                    tracing::info!("Embedded server shutting down");
                };
                if let Err(e) = trace_weft_server::start_server_with_shutdown(
                    &info.db_path,
                    info.port,
                    PathBuf::from(&info.blob_dir),
                    shutdown,
                )
                .await
                {
                    tracing::error!("Embedded server failed: {}", e);
                }
            });
        });
        *guard = Some(tx);
        Ok(())
    }

    /// Signal the running server to stop gracefully. Returns whether a server
    /// was running.
    fn stop(&self) -> bool {
        if let Some(tx) = self.shutdown.lock().unwrap().take() {
            let _ = tx.send(());
            true
        } else {
            false
        }
    }

    fn is_running(&self) -> bool {
        self.shutdown.lock().unwrap().is_some()
    }
}

#[tauri::command]
fn server_info(ctl: State<'_, ServerControl>) -> ServerInfo {
    ctl.info.clone()
}

#[tauri::command]
fn server_running(ctl: State<'_, ServerControl>) -> bool {
    ctl.is_running()
}

#[tauri::command]
fn server_start(ctl: State<'_, ServerControl>) -> Result<(), String> {
    ctl.start()
}

#[tauri::command]
fn server_stop(ctl: State<'_, ServerControl>) -> Result<(), String> {
    if ctl.stop() {
        Ok(())
    } else {
        Err("server not running".into())
    }
}

fn main() {
    tracing_subscriber::fmt::init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            server_info,
            server_running,
            server_start,
            server_stop,
        ])
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

            // The embedded server is local-first: with no API keys configured it enables the
            // dev bypass so the bundled UI works without auth (see
            // trace_weft_server::auth::AuthConfig::from_env_local_first).
            let control = ServerControl::new(ServerInfo {
                port,
                db_path: db_path.to_string_lossy().into_owned(),
                blob_dir: blob_dir.to_string_lossy().into_owned(),
            });
            app.manage(control);

            // Launch the server on app start.
            app.state::<ServerControl>()
                .start()
                .map_err(|e| format!("failed to start embedded server: {e}"))?;

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // Drain the embedded server cleanly when the app is exiting.
    app.run(|app_handle, event| {
        if let RunEvent::ExitRequested { .. } = event
            && let Some(control) = app_handle.try_state::<ServerControl>()
        {
            control.stop();
        }
    });
}
