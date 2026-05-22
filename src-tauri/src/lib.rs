//! OmniMedia - Rust backend entry point.
//!
//! Layout:
//!   commands/   - all #[tauri::command] handlers (thin, validate + delegate)
//!   recorder/   - screen + audio capture engine
//!   ffmpeg.rs - bundled `ffmpeg` resolution (resources/ vs. PATH)
//!   editor/     - video timeline / FFmpeg pipeline (filled in Step 3)
//!   image/      - screenshot + annotation pipeline (filled in Step 4)
//!   error.rs    - AppError + AppResult shared everywhere
//!   state.rs    - long-lived state managed by Tauri

mod commands;
mod editor;
mod error;
mod ffmpeg;
mod image;
#[cfg(target_os = "windows")]
mod input_hook;
mod recorder;
mod state;
#[cfg(target_os = "windows")]
mod win_helpers;

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .setup(|app| {
            #[cfg(target_os = "windows")]
            {
                use tauri::Manager;
                // configure_webview_media() blocks on a message that has to be
                // dispatched on the main thread. Setup itself runs on the main
                // thread, so we MUST defer to a worker; otherwise we deadlock
                // for 5s and the profile never gets the camera pre-grant.
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    // Give WebView2 a moment to finish initializing the
                    // controller before we touch it.
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    if let Some(main) = handle.get_webview_window("main") {
                        match crate::win_helpers::configure_webview_media(&main) {
                            Ok(()) => tracing::info!(
                                "WebView2 camera/mic permissions configured on main window"
                            ),
                            Err(e) => {
                                tracing::warn!("startup configure_webview_media: {e}")
                            }
                        }
                    } else {
                        tracing::warn!("main window not available during startup");
                    }
                });
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = app;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::system_info,
            commands::recorder::list_recording_sources,
            commands::recorder::list_audio_input_devices,
            commands::recorder::start_recording,
            commands::recorder::stop_recording,
            commands::recorder::recording_status,
            commands::overlay::open_annotation_overlay,
            commands::overlay::close_annotation_overlay,
            commands::overlay::set_overlay_passthrough,
            commands::overlay::overlay_set_tool,
            commands::overlay::overlay_dispatch,
            commands::overlay::overlay_is_open,
            commands::screenshot::capture_screenshot,
            commands::input::start_input_capture,
            commands::input::stop_input_capture,
            commands::webcam::open_webcam_overlay,
            commands::webcam::prepare_webcam_overlay,
            commands::webcam::close_webcam_overlay,
            commands::webcam::webcam_is_open,
            commands::editor::trim_video,
            commands::editor::export_gif,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Structured logging via `tracing`. Driven by RUST_LOG env var; defaults to
/// `info` for our crate so dev runs are useful without being noisy.
fn init_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,omnimedia_lib=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false))
        .init();
}
