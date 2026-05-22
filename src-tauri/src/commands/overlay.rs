//! On-screen annotation overlay (drawing while recording).
//!
//! We use **two** windows so the user can draw on top of any app while still
//! interacting with the toolbar:
//!
//! * **`overlay-canvas`** — fullscreen, transparent, click-through toggleable.
//!   This window IS captured by the recorder, so anything painted here ends up
//!   baked into the recording.
//! * **`overlay-toolbar`** — small floating toolbar with tools, color, etc.
//!   This window is **excluded from capture** via `SetWindowDisplayAffinity` so
//!   it never appears in the recording.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::error::{AppError, AppResult};

const W_CANVAS: &str = "overlay-canvas";
const W_TOOLBAR: &str = "overlay-toolbar";

// --- Tool/state DTOs (mirrored in src/lib/types.ts) ------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverlayTool {
    Pen,
    Rect,
    Circle,
    Arrow,
    Eraser,
    Text,
    Spotlight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayToolState {
    pub tool: OverlayTool,
    pub color: String,   // CSS color string
    pub size: u32,       // stroke width / shape thickness in px
    pub passthrough: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayCommand {
    pub kind: String, // "undo" | "clear" | "saveSnapshot" | ...
}

const EVT_TOOL_STATE: &str = "omnimedia://overlay/tool-state";
const EVT_COMMAND: &str = "omnimedia://overlay/command";

// --- Commands --------------------------------------------------------------

#[tauri::command]
pub fn open_annotation_overlay(app: AppHandle) -> AppResult<()> {
    if app.get_webview_window(W_CANVAS).is_some()
        || app.get_webview_window(W_TOOLBAR).is_some()
    {
        if let Some(c) = app.get_webview_window(W_CANVAS) {
            let _ = c.show();
        }
        if let Some(t) = app.get_webview_window(W_TOOLBAR) {
            let _ = t.show();
            let _ = t.set_focus();
        }
        return Ok(());
    }

    // Span all attached monitors so the overlay covers everything visible.
    // Fallback to the primary monitor if union calculation isn't available.
    let (canvas_x, canvas_y, canvas_w, canvas_h) = bounding_rect(&app)?;

    WebviewWindowBuilder::new(
        &app,
        W_CANVAS,
        WebviewUrl::App("index.html#overlay-canvas".into()),
    )
    .title("OmniMedia Overlay")
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .focused(false)
    .inner_size(canvas_w, canvas_h)
    .position(canvas_x, canvas_y)
    .visible(true)
    .build()
    .map_err(|e| AppError::Other(anyhow::anyhow!("create overlay-canvas: {e}")))?;

    // Toolbar — small floating panel near the top of the primary monitor.
    let (tb_x, tb_y, tb_w, tb_h) = primary_toolbar_rect(&app);
    WebviewWindowBuilder::new(
        &app,
        W_TOOLBAR,
        WebviewUrl::App("index.html#overlay-toolbar".into()),
    )
    .title("OmniMedia Tools")
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .focused(true)
    .inner_size(tb_w, tb_h)
    .position(tb_x, tb_y)
    .visible(true)
    .build()
    .map_err(|e| AppError::Other(anyhow::anyhow!("create overlay-toolbar: {e}")))?;

    // Exclude the toolbar from the recorder's capture pipeline. This is what
    // keeps it from appearing in the recorded video.
    #[cfg(target_os = "windows")]
    if let Some(toolbar) = app.get_webview_window(W_TOOLBAR) {
        if let Ok(hwnd) = toolbar.hwnd() {
            let _ = crate::win_helpers::set_excluded_from_capture(hwnd.0 as isize, true);
        }
    }

    // Default the canvas to passthrough so we don't steal cursor input until
    // the user picks a tool.
    if let Some(canvas) = app.get_webview_window(W_CANVAS) {
        if let Err(e) = canvas.set_ignore_cursor_events(true) {
            // If passthrough cannot be enabled, this fullscreen transparent
            // window can trap all clicks and appear as a white/blank screen.
            let _ = close_annotation_overlay(app.clone());
            return Err(AppError::Other(anyhow::anyhow!(
                "overlay passthrough setup failed: {e}"
            )));
        }
    }

    Ok(())
}

#[tauri::command]
pub fn close_annotation_overlay(app: AppHandle) -> AppResult<()> {
    if let Some(c) = app.get_webview_window(W_CANVAS) {
        let _ = c.close();
    }
    if let Some(t) = app.get_webview_window(W_TOOLBAR) {
        let _ = t.close();
    }
    Ok(())
}

/// Toggle whether the canvas window passes mouse input through to the apps
/// underneath. Called by the toolbar when the user picks/clears a tool.
#[tauri::command]
pub fn set_overlay_passthrough(app: AppHandle, passthrough: bool) -> AppResult<()> {
    if let Some(c) = app.get_webview_window(W_CANVAS) {
        c.set_ignore_cursor_events(passthrough)
            .map_err(|e| AppError::Other(anyhow::anyhow!("ignore_cursor_events: {e}")))?;
    }
    Ok(())
}

/// Broadcast the current tool state from the toolbar window to the canvas.
/// We funnel this through Rust so both windows stay in sync without needing a
/// shared front-end state store.
#[tauri::command]
pub fn overlay_set_tool(app: AppHandle, state: OverlayToolState) -> AppResult<()> {
    let _ = app.emit_to(W_CANVAS, EVT_TOOL_STATE, state.clone());
    let _ = app.emit_to(W_TOOLBAR, EVT_TOOL_STATE, state);
    Ok(())
}

/// One-shot command from the toolbar (undo, clear, save-snapshot, etc.).
#[tauri::command]
pub fn overlay_dispatch(app: AppHandle, command: OverlayCommand) -> AppResult<()> {
    let _ = app.emit_to(W_CANVAS, EVT_COMMAND, command);
    Ok(())
}

#[tauri::command]
pub fn overlay_is_open(app: AppHandle) -> AppResult<bool> {
    Ok(app.get_webview_window(W_CANVAS).is_some())
}

// --- Geometry helpers ------------------------------------------------------

/// Bounding rectangle of all attached monitors (logical coords). Falls back
/// to the primary monitor if multi-monitor info is unavailable.
fn bounding_rect(app: &AppHandle) -> AppResult<(f64, f64, f64, f64)> {
    if let Ok(monitors) = app.available_monitors() {
        if !monitors.is_empty() {
            let (mut min_x, mut min_y) = (i32::MAX, i32::MAX);
            let (mut max_x, mut max_y) = (i32::MIN, i32::MIN);
            for m in &monitors {
                let p = m.position();
                let s = m.size();
                let scale = m.scale_factor().max(1.0);
                let lw = (s.width as f64 / scale).round() as i32;
                let lh = (s.height as f64 / scale).round() as i32;
                let lx = (p.x as f64 / scale).round() as i32;
                let ly = (p.y as f64 / scale).round() as i32;
                min_x = min_x.min(lx);
                min_y = min_y.min(ly);
                max_x = max_x.max(lx + lw);
                max_y = max_y.max(ly + lh);
            }
            if min_x != i32::MAX {
                return Ok((
                    min_x as f64,
                    min_y as f64,
                    (max_x - min_x).max(1) as f64,
                    (max_y - min_y).max(1) as f64,
                ));
            }
        }
    }
    let m = app
        .primary_monitor()
        .ok()
        .flatten()
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("no primary monitor")))?;
    let s = m.size();
    let p = m.position();
    let scale = m.scale_factor().max(1.0);
    Ok((
        p.x as f64 / scale,
        p.y as f64 / scale,
        s.width as f64 / scale,
        s.height as f64 / scale,
    ))
}

fn primary_toolbar_rect(app: &AppHandle) -> (f64, f64, f64, f64) {
    let (w, h) = (700.0_f64, 64.0_f64);
    let (px, py, pw, _ph) = if let Ok(Some(m)) = app.primary_monitor() {
        let s = m.size();
        let p = m.position();
        let scale = m.scale_factor().max(1.0);
        (
            p.x as f64 / scale,
            p.y as f64 / scale,
            s.width as f64 / scale,
            s.height as f64 / scale,
        )
    } else {
        (0.0, 0.0, 1280.0, 720.0)
    };
    let x = px + (pw - w) / 2.0;
    let y = py + 24.0;
    (x, y, w, h)
}
