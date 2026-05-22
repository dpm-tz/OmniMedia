import { useEffect, useState } from "react";
import {
  Pen,
  Square,
  Circle as CircleIcon,
  ArrowUpRight,
  Eraser,
  Undo2,
  Trash2,
  X,
  Hand,
  Lightbulb,
} from "lucide-react";
import { api } from "../../lib/bridge";
import type { OverlayTool, OverlayToolState } from "../../lib/types";
import { cn } from "../../lib/cn";

const COLORS = [
  "#ff2d55",
  "#ff9500",
  "#ffd60a",
  "#34c759",
  "#0a84ff",
  "#bf5af2",
  "#ffffff",
  "#000000",
];

const TOOLS: { id: OverlayTool; label: string; Icon: typeof Pen }[] = [
  { id: "pen", label: "Pen", Icon: Pen },
  { id: "rect", label: "Rectangle", Icon: Square },
  { id: "circle", label: "Circle", Icon: CircleIcon },
  { id: "arrow", label: "Arrow", Icon: ArrowUpRight },
  { id: "eraser", label: "Eraser", Icon: Eraser },
  { id: "spotlight", label: "Spotlight", Icon: Lightbulb },
];

export function OverlayToolbar() {
  const [tool, setTool] = useState<OverlayTool>("pen");
  const [color, setColor] = useState<string>("#ff2d55");
  const [size, setSize] = useState<number>(4);
  const [passthrough, setPassthrough] = useState<boolean>(true);

  const apply = (next: Partial<OverlayToolState>) => {
    const state: OverlayToolState = {
      tool: next.tool ?? tool,
      color: next.color ?? color,
      size: next.size ?? size,
      passthrough: next.passthrough ?? passthrough,
    };
    setTool(state.tool);
    setColor(state.color);
    setSize(state.size);
    setPassthrough(state.passthrough);
    void api.overlay.setTool(state);
  };

  useEffect(() => {
    void api.overlay.setTool({ tool, color, size, passthrough });
  }, []);

  return (
    <div
      className="flex h-full w-full select-none items-center gap-2 rounded-2xl border border-white/10 bg-[#1a1a1ec0] px-3 text-white backdrop-blur"
      data-tauri-drag-region
      style={{ WebkitUserSelect: "none" }}
    >
      <button
        type="button"
        onClick={() => apply({ passthrough: !passthrough })}
        title={passthrough ? "Click-through ON (cursor goes to apps)" : "Drawing ON"}
        className={cn(
          "rounded-md p-2 hover:bg-white/10",
          !passthrough && "bg-emerald-500/30 text-emerald-200",
        )}
      >
        <Hand className="h-4 w-4" />
      </button>

      <div className="mx-1 h-6 w-px bg-white/10" />

      {TOOLS.map(({ id, label, Icon }) => (
        <button
          key={id}
          type="button"
          title={label}
          onClick={() => apply({ tool: id, passthrough: false })}
          className={cn(
            "rounded-md p-2 hover:bg-white/10",
            tool === id && !passthrough && "bg-accent-500/30 text-accent-200",
          )}
        >
          <Icon className="h-4 w-4" />
        </button>
      ))}

      <div className="mx-1 h-6 w-px bg-white/10" />

      <div className="flex items-center gap-1">
        {COLORS.map((c) => (
          <button
            key={c}
            type="button"
            aria-label={`color ${c}`}
            onClick={() => apply({ color: c })}
            className={cn(
              "h-5 w-5 rounded-full border border-white/20 transition-transform",
              color === c && "scale-125 ring-2 ring-white",
            )}
            style={{ backgroundColor: c }}
          />
        ))}
      </div>

      <div className="mx-1 h-6 w-px bg-white/10" />

      <input
        type="range"
        min={1}
        max={32}
        value={size}
        onChange={(e) => apply({ size: Number(e.target.value) })}
        className="w-24 accent-accent-400"
        title={`Stroke size: ${size}px`}
      />
      <div className="w-6 text-center text-xs tabular-nums text-white/70">{size}</div>

      <div className="mx-1 h-6 w-px bg-white/10" />

      <button
        type="button"
        title="Undo"
        onClick={() => api.overlay.dispatch({ kind: "undo" })}
        className="rounded-md p-2 hover:bg-white/10"
      >
        <Undo2 className="h-4 w-4" />
      </button>
      <button
        type="button"
        title="Clear"
        onClick={() => api.overlay.dispatch({ kind: "clear" })}
        className="rounded-md p-2 hover:bg-white/10"
      >
        <Trash2 className="h-4 w-4" />
      </button>

      <div className="ml-auto" />

      <button
        type="button"
        title="Close overlay"
        onClick={() => api.overlay.close()}
        className="rounded-md p-2 hover:bg-red-500/30"
      >
        <X className="h-4 w-4" />
      </button>
    </div>
  );
}
