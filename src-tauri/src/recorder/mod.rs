//! Screen recording engine.
//!
//! - **Windows:** [`win`] uses Windows Graphics Capture (`windows-capture`) for
//!   video and FFmpeg WASAPI loopback for system audio.
//! - **Other desktops:** not implemented yet (`CaptureUnsupported`).

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use uuid::Uuid;

#[cfg(target_os = "windows")]
use tauri::Manager;
use tauri::AppHandle;

use crate::error::{AppError, AppResult};

#[cfg(target_os = "windows")]
mod win;

// ---------------------------------------------------------------------------
// Public DTOs (mirrored in src/lib/types.ts)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSource {
    pub id: String,
    pub label: String,
    pub kind: SourceKind,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInputDevice {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Screen,
    Window,
    #[allow(dead_code)]
    Area,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingOptions {
    pub source_id: String,
    pub fps: u32,
    pub capture_system_audio: bool,
    pub capture_microphone: bool,
    #[serde(default)]
    pub microphone_device_id: Option<String>,
    #[serde(default)]
    pub output_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
#[allow(dead_code)]
pub enum RecordingStatus {
    Idle,
    Recording {
        id: String,
        #[serde(rename = "startedAt")]
        started_at: String,
    },
    Stopped {
        id: String,
        #[serde(rename = "outputPath")]
        output_path: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingResult {
    pub id: String,
    pub output_path: String,
    pub duration_ms: u64,
    pub size_bytes: u64,
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

pub struct RecorderEngine {
    session: Option<ActiveSession>,
}

struct ActiveSession {
    stop: Arc<AtomicBool>,
    worker: JoinHandle<Result<(), String>>,
    id: String,
    started_at_ms: i64,
    output_path: PathBuf,
    /// HWNDs (as isize) re-included in capture on stop. Only used on Windows.
    #[cfg(target_os = "windows")]
    excluded_windows: Vec<isize>,
    #[cfg(not(target_os = "windows"))]
    #[allow(dead_code)]
    excluded_windows: Vec<isize>,
}

impl Default for RecorderEngine {
    fn default() -> Self {
        Self { session: None }
    }
}

impl RecorderEngine {
    pub fn list_sources(&self) -> AppResult<Vec<RecordingSource>> {
        #[cfg(target_os = "windows")]
        {
            win::list_sources()
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err(AppError::CaptureUnsupported)
        }
    }

    pub fn list_audio_input_devices(&self, app: AppHandle) -> AppResult<Vec<AudioInputDevice>> {
        #[cfg(target_os = "windows")]
        {
            win::list_audio_input_devices(&app)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = app;
            Err(AppError::CaptureUnsupported)
        }
    }

    pub fn start(&mut self, app: AppHandle, opts: RecordingOptions) -> AppResult<RecordingStatus> {
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (app, opts);
            return Err(AppError::CaptureUnsupported);
        }

        #[cfg(target_os = "windows")]
        {
            start_windows(self, app, opts)
        }
    }

    pub fn stop(&mut self) -> AppResult<RecordingResult> {
        let s = self.session.take().ok_or(AppError::RecorderIdle)?;
        s.stop.store(true, Ordering::SeqCst);

        let join_result = s.worker.join();

        // Always try to re-include hidden windows in capture, even if the
        // worker errored. Otherwise the user's UI stays invisible to other
        // capture tools until they restart the app.
        #[cfg(target_os = "windows")]
        for hwnd in &s.excluded_windows {
            crate::win_helpers::set_capture_visible(*hwnd);
        }

        match join_result {
            Ok(Ok(())) => {}
            Ok(Err(msg)) => {
                tracing::error!(%msg, "recording thread reported failure");
                return Err(AppError::RecordingFailed(msg));
            }
            Err(_) => {
                return Err(AppError::RecordingFailed(
                    "recording thread panicked".into(),
                ));
            }
        }

        let now_ms = Utc::now().timestamp_millis();
        let duration_ms = (now_ms - s.started_at_ms).max(0) as u64;
        let size_bytes = std::fs::metadata(&s.output_path)
            .map(|m| m.len())
            .unwrap_or(0);

        tracing::info!(id = %s.id, duration_ms, size_bytes, "stopped recording");

        Ok(RecordingResult {
            id: s.id,
            output_path: s.output_path.to_string_lossy().into_owned(),
            duration_ms,
            size_bytes,
        })
    }

    pub fn status(&self) -> RecordingStatus {
        match &self.session {
            None => RecordingStatus::Idle,
            Some(s) => RecordingStatus::Recording {
                id: s.id.clone(),
                started_at: chrono::DateTime::<Utc>::from_timestamp_millis(s.started_at_ms)
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_default(),
            },
        }
    }
}

#[cfg(target_os = "windows")]
fn start_windows(
    engine: &mut RecorderEngine,
    app: AppHandle,
    opts: RecordingOptions,
) -> AppResult<RecordingStatus> {
    if engine.session.is_some() {
        return Err(AppError::RecorderBusy);
    }

    let sources = win::list_sources()?;
    if !sources.iter().any(|s| s.id == opts.source_id) {
        return Err(AppError::SourceNotFound(opts.source_id));
    }

    if crate::ffmpeg::resolve_ffmpeg_path(&app).is_err() {
        return Err(AppError::FfmpegNotFound);
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let output_dir = opts
        .output_dir
        .clone()
        .or_else(default_output_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&output_dir)?;
    let output_path = output_dir.join(format!("omnimedia-{}.mp4", &id[..8]));

    tracing::info!(
        id = %id,
        source = %opts.source_id,
        fps = opts.fps,
        audio = opts.capture_system_audio,
        out = ?output_path,
        "starting recording session"
    );

    // Hide the recorder UI windows from the capture pipeline so they don't
    // show up in the recording. We exclude:
    //   * the main app window (controls, toolbars)
    //   * the floating annotation toolbar (if open) — auto-detected by label
    // The transparent annotation canvas is intentionally NOT excluded so
    // overlays drawn during recording are baked into the video.
    let mut excluded_windows: Vec<isize> = Vec::new();
    for label in ["main", "overlay-toolbar"] {
        if let Some(win_handle) = app.get_webview_window(label) {
            if let Ok(hwnd) = win_handle.hwnd() {
                let raw = hwnd.0 as isize;
                if let Err(e) = crate::win_helpers::set_excluded_from_capture(raw, true) {
                    tracing::warn!(label, %e, "failed to exclude window from capture");
                } else {
                    excluded_windows.push(raw);
                    tracing::debug!(label, hwnd = raw, "excluded window from capture");
                }
            }
        }
    }

    let stop = Arc::new(AtomicBool::new(false));
    let worker = win::spawn_session_thread(
        app,
        id.clone(),
        opts,
        output_path.clone(),
        Arc::clone(&stop),
    );

    engine.session = Some(ActiveSession {
        stop,
        worker,
        id: id.clone(),
        started_at_ms: now.timestamp_millis(),
        output_path,
        excluded_windows,
    });

    Ok(RecordingStatus::Recording {
        id,
        started_at: now.to_rfc3339(),
    })
}

fn default_output_dir() -> Option<PathBuf> {
    dirs::video_dir().map(|d| d.join("OmniMedia"))
}
