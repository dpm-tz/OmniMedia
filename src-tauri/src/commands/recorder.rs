//! Tauri commands that the React recorder UI calls into.
//!
//! These are intentionally thin: argument validation + locking + delegation
//! to `RecorderEngine`. All real work happens in `crate::recorder`.

use tauri::{AppHandle, State};

use crate::error::AppResult;
use crate::recorder::{
    AudioInputDevice, RecordingOptions, RecordingResult, RecordingSource, RecordingStatus,
};
use crate::state::AppState;

#[tauri::command]
pub fn list_recording_sources(state: State<'_, AppState>) -> AppResult<Vec<RecordingSource>> {
    let engine = state.recorder.lock();
    engine.list_sources()
}

#[tauri::command]
pub fn list_audio_input_devices(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<Vec<AudioInputDevice>> {
    let engine = state.recorder.lock();
    engine.list_audio_input_devices(app)
}

#[tauri::command]
pub fn start_recording(
    app: AppHandle,
    options: RecordingOptions,
    state: State<'_, AppState>,
) -> AppResult<RecordingStatus> {
    let mut engine = state.recorder.lock();
    engine.start(app, options)
}

#[tauri::command]
pub fn stop_recording(state: State<'_, AppState>) -> AppResult<RecordingResult> {
    let mut engine = state.recorder.lock();
    engine.stop()
}

#[tauri::command]
pub fn recording_status(state: State<'_, AppState>) -> AppResult<RecordingStatus> {
    Ok(state.recorder.lock().status())
}
