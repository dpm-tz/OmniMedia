# Tauri + React + Typescript

This template should help get you started developing with Tauri, React and Typescript in Vite.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## How to run

### Prerequisites

- [Node.js](https://nodejs.org/) (LTS recommended)
- [Rust](https://www.rust-lang.org/tools/install) (stable) and the usual [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS (on Windows: Visual Studio Build Tools with the C++ workload)




### Install dependencies

From the `omnimedia` directory:

```bash
npm install
```

### FFmpeg (for recording / export)

The app looks for FFmpeg in bundled `src-tauri/resources/ffmpeg/`, then on your `PATH`. For local development, either:

- run **`npm run setup:ffmpeg`** once to download a static build into `src-tauri/resources/ffmpeg/`, or
- install FFmpeg system-wide (e.g. `winget install FFmpeg` on Windows).

For **Windows system-audio (WASAPI) loopback**, the default bundled build may be insufficient; see `npm run setup:ffmpeg-wasapi` or the scripts in the repo.

### Run the desktop app (recommended)

```bash
npm run tauri:dev
```

This starts the Vite dev server and opens the Tauri window (`beforeDevCommand` runs `npm run dev`).

### Run the web UI only (no Tauri / Rust)

```bash
npm run dev
```

Useful for front-end work; Tauri APIs and native features will not be available.

### Production build

```bash
npm run tauri:build
```

Release builds run `npm run build` and **`npm run setup:ffmpeg`** as part of the Tauri `beforeBuildCommand`.
