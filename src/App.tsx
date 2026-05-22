import { useEffect, useState } from "react";
import { Sidebar, type RouteId } from "./components/Sidebar";
import { RecorderPage } from "./features/recorder/RecorderPage";
import { EditorPage } from "./features/editor/EditorPage";
import { ImagePage } from "./features/image/ImagePage";
import { AnimationPage } from "./features/animation/AnimationPage";
import { OverlayCanvas } from "./features/overlay/OverlayCanvas";
import { OverlayToolbar } from "./features/overlay/OverlayToolbar";
import { WebcamOverlay } from "./features/webcam/WebcamOverlay";
import { api } from "./lib/bridge";
import type { SystemInfo } from "./lib/types";

// The annotation/webcam overlays reuse the same React bundle but mount
// different surfaces per window (`index.html#<window>` URL hash).
type WindowKind =
  | "main"
  | "overlay-canvas"
  | "overlay-toolbar"
  | "webcam-overlay";

function readWindowKind(): WindowKind {
  const hash = window.location.hash.replace(/^#/, "");
  if (
    hash === "overlay-canvas" ||
    hash === "overlay-toolbar" ||
    hash === "webcam-overlay"
  ) {
    return hash;
  }
  return "main";
}

export default function App() {
  const kind = readWindowKind();

  if (kind === "overlay-canvas") return <OverlayCanvas />;
  if (kind === "overlay-toolbar") return <OverlayToolbar />;
  if (kind === "webcam-overlay") return <WebcamOverlay />;

  return <MainApp />;
}

function MainApp() {
  const [route, setRoute] = useState<RouteId>("recorder");
  const [sysInfo, setSysInfo] = useState<SystemInfo | null>(null);

  useEffect(() => {
    api.system.info().then(setSysInfo).catch(console.error);
  }, []);

  return (
    <div className="flex h-full w-full overflow-hidden">
      <Sidebar active={route} onSelect={setRoute} />
      <main className="flex-1 overflow-auto">
        {route === "recorder" && <RecorderPage />}
        {route === "editor" && <EditorPage />}
        {route === "image" && <ImagePage />}
        {route === "animation" && <AnimationPage />}
      </main>
      {sysInfo && (
        <div className="pointer-events-none fixed bottom-2 right-3 text-[10px] text-surface-200/40">
          {sysInfo.os}/{sysInfo.arch} · ffmpeg:{" "}
          {sysInfo.ffmpegAvailable ? "yes" : "no"}
        </div>
      )}
    </div>
  );
}
