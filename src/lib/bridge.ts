// Thin, fully typed wrapper around `@tauri-apps/api`.
// Every Rust command that the UI may call lives here so that:
//  - TypeScript catches signature drift at compile time.
//  - The UI never imports `invoke` directly (single audit surface).
//  - We can stub the bridge in tests by swapping this module.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  type AudioInputDevice,
  EVENT,
  type EditResult,
  type GifOptions,
  type InputClickEvent,
  type InputKeyEvent,
  type OverlayCommand,
  type OverlayToolState,
  type RecorderProgressEvent,
  type RecordingOptions,
  type RecordingResult,
  type RecordingSource,
  type RecordingStatus,
  type ScreenshotOptions,
  type ScreenshotResult,
  type SystemInfo,
  type TrimOptions,
} from "./types";

// Generic invoke that preserves typing while keeping a single error funnel.
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await tauriInvoke<T>(cmd, args);
  } catch (err) {
    // Rust commands return `Result<T, AppError>`; AppError serializes to a
    // string via Display. Re-throw as a plain Error for the UI layer.
    throw new Error(typeof err === "string" ? err : JSON.stringify(err));
  }
}

export const api = {
  system: {
    info: () => call<SystemInfo>("system_info"),
  },
  recorder: {
    listSources: () => call<RecordingSource[]>("list_recording_sources"),
    listAudioInputs: () => call<AudioInputDevice[]>("list_audio_input_devices"),
    start: (options: RecordingOptions) =>
      call<RecordingStatus>("start_recording", { options }),
    stop: () => call<RecordingResult>("stop_recording"),
    status: () => call<RecordingStatus>("recording_status"),
  },
  overlay: {
    open: () => call<void>("open_annotation_overlay"),
    close: () => call<void>("close_annotation_overlay"),
    setPassthrough: (passthrough: boolean) =>
      call<void>("set_overlay_passthrough", { passthrough }),
    setTool: (state: OverlayToolState) =>
      call<void>("overlay_set_tool", { state }),
    dispatch: (command: OverlayCommand) =>
      call<void>("overlay_dispatch", { command }),
    isOpen: () => call<boolean>("overlay_is_open"),
  },
  screenshot: {
    capture: (options: ScreenshotOptions = {}) =>
      call<ScreenshotResult>("capture_screenshot", { options }),
  },
  input: {
    start: () => call<void>("start_input_capture"),
    stop: () => call<void>("stop_input_capture"),
  },
  webcam: {
    open: () => call<void>("open_webcam_overlay"),
    prepare: () => call<void>("prepare_webcam_overlay"),
    close: () => call<void>("close_webcam_overlay"),
    isOpen: () => call<boolean>("webcam_is_open"),
  },
  editor: {
    trim: (options: TrimOptions) => call<EditResult>("trim_video", { options }),
    gif: (options: GifOptions) => call<EditResult>("export_gif", { options }),
  },
};

// Strongly-typed event subscriptions. Each helper returns an unlisten fn
// that callers must invoke on cleanup (e.g. inside a useEffect return).
export const events = {
  onRecorderProgress: (cb: (e: RecorderProgressEvent) => void): Promise<UnlistenFn> =>
    listen<RecorderProgressEvent>(EVENT.RecorderProgress, (e) => cb(e.payload)),
  onRecorderStopped: (cb: (result: RecordingResult) => void): Promise<UnlistenFn> =>
    listen<RecordingResult>(EVENT.RecorderStopped, (e) => cb(e.payload)),
  onRecorderError: (cb: (message: string) => void): Promise<UnlistenFn> =>
    listen<string>(EVENT.RecorderError, (e) => cb(e.payload)),
  onRecorderWarn: (cb: (message: string) => void): Promise<UnlistenFn> =>
    listen<string>(EVENT.RecorderWarn, (e) => cb(e.payload)),
  onOverlayToolState: (cb: (state: OverlayToolState) => void): Promise<UnlistenFn> =>
    listen<OverlayToolState>(EVENT.OverlayToolState, (e) => cb(e.payload)),
  onOverlayCommand: (cb: (cmd: OverlayCommand) => void): Promise<UnlistenFn> =>
    listen<OverlayCommand>(EVENT.OverlayCommand, (e) => cb(e.payload)),
  onInputClick: (cb: (e: InputClickEvent) => void): Promise<UnlistenFn> =>
    listen<InputClickEvent>(EVENT.InputClick, (e) => cb(e.payload)),
  onInputKey: (cb: (e: InputKeyEvent) => void): Promise<UnlistenFn> =>
    listen<InputKeyEvent>(EVENT.InputKey, (e) => cb(e.payload)),
};
