//! Global low-level mouse + keyboard hooks (WH_MOUSE_LL / WH_KEYBOARD_LL).
//!
//! While the annotation overlay is open we want to react to clicks/keystrokes
//! that happen anywhere on the desktop — so we install Win32 low-level hooks
//! on a dedicated thread. The hooks emit lightweight Tauri events to the
//! `overlay-canvas` window, which renders click ripples and a keystroke HUD.
//!
//! The hooks are removed when the overlay closes, so they are *not* a
//! persistent system-wide keylogger; they're scoped to the user's active
//! recording/annotation session.

#![cfg(target_os = "windows")]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::OnceLock;
use std::thread::JoinHandle;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT,
    PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HHOOK,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_QUIT,
    WM_RBUTTONDOWN, WM_SYSKEYDOWN,
};

pub const EVT_INPUT_CLICK: &str = "omnimedia://input/click";
pub const EVT_INPUT_KEY: &str = "omnimedia://input/key";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ClickPayload {
    x: i32,
    y: i32,
    button: &'static str,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct KeyPayload {
    vk: u32,
    scan: u32,
    /// Whether modifier flags are active (Shift / Ctrl / Alt / Meta).
    shift: bool,
    ctrl: bool,
    alt: bool,
    meta: bool,
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static HOOK_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
static RUNNING: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn mouse_proc(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 {
        let msg = w as u32;
        if msg == WM_LBUTTONDOWN || msg == WM_RBUTTONDOWN || msg == WM_MBUTTONDOWN {
            let info = unsafe { &*(l as *const MSLLHOOKSTRUCT) };
            if let Some(app) = APP.get() {
                let payload = ClickPayload {
                    x: info.pt.x,
                    y: info.pt.y,
                    button: match msg {
                        WM_LBUTTONDOWN => "left",
                        WM_RBUTTONDOWN => "right",
                        _ => "middle",
                    },
                };
                let _ = app.emit(EVT_INPUT_CLICK, payload);
            }
        }
    }
    unsafe { CallNextHookEx(0 as HHOOK, code, w, l) }
}

unsafe extern "system" fn keyboard_proc(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 {
        let msg = w as u32;
        if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
            let info = unsafe { &*(l as *const KBDLLHOOKSTRUCT) };
            if let Some(app) = APP.get() {
                let (shift, ctrl, alt, meta) = mod_state();
                let payload = KeyPayload {
                    vk: info.vkCode,
                    scan: info.scanCode,
                    shift,
                    ctrl,
                    alt,
                    meta,
                };
                let _ = app.emit(EVT_INPUT_KEY, payload);
            }
        }
    }
    unsafe { CallNextHookEx(0 as HHOOK, code, w, l) }
}

fn mod_state() -> (bool, bool, bool, bool) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    fn down(vk: i32) -> bool {
        unsafe { (GetAsyncKeyState(vk) as u16) & 0x8000 != 0 }
    }
    let shift = down(VK_SHIFT as i32);
    let ctrl = down(VK_CONTROL as i32);
    let alt = down(VK_MENU as i32);
    let meta = down(VK_LWIN as i32) || down(VK_RWIN as i32);
    (shift, ctrl, alt, meta)
}

pub fn start(app: AppHandle) -> Result<(), String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let _ = APP.set(app);

    let handle = std::thread::Builder::new()
        .name("omnimedia-input-hook".into())
        .spawn(move || unsafe {
            HOOK_THREAD_ID.store(GetCurrentThreadId(), Ordering::SeqCst);

            let hinst = GetModuleHandleW(std::ptr::null());
            let hm = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hinst, 0);
            let hk = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hinst, 0);

            if hm == 0 as HHOOK || hk == 0 as HHOOK {
                tracing::warn!("SetWindowsHookExW failed; input-overlay events disabled");
            }

            let mut msg: MSG = std::mem::zeroed();
            // Pump messages so the OS keeps feeding our hook procs. WM_QUIT
            // returns 0 from GetMessageW, ending the loop.
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            if hm != 0 as HHOOK {
                UnhookWindowsHookEx(hm);
            }
            if hk != 0 as HHOOK {
                UnhookWindowsHookEx(hk);
            }

            HOOK_THREAD_ID.store(0, Ordering::SeqCst);
        })
        .map_err(|e| e.to_string())?;

    *HOOK_THREAD.lock() = Some(handle);
    Ok(())
}

pub fn stop() {
    if !RUNNING.swap(false, Ordering::SeqCst) {
        return;
    }
    let tid = HOOK_THREAD_ID.swap(0, Ordering::SeqCst);
    if tid != 0 {
        unsafe {
            PostThreadMessageW(tid, WM_QUIT, 0, 0);
        }
    }
    if let Some(handle) = HOOK_THREAD.lock().take() {
        let _ = handle.join();
    }
}
