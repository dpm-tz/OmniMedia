import { Video, Scissors, Image as ImageIcon, Sparkles } from "lucide-react";
import { cn } from "../lib/cn";
import type { ComponentType } from "react";

export type RouteId = "recorder" | "editor" | "image" | "animation";

interface NavItem {
  id: RouteId;
  label: string;
  icon: ComponentType<{ className?: string }>;
}

const NAV: NavItem[] = [
  { id: "recorder", label: "Recorder", icon: Video },
  { id: "editor", label: "Video Editor", icon: Scissors },
  { id: "image", label: "Image / Annotate", icon: ImageIcon },
  { id: "animation", label: "Animation", icon: Sparkles },
];

interface SidebarProps {
  active: RouteId;
  onSelect: (id: RouteId) => void;
}

export function Sidebar({ active, onSelect }: SidebarProps) {
  return (
    <aside className="flex h-full w-56 shrink-0 flex-col border-r border-surface-800 bg-surface-900/60 backdrop-blur">
      <div className="flex items-center gap-2 px-4 pt-5 pb-6">
        <div className="grid h-8 w-8 place-items-center rounded-md bg-accent-500 text-surface-950 font-bold">
          O
        </div>
        <div className="leading-tight">
          <div className="text-sm font-semibold">OmniMedia</div>
          <div className="text-[11px] text-surface-200/60">v0.1.0</div>
        </div>
      </div>

      <nav className="flex flex-col gap-1 px-2">
        {NAV.map(({ id, label, icon: Icon }) => {
          const isActive = active === id;
          return (
            <button
              key={id}
              type="button"
              onClick={() => onSelect(id)}
              className={cn(
                "flex items-center gap-3 rounded-md px-3 py-2 text-sm transition-colors",
                "hover:bg-surface-800",
                isActive
                  ? "bg-surface-800 text-white"
                  : "text-surface-200/80",
              )}
            >
              <Icon className="h-4 w-4" />
              {label}
            </button>
          );
        })}
      </nav>

      <div className="mt-auto px-4 pb-4 text-[11px] text-surface-200/40">
        Tauri 2.0 · Rust · React
      </div>
    </aside>
  );
}
