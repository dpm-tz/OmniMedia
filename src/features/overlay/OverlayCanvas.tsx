import { useEffect, useRef, useState } from "react";
import { api, events } from "../../lib/bridge";
import type {
  InputClickEvent,
  InputKeyEvent,
  OverlayCommand,
  OverlayToolState,
} from "../../lib/types";
import { modifierString, vkLabel } from "./keymap";

type Pt = { x: number; y: number };

interface Ripple {
  id: number;
  x: number;
  y: number;
  button: "left" | "right" | "middle";
  startedAt: number;
}

interface KeyPill {
  id: number;
  text: string;
  startedAt: number;
}

const RIPPLE_DURATION_MS = 700;
const KEY_DURATION_MS = 1600;
const KEY_HUD_MAX = 6;

interface Stroke {
  tool: OverlayToolState["tool"];
  color: string;
  size: number;
  points: Pt[];
}

const DEFAULT_STATE: OverlayToolState = {
  tool: "pen",
  color: "#ff2d55",
  size: 4,
  passthrough: true,
};

export function OverlayCanvas() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const overlayRef = useRef<HTMLDivElement | null>(null);
  const [tool, setTool] = useState<OverlayToolState>(DEFAULT_STATE);
  const [strokes, setStrokes] = useState<Stroke[]>([]);
  const drawing = useRef<Stroke | null>(null);
  const [spotlight, setSpotlight] = useState<Pt | null>(null);
  const ripplesRef = useRef<Ripple[]>([]);
  const keysRef = useRef<KeyPill[]>([]);
  const seqRef = useRef<number>(0);
  const animRef = useRef<number | null>(null);

  useEffect(() => {
    void api.input.start().catch(() => {});
    return () => {
      void api.input.stop().catch(() => {});
    };
  }, []);

  useEffect(() => {
    const subs = [
      events.onOverlayToolState((s) => {
        setTool(s);
        void api.overlay.setPassthrough(s.passthrough);
      }),
      events.onOverlayCommand((cmd: OverlayCommand) => {
        if (cmd.kind === "undo") setStrokes((s) => s.slice(0, -1));
        else if (cmd.kind === "clear") setStrokes([]);
      }),
      events.onInputClick((e: InputClickEvent) => {
        const id = ++seqRef.current;
        ripplesRef.current.push({
          id,
          x: e.x,
          y: e.y,
          button: e.button,
          startedAt: performance.now(),
        });
        ensureAnimating();
      }),
      events.onInputKey((e: InputKeyEvent) => {
        const id = ++seqRef.current;
        const mods = modifierString(e.shift, e.ctrl, e.alt, e.meta);
        const label = vkLabel(e.vk, e.shift);
        const text = mods ? `${mods} + ${label}` : label;
        keysRef.current.push({ id, text, startedAt: performance.now() });
        if (keysRef.current.length > KEY_HUD_MAX) {
          keysRef.current.splice(0, keysRef.current.length - KEY_HUD_MAX);
        }
        ensureAnimating();
      }),
    ];
    return () => {
      subs.forEach((p) => void p.then((fn) => fn()));
      if (animRef.current != null) cancelAnimationFrame(animRef.current);
      animRef.current = null;
    };
  }, []);

  function ensureAnimating() {
    if (animRef.current != null) return;
    const tick = () => {
      const now = performance.now();
      ripplesRef.current = ripplesRef.current.filter(
        (r) => now - r.startedAt < RIPPLE_DURATION_MS,
      );
      keysRef.current = keysRef.current.filter(
        (k) => now - k.startedAt < KEY_DURATION_MS,
      );
      redraw();
      if (
        ripplesRef.current.length === 0 &&
        keysRef.current.length === 0 &&
        !drawing.current
      ) {
        animRef.current = null;
        return;
      }
      animRef.current = requestAnimationFrame(tick);
    };
    animRef.current = requestAnimationFrame(tick);
  }

  useEffect(() => {
    const c = canvasRef.current;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    const resize = () => {
      c.width = window.innerWidth * dpr;
      c.height = window.innerHeight * dpr;
      c.style.width = `${window.innerWidth}px`;
      c.style.height = `${window.innerHeight}px`;
      const ctx = c.getContext("2d");
      if (ctx) ctx.scale(dpr, dpr);
      redraw();
    };
    resize();
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, []);

  useEffect(() => {
    redraw();
  }, [strokes, tool, spotlight]);

  function redraw() {
    const c = canvasRef.current;
    if (!c) return;
    const ctx = c.getContext("2d");
    if (!ctx) return;
    ctx.save();
    const dpr = window.devicePixelRatio || 1;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, c.width, c.height);
    ctx.scale(dpr, dpr);

    for (const s of [...strokes, drawing.current].filter(Boolean) as Stroke[]) {
      drawStroke(ctx, s);
    }

    if (spotlight && tool.tool === "spotlight") {
      drawSpotlight(ctx, spotlight, tool.size);
    }

    drawRipples(ctx);
    drawKeysHud(ctx);
    ctx.restore();
  }

  function drawRipples(ctx: CanvasRenderingContext2D) {
    const now = performance.now();
    for (const r of ripplesRef.current) {
      const t = (now - r.startedAt) / RIPPLE_DURATION_MS;
      if (t < 0 || t > 1) continue;
      const ease = 1 - Math.pow(1 - t, 3);
      const radius = 18 + ease * 56;
      const alpha = 1 - t;
      const color =
        r.button === "left"
          ? "10, 132, 255"
          : r.button === "right"
            ? "255, 69, 58"
            : "255, 214, 10";
      ctx.beginPath();
      ctx.arc(r.x, r.y, radius, 0, Math.PI * 2);
      ctx.lineWidth = 4;
      ctx.strokeStyle = `rgba(${color}, ${alpha})`;
      ctx.stroke();

      ctx.beginPath();
      ctx.arc(r.x, r.y, Math.max(2, 8 - ease * 6), 0, Math.PI * 2);
      ctx.fillStyle = `rgba(${color}, ${0.7 * alpha})`;
      ctx.fill();
    }
  }

  function drawKeysHud(ctx: CanvasRenderingContext2D) {
    if (keysRef.current.length === 0) return;
    const now = performance.now();
    const padX = 14;
    const padY = 8;
    const gap = 8;
    const fontSize = 18;
    ctx.font = `600 ${fontSize}px Inter, system-ui, -apple-system, "Segoe UI", sans-serif`;
    ctx.textBaseline = "middle";

    const visible = keysRef.current
      .map((k) => ({ k, age: now - k.startedAt }))
      .filter(({ age }) => age >= 0 && age < KEY_DURATION_MS);
    if (visible.length === 0) return;

    let totalWidth = 0;
    const widths = visible.map(({ k }) => {
      const w = ctx.measureText(k.text).width + padX * 2;
      totalWidth += w + gap;
      return w;
    });
    totalWidth -= gap;

    const winW = ctx.canvas.width / (window.devicePixelRatio || 1);
    const winH = ctx.canvas.height / (window.devicePixelRatio || 1);
    let x = (winW - totalWidth) / 2;
    const y = winH - 80;

    for (let i = 0; i < visible.length; i++) {
      const { k, age } = visible[i];
      const w = widths[i];
      const t = age / KEY_DURATION_MS;
      const alpha = 1 - Math.max(0, t - 0.7) / 0.3;
      const yShift = -age * 0.02;
      roundedRect(ctx, x, y + yShift, w, fontSize + padY * 2, 12);
      ctx.fillStyle = `rgba(20, 20, 28, ${0.8 * alpha})`;
      ctx.fill();
      ctx.lineWidth = 1;
      ctx.strokeStyle = `rgba(255, 255, 255, ${0.18 * alpha})`;
      ctx.stroke();

      ctx.fillStyle = `rgba(255, 255, 255, ${alpha})`;
      ctx.fillText(k.text, x + padX, y + yShift + fontSize / 2 + padY);
      x += w + gap;
    }
  }

  function roundedRect(
    ctx: CanvasRenderingContext2D,
    x: number,
    y: number,
    w: number,
    h: number,
    r: number,
  ) {
    const rr = Math.min(r, w / 2, h / 2);
    ctx.beginPath();
    ctx.moveTo(x + rr, y);
    ctx.arcTo(x + w, y, x + w, y + h, rr);
    ctx.arcTo(x + w, y + h, x, y + h, rr);
    ctx.arcTo(x, y + h, x, y, rr);
    ctx.arcTo(x, y, x + w, y, rr);
    ctx.closePath();
  }

  function drawStroke(ctx: CanvasRenderingContext2D, s: Stroke) {
    if (s.points.length === 0) return;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.strokeStyle = s.color;
    ctx.fillStyle = s.color;
    ctx.lineWidth = s.size;

    if (s.tool === "eraser") {
      ctx.globalCompositeOperation = "destination-out";
      ctx.lineWidth = s.size * 4;
      drawPath(ctx, s.points);
      ctx.globalCompositeOperation = "source-over";
      return;
    }

    if (s.tool === "pen") {
      drawPath(ctx, s.points);
      return;
    }
    if (s.tool === "rect" && s.points.length >= 2) {
      const a = s.points[0];
      const b = s.points[s.points.length - 1];
      ctx.strokeRect(a.x, a.y, b.x - a.x, b.y - a.y);
      return;
    }
    if (s.tool === "circle" && s.points.length >= 2) {
      const a = s.points[0];
      const b = s.points[s.points.length - 1];
      const cx = (a.x + b.x) / 2;
      const cy = (a.y + b.y) / 2;
      const rx = Math.abs(b.x - a.x) / 2;
      const ry = Math.abs(b.y - a.y) / 2;
      ctx.beginPath();
      ctx.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2);
      ctx.stroke();
      return;
    }
    if (s.tool === "arrow" && s.points.length >= 2) {
      const a = s.points[0];
      const b = s.points[s.points.length - 1];
      drawArrow(ctx, a, b, s.size);
      return;
    }
  }

  function drawPath(ctx: CanvasRenderingContext2D, pts: Pt[]) {
    if (pts.length < 2) {
      const p = pts[0];
      ctx.beginPath();
      ctx.arc(p.x, p.y, ctx.lineWidth / 2, 0, Math.PI * 2);
      ctx.fill();
      return;
    }
    ctx.beginPath();
    ctx.moveTo(pts[0].x, pts[0].y);
    for (let i = 1; i < pts.length; i++) {
      ctx.lineTo(pts[i].x, pts[i].y);
    }
    ctx.stroke();
  }

  function drawArrow(ctx: CanvasRenderingContext2D, a: Pt, b: Pt, size: number) {
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const ang = Math.atan2(dy, dx);
    const head = Math.max(12, size * 4);
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(b.x, b.y);
    ctx.lineTo(
      b.x - head * Math.cos(ang - Math.PI / 6),
      b.y - head * Math.sin(ang - Math.PI / 6),
    );
    ctx.lineTo(
      b.x - head * Math.cos(ang + Math.PI / 6),
      b.y - head * Math.sin(ang + Math.PI / 6),
    );
    ctx.closePath();
    ctx.fill();
  }

  function drawSpotlight(ctx: CanvasRenderingContext2D, p: Pt, size: number) {
    const r = Math.max(80, size * 14);
    ctx.save();
    ctx.fillStyle = "rgba(0,0,0,0.55)";
    ctx.fillRect(0, 0, ctx.canvas.width, ctx.canvas.height);
    ctx.globalCompositeOperation = "destination-out";
    const grad = ctx.createRadialGradient(p.x, p.y, r * 0.55, p.x, p.y, r);
    grad.addColorStop(0, "rgba(0,0,0,1)");
    grad.addColorStop(1, "rgba(0,0,0,0)");
    ctx.fillStyle = grad;
    ctx.beginPath();
    ctx.arc(p.x, p.y, r, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }

  function pointerDown(e: React.PointerEvent) {
    if (tool.passthrough) return;
    if (tool.tool === "spotlight") {
      setSpotlight({ x: e.clientX, y: e.clientY });
      return;
    }
    drawing.current = {
      tool: tool.tool,
      color: tool.color,
      size: tool.size,
      points: [{ x: e.clientX, y: e.clientY }],
    };
    (e.target as Element).setPointerCapture(e.pointerId);
    redraw();
  }

  function pointerMove(e: React.PointerEvent) {
    if (tool.tool === "spotlight" && spotlight) {
      setSpotlight({ x: e.clientX, y: e.clientY });
      return;
    }
    if (!drawing.current) return;
    drawing.current.points.push({ x: e.clientX, y: e.clientY });
    redraw();
  }

  function pointerUp() {
    if (!drawing.current) return;
    setStrokes((s) => [...s, drawing.current as Stroke]);
    drawing.current = null;
  }

  return (
    <div
      ref={overlayRef}
      onPointerDown={pointerDown}
      onPointerMove={pointerMove}
      onPointerUp={pointerUp}
      onPointerCancel={pointerUp}
      style={{
        position: "fixed",
        inset: 0,
        pointerEvents: tool.passthrough ? "none" : "auto",
        cursor:
          tool.tool === "eraser"
            ? "cell"
            : tool.tool === "spotlight"
              ? "none"
              : "crosshair",
        background: "transparent",
      }}
    >
      <canvas
        ref={canvasRef}
        style={{
          width: "100%",
          height: "100%",
          display: "block",
        }}
      />
    </div>
  );
}
