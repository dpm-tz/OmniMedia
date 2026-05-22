//! Generic, low-risk commands that don't fit a specific feature module.
use serde::Serialize;
use tauri::AppHandle;

use crate::error::AppResult;
use crate::ffmpeg;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
    pub app_version: String,
    pub ffmpeg_available: bool,
}

#[tauri::command]
pub fn system_info(app: AppHandle) -> AppResult<SystemInfo> {
    Ok(SystemInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        ffmpeg_available: ffmpeg::resolve_ffmpeg_path(&app).is_ok(),
    })
}
