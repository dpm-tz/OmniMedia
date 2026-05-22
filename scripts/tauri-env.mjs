/**
 * Prepends Cargo's bin dir to PATH, then runs the local Tauri CLI.
 * Fixes "cargo metadata ... program not found" when Rust is installed but
 * `%USERPROFILE%\.cargo\bin` is not on the system PATH (common on Windows).
 */
import { spawnSync } from "node:child_process";
import path from "node:path";
import { homedir } from "node:os";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const cargoBin = path.join(homedir(), ".cargo", "bin");
const sep = path.delimiter;
if (!process.env.PATH?.split(sep).some((p) => p.toLowerCase() === cargoBin.toLowerCase())) {
  process.env.PATH = `${cargoBin}${sep}${process.env.PATH ?? ""}`;
}

const ext = process.platform === "win32" ? ".cmd" : "";
const tauriBin = path.join(root, "node_modules", ".bin", `tauri${ext}`);
if (!existsSync(tauriBin)) {
  console.error("Tauri CLI not found. Run: npm install");
  process.exit(1);
}

const args = process.argv.slice(2);
const r = spawnSync(tauriBin, args, {
  stdio: "inherit",
  cwd: root,
  env: process.env,
  shell: process.platform === "win32",
});
process.exit(r.status ?? 1);
