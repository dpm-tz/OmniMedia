import { useCallback, useEffect, useRef, useState } from "react";
import { Mic, MicOff, X, Camera as CameraIcon, RefreshCw } from "lucide-react";
import { api } from "../../lib/bridge";

type Status = "loading" | "ready" | "error";

interface CameraError {
  message: string;
  name: string;
  hint?: string;
}

function describeError(err: unknown): CameraError {
  const name = err instanceof DOMException ? err.name : (err as Error)?.name ?? "";
  const raw = (err as Error)?.message ?? String(err);
  switch (name) {
    case "NotFoundError":
    case "DevicesNotFoundError":
      return {
        name,
        message: "No camera found.",
        hint: "Check that a webcam is connected and detected by Windows.",
      };
    case "NotAllowedError":
    case "PermissionDeniedError":
      return {
        name,
        message: "Camera access was denied.",
        hint:
          "Open Settings > Privacy & security > Camera. Turn on 'Camera access' AND 'Let desktop apps access your camera'.",
      };
    case "NotReadableError":
    case "TrackStartError":
      return {
        name,
        message: "Camera is in use by another app.",
        hint: "Close Zoom, Teams, browser tabs, or any other app that uses the camera.",
      };
    case "OverconstrainedError":
      return {
        name,
        message: "Camera does not support the requested settings.",
      };
    case "":
      return { name: "Error", message: raw };
    default:
      return { name, message: raw };
  }
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

function withTimeout<T>(promise: Promise<T>, ms: number, label: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const t = setTimeout(() => reject(new Error(`${label} timed out after ${ms}ms`)), ms);
    promise.then(
      (v) => {
        clearTimeout(t);
        resolve(v);
      },
      (e) => {
        clearTimeout(t);
        reject(e);
      },
    );
  });
}

async function getCameraStreamWithRetry(): Promise<MediaStream> {
  // Permission state propagates asynchronously after the Rust backend installs
  // the WebView2 handler. Retry transient denials.
  const delays = [0, 250, 700, 1500];
  let lastError: unknown;
  for (const delay of delays) {
    if (delay > 0) await sleep(delay);
    try {
      await api.webcam?.prepare?.().catch(() => {});
      return await withTimeout(
        navigator.mediaDevices.getUserMedia({
          video: { width: { ideal: 640 }, height: { ideal: 480 } },
          audio: false,
        }),
        4000,
        "getUserMedia",
      );
    } catch (e) {
      lastError = e;
      const name = e instanceof DOMException ? e.name : "";
      if (
        name !== "NotAllowedError" &&
        name !== "PermissionDeniedError" &&
        !(e instanceof Error && /timed out/.test(e.message))
      ) {
        throw e;
      }
    }
  }
  throw lastError ?? new Error("getUserMedia failed");
}

export function WebcamOverlay() {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const [status, setStatus] = useState<Status>("loading");
  const [error, setError] = useState<CameraError | null>(null);
  const [muted, setMuted] = useState(true);
  const [shape, setShape] = useState<"circle" | "rect">("circle");

  const stopStream = useCallback(() => {
    const s = streamRef.current;
    if (s) {
      s.getTracks().forEach((t) => t.stop());
      streamRef.current = null;
    }
    if (videoRef.current) {
      videoRef.current.srcObject = null;
    }
  }, []);

  const start = useCallback(async () => {
    stopStream();
    setStatus("loading");
    setError(null);

    try {
      if (!navigator.mediaDevices?.getUserMedia) {
        throw new Error("Camera API is not available in this window.");
      }

      const stream = await getCameraStreamWithRetry();
      streamRef.current = stream;

      const video = videoRef.current;
      if (video) {
        video.srcObject = stream;
        video.muted = true;
        await video.play().catch(() => {});
      }
      setStatus("ready");
    } catch (e) {
      console.error("[webcam] failed to start camera", e);
      stopStream();
      setStatus("error");
      setError(describeError(e));
    }
  }, [stopStream]);

  useEffect(() => {
    void start();
    return stopStream;
  }, [start, stopStream]);

  useEffect(() => {
    const video = videoRef.current;
    const stream = streamRef.current;
    if (!video || !stream || video.srcObject === stream) return;
    video.srcObject = stream;
    video.muted = true;
    void video.play().catch(() => {});
  });

  return (
    <div
      className="group relative h-screen w-screen overflow-hidden rounded-[24px] border border-white/10 bg-black/80 backdrop-blur"
      data-tauri-drag-region
      style={{
        WebkitUserSelect: "none",
        cursor: "move",
        clipPath: shape === "circle" ? "ellipse(50% 50% at 50% 50%)" : undefined,
      }}
    >
      <video
        ref={videoRef}
        playsInline
        autoPlay
        muted={muted}
        className="h-full w-full object-cover"
        style={{ transform: "scaleX(-1)" }}
      />

      {status === "loading" && (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 px-3 text-center text-xs text-white/80">
          <RefreshCw className="h-5 w-5 animate-spin" />
          <span>Starting camera…</span>
        </div>
      )}

      {status === "error" && error && (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-1.5 p-3 text-center text-[11px] text-white/90">
          <CameraIcon className="h-5 w-5" />
          <span className="font-medium">{error.message}</span>
          {error.hint && <span className="text-white/60">{error.hint}</span>}
          <span className="text-white/40">[{error.name}]</span>
          <button
            type="button"
            className="mt-1 inline-flex items-center gap-1 rounded-md bg-white/10 px-2 py-1 hover:bg-white/20"
            onClick={() => void start()}
          >
            <RefreshCw className="h-3 w-3" /> Retry
          </button>
        </div>
      )}

      <div className="pointer-events-none absolute inset-x-0 bottom-1 flex items-center justify-center opacity-0 transition-opacity group-hover:opacity-100">
        <div className="pointer-events-auto flex items-center gap-1 rounded-full bg-black/60 px-2 py-1 text-white">
          <button
            type="button"
            className="rounded-full p-1.5 hover:bg-white/15"
            title={muted ? "Unmute preview" : "Mute preview"}
            onClick={() => setMuted((m) => !m)}
          >
            {muted ? <MicOff className="h-3.5 w-3.5" /> : <Mic className="h-3.5 w-3.5" />}
          </button>
          <button
            type="button"
            className="rounded-full p-1.5 hover:bg-white/15"
            title={shape === "circle" ? "Switch to rectangle" : "Switch to circle"}
            onClick={() => setShape((s) => (s === "circle" ? "rect" : "circle"))}
          >
            <CameraIcon className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            className="rounded-full p-1.5 hover:bg-red-500/40"
            title="Close webcam"
            onClick={() => api.webcam?.close?.()}
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
    </div>
  );
}
