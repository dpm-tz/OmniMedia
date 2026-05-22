//! Video editing commands powered by the bundled FFmpeg.
//!
//! - `trim_video`  → fast trim using `-ss/-to` + stream copy when possible.
//! - `export_gif`  → palette-aware GIF encode (palettegen + paletteuse) with
//!   tunable fps/width.

use std::path::PathBuf;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrimOptions {
    pub input_path: PathBuf,
    pub start_seconds: f64,
    pub end_seconds: f64,
    /// If `None`, output goes next to the input as `<name>-trim.<ext>`.
    #[serde(default)]
    pub output_path: Option<PathBuf>,
    /// `true` = stream copy (super fast, may snap to nearest keyframe).
    /// `false` = re-encode (slower, frame-accurate).
    #[serde(default = "default_true")]
    pub stream_copy: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditResult {
    pub output_path: String,
    pub size_bytes: u64,
    pub duration_ms: u64,
}

#[tauri::command]
pub async fn trim_video(app: AppHandle, options: TrimOptions) -> AppResult<EditResult> {
    let ffmpeg = crate::ffmpeg::resolve_ffmpeg_path(&app).map_err(|_| AppError::FfmpegNotFound)?;

    if !options.input_path.is_file() {
        return Err(AppError::Other(anyhow::anyhow!(
            "input not found: {}",
            options.input_path.display()
        )));
    }
    if options.end_seconds <= options.start_seconds {
        return Err(AppError::Other(anyhow::anyhow!(
            "end ({}) must be greater than start ({})",
            options.end_seconds,
            options.start_seconds
        )));
    }

    let output = options.output_path.unwrap_or_else(|| {
        derive_output_path(&options.input_path, "trim", extension_for(&options.input_path))
    });
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let started = std::time::Instant::now();
    let mut cmd = crate::ffmpeg::spawn_ffmpeg_command(&ffmpeg);
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-y")
        .arg("-ss")
        .arg(format!("{:.3}", options.start_seconds))
        .arg("-to")
        .arg(format!("{:.3}", options.end_seconds))
        .arg("-i")
        .arg(&options.input_path);
    if options.stream_copy {
        cmd.arg("-c").arg("copy");
    } else {
        cmd.arg("-c:v")
            .arg("libx264")
            .arg("-preset")
            .arg("medium")
            .arg("-crf")
            .arg("20")
            .arg("-c:a")
            .arg("aac");
    }
    cmd.arg("-movflags").arg("+faststart").arg(&output);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let result = cmd
        .output()
        .map_err(|e| AppError::Other(anyhow::anyhow!("spawn FFmpeg: {e}")))?;
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AppError::Other(anyhow::anyhow!(
            "trim failed ({}). FFmpeg stderr: {}",
            result.status,
            stderr.trim()
        )));
    }

    let size_bytes = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);

    Ok(EditResult {
        output_path: output.to_string_lossy().into_owned(),
        size_bytes,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GifOptions {
    pub input_path: PathBuf,
    /// Defaults to 0.0.
    #[serde(default)]
    pub start_seconds: Option<f64>,
    /// Defaults to end-of-file.
    #[serde(default)]
    pub end_seconds: Option<f64>,
    /// Output frame rate (frames per second). 12-30 is typical for a GIF.
    #[serde(default = "default_gif_fps")]
    pub fps: u32,
    /// Output width in px (height auto-scales). 0 / None = source width.
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub output_path: Option<PathBuf>,
}

fn default_gif_fps() -> u32 {
    15
}

#[tauri::command]
pub async fn export_gif(app: AppHandle, options: GifOptions) -> AppResult<EditResult> {
    let ffmpeg = crate::ffmpeg::resolve_ffmpeg_path(&app).map_err(|_| AppError::FfmpegNotFound)?;

    if !options.input_path.is_file() {
        return Err(AppError::Other(anyhow::anyhow!(
            "input not found: {}",
            options.input_path.display()
        )));
    }

    let output = options
        .output_path
        .unwrap_or_else(|| derive_output_path(&options.input_path, "gif", "gif"));
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let fps = options.fps.clamp(5, 30);
    let scale_w = options.width.unwrap_or(0);
    let scale_filter = if scale_w == 0 {
        "scale=iw:ih:flags=lanczos".to_string()
    } else {
        format!("scale={scale_w}:-1:flags=lanczos")
    };
    let vf = format!(
        "fps={fps},{scale_filter},split[s0][s1];[s0]palettegen=stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=5"
    );

    let started = std::time::Instant::now();
    let mut cmd = crate::ffmpeg::spawn_ffmpeg_command(&ffmpeg);
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-y");
    if let Some(s) = options.start_seconds {
        cmd.arg("-ss").arg(format!("{:.3}", s.max(0.0)));
    }
    if let Some(e) = options.end_seconds {
        cmd.arg("-to").arg(format!("{:.3}", e.max(0.0)));
    }
    cmd.arg("-i")
        .arg(&options.input_path)
        .arg("-filter_complex")
        .arg(&vf)
        .arg("-loop")
        .arg("0")
        .arg(&output);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let result = cmd
        .output()
        .map_err(|e| AppError::Other(anyhow::anyhow!("spawn FFmpeg: {e}")))?;
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AppError::Other(anyhow::anyhow!(
            "gif export failed ({}). FFmpeg stderr: {}",
            result.status,
            stderr.trim()
        )));
    }

    let size_bytes = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);

    Ok(EditResult {
        output_path: output.to_string_lossy().into_owned(),
        size_bytes,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

fn derive_output_path(input: &std::path::Path, suffix: &str, ext: &str) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into());
    let parent = input.parent().unwrap_or_else(|| std::path::Path::new("."));
    parent.join(format!("{stem}-{suffix}.{ext}"))
}

fn extension_for(input: &std::path::Path) -> &'static str {
    match input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "mov" => "mov",
        "mkv" => "mkv",
        "webm" => "webm",
        _ => "mp4",
    }
}
