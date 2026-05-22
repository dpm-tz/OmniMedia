/**
 * Downloads a static FFmpeg build for the **current Tauri bundle target**
 * (uses TAURI_ENV_PLATFORM / TAURI_ENV_ARCH when invoked from `beforeBuildCommand`)
 * and installs it under `src-tauri/resources/ffmpeg/`.
 *
 * NOTE: This build does NOT include WASAPI loopback support for system audio.
 * For WASAPI support, use: npm run setup:ffmpeg-wasapi
 * Or install full FFmpeg on your system: winget install FFmpeg
 *
 * Windows/Linux: BtbN GPL builds (https://github.com/BtbN/FFmpeg-Builds)
 * macOS: evermeet.cx static zip (https://evermeet.cx/ffmpeg/)
 *
 * Override: FFMPEG_DOWNLOAD_URL=<direct url>, FFMPEG_ARCHIVE_EXT=zip|tar.xz
 */

import {
  mkdirSync,
  existsSync,
  rmSync,
  copyFileSync,
  chmodSync,
  readdirSync,
  statSync,
} from "node:fs";
import { writeFile } from "node:fs/promises";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync, execSync } from "node:child_process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(__dirname, "..");
const outDir = join(repoRoot, "src-tauri", "resources", "ffmpeg");

function tauriTarget() {
  const p = process.env.TAURI_ENV_PLATFORM;
  const a = process.env.TAURI_ENV_ARCH;
  if (p && a) return { platform: p, arch: a };
  const platform =
    process.platform === "win32"
      ? "windows"
      : process.platform === "darwin"
        ? "darwin"
        : "linux";
  const arch =
    process.arch === "x64"
      ? "x86_64"
      : process.arch === "arm64"
        ? "aarch64"
        : process.arch;
  return { platform, arch };
}

/**
 * Download to disk with the file fully closed before return (avoids Windows
 * "file is being used by another process" when Expand-Archive runs immediately).
 */
async function download(url, dest) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) {
    throw new Error(`GET ${url} → ${res.status} ${res.statusText}`);
  }
  const buf = Buffer.from(await res.arrayBuffer());
  await writeFile(dest, buf);
}

function unzip(archive, dest) {
  mkdirSync(dest, { recursive: true });
  // Windows 10+ ships `tar` with ZIP support; avoids Expand-Archive file-lock issues.
  if (process.platform === "win32") {
    execSync(`tar -xf ${JSON.stringify(archive)} -C ${JSON.stringify(dest)}`, {
      stdio: "inherit",
      shell: true,
    });
    return;
  }
  const r = spawnSync("unzip", ["-o", "-q", archive, "-d", dest], { stdio: "inherit" });
  if (r.status !== 0) {
    throw new Error(
      "unzip failed (install `unzip`, or set FFMPEG_DOWNLOAD_URL / extract manually).",
    );
  }
}

function untarXz(archive, dest) {
  mkdirSync(dest, { recursive: true });
  execSync(`tar -xf ${JSON.stringify(archive)} -C ${JSON.stringify(dest)}`, {
    stdio: "inherit",
    shell: true,
  });
}

function findBinary(root, name) {
  const stack = [root];
  while (stack.length) {
    const d = stack.pop();
    let entries;
    try {
      entries = readdirSync(d, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const ent of entries) {
      const p = join(d, ent.name);
      if (ent.isDirectory()) stack.push(p);
      else if (ent.name === name) return p;
    }
  }
  return null;
}

function copyDirectory(src, dest) {
  mkdirSync(dest, { recursive: true });
  for (const ent of readdirSync(src, { withFileTypes: true })) {
    const srcPath = join(src, ent.name);
    const destPath = join(dest, ent.name);
    if (ent.isDirectory()) {
      copyDirectory(srcPath, destPath);
    } else {
      copyFileSync(srcPath, destPath);
    }
  }
}

function cleanOldFFmpegFiles(destDir) {
  if (!existsSync(destDir)) return;
  for (const entry of readdirSync(destDir, { withFileTypes: true })) {
    if (entry.name === "NOTICE.txt") {
      continue;
    }
    rmSync(join(destDir, entry.name), { recursive: true, force: true });
  }
}

function pickDownload() {
  if (process.env.FFMPEG_DOWNLOAD_URL) {
    return {
      url: process.env.FFMPEG_DOWNLOAD_URL,
      ext: process.env.FFMPEG_ARCHIVE_EXT || "zip",
    };
  }
  const { platform, arch } = tauriTarget();
  if (platform === "windows") {
    // BtbN's FFmpeg: If WASAPI support needed, rebuild with:
    // FFMPEG_DOWNLOAD_URL=https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl-shared.zip
    // npm run setup:ffmpeg
    const url =
      arch === "aarch64"
        ? "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-winarm64-gpl-shared.zip"
        : "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl-shared.zip";
    return { url, ext: "zip" };
  }
  if (platform === "linux") {
    const url =
      arch === "aarch64"
        ? "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linuxarm64-gpl.tar.xz"
        : "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz";
    return { url, ext: "tar.xz" };
  }
  if (platform === "darwin") {
    return { url: "https://evermeet.cx/ffmpeg/getrelease/zip", ext: "zip" };
  }
  throw new Error(`Unsupported platform: ${platform}`);
}

async function main() {
  const { platform } = tauriTarget();
  const isWin = platform === "windows";
  const binName = isWin ? "ffmpeg.exe" : "ffmpeg";
  const destBin = join(outDir, binName);

  if (existsSync(destBin) && process.env.FORCE_FFMPEG !== "1") {
    console.log(
      "[prepare-ffmpeg] binary already exists — skipping download. Set FORCE_FFMPEG=1 to refresh.",
    );
    return;
  }

  mkdirSync(outDir, { recursive: true });
  const tmpRoot = join(outDir, ".tmp");
  if (existsSync(tmpRoot)) rmSync(tmpRoot, { recursive: true, force: true });
  mkdirSync(tmpRoot, { recursive: true });

  const { url, ext } = pickDownload();
  console.log("[prepare-ffmpeg] target:", tauriTarget(), "\n", url);
  const archivePath = join(tmpRoot, `ffmpeg-download.${ext.replace(".", "_")}`);
  await download(url, archivePath);

  const extractDir = join(tmpRoot, "extracted");
  mkdirSync(extractDir, { recursive: true });
  if (ext === "zip") unzip(archivePath, extractDir);
  else untarXz(archivePath, extractDir);

  const found = findBinary(extractDir, binName);
  if (!found) throw new Error(`Could not find ${binName} in extracted archive.`);

  if (existsSync(destBin)) rmSync(destBin, { force: true });
  copyFileSync(found, destBin);
  if (!isWin) chmodSync(destBin, 0o755);

  if (existsSync(tmpRoot)) rmSync(tmpRoot, { recursive: true, force: true });

  console.log("[prepare-ffmpeg] installed:", destBin, "size:", statSync(destBin).size);
}

main().catch((e) => {
  console.error("[prepare-ffmpeg]", e);
  process.exit(1);
});
