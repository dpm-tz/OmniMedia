//! Application-wide error type.
//!
//! Every `#[tauri::command]` returns `AppResult<T>`, which serializes errors
//! as a plain string for the frontend. Internally we keep typed variants so
//! Rust call sites can match on them.

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("recorder is already running")]
    RecorderBusy,

    #[error("recorder is not running")]
    RecorderIdle,

    #[error("source not found: {0}")]
    SourceNotFound(String),

    #[error("screen capture is not supported on this platform")]
    #[allow(dead_code)] // Returned on macOS/Linux until their pipelines land.
    CaptureUnsupported,

    #[error("screen capture permission was denied")]
    #[allow(dead_code)]
    CapturePermissionDenied,

    #[error(
        "FFmpeg is missing. Run `npm run setup:ffmpeg` in the project root (bundled build), \
         install FFmpeg on your PATH for development, or reinstall the application."
    )]
    FfmpegNotFound,

    #[error("recording failed: {0}")]
    RecordingFailed(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

// Serde sees AppError as a string when crossing the IPC boundary so the
// frontend gets a clean message instead of a tagged enum it doesn't know.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = std::result::Result<T, AppError>;
