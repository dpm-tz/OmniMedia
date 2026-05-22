//! Screenshot capture commands.
//!
//! Uses the same Windows Graphics Capture pipeline as the recorder so a single
//! frame is grabbed straight from the GPU, converted from BGRA to RGBA, and
//! saved to disk as PNG. Frontend may pass a sub-rectangle for region capture.

use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotOptions {
    /// `display-N` (1-indexed), like the recorder's source ids. None = primary.
    #[serde(default)]
    pub source_id: Option<String>,
    /// Optional crop rect in the source's pixel space.
    #[serde(default)]
    pub region: Option<Region>,
    /// Optional override; defaults to <Pictures>/OmniMedia/Screenshots.
    #[serde(default)]
    pub output_dir: Option<PathBuf>,
    /// File format. Currently only `png` is supported.
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "png".into()
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotResult {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub size_bytes: u64,
}

#[tauri::command]
pub fn capture_screenshot(_app: AppHandle, options: ScreenshotOptions) -> AppResult<ScreenshotResult> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = options;
        return Err(AppError::CaptureUnsupported);
    }

    #[cfg(target_os = "windows")]
    {
        windows_capture_screenshot(options)
    }
}

#[cfg(target_os = "windows")]
fn windows_capture_screenshot(opts: ScreenshotOptions) -> AppResult<ScreenshotResult> {
    use windows_capture::capture::{Context, GraphicsCaptureApiHandler};
    use windows_capture::frame::Frame;
    use windows_capture::graphics_capture_api::InternalCaptureControl;
    use windows_capture::monitor::Monitor;
    use windows_capture::settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    };

    let monitor = if let Some(id) = &opts.source_id {
        if let Some(rest) = id.strip_prefix("display-") {
            let idx: usize = rest
                .parse()
                .map_err(|_| AppError::SourceNotFound(id.clone()))?;
            Monitor::from_index(idx).map_err(|e| AppError::RecordingFailed(e.to_string()))?
        } else {
            // For window sources we still snapshot the primary monitor — full
            // window screenshot would need additional plumbing.
            Monitor::primary().map_err(|e| AppError::RecordingFailed(e.to_string()))?
        }
    } else {
        Monitor::primary().map_err(|e| AppError::RecordingFailed(e.to_string()))?
    };

    let captured: Arc<Mutex<Option<(Vec<u8>, u32, u32)>>> = Arc::new(Mutex::new(None));
    let stop = Arc::new(AtomicBool::new(false));
    let region = opts.region;

    struct Handler {
        captured: Arc<Mutex<Option<(Vec<u8>, u32, u32)>>>,
        stop: Arc<AtomicBool>,
        region: Option<Region>,
    }

    impl GraphicsCaptureApiHandler for Handler {
        type Flags = (
            Arc<Mutex<Option<(Vec<u8>, u32, u32)>>>,
            Arc<AtomicBool>,
            Option<Region>,
        );
        type Error = String;

        fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
            let (captured, stop, region) = ctx.flags;
            Ok(Self {
                captured,
                stop,
                region,
            })
        }

        fn on_frame_arrived(
            &mut self,
            frame: &mut Frame,
            capture_control: InternalCaptureControl,
        ) -> Result<(), Self::Error> {
            if self.stop.load(Ordering::SeqCst) {
                capture_control.stop();
                return Ok(());
            }

            let fw = frame.width();
            let fh = frame.height();

            let mut buf = frame.buffer().map_err(|e| e.to_string())?;
            let pixels = buf.as_nopadding_buffer().map_err(|e| e.to_string())?;

            let (out, ow, oh) = if let Some(r) = self.region {
                let x = r.x.min(fw.saturating_sub(1));
                let y = r.y.min(fh.saturating_sub(1));
                let w = r.width.min(fw.saturating_sub(x)).max(1);
                let h = r.height.min(fh.saturating_sub(y)).max(1);
                let mut out = vec![0u8; (w * h * 4) as usize];
                let row_in = (fw * 4) as usize;
                let row_out = (w * 4) as usize;
                for row in 0..h {
                    let src_off = ((y + row) as usize) * row_in + (x as usize) * 4;
                    let dst_off = (row as usize) * row_out;
                    out[dst_off..dst_off + row_out]
                        .copy_from_slice(&pixels[src_off..src_off + row_out]);
                }
                (out, w, h)
            } else {
                (pixels.to_vec(), fw, fh)
            };

            // BGRA → RGBA in place.
            let mut rgba = out;
            for px in rgba.chunks_exact_mut(4) {
                px.swap(0, 2);
            }

            *self.captured.lock().map_err(|e| e.to_string())? = Some((rgba, ow, oh));
            self.stop.store(true, Ordering::SeqCst);
            capture_control.stop();
            Ok(())
        }

        fn on_closed(&mut self) -> Result<(), Self::Error> {
            self.stop.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    let settings = Settings::new(
        monitor,
        CursorCaptureSettings::WithoutCursor,
        DrawBorderSettings::Default,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        (Arc::clone(&captured), Arc::clone(&stop), region),
    );

    // Free-threaded capture so we can apply a hard timeout instead of blocking
    // forever on a frame that may never arrive (rare on idle displays).
    let capture_control = Handler::start_free_threaded(settings)
        .map_err(|e| AppError::RecordingFailed(format!("start capture: {e}")))?;

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !capture_control.is_finished() {
        if captured.lock().map(|g| g.is_some()).unwrap_or(false) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    if !capture_control.is_finished() {
        stop.store(true, Ordering::SeqCst);
        let _ = capture_control.stop();
    } else {
        let _ = capture_control.wait();
    }

    let (rgba, w, h) = captured
        .lock()
        .map_err(|e| AppError::Other(anyhow::anyhow!("{e}")))?
        .clone()
        .ok_or_else(|| {
            AppError::RecordingFailed(
                "no frame captured within 5s — the display may be idle or unavailable".into(),
            )
        })?;

    let out_dir = opts
        .output_dir
        .clone()
        .or_else(default_screenshot_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&out_dir)?;

    let stamp = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let ext = match opts.format.to_lowercase().as_str() {
        "jpg" | "jpeg" => "jpg",
        _ => "png",
    };
    let out_path = out_dir.join(format!("omnimedia-shot-{stamp}.{ext}"));

    let img = image::RgbaImage::from_raw(w, h, rgba)
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("RgbaImage::from_raw failed")))?;
    img.save(&out_path)
        .map_err(|e| AppError::Other(anyhow::anyhow!("save image: {e}")))?;

    let size_bytes = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);

    Ok(ScreenshotResult {
        path: out_path.to_string_lossy().into_owned(),
        width: w,
        height: h,
        size_bytes,
    })
}

fn default_screenshot_dir() -> Option<PathBuf> {
    dirs::picture_dir().map(|d| d.join("OmniMedia").join("Screenshots"))
}
