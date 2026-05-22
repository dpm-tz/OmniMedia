//! Long-lived application state managed by Tauri.
//!
//! Anything that needs to outlive a single command call (recorder handle,
//! cached sources, queue of background jobs) goes here. Tauri injects this
//! into commands via `tauri::State<'_, AppState>`.

use parking_lot::Mutex;
use std::sync::Arc;

use crate::recorder::RecorderEngine;

#[derive(Default)]
pub struct AppState {
    /// The recorder engine is wrapped in an `Arc<Mutex<_>>` so multiple
    /// commands can hold short locks without contending on the global state.
    pub recorder: Arc<Mutex<RecorderEngine>>,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }
}
