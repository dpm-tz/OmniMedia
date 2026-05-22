import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
// Production builds are loaded from Tauri's custom asset protocol, not
// `http://localhost`. With the default `base: "/"`, `<link href="/assets/...">`
// resolves to the wrong origin and **styles never load** (unstyled white UI;
// JS still runs). Relative URLs fix that.
export default defineConfig(async ({ command }) => ({
  base: command === "build" ? "./" : "/",

  plugins: [react(), tailwindcss()],

  // Vite options tailored for Tauri development.
  // Applied during `tauri dev` and `tauri build`.
  clearScreen: false,
  server: {
    // Tauri expects a fixed port; fail fast if it is unavailable.
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Vite must not watch the Rust side; cargo handles it.
      ignored: ["**/src-tauri/**"],
    },
  },
}));
