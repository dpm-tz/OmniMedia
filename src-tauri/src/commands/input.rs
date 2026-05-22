//! Commands for the global input-event listener (click ripples + keystrokes).
//!
//! The actual Win32 hooks live in [`crate::input_hook`] (Windows-only). The
//! commands here are the Tauri-facing wrappers, gated to a no-op on other
//! platforms so the same JS bridge works everywhere.

use tauri::AppHandle;

use crate::error::AppResult;

#[tauri::command]
pub fn start_input_capture(app: AppHandle) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        crate::input_hook::start(app).map_err(|e| {
            crate::error::AppError::Other(anyhow::anyhow!("install input hooks: {e}"))
        })?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
    }
    Ok(())
}

#[tauri::command]
pub fn stop_input_capture() -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        crate::input_hook::stop();
    }
    Ok(())
}
