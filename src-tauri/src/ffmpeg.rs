//! Resolves FFmpeg: **bundled** in `bundle.resources` first, then dev tree, then `PATH`.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

/// Path relative to Tauri [`PathResolver::resource_dir`] (configured in `tauri.conf.json`).
pub(crate) fn bundled_relative_path() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffmpeg/ffmpeg.exe"
    } else {
        "ffmpeg/ffmpeg"
    }
}

/// Resolve an FFmpeg executable suitable for `std::process::Command`.
///
/// Order:
/// 1. **Packaged app:** `resource_dir()` + `ffmpeg/ffmpeg` (or `.exe`).
/// 2. **Development:** `src-tauri/resources/...` next to `CARGO_MANIFEST_DIR`.
/// 3. **`PATH`:** system `ffmpeg` (optional dev convenience).
pub(crate) fn resolve_ffmpeg_path(app: &AppHandle) -> Result<PathBuf, String> {
    let rel = bundled_relative_path();

    if let Ok(dir) = app.path().resource_dir() {
        let p = dir.join(rel);
        if p.is_file() {
            tracing::debug!(path = %p.display(), "using bundled FFmpeg (resource dir)");
            return Ok(p);
        }
    }

    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(rel);
    if dev.is_file() {
        tracing::debug!(path = %dev.display(), "using bundled FFmpeg (dev tree)");
        return Ok(dev);
    }

    if let Ok(p) = which::which("ffmpeg") {
        tracing::debug!(path = %p.display(), "using PATH FFmpeg");
        return Ok(p);
    }

    Err(
        "FFmpeg not found. For release builds, run `npm run setup:ffmpeg` before `npm run tauri build`. \
         For development, run that once or install FFmpeg on your PATH."
            .into(),
    )
}

pub(crate) fn spawn_ffmpeg_command(exe: &Path) -> std::process::Command {
    std::process::Command::new(exe)
}
