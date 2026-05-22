// Shared types that mirror the `serde`-serialized structs on the Rust side.
// Keeping these in sync is the contract between the two layers; if a Rust
// struct changes, update its twin here.

export type RecordingId = string;

export interface RecordingSource {
  id: string;
  label: string;
  kind: "screen" | "window" | "area";
  width: number;
  height: number;
}

export interface AudioInputDevice {
  id: string;
  label: string;
}

export interface RecordingOptions {
  sourceId: string;
  fps: number;
  captureSystemAudio: boolean;
  captureMicrophone: boolean;
  microphoneDeviceId?: string;
  outputDir?: string;
}

export type RecordingStatus =
  | { state: "idle" }
  | { state: "recording"; id: RecordingId; startedAt: string }
  | { state: "stopped"; id: RecordingId; outputPath: string };

export interface RecordingResult {
  id: RecordingId;
  outputPath: string;
  durationMs: number;
  sizeBytes: number;
}

export interface SystemInfo {
  os: string;
  arch: string;
  appVersion: string;
  ffmpegAvailable: boolean;
}

// Events emitted by Rust over Tauri's event bus. Names must match the
// `emit` calls inside src-tauri/src/recorder/mod.rs and overlay.rs.
export const EVENT = {
  RecorderProgress: "omnimedia://recorder/progress",
  RecorderStopped: "omnimedia://recorder/stopped",
  RecorderError: "omnimedia://recorder/error",
  RecorderWarn: "omnimedia://recorder/warn",
  OverlayToolState: "omnimedia://overlay/tool-state",
  OverlayCommand: "omnimedia://overlay/command",
  InputClick: "omnimedia://input/click",
  InputKey: "omnimedia://input/key",
} as const;

export interface InputClickEvent {
  x: number;
  y: number;
  button: "left" | "right" | "middle";
}

export interface InputKeyEvent {
  vk: number;
  scan: number;
  shift: boolean;
  ctrl: boolean;
  alt: boolean;
  meta: boolean;
}

export interface RecorderProgressEvent {
  id: RecordingId;
  elapsedMs: number;
  framesCaptured: number;
}

// ---------- Annotation overlay ----------

export type OverlayTool =
  | "pen"
  | "rect"
  | "circle"
  | "arrow"
  | "eraser"
  | "text"
  | "spotlight";

export interface OverlayToolState {
  tool: OverlayTool;
  color: string;
  size: number;
  passthrough: boolean;
}

export interface OverlayCommand {
  kind: "undo" | "clear" | "saveSnapshot";
}

// ---------- Screenshots ----------

export interface ScreenshotRegion {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ScreenshotOptions {
  sourceId?: string;
  region?: ScreenshotRegion;
  outputDir?: string;
  format?: "png" | "jpg";
}

export interface ScreenshotResult {
  path: string;
  width: number;
  height: number;
  sizeBytes: number;
}

// ---------- Editor (trim / GIF) ----------

export interface TrimOptions {
  inputPath: string;
  startSeconds: number;
  endSeconds: number;
  outputPath?: string;
  streamCopy?: boolean;
}

export interface GifOptions {
  inputPath: string;
  startSeconds?: number;
  endSeconds?: number;
  fps?: number;
  width?: number;
  outputPath?: string;
}

export interface EditResult {
  outputPath: string;
  sizeBytes: number;
  durationMs: number;
}
