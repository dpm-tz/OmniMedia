import { useEffect, useState } from "react";
import {
  Play,
  Square,
  RefreshCw,
  Camera as CameraIcon,
  Pencil,
  EyeOff,
  Webcam as WebcamIcon,
} from "lucide-react";
import { api, events } from "../../lib/bridge";
import type {
  AudioInputDevice,
  RecordingResult,
  RecordingSource,
  RecordingStatus,
  ScreenshotResult,
} from "../../lib/types";
import { cn } from "../../lib/cn";

export function RecorderPage() {
  const [sources, setSources] = useState<RecordingSource[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [fps, setFps] = useState(60);
  const [systemAudio, setSystemAudio] = useState(true);
  const [microphone, setMicrophone] = useState(false);
  const [audioInputs, setAudioInputs] = useState<AudioInputDevice[]>([]);
  const [selectedMicId, setSelectedMicId] = useState<string | null>(null);
  const [autoOverlay, setAutoOverlay] = useState(false);

  const [status, setStatus] = useState<RecordingStatus>({ state: "idle" });
  const [lastResult, setLastResult] = useState<RecordingResult | null>(null);
  const [lastShot, setLastShot] = useState<ScreenshotResult | null>(null);
  const [elapsedMs, setElapsedMs] = useState(0);
  const [framesCaptured, setFramesCaptured] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [warning, setWarning] = useState<string | null>(null);
  const [ffmpegOk, setFfmpegOk] = useState<boolean | null>(null);
  const [overlayOpen, setOverlayOpen] = useState(false);
  const [webcamOpen, setWebcamOpen] = useState(false);

  useEffect(() => {
    void refreshSources();
    void api.system
      .info()
      .then((i) => setFfmpegOk(i.ffmpegAvailable))
      .catch(() => setFfmpegOk(null));
    void api.overlay.isOpen().then(setOverlayOpen).catch(() => {});
    void api.webcam.isOpen().then(setWebcamOpen).catch(() => {});

    const unsubs: Array<Promise<() => void>> = [
      events.onRecorderProgress((e) => {
        setElapsedMs(e.elapsedMs);
        setFramesCaptured(e.framesCaptured);
      }),
      events.onRecorderStopped((result) => {
        setLastResult(result);
        setStatus({
          state: "stopped",
          id: result.id,
          outputPath: result.outputPath,
        });
      }),
      events.onRecorderError((msg) => setError(msg)),
      events.onRecorderWarn((msg) => setWarning(msg)),
    ];

    return () => {
      unsubs.forEach((p) => void p.then((fn) => fn()));
    };
  }, []);

  useEffect(() => {
    if (status.state !== "recording") return;

    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        void stop();
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [status.state]);

  async function refreshSources() {
    try {
      const list = await api.recorder.listSources();
      setSources(list);
      if (list.length && !selectedId) setSelectedId(list[0].id);
      const mics = await api.recorder.listAudioInputs();
      setAudioInputs(mics);
      if (mics.length && !selectedMicId) setSelectedMicId(mics[0].id);
    } catch (e) {
      setError((e as Error).message);
    }
  }

  async function start() {
    if (!selectedId) return;
    setError(null);
    setWarning(null);
    setLastResult(null);
    try {
      if (autoOverlay && !overlayOpen) {
        await api.overlay.open();
        setOverlayOpen(true);
      }
      // System audio and microphone can both be enabled — the backend
      // captures the WASAPI loopback in Rust and mixes it with the dshow
      // microphone track during the final mux pass.
      const next = await api.recorder.start({
        sourceId: selectedId,
        fps,
        captureSystemAudio: systemAudio,
        captureMicrophone: microphone,
        microphoneDeviceId: microphone ? (selectedMicId ?? undefined) : undefined,
      });
      setStatus(next);
    } catch (e) {
      setError((e as Error).message);
    }
  }

  async function stop() {
    try {
      const result = await api.recorder.stop();
      setLastResult(result);
      setStatus({
        state: "stopped",
        id: result.id,
        outputPath: result.outputPath,
      });
      if (autoOverlay) {
        await api.overlay.close().catch(() => {});
        setOverlayOpen(false);
      }
    } catch (e) {
      setError((e as Error).message);
    }
  }

  async function toggleOverlay() {
    try {
      if (overlayOpen) {
        await api.overlay.close();
        setOverlayOpen(false);
      } else {
        await api.overlay.open();
        setOverlayOpen(true);
      }
    } catch (e) {
      setError((e as Error).message);
    }
  }

  async function toggleWebcam() {
    try {
      if (webcamOpen) {
        await api.webcam.close();
        setWebcamOpen(false);
      } else {
        await api.webcam.open();
        setWebcamOpen(true);
      }
    } catch (e) {
      setError((e as Error).message);
    }
  }

  async function takeScreenshot() {
    setError(null);
    try {
      const r = await api.screenshot.capture({
        sourceId: selectedId ?? undefined,
      });
      setLastShot(r);
    } catch (e) {
      setError((e as Error).message);
    }
  }

  const isRecording = status.state === "recording";

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-6 p-8">
      <header>
        <h1 className="text-2xl font-semibold">Screen Recorder</h1>
        <p className="text-sm text-surface-200/60">
          Capture screen + audio. Heavy lifting runs in the Rust backend.
        </p>
      </header>

      {ffmpegOk === false && (
        <div className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm text-amber-100">
          <strong className="font-medium">FFmpeg not found.</strong> For development run{" "}
          <code className="rounded bg-surface-800 px-1">npm run setup:ffmpeg</code> once, or install
          FFmpeg on your PATH. Release installers bundle it automatically when you build with Tauri.{" "}
          <a
            className="text-accent-400 underline hover:text-accent-300"
            href="https://ffmpeg.org/download.html"
            target="_blank"
            rel="noreferrer"
          >
            Download
          </a>
        </div>
      )}

      <section className="rounded-lg border border-surface-800 bg-surface-900/40 p-5">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-sm font-medium">Source</h2>
          <button
            type="button"
            onClick={refreshSources}
            className="flex items-center gap-1 rounded-md px-2 py-1 text-xs text-surface-200/70 hover:bg-surface-800"
          >
            <RefreshCw className="h-3 w-3" />
            Refresh
          </button>
        </div>

        <select
          value={selectedId ?? ""}
          onChange={(e) => setSelectedId(e.target.value)}
          disabled={isRecording || sources.length === 0}
          className="w-full rounded-md border border-surface-700 bg-surface-950 px-3 py-2 text-sm focus:border-accent-500 focus:outline-none disabled:opacity-50"
        >
          {sources.length === 0 ? (
            <option value="">No sources detected (backend stub)</option>
          ) : (
            sources.map((s) => (
              <option key={s.id} value={s.id}>
                [{s.kind}] {s.label} — {s.width}×{s.height}
              </option>
            ))
          )}
        </select>

        <div className="mt-4 grid grid-cols-2 gap-4 text-sm">
          <label className="flex flex-col gap-1">
            <span className="text-surface-200/70">Frames per second</span>
            <input
              type="number"
              min={15}
              max={120}
              value={fps}
              disabled={isRecording}
              onChange={(e) => setFps(Number(e.target.value))}
              className="rounded-md border border-surface-700 bg-surface-950 px-3 py-2 focus:border-accent-500 focus:outline-none disabled:opacity-50"
            />
          </label>

          <div className="flex flex-col gap-2">
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={systemAudio}
                disabled={isRecording}
                onChange={(e) => setSystemAudio(e.target.checked)}
              />
              System audio
            </label>
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={microphone}
                disabled={isRecording}
                onChange={(e) => {
                  const on = e.target.checked;
                  setMicrophone(on);
                  if (on && !selectedMicId && audioInputs.length > 0) {
                    setSelectedMicId(audioInputs[0].id);
                  }
                }}
              />
              Microphone
            </label>
            {microphone && (
              <label className="flex flex-col gap-1">
                <span className="text-surface-200/70">Microphone device</span>
                <select
                  value={selectedMicId ?? ""}
                  onChange={(e) => setSelectedMicId(e.target.value)}
                  disabled={isRecording || audioInputs.length === 0}
                  className="rounded-md border border-surface-700 bg-surface-950 px-2 py-1 text-sm focus:border-accent-500 focus:outline-none disabled:opacity-50"
                >
                  {audioInputs.length === 0 ? (
                    <option value="">No microphone device found</option>
                  ) : (
                    audioInputs.map((d) => (
                      <option key={d.id} value={d.id}>
                        {d.label}
                      </option>
                    ))
                  )}
                </select>
              </label>
            )}
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={autoOverlay}
                disabled={isRecording}
                onChange={(e) => setAutoOverlay(e.target.checked)}
              />
              Show annotation overlay during recording
            </label>
          </div>
        </div>
      </section>

      <section className="flex items-center gap-3">
        <button
          type="button"
          onClick={start}
          disabled={isRecording || !selectedId}
          className={cn(
            "flex items-center gap-2 rounded-md px-4 py-2 text-sm font-medium",
            "bg-accent-500 text-surface-950 hover:bg-accent-400",
            "disabled:cursor-not-allowed disabled:opacity-40",
          )}
        >
          <Play className="h-4 w-4" />
          Start recording
        </button>
        <button
          type="button"
          onClick={stop}
          disabled={!isRecording}
          className={cn(
            "flex items-center gap-2 rounded-md border border-surface-700 px-4 py-2 text-sm font-medium",
            "hover:bg-surface-800",
            "disabled:cursor-not-allowed disabled:opacity-40",
          )}
        >
          <Square className="h-4 w-4" />
          Stop
        </button>

        <button
          type="button"
          onClick={toggleOverlay}
          className={cn(
            "flex items-center gap-2 rounded-md border border-surface-700 px-3 py-2 text-sm font-medium",
            "hover:bg-surface-800",
            overlayOpen && "border-accent-500 bg-accent-500/10 text-accent-200",
          )}
          title={overlayOpen ? "Hide overlay" : "Show overlay"}
        >
          {overlayOpen ? <EyeOff className="h-4 w-4" /> : <Pencil className="h-4 w-4" />}
          {overlayOpen ? "Overlay" : "Annotate"}
        </button>

        <button
          type="button"
          onClick={toggleWebcam}
          className={cn(
            "flex items-center gap-2 rounded-md border border-surface-700 px-3 py-2 text-sm font-medium",
            "hover:bg-surface-800",
            webcamOpen && "border-accent-500 bg-accent-500/10 text-accent-200",
          )}
          title={webcamOpen ? "Hide webcam" : "Show webcam"}
        >
          <WebcamIcon className="h-4 w-4" />
          Webcam
        </button>

        <button
          type="button"
          onClick={takeScreenshot}
          className="flex items-center gap-2 rounded-md border border-surface-700 px-3 py-2 text-sm font-medium hover:bg-surface-800"
          title="Capture screenshot of selected source"
        >
          <CameraIcon className="h-4 w-4" />
          Screenshot
        </button>

        {isRecording && (
          <div className="ml-auto flex items-center gap-2 text-sm">
            <span className="h-2 w-2 animate-pulse rounded-full bg-red-500" />
            <span className="font-mono">
              {(elapsedMs / 1000).toFixed(1)}s · {framesCaptured} frames
            </span>
            <span className="text-xs text-surface-200/50">Esc to stop</span>
          </div>
        )}
      </section>

      {warning && (
        <div className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm text-amber-100">
          {warning}
        </div>
      )}

      {error && (
        <div className="rounded-md border border-red-500/40 bg-red-500/10 p-3 text-sm text-red-300">
          {error}
        </div>
      )}

      {lastResult && (
        <div className="rounded-md border border-surface-800 bg-surface-900/40 p-4 text-sm">
          <div className="mb-1 font-medium">Last recording</div>
          <div className="font-mono text-xs text-surface-200/70">
            {lastResult.outputPath}
          </div>
          <div className="mt-1 text-xs text-surface-200/60">
            {(lastResult.durationMs / 1000).toFixed(1)}s ·{" "}
            {(lastResult.sizeBytes / 1024 / 1024).toFixed(2)} MB
          </div>
        </div>
      )}

      {lastShot && (
        <div className="rounded-md border border-surface-800 bg-surface-900/40 p-4 text-sm">
          <div className="mb-1 font-medium">Last screenshot</div>
          <div className="font-mono text-xs text-surface-200/70">
            {lastShot.path}
          </div>
          <div className="mt-1 text-xs text-surface-200/60">
            {lastShot.width}×{lastShot.height} ·{" "}
            {(lastShot.sizeBytes / 1024).toFixed(0)} KB
          </div>
        </div>
      )}
    </div>
  );
}
