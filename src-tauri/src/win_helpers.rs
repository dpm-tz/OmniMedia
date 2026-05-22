//! Thin Win32 helpers used by the recorder + overlay + webcam subsystems.

#![cfg(target_os = "windows")]

use std::sync::mpsc;
use std::time::Duration;

use tauri::{Manager, WebviewWindow};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
};

fn to_hwnd(raw: isize) -> HWND {
    raw as HWND
}

/// Hide a window from screen-capture APIs (Windows 10 2004+ / Server 2019+).
pub fn set_excluded_from_capture(raw_hwnd: isize, excluded: bool) -> Result<(), String> {
    let hwnd = to_hwnd(raw_hwnd);
    let affinity = if excluded { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
    let ok = unsafe { SetWindowDisplayAffinity(hwnd, affinity) };
    if ok == 0 {
        let err = std::io::Error::last_os_error();
        return Err(format!(
            "SetWindowDisplayAffinity failed (excluded={excluded}): {err}"
        ));
    }
    Ok(())
}

pub fn set_capture_visible(raw_hwnd: isize) {
    let _ = set_excluded_from_capture(raw_hwnd, false);
}

/// Configure a webview window for camera/microphone access via WebView2.
///
/// This installs a `PermissionRequested` handler that auto-allows camera and
/// microphone, **and** pre-grants the permission for the current origin via
/// the profile (so a previously-denied state is overridden).
///
/// The call blocks until the handler is registered on the main thread, which
/// closes the race where JS calls `getUserMedia` before the handler exists.
pub fn configure_webview_media(window: &WebviewWindow) -> Result<(), String> {
    let origins = collect_origins(window);
    let (tx, rx) = mpsc::channel::<Result<(), String>>();

    window
        .with_webview(move |wv| {
            let result = configure_platform_webview_media(wv, &origins);
            let _ = tx.send(result);
        })
        .map_err(|e| format!("with_webview: {e}"))?;

    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(result) => result,
        Err(_) => Err("timed out installing webview media handler".into()),
    }
}

fn collect_origins(window: &WebviewWindow) -> Vec<String> {
    let mut origins: Vec<String> = Vec::new();
    if let Ok(url) = window.url() {
        if let Some(origin) = origin_from_url(&url) {
            origins.push(origin);
        }
    }
    // Common Tauri 2 + WebView2 origins (dev + prod).
    for fallback in [
        "http://localhost:1420",
        "http://tauri.localhost",
        "https://tauri.localhost",
    ] {
        if !origins.iter().any(|o| o == fallback) {
            origins.push(fallback.to_string());
        }
    }
    origins
}

fn origin_from_url(url: &tauri::Url) -> Option<String> {
    let scheme = url.scheme();
    let host = url.host_str()?;
    Some(match url.port() {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    })
}

fn configure_platform_webview_media(
    webview: tauri::webview::PlatformWebview,
    origins: &[String],
) -> Result<(), String> {
    install_permission_handler(&webview)?;
    tracing::info!("WebView2 PermissionRequested handler installed");
    // Pre-granting is best-effort: SetPermissionState may not exist on older
    // WebView2 runtimes, but the live handler still works.
    match pregrant_permissions(&webview, origins) {
        Ok(n) => tracing::info!("WebView2 camera/mic pre-granted for {n} origin(s)"),
        Err(e) => tracing::warn!("pregrant permissions failed: {e}"),
    }
    Ok(())
}

/// Install a `PermissionRequested` listener that auto-allows camera + mic.
fn install_permission_handler(webview: &tauri::webview::PlatformWebview) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use webview2_com::PermissionRequestedEventHandler;

    // The CoreWebView2 instance can be momentarily unavailable right after
    // window creation; retry with increasing back-off before giving up.
    let core = unsafe {
        let mut last_err: Option<String> = None;
        let mut got: Option<ICoreWebView2> = None;
        for i in 0..40 {
            match webview.controller().CoreWebView2() {
                Ok(c) => {
                    got = Some(c);
                    break;
                }
                Err(e) => {
                    last_err = Some(format!("{e}"));
                    std::thread::sleep(Duration::from_millis(if i < 10 { 50 } else { 100 }));
                }
            }
        }
        got.ok_or_else(|| {
            format!(
                "CoreWebView2 not ready after retries: {}",
                last_err.unwrap_or_default()
            )
        })?
    };

    unsafe {
        let mut token = 0i64;
        core.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(|_, args| {
                let Some(args) = args else {
                    return Ok(());
                };
                let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                args.PermissionKind(&mut kind)?;
                if kind == COREWEBVIEW2_PERMISSION_KIND_CAMERA
                    || kind == COREWEBVIEW2_PERMISSION_KIND_MICROPHONE
                {
                    args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
                }
                Ok(())
            })),
            &mut token,
        )
        .map_err(|e| format!("add_PermissionRequested: {e}"))?;
    }
    Ok(())
}

/// Pre-grant camera + microphone for the webview's profile (per-origin).
/// Returns the number of origins successfully pre-granted.
fn pregrant_permissions(
    webview: &tauri::webview::PlatformWebview,
    origins: &[String],
) -> Result<usize, String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use windows_core::Interface;

    let mut granted = 0usize;
    unsafe {
        let core = webview
            .controller()
            .CoreWebView2()
            .map_err(|e| format!("CoreWebView2: {e}"))?;
        let core13 = core
            .cast::<ICoreWebView2_13>()
            .map_err(|e| format!("cast ICoreWebView2_13 (need WebView2 Runtime 116+): {e}"))?;
        let profile = core13.Profile().map_err(|e| format!("Profile: {e}"))?;
        let profile4 = profile
            .cast::<ICoreWebView2Profile4>()
            .map_err(|e| format!("cast ICoreWebView2Profile4 (need WebView2 Runtime 122+): {e}"))?;

        for origin in origins {
            let mut wide: Vec<u16> = origin.encode_utf16().collect();
            wide.push(0);
            let pcwstr = windows_core::PCWSTR::from_raw(wide.as_ptr());
            let mut origin_ok = true;
            for kind in [
                COREWEBVIEW2_PERMISSION_KIND_CAMERA,
                COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
            ] {
                if let Err(e) = profile4.SetPermissionState(
                    kind,
                    pcwstr,
                    COREWEBVIEW2_PERMISSION_STATE_ALLOW,
                    None,
                ) {
                    tracing::debug!("SetPermissionState({origin}): {e}");
                    origin_ok = false;
                }
            }
            if origin_ok {
                granted += 1;
            }
        }
    }
    Ok(granted)
}

/// Return keyboard focus to the main app window after opening a child window.
pub fn refocus_main(app: &tauri::AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.unminimize();
        let _ = main.show();
        let _ = main.set_focus();
    }
}
