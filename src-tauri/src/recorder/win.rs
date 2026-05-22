//! Windows: `windows-capture` (WGC) → raw BGRA → FFmpeg (libx264 + WASAPI loopback AAC).

use std::{
    collections::HashSet,
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use wasapi::{initialize_mta, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};
use windows_capture::capture::{Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

use super::{AudioInputDevice, RecordingOptions, RecordingSource, SourceKind};
use crate::error::AppResult;
use crate::error::AppError;

pub(super) const EVT_PROGRESS: &str = "omnimedia://recorder/progress";
#[allow(dead_code)]
pub(super) const EVT_ERROR: &str = "omnimedia://recorder/error";
pub(super) const EVT_WARN: &str = "omnimedia://recorder/warn";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    id: String,
    elapsed_ms: u64,
    frames_captured: u64,
}

enum CaptureTarget {
    Monitor(Monitor),
    Window(Window),
}

pub(super) fn list_sources() -> AppResult<Vec<RecordingSource>> {
    let mut out = Vec::new();

    for m in Monitor::enumerate().map_err(|e| AppError::RecordingFailed(e.to_string()))? {
        let idx = m
            .index()
            .map_err(|e| AppError::RecordingFailed(e.to_string()))?;
        let width = m
            .width()
            .map_err(|e| AppError::RecordingFailed(e.to_string()))?;
        let height = m
            .height()
            .map_err(|e| AppError::RecordingFailed(e.to_string()))?;
        let label = m
            .name()
            .map_err(|e| AppError::RecordingFailed(e.to_string()))?;

        out.push(RecordingSource {
            id: format!("display-{idx}"),
            label,
            kind: SourceKind::Screen,
            width,
            height,
        });
    }

    for w in Window::enumerate().map_err(|e| AppError::RecordingFailed(e.to_string()))? {
        let title = match w.title() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if title.is_empty() {
            continue;
        }

        let rect = match w.rect() {
            Ok(r) => r,
            Err(_) => continue,
        };
        let width = (rect.right - rect.left).max(0) as u32;
        let height = (rect.bottom - rect.top).max(0) as u32;
        if width == 0 || height == 0 {
            continue;
        }

        let hwnd = w.as_raw_hwnd() as usize;
        out.push(RecordingSource {
            id: format!("window-{hwnd:x}"),
            label: title,
            kind: SourceKind::Window,
            width,
            height,
        });
    }

    Ok(out)
}

pub(super) fn list_audio_input_devices(app: &AppHandle) -> AppResult<Vec<AudioInputDevice>> {
    let ffmpeg_exe = crate::ffmpeg::resolve_ffmpeg_path(app).map_err(|_| AppError::FfmpegNotFound)?;
    let out = std::process::Command::new(ffmpeg_exe)
        .args(["-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| AppError::RecordingFailed(format!("query audio devices: {e}")))?;

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let mut in_audio = false;
    let mut seen = HashSet::<String>::new();
    let mut devices = Vec::<AudioInputDevice>::new();

    for raw in combined.lines() {
        let line = raw.trim();
        if line.contains("DirectShow audio devices") {
            in_audio = true;
            continue;
        }
        if in_audio && line.contains("DirectShow video devices") {
            break;
        }
        if !in_audio || line.contains("Alternative name") {
            continue;
        }
        if let Some(name) = first_quoted_segment(line) {
            if seen.insert(name.to_string()) {
                devices.push(AudioInputDevice {
                    id: name.to_string(),
                    label: name.to_string(),
                });
            }
        }
    }

    Ok(devices)
}

pub(super) fn spawn_session_thread(
    app: AppHandle,
    recording_id: String,
    opts: RecordingOptions,
    output_path: PathBuf,
    stop: Arc<AtomicBool>,
) -> JoinHandle<Result<(), String>> {
    std::thread::spawn(move || run_session(app, recording_id, opts, output_path, stop))
}

fn run_session(
    app: AppHandle,
    recording_id: String,
    opts: RecordingOptions,
    output_path: PathBuf,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let ffmpeg_exe = crate::ffmpeg::resolve_ffmpeg_path(&app)?;

    let target = resolve_target(&opts.source_id)?;
    // Validated up front so we fail fast if the handle is stale; actual W×H come
    // from the first captured frame (DisplayConfig / window rects can disagree
    // with Windows Graphics Capture output, which breaks rawvideo FFmpeg input).
    let _ = target_dimensions(&target)?;
    let fps = opts.fps.clamp(1, 120);

    // We always capture system audio via the Rust WASAPI loopback path. The
    // FFmpeg -f wasapi -loopback 1 demuxer is only present on very recent
    // FFmpeg builds and tends to be flaky (silent failures, no default
    // device errors that don't surface). Using our own loopback gives us
    // reliable diagnostics and works on every FFmpeg build that can encode
    // AAC. Captured audio is merged in after the video pipe is closed.
    let mut loopback_capture: Option<LoopbackCapture> = None;
    if opts.capture_system_audio {
        let raw_audio_path = output_path.with_extension("sysaudio.f32le");
        match start_wasapi_loopback_capture(raw_audio_path, Arc::clone(&stop)) {
            Ok(cap) => {
                tracing::info!("system audio (WASAPI loopback) capture initialized");
                loopback_capture = Some(cap);
            }
            Err(e) => {
                tracing::warn!(%e, "WASAPI loopback capture failed to initialize");
                let _ = app_warn(
                    &app,
                    &format!(
                        "System audio could not be initialized ({e}). Recording without system audio."
                    ),
                );
            }
        }
    }

    let mut mic_capture: Option<LoopbackCapture> = None;
    if opts.capture_microphone {
        if let Some(ref device_name) = opts.microphone_device_id {
            let raw_audio_path = output_path.with_extension("micaudio.f32le");
            match start_wasapi_mic_capture(device_name.clone(), raw_audio_path, Arc::clone(&stop)) {
                Ok(cap) => {
                    tracing::info!("microphone (WASAPI capture) initialized: {}", device_name);
                    mic_capture = Some(cap);
                }
                Err(e) => {
                    tracing::warn!(%e, "WASAPI microphone capture failed to initialize");
                    let _ = app_warn(
                        &app,
                        &format!(
                            "Microphone could not be initialized ({e}). Recording without microphone."
                        ),
                    );
                }
            }
        }
    }

    let pipe: Arc<Mutex<Option<PipeState>>> = Arc::new(Mutex::new(None));

    let flags = OmniFlags {
        stop: Arc::clone(&stop),
        app: app.clone(),
        recording_id,
        ffmpeg_exe: ffmpeg_exe.clone(),
        output_path: output_path.clone(),
        fps,
        microphone_device: None, // We capture microphone via WASAPI natively, so we don't pass it to FFmpeg DirectShow
        pipe: Arc::clone(&pipe),
    };

    // Some Windows Graphics Capture environments reject custom minimum update
    // intervals; keep the default to avoid startup failures on those platforms.
    let min_interval = MinimumUpdateIntervalSettings::Default;

    let capture_done: Result<(), String> = (|| {
        let capture_control = match target {
            CaptureTarget::Monitor(monitor) => {
                let settings = Settings::new(
                    monitor,
                    CursorCaptureSettings::WithCursor,
                    DrawBorderSettings::Default,
                    SecondaryWindowSettings::Default,
                    min_interval,
                    DirtyRegionSettings::Default,
                    ColorFormat::Bgra8,
                    flags.clone(),
                );
                OmniRecorder::start_free_threaded(settings).map_err(|e| format!("{e}"))?
            }
            CaptureTarget::Window(window) => {
                let settings = Settings::new(
                    window,
                    CursorCaptureSettings::WithCursor,
                    DrawBorderSettings::Default,
                    SecondaryWindowSettings::Default,
                    min_interval,
                    DirtyRegionSettings::Default,
                    ColorFormat::Bgra8,
                    flags.clone(),
                );
                OmniRecorder::start_free_threaded(settings).map_err(|e| format!("{e}"))?
            }
        };

        // Poll the stop flag so we can force-quit WGC even when frames stop
        // arriving (e.g. after opening another always-on-top overlay).
        while !stop.load(Ordering::SeqCst) && !capture_control.is_finished() {
            thread::sleep(Duration::from_millis(50));
        }

        // Watchdog: if the capture thread refuses to exit within 3s, kill the
        // FFmpeg child so the writer pipe closes and WGC unblocks.
        let watchdog_pipe = Arc::clone(&pipe);
        let watchdog = thread::spawn(move || {
            thread::sleep(Duration::from_secs(3));
            if let Ok(mut g) = watchdog_pipe.lock() {
                if let Some(ps) = g.as_mut() {
                    if let Some(w) = ps.writer.take() {
                        drop(w);
                    }
                    let _ = ps.child.kill();
                }
            }
        });

        let join_result = if capture_control.is_finished() {
            capture_control
                .wait()
                .map_err(|e| format!("wait for screen capture: {e}"))
        } else {
            capture_control
                .stop()
                .map_err(|e| format!("stop screen capture: {e}"))
        };

        // Watchdog still running? It will fire harmlessly after the timeout;
        // we don't join it because doing so would re-introduce a blocking wait.
        // Detach it explicitly to make intent clear.
        let _ = watchdog;

        join_result
    })();

    // Helper: when we bail early, also tear down the loopback worker and
    // remove the temp .f32le file so we don't leave noise on disk.
    let abort_captures = |cap1: Option<LoopbackCapture>, cap2: Option<LoopbackCapture>| {
        if let Some(cap) = cap1 {
            let raw = cap.path().to_path_buf();
            let _ = cap.join();
            let _ = std::fs::remove_file(raw);
        }
        if let Some(cap) = cap2 {
            let raw = cap.path().to_path_buf();
            let _ = cap.join();
            let _ = std::fs::remove_file(raw);
        }
    };

    if capture_done.is_err() {
        if let Ok(mut g) = pipe.lock() {
            if let Some(ps) = g.take() {
                kill_pipe_state(ps);
            }
        }
        abort_captures(loopback_capture, mic_capture);
        return Err(capture_done.unwrap_err());
    }

    let Some(mut ps) = pipe
        .lock()
        .map_err(|e| e.to_string())?
        .take()
    else {
        abort_captures(loopback_capture, mic_capture);
        return Err(
            "recording produced no video frames (FFmpeg was never started). Try another source or check capture permissions.".into(),
        );
    };

    if let Some(mut w) = ps.writer.take() {
        let _ = w.flush();
        drop(w);
    }

    // Wait up to 10s for FFmpeg to finalize the muxer; otherwise force-kill.
    let status = wait_with_timeout(&mut ps.child, Duration::from_secs(10))
        .map_err(|e| format!("waiting for FFmpeg: {e}"))?;
    let stderr = ps
        .stderr_join
        .join()
        .unwrap_or_else(|_| "FFmpeg stderr reader thread panicked".into());

    if !status.success() {
        abort_captures(loopback_capture, mic_capture);
        if stderr.is_empty() {
            return Err(format!("FFmpeg exited with {status}"));
        }
        return Err(format!("FFmpeg exited with {status}: {stderr}"));
    }

    let mut sys_audio_file: Option<PathBuf> = None;
    if let Some(cap) = loopback_capture {
        let raw_path = cap.path().to_path_buf();
        match cap.join() {
            Ok(()) => {
                sys_audio_file = Some(raw_path);
            }
            Err(e) => {
                tracing::warn!(%e, "WASAPI loopback capture worker failed");
                let _ = app_warn(
                    &app,
                    &format!("System audio capture stopped early: {e}"),
                );
                let _ = std::fs::remove_file(raw_path);
            }
        }
    }

    let mut mic_audio_file: Option<PathBuf> = None;
    if let Some(cap) = mic_capture {
        let raw_path = cap.path().to_path_buf();
        match cap.join() {
            Ok(()) => {
                mic_audio_file = Some(raw_path);
            }
            Err(e) => {
                tracing::warn!(%e, "WASAPI microphone capture worker failed");
                let _ = app_warn(
                    &app,
                    &format!("Microphone capture stopped early: {e}"),
                );
                let _ = std::fs::remove_file(raw_path);
            }
        }
    }

    if sys_audio_file.is_some() || mic_audio_file.is_some() {
        match merge_audio_tracks(
            &ffmpeg_exe,
            &output_path,
            sys_audio_file.as_deref(),
            mic_audio_file.as_deref(),
        ) {
            Ok(MergeOutcome::Merged) => {
                tracing::info!("audio tracks merged into final MP4");
            }
            Ok(MergeOutcome::Skipped(reason)) => {
                tracing::warn!(reason, "audio merge skipped");
                let _ = app_warn(
                    &app,
                    &format!("Audio was not added to the recording: {reason}"),
                );
            }
            Err(e) => {
                tracing::error!(%e, "merge audio tracks failed");
                let _ = app_warn(
                    &app,
                    &format!("Failed to mux audio into the recording: {e}"),
                );
            }
        }
    }

    Ok(())
}

struct LoopbackCapture {
    path: PathBuf,
    worker: JoinHandle<Result<(), String>>,
}

impl LoopbackCapture {
    fn path(&self) -> &Path {
        self.path.as_path()
    }
    fn join(self) -> Result<(), String> {
        self.worker
            .join()
            .map_err(|_| "WASAPI capture thread panicked".to_string())?
    }
}

/// Starts capturing the default render device as loopback to a raw f32le file.
///
/// WASAPI initialization runs **synchronously** on the caller so a failure
/// (no audio device, format unsupported, etc.) is reported before recording
/// begins; only the read loop is moved to a background thread.
fn start_wasapi_loopback_capture(
    path: PathBuf,
    stop: Arc<AtomicBool>,
) -> Result<LoopbackCapture, String> {
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let path_clone = path.clone();
    let worker = std::thread::spawn(move || {
        let result = capture_system_audio_loopback(path_clone, stop, tx);
        if let Err(e) = &result {
            tracing::warn!(%e, "WASAPI loopback capture thread exited with error");
        }
        result
    });

    // Block until the loop signals "initialized OK" (or returns an init error).
    // 5 s is plenty for COM/WASAPI to come up; longer than that is a real issue.
    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(LoopbackCapture { path, worker }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("WASAPI loopback initialization timed out".into()),
    }
}

fn capture_system_audio_loopback(
    path: PathBuf,
    stop: Arc<AtomicBool>,
    init_tx: std::sync::mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    // Helper that reports init failure to the caller AND returns Err so the
    // join handle also surfaces the message.
    macro_rules! init_err {
        ($e:expr) => {{
            let msg: String = $e;
            let _ = init_tx.send(Err(msg.clone()));
            return Err(msg);
        }};
    }

    if let Err(e) = initialize_mta().ok() {
        init_err!(format!("initialize WASAPI MTA: {e}"));
    }

    let desired_format = WaveFormat::new(32, 32, &SampleType::Float, 48_000, 2, None);
    let enumerator = match DeviceEnumerator::new() {
        Ok(v) => v,
        Err(e) => init_err!(format!("create WASAPI device enumerator: {e}")),
    };
    let device = match enumerator.get_default_device(&Direction::Render) {
        Ok(v) => v,
        Err(e) => init_err!(format!(
            "get default render (speakers) device: {e}. Is an audio output device connected?"
        )),
    };
    let mut audio_client = match device.get_iaudioclient() {
        Ok(v) => v,
        Err(e) => init_err!(format!("create WASAPI audio client: {e}")),
    };
    let (_default_period, min_period) = match audio_client.get_device_period() {
        Ok(v) => v,
        Err(e) => init_err!(format!("WASAPI get device period: {e}")),
    };
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: min_period,
    };
    if let Err(e) =
        audio_client.initialize_client(&desired_format, &Direction::Capture, &mode)
    {
        init_err!(format!(
            "initialize WASAPI loopback client (48 kHz stereo float): {e}"
        ));
    }
    let h_event = match audio_client.set_get_eventhandle() {
        Ok(v) => v,
        Err(e) => init_err!(format!("WASAPI event handle: {e}")),
    };
    let capture_client = match audio_client.get_audiocaptureclient() {
        Ok(v) => v,
        Err(e) => init_err!(format!("WASAPI capture client: {e}")),
    };

    let file = match std::fs::File::create(&path) {
        Ok(f) => f,
        Err(e) => init_err!(format!("create loopback file {}: {e}", path.display())),
    };
    let mut out = std::io::BufWriter::new(file);
    let blockalign = desired_format.get_blockalign() as usize;
    let mut queue = std::collections::VecDeque::<u8>::new();

    if let Err(e) = audio_client.start_stream() {
        init_err!(format!("start WASAPI stream: {e}"));
    }

    // Initialization complete — unblock the caller.
    let _ = init_tx.send(Ok(()));

    let mut total_bytes: u64 = 0;
    while !stop.load(Ordering::SeqCst) {
        let new_frames = capture_client
            .get_next_packet_size()
            .map_err(|e| format!("WASAPI packet size: {e}"))?
            .unwrap_or(0);
        if new_frames > 0 {
            let needed = new_frames as usize * blockalign;
            let additional = needed.saturating_sub(queue.capacity().saturating_sub(queue.len()));
            queue.reserve(additional);
            capture_client
                .read_from_device_to_deque(&mut queue)
                .map_err(|e| format!("WASAPI read: {e}"))?;
            if !queue.is_empty() {
                let mut chunk = vec![0_u8; queue.len()];
                for b in &mut chunk {
                    *b = queue.pop_front().unwrap_or(0);
                }
                out.write_all(&chunk)
                    .map_err(|e| format!("write loopback file: {e}"))?;
                total_bytes = total_bytes.saturating_add(chunk.len() as u64);
            }
        }
        // 100 ms ceiling so we notice the stop flag promptly even when the
        // device goes silent (was previously 100_000 ms = 100 seconds, which
        // caused the recorder to hang at end-of-session waiting for WASAPI to
        // wake up).
        let _ = h_event.wait_for_event(100);
    }
    while let Some(v) = queue.pop_front() {
        out.write_all(&[v])
            .map_err(|e| format!("flush loopback queue: {e}"))?;
        total_bytes = total_bytes.saturating_add(1);
    }
    out.flush()
        .map_err(|e| format!("flush loopback file: {e}"))?;
    let _ = audio_client.stop_stream();
    tracing::info!(bytes = total_bytes, "WASAPI loopback capture stopped");
    Ok(())
}

enum MergeOutcome {
    Merged,
    Skipped(&'static str),
}

fn start_wasapi_mic_capture(
    device_name: String,
    path: PathBuf,
    stop: Arc<AtomicBool>,
) -> Result<LoopbackCapture, String> {
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let path_clone = path.clone();
    let stop_clone = Arc::clone(&stop);
    let worker = std::thread::spawn(move || {
        let result = capture_wasapi_mic(device_name, path_clone, stop_clone, tx);
        if let Err(e) = &result {
            tracing::warn!(%e, "WASAPI mic capture thread exited with error");
        }
        result
    });

    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(LoopbackCapture { path, worker }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("WASAPI mic initialization timed out".into()),
    }
}

fn capture_wasapi_mic(
    device_name: String,
    path: PathBuf,
    stop: Arc<AtomicBool>,
    init_tx: std::sync::mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    macro_rules! init_err {
        ($e:expr) => {{
            let msg: String = $e;
            let _ = init_tx.send(Err(msg.clone()));
            return Err(msg);
        }};
    }

    if let Err(e) = initialize_mta().ok() {
        init_err!(format!("initialize WASAPI MTA: {e}"));
    }

    let desired_format = WaveFormat::new(32, 32, &SampleType::Float, 48_000, 2, None);
    let enumerator = match DeviceEnumerator::new() {
        Ok(v) => v,
        Err(e) => init_err!(format!("create WASAPI device enumerator: {e}")),
    };
    
    let device = match find_capture_device_by_name(&enumerator, &device_name) {
        Ok(v) => v,
        Err(e) => init_err!(format!(
            "get capture device '{device_name}': {e}"
        )),
    };

    let mut audio_client = match device.get_iaudioclient() {
        Ok(v) => v,
        Err(e) => init_err!(format!("create WASAPI audio client: {e}")),
    };
    let (_default_period, min_period) = match audio_client.get_device_period() {
        Ok(v) => v,
        Err(e) => init_err!(format!("WASAPI get device period: {e}")),
    };
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: min_period,
    };
    if let Err(e) =
        audio_client.initialize_client(&desired_format, &Direction::Capture, &mode)
    {
        init_err!(format!(
            "initialize WASAPI capture client (48 kHz stereo float): {e}"
        ));
    }
    let h_event = match audio_client.set_get_eventhandle() {
        Ok(v) => v,
        Err(e) => init_err!(format!("WASAPI event handle: {e}")),
    };
    let capture_client = match audio_client.get_audiocaptureclient() {
        Ok(v) => v,
        Err(e) => init_err!(format!("WASAPI capture client: {e}")),
    };

    let file = match std::fs::File::create(&path) {
        Ok(f) => f,
        Err(e) => init_err!(format!("create microphone capture file {}: {e}", path.display())),
    };
    let mut out = std::io::BufWriter::new(file);
    let blockalign = desired_format.get_blockalign() as usize;
    let mut queue = std::collections::VecDeque::<u8>::new();

    if let Err(e) = audio_client.start_stream() {
        init_err!(format!("start WASAPI stream: {e}"));
    }

    let _ = init_tx.send(Ok(()));

    let mut total_bytes: u64 = 0;
    while !stop.load(Ordering::SeqCst) {
        let new_frames = capture_client
            .get_next_packet_size()
            .map_err(|e| format!("WASAPI packet size: {e}"))?
            .unwrap_or(0);
        if new_frames > 0 {
            let needed = new_frames as usize * blockalign;
            let additional = needed.saturating_sub(queue.capacity().saturating_sub(queue.len()));
            queue.reserve(additional);
            capture_client
                .read_from_device_to_deque(&mut queue)
                .map_err(|e| format!("WASAPI read: {e}"))?;
            if !queue.is_empty() {
                let mut chunk = vec![0_u8; queue.len()];
                for b in &mut chunk {
                    *b = queue.pop_front().unwrap_or(0);
                }
                out.write_all(&chunk)
                    .map_err(|e| format!("write capture file: {e}"))?;
                total_bytes = total_bytes.saturating_add(chunk.len() as u64);
            }
        }
        let _ = h_event.wait_for_event(100);
    }
    while let Some(v) = queue.pop_front() {
        out.write_all(&[v])
            .map_err(|e| format!("flush capture queue: {e}"))?;
        total_bytes = total_bytes.saturating_add(1);
    }
    out.flush()
        .map_err(|e| format!("flush capture file: {e}"))?;
    let _ = audio_client.stop_stream();
    tracing::info!(bytes = total_bytes, "WASAPI microphone capture stopped");
    Ok(())
}

fn find_capture_device_by_name(
    enumerator: &DeviceEnumerator,
    device_name: &str,
) -> Result<wasapi::Device, String> {
    let collection = enumerator
        .get_device_collection(&Direction::Capture)
        .map_err(|e| format!("get capture device collection: {e}"))?;
    let count = collection
        .get_nbr_devices()
        .map_err(|e| format!("get device count: {e}"))?;
    for i in 0..count {
        if let Ok(dev) = collection.get_device_at_index(i) {
            if let Ok(friendly) = dev.get_friendlyname() {
                if friendly == device_name {
                    return Ok(dev);
                }
            }
        }
    }
    enumerator
        .get_default_device(&Direction::Capture)
        .map_err(|e| format!("get default capture device: {e}"))
}

fn merge_audio_tracks(
    ffmpeg_exe: &Path,
    video_path: &Path,
    sys_audio_path: Option<&Path>,
    mic_audio_path: Option<&Path>,
) -> Result<MergeOutcome, String> {
    let has_sys = sys_audio_path.map(|p| p.is_file()).unwrap_or(false);
    let has_mic = mic_audio_path.map(|p| p.is_file()).unwrap_or(false);

    if !has_sys && !has_mic {
        return Ok(MergeOutcome::Skipped("no audio files exist to merge"));
    }

    let sys_ok = if has_sys {
        let p = sys_audio_path.unwrap();
        let meta = std::fs::metadata(p).map_err(|e| format!("{e}"))?;
        if meta.len() < 4096 {
            let _ = std::fs::remove_file(p);
            false
        } else {
            true
        }
    } else {
        false
    };

    let mic_ok = if has_mic {
        let p = mic_audio_path.unwrap();
        let meta = std::fs::metadata(p).map_err(|e| format!("{e}"))?;
        if meta.len() < 4096 {
            let _ = std::fs::remove_file(p);
            false
        } else {
            true
        }
    } else {
        false
    };

    if !sys_ok && !mic_ok {
        return Ok(MergeOutcome::Skipped("audio files were too short or empty"));
    }

    let merged = video_path.with_extension("merged.mp4");
    let mut cmd = crate::ffmpeg::spawn_ffmpeg_command(ffmpeg_exe);
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-y")
        .arg("-i")
        .arg(video_path);

    if sys_ok {
        cmd.arg("-f")
            .arg("f32le")
            .arg("-ar")
            .arg("48000")
            .arg("-ac")
            .arg("2")
            .arg("-i")
            .arg(sys_audio_path.unwrap());
    }

    if mic_ok {
        cmd.arg("-f")
            .arg("f32le")
            .arg("-ar")
            .arg("48000")
            .arg("-ac")
            .arg("2")
            .arg("-i")
            .arg(mic_audio_path.unwrap());
    }

    if sys_ok && mic_ok {
        cmd.arg("-filter_complex")
            .arg("[1:a:0][2:a:0]amix=inputs=2:normalize=0:duration=longest[aout]")
            .arg("-map")
            .arg("0:v:0")
            .arg("-map")
            .arg("[aout]");
    } else {
        cmd.arg("-map").arg("0:v:0").arg("-map").arg("1:a:0");
    }

    cmd.arg("-c:v")
        .arg("copy")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg("192k")
        .arg("-movflags")
        .arg("+faststart")
        .arg(&merged)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let out = cmd
        .output()
        .map_err(|e| format!("merge audio tracks spawn: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "merge audio tracks failed ({}): {}",
            out.status,
            stderr.trim()
        ));
    }
    let _ = std::fs::remove_file(video_path);
    std::fs::rename(&merged, video_path).map_err(|e| format!("replace merged output: {e}"))?;
    
    if sys_ok {
        let _ = std::fs::remove_file(sys_audio_path.unwrap());
    }
    if mic_ok {
        let _ = std::fs::remove_file(mic_audio_path.unwrap());
    }
    
    Ok(MergeOutcome::Merged)
}


#[derive(Clone)]
struct OmniFlags {
    stop: Arc<AtomicBool>,
    app: AppHandle,
    recording_id: String,
    ffmpeg_exe: PathBuf,
    output_path: PathBuf,
    fps: u32,
    microphone_device: Option<String>,
    pipe: Arc<Mutex<Option<PipeState>>>,
}

struct PipeState {
    width: u32,
    height: u32,
    writer: Option<BufWriter<std::process::ChildStdin>>,
    child: Child,
    stderr_join: JoinHandle<String>,
}

fn kill_pipe_state(ps: PipeState) {
    let _ = shutdown_pipe_collect_stderr(ps);
}

/// Poll-wait for a child process to exit; force-kill if the timeout passes.
fn wait_with_timeout(
    child: &mut Child,
    timeout: Duration,
) -> std::io::Result<std::process::ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait()? {
            Some(status) => return Ok(status),
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    return child.wait();
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// Close stdin, stop FFmpeg, and return captured stderr (for error messages).
fn shutdown_pipe_collect_stderr(mut ps: PipeState) -> String {
    let _ = ps.writer.take().map(|mut w| {
        let _ = w.flush();
    });
    let _ = ps.child.kill();
    let _ = ps.child.wait();
    ps.stderr_join.join().unwrap_or_default()
}

struct OmniRecorder {
    flags: OmniFlags,
    started: Instant,
    frames_captured: u64,
    last_emit: Instant,
}

impl GraphicsCaptureApiHandler for OmniRecorder {
    type Flags = OmniFlags;
    type Error = String;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let flags = ctx.flags;
        tracing::debug!(id = %flags.recording_id, "WGC capture session created");
        Ok(Self {
            flags,
            started: Instant::now(),
            frames_captured: 0,
            last_emit: Instant::now(),
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if self.flags.stop.load(Ordering::SeqCst) {
            let mut guard = self.flags.pipe.lock().map_err(|e| e.to_string())?;
            if let Some(ps) = guard.as_mut() {
                if let Some(mut w) = ps.writer.take() {
                    w.flush().map_err(|e| e.to_string())?;
                    drop(w);
                }
            }
            capture_control.stop();
            return Ok(());
        }

        let fw = frame.width();
        let fh = frame.height();
        if fw == 0 || fh == 0 {
            return Err("capture returned a zero-sized frame".into());
        }
        ensure_ffmpeg_for_frame(&self.flags, fw, fh)?;

        let mut buf = frame.buffer().map_err(|e| e.to_string())?;
        let pixels = buf.as_nopadding_buffer().map_err(|e| e.to_string())?;

        let write_result = {
            let mut guard = self.flags.pipe.lock().map_err(|e| e.to_string())?;
            let ps = guard
                .as_mut()
                .ok_or_else(|| "FFmpeg stdin is not initialized".to_string())?;
            let w = ps
                .writer
                .as_mut()
                .ok_or_else(|| "FFmpeg stdin is already closed".to_string())?;
            w.write_all(pixels)
        };

        if let Err(e) = write_result {
            let is_pipe = matches!(e.kind(), std::io::ErrorKind::BrokenPipe)
                || e.raw_os_error() == Some(232);
            let stderr = self
                .flags
                .pipe
                .lock()
                .map_err(|e| e.to_string())?
                .take()
                .map(shutdown_pipe_collect_stderr)
                .unwrap_or_default();
            return Err(if is_pipe {
                ffmpeg_pipe_closed_message(e, stderr)
            } else {
                e.to_string()
            });
        }

        self.frames_captured += 1;
        if self.last_emit.elapsed().as_millis() >= 250 {
            self.last_emit = Instant::now();
            let payload = Progress {
                id: self.flags.recording_id.clone(),
                elapsed_ms: self.started.elapsed().as_millis() as u64,
                frames_captured: self.frames_captured,
            };
            let _ = self.flags.app.emit(EVT_PROGRESS, payload);
        }

        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        tracing::info!(id = %self.flags.recording_id, "WGC capture item closed");
        self.flags.stop.store(true, Ordering::SeqCst);
        Ok(())
    }
}

fn resolve_target(source_id: &str) -> Result<CaptureTarget, String> {
    if let Some(rest) = source_id.strip_prefix("display-") {
        let idx: usize = rest.parse().map_err(|_| "invalid display id")?;
        let m = Monitor::from_index(idx).map_err(|e| e.to_string())?;
        return Ok(CaptureTarget::Monitor(m));
    }
    if let Some(rest) = source_id.strip_prefix("window-") {
        let addr = usize::from_str_radix(rest, 16)
            .or_else(|_| rest.parse::<usize>())
            .map_err(|_| "invalid window id")?;
        let w = Window::from_raw_hwnd(addr as *mut std::ffi::c_void);
        return Ok(CaptureTarget::Window(w));
    }
    Err(format!("unknown source id: {source_id}"))
}

fn target_dimensions(target: &CaptureTarget) -> Result<(u32, u32), String> {
    match target {
        CaptureTarget::Monitor(m) => Ok((
            m.width().map_err(|e| e.to_string())?,
            m.height().map_err(|e| e.to_string())?,
        )),
        CaptureTarget::Window(w) => {
            let r = w.rect().map_err(|e| e.to_string())?;
            Ok((
                (r.right - r.left).max(0) as u32,
                (r.bottom - r.top).max(0) as u32,
            ))
        }
    }
}

fn ensure_ffmpeg_for_frame(flags: &OmniFlags, fw: u32, fh: u32) -> Result<(), String> {
    let mut guard = flags.pipe.lock().map_err(|e| e.to_string())?;
    if let Some(ps) = guard.as_ref() {
        if ps.width != fw || ps.height != fh {
            return Err(format!(
                "capture dimensions changed mid-recording (locked {}×{}, frame {}×{})",
                ps.width, ps.height, fw, fh
            ));
        }
        return Ok(());
    }

    let (mut child, stderr_join) = spawn_ffmpeg(
        flags.ffmpeg_exe.as_path(),
        &flags.output_path,
        fw,
        fh,
        flags.fps,
        flags.microphone_device.as_deref(),
    )?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "failed to open FFmpeg stdin".to_string())?;
    let cap = (fw as usize)
        .saturating_mul(fh as usize)
        .saturating_mul(4)
        .max(65_536);

    *guard = Some(PipeState {
        width: fw,
        height: fh,
        writer: Some(BufWriter::with_capacity(cap, stdin)),
        child,
        stderr_join,
    });

    tracing::debug!(
        id = %flags.recording_id,
        fw,
        fh,
        "started FFmpeg using dimensions from first captured frame"
    );

    Ok(())
}

fn app_warn(app: &AppHandle, message: &str) -> Result<(), String> {
    app.emit(EVT_WARN, message.to_string())
        .map_err(|e| e.to_string())
}

fn ffmpeg_pipe_closed_message(e: std::io::Error, stderr: String) -> String {
    let mut msg = format!("FFmpeg closed the video pipe ({e}).");
    if stderr.trim().is_empty() {
        msg.push_str(
            " No stderr was captured (try turning off system audio, run `npm run setup:ffmpeg`, or use a full FFmpeg build with libx264).",
        );
    } else {
        msg.push_str("\n--- FFmpeg stderr ---\n");
        msg.push_str(stderr.trim());
    }
    msg
}

/// Spawns the main FFmpeg encoding process.
///
/// FFmpeg only handles two streams here: the BGRA video pipe and, optionally,
/// the microphone via DirectShow. System audio is captured by the Rust WASAPI
/// loopback path (`start_wasapi_loopback_capture`) and merged in afterwards
/// — that way we have a single, reliable code path that works across every
/// supported FFmpeg build.
fn spawn_ffmpeg(
    ffmpeg_exe: &Path,
    output: &Path,
    width: u32,
    height: u32,
    fps: u32,
    microphone_device: Option<&str>,
) -> Result<(Child, JoinHandle<String>), String> {
    let size = format!("{width}x{height}");
    let fps_s = fps.to_string();

    let mut cmd = crate::ffmpeg::spawn_ffmpeg_command(ffmpeg_exe);
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-y");

    cmd.arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("bgra")
        .arg("-s")
        .arg(&size)
        .arg("-r")
        .arg(&fps_s)
        .arg("-thread_queue_size")
        .arg("512")
        .arg("-i")
        .arg("pipe:0");

    let has_mic = microphone_device.is_some();
    if let Some(device) = microphone_device {
        cmd.arg("-f")
            .arg("dshow")
            .arg("-thread_queue_size")
            .arg("1024")
            .arg("-rtbufsize")
            .arg("64M")
            .arg("-i")
            .arg(format!("audio={device}"));
    }

    cmd.arg("-vf").arg("crop=trunc(iw/2)*2:trunc(ih/2)*2");

    cmd.arg("-map").arg("0:v:0");
    if has_mic {
        cmd.arg("-map")
            .arg("1:a:0")
            .arg("-c:a")
            .arg("aac")
            .arg("-b:a")
            .arg("192k")
            // -shortest ensures the muxer finalizes promptly when the video
            // pipe (stdin) closes; the microphone input never reaches EOF on
            // its own so we'd hang otherwise.
            .arg("-shortest");
    }

    cmd.arg("-c:v")
        .arg("libx264")
        .arg("-preset")
        .arg("ultrafast")
        .arg("-tune")
        .arg("zerolatency")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-crf")
        .arg("20")
        .arg("-movflags")
        .arg("+faststart")
        .arg(output)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn FFmpeg ({e}). Is it installed and on PATH?"))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to open FFmpeg stderr".to_string())?;

    let stderr_join = thread::spawn(move || drain_stderr(stderr));

    Ok((child, stderr_join))
}

fn first_quoted_segment(line: &str) -> Option<&str> {
    let start = line.find('"')?;
    let rest = &line[start + 1..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn drain_stderr(mut stderr: std::process::ChildStderr) -> String {
    let mut buf = Vec::new();
    if stderr.read_to_end(&mut buf).is_err() {
        return String::new();
    }
    let s = String::from_utf8_lossy(&buf).trim().to_string();
    if s.chars().count() > 12_000 {
        s.chars()
            .rev()
            .take(12_000)
            .collect::<Vec<char>>()
            .into_iter()
            .rev()
            .collect::<String>()
    } else {
        s
    }
}
