// Minimal Win32 VK_CODE → display label table for the keystroke HUD. The set
// is intentionally not exhaustive; anything missing falls through to the
// printable-character fallback.

const VK: Record<number, string> = {
  0x08: "Back",
  0x09: "Tab",
  0x0d: "Enter",
  0x10: "Shift",
  0x11: "Ctrl",
  0x12: "Alt",
  0x13: "Pause",
  0x14: "Caps",
  0x1b: "Esc",
  0x20: "Space",
  0x21: "PgUp",
  0x22: "PgDn",
  0x23: "End",
  0x24: "Home",
  0x25: "←",
  0x26: "↑",
  0x27: "→",
  0x28: "↓",
  0x2c: "PrtSc",
  0x2d: "Ins",
  0x2e: "Del",
  0x5b: "Win",
  0x5c: "Win",
  0x5d: "Menu",
  0x70: "F1",
  0x71: "F2",
  0x72: "F3",
  0x73: "F4",
  0x74: "F5",
  0x75: "F6",
  0x76: "F7",
  0x77: "F8",
  0x78: "F9",
  0x79: "F10",
  0x7a: "F11",
  0x7b: "F12",
  0xba: ";",
  0xbb: "=",
  0xbc: ",",
  0xbd: "-",
  0xbe: ".",
  0xbf: "/",
  0xc0: "`",
  0xdb: "[",
  0xdc: "\\",
  0xdd: "]",
  0xde: "'",
};

export function vkLabel(vk: number, shift: boolean): string {
  if (VK[vk]) return VK[vk];
  // 0..9
  if (vk >= 0x30 && vk <= 0x39) {
    const digits = ")!@#$%^&*(";
    const i = vk - 0x30;
    return shift ? digits[i] : String(i);
  }
  // A..Z
  if (vk >= 0x41 && vk <= 0x5a) {
    const ch = String.fromCharCode(vk);
    return shift ? ch : ch.toLowerCase();
  }
  // Numpad 0..9
  if (vk >= 0x60 && vk <= 0x69) return `Num${vk - 0x60}`;
  return `0x${vk.toString(16)}`;
}

export function modifierString(
  shift: boolean,
  ctrl: boolean,
  alt: boolean,
  meta: boolean,
): string {
  const parts: string[] = [];
  if (ctrl) parts.push("Ctrl");
  if (alt) parts.push("Alt");
  if (meta) parts.push("Win");
  if (shift) parts.push("Shift");
  return parts.join(" + ");
}
