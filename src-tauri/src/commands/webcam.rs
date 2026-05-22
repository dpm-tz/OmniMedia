//! Webcam overlay window management.
//!
//! The webcam preview is a small, draggable, always-on-top window that uses
//! the webview's `getUserMedia` to render a live camera feed. The window is
//! intentionally **not** excluded from screen capture, so the preview shows
//! up in the recorded video.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::error::{AppError, AppResult};

const W_WEBCAM: &str = "webcam-overlay";

#[tauri::command]
pub fn open_webcam_overlay(app: AppHandle) -> AppResult<()> {
    if let Some(w) = app.get_webview_window(W_WEBCAM) {
        let _ = w.show();
        configure_webcam_window(&w);
        #[cfg(target_os = "windows")]
        crate::win_helpers::refocus_main(&app);
        return Ok(());
    }

    let (w_px, h_px) = (320.0_f64, 240.0_f64);
    let (mx, my, mw, mh) = if let Ok(Some(m)) = app.primary_monitor() {
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
    let x = mx + mw - w_px - 24.0;
    let y = my + mh - h_px - 56.0;

    let window = WebviewWindowBuilder::new(
        &app,
        W_WEBCAM,
        WebviewUrl::App("index.html#webcam-overlay".into()),
    )
    .title("Webcam")
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(true)
    .focused(false)
    .inner_size(w_px, h_px)
    .min_inner_size(160.0, 120.0)
    .position(x, y)
    .visible(true)
    .build()
    .map_err(|e| AppError::Other(anyhow::anyhow!("create webcam-overlay: {e}")))?;

    // Give WebView2 a moment to finish controller init before installing
    // the camera permission handler.  Without this, the with_webview
    // closure often cannot obtain CoreWebView2 and the permission handler
    // is never registered, causing getUserMedia to be denied.
    std::thread::sleep(std::time::Duration::from_millis(300));
    configure_webcam_window(&window);

    #[cfg(target_os = "windows")]
    crate::win_helpers::refocus_main(&app);

    Ok(())
}

/// Called by the webcam overlay UI before `getUserMedia` so permissions are
/// configured even if the page loaded before the handler was registered.
/// This is a **blocking** call on the backend that waits until the handler
/// is fully installed before returning, closing the race window.
#[tauri::command]
pub fn prepare_webcam_overlay(app: AppHandle) -> AppResult<()> {
    if let Some(w) = app.get_webview_window(W_WEBCAM) {
        if let Err(e) = try_configure_webcam_window(&w) {
            tracing::warn!("prepare_webcam_overlay: {e}");
            return Err(AppError::Other(anyhow::anyhow!(
                "Camera permissions could not be configured: {e}"
            )));
        }
    }
    Ok(())
}

#[tauri::command]
pub fn close_webcam_overlay(app: AppHandle) -> AppResult<()> {
    if let Some(w) = app.get_webview_window(W_WEBCAM) {
        let _ = w.close();
    }
    Ok(())
}

#[tauri::command]
pub fn webcam_is_open(app: AppHandle) -> AppResult<bool> {
    Ok(app.get_webview_window(W_WEBCAM).is_some())
}

fn configure_webcam_window(window: &WebviewWindow) {
    let _ = try_configure_webcam_window(window);
}

fn try_configure_webcam_window(window: &WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        crate::win_helpers::configure_webview_media(window).map_err(|e| {
            tracing::warn!("webcam media permissions: {e}");
            e
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        Ok(())
    }
}
