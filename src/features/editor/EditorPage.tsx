import { useEffect, useRef, useState } from "react";
import { Scissors, Image as ImageIcon, FolderOpen, Play, Loader2 } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api } from "../../lib/bridge";
import type { EditResult } from "../../lib/types";
import { cn } from "../../lib/cn";

type Mode = "trim" | "gif";

export function EditorPage() {
  const [mode, setMode] = useState<Mode>("trim");
  const [inputPath, setInputPath] = useState<string | null>(null);
  const [duration, setDuration] = useState<number>(0);
  const [start, setStart] = useState<number>(0);
  const [end, setEnd] = useState<number>(0);
  const [streamCopy, setStreamCopy] = useState<boolean>(true);

  const [gifFps, setGifFps] = useState<number>(15);
  const [gifWidth, setGifWidth] = useState<number>(720);

  const [busy, setBusy] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<EditResult | null>(null);

  const videoRef = useRef<HTMLVideoElement | null>(null);
  const fileUrl = useFileUrl(inputPath);

  useEffect(() => {
    setStart(0);
    setEnd(0);
    setDuration(0);
    setResult(null);
    setError(null);
  }, [inputPath]);

  async function pickInput() {
    try {
      const picked = await openDialog({
        multiple: false,
        directory: false,
        filters: [
          {
            name: "Video",
            extensions: ["mp4", "mov", "mkv", "webm", "avi", "m4v"],
          },
        ],
      });
      if (typeof picked === "string") setInputPath(picked);
    } catch (e) {
      setError((e as Error).message);
    }
  }

  async function run() {
    if (!inputPath) return;
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      if (mode === "trim") {
        const r = await api.editor.trim({
          inputPath,
          startSeconds: start,
          endSeconds: end > 0 ? end : duration,
          streamCopy,
        });
        setResult(r);
      } else {
        const r = await api.editor.gif({
          inputPath,
          startSeconds: start || undefined,
          endSeconds: end > 0 ? end : undefined,
          fps: gifFps,
          width: gifWidth || undefined,
        });
        setResult(r);
      }
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-6 p-8">
      <header>
        <h1 className="text-2xl font-semibold">Video Editor</h1>
        <p className="text-sm text-surface-200/60">
          Trim a clip or convert it to a high-quality GIF. FFmpeg runs from the
          bundled binary.
        </p>
      </header>

      <section className="rounded-lg border border-surface-800 bg-surface-900/40 p-5">
        <div className="mb-3 flex items-center gap-3">
          <button
            type="button"
            onClick={() => setMode("trim")}
            className={cn(
              "flex items-center gap-2 rounded-md border border-surface-700 px-3 py-2 text-sm",
              mode === "trim" && "border-accent-500 bg-accent-500/10 text-accent-200",
            )}
          >
            <Scissors className="h-4 w-4" /> Trim / Cut
          </button>
          <button
            type="button"
            onClick={() => setMode("gif")}
            className={cn(
              "flex items-center gap-2 rounded-md border border-surface-700 px-3 py-2 text-sm",
              mode === "gif" && "border-accent-500 bg-accent-500/10 text-accent-200",
            )}
          >
            <ImageIcon className="h-4 w-4" /> Export GIF
          </button>

          <button
            type="button"
            onClick={pickInput}
            className="ml-auto flex items-center gap-2 rounded-md border border-surface-700 px-3 py-2 text-sm hover:bg-surface-800"
          >
            <FolderOpen className="h-4 w-4" /> Open video…
          </button>
        </div>

        {inputPath ? (
          <div className="space-y-4">
            <div className="overflow-hidden rounded-md border border-surface-800 bg-black">
              <video
                ref={videoRef}
                key={fileUrl ?? inputPath}
                src={fileUrl ?? undefined}
                controls
                className="aspect-video w-full"
                onLoadedMetadata={(e) => {
                  const d = (e.currentTarget as HTMLVideoElement).duration;
                  if (Number.isFinite(d)) {
                    setDuration(d);
                    if (end === 0) setEnd(d);
                  }
                }}
              />
            </div>

            <div className="font-mono text-xs text-surface-200/70">
              {inputPath}
            </div>

            <div className="grid grid-cols-2 gap-4 text-sm">
              <NumField
                label="Start (s)"
                value={start}
                step={0.1}
                min={0}
                max={Math.max(0, end - 0.1)}
                onChange={setStart}
                onJump={() => seek(videoRef, start)}
              />
              <NumField
                label="End (s)"
                value={end}
                step={0.1}
                min={Math.max(0, start + 0.1)}
                max={duration || 99999}
                onChange={setEnd}
                onJump={() => seek(videoRef, end)}
              />
            </div>

            {mode === "trim" ? (
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={streamCopy}
                  onChange={(e) => setStreamCopy(e.target.checked)}
                />
                Stream copy (fast, snaps to keyframes)
              </label>
            ) : (
              <div className="grid grid-cols-2 gap-4 text-sm">
                <NumField
                  label="GIF fps"
                  value={gifFps}
                  step={1}
                  min={5}
                  max={30}
                  onChange={(v) => setGifFps(Math.round(v))}
                />
                <NumField
                  label="GIF width (px, 0 = source)"
                  value={gifWidth}
                  step={20}
                  min={0}
                  max={4096}
                  onChange={(v) => setGifWidth(Math.max(0, Math.round(v)))}
                />
              </div>
            )}

            <div className="flex items-center gap-3">
              <button
                type="button"
                onClick={run}
                disabled={busy || end <= start}
                className={cn(
                  "flex items-center gap-2 rounded-md bg-accent-500 px-4 py-2 text-sm font-medium text-surface-950",
                  "hover:bg-accent-400",
                  "disabled:cursor-not-allowed disabled:opacity-50",
                )}
              >
                {busy ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <Play className="h-4 w-4" />
                )}
                {mode === "trim" ? "Trim" : "Export GIF"}
              </button>
              <span className="text-xs text-surface-200/60">
                {duration > 0 ? `Source duration: ${duration.toFixed(2)}s` : ""}
              </span>
            </div>
          </div>
        ) : (
          <p className="text-sm text-surface-200/70">
            Open a video to start. We support MP4, MOV, MKV, WEBM, AVI.
          </p>
        )}
      </section>

      {error && (
        <div className="rounded-md border border-red-500/40 bg-red-500/10 p-3 text-sm text-red-300">
          {error}
        </div>
      )}

      {result && (
        <div className="rounded-md border border-surface-800 bg-surface-900/40 p-4 text-sm">
          <div className="mb-1 font-medium">
            {mode === "trim" ? "Trim complete" : "GIF exported"}
          </div>
          <div className="font-mono text-xs text-surface-200/70">
            {result.outputPath}
          </div>
          <div className="mt-1 text-xs text-surface-200/60">
            {(result.sizeBytes / 1024 / 1024).toFixed(2)} MB ·{" "}
            {(result.durationMs / 1000).toFixed(1)}s of processing
          </div>
        </div>
      )}
    </div>
  );
}

interface NumFieldProps {
  label: string;
  value: number;
  step: number;
  min: number;
  max: number;
  onChange: (v: number) => void;
  onJump?: () => void;
}

function NumField({ label, value, step, min, max, onChange, onJump }: NumFieldProps) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-surface-200/70">{label}</span>
      <div className="flex items-center gap-2">
        <input
          type="number"
          value={Number.isFinite(value) ? value : 0}
          step={step}
          min={min}
          max={max}
          onChange={(e) => onChange(Number(e.target.value))}
          className="w-full rounded-md border border-surface-700 bg-surface-950 px-3 py-2 focus:border-accent-500 focus:outline-none"
        />
        {onJump && (
          <button
            type="button"
            onClick={onJump}
            className="rounded-md border border-surface-700 px-2 py-1 text-xs hover:bg-surface-800"
            title="Seek video to this time"
          >
            seek
          </button>
        )}
      </div>
    </label>
  );
}

function seek(ref: React.RefObject<HTMLVideoElement | null>, t: number) {
  const v = ref.current;
  if (!v) return;
  v.currentTime = Math.max(0, t);
}

/**
 * Tauri exposes local files via the `convertFileSrc` helper so the webview can
 * load them through a custom protocol without breaking the sandbox.
 */
function useFileUrl(path: string | null): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    if (!path) {
      setUrl(null);
      return;
    }
    void import("@tauri-apps/api/core").then((m) => {
      if (cancelled) return;
      try {
        setUrl(m.convertFileSrc(path));
      } catch {
        setUrl(null);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [path]);
  return url;
}
