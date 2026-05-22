//! All `#[tauri::command]` functions live under this module so the registry
//! call in `lib.rs` stays a single line and the surface area is grep-able.

pub mod editor;
pub mod input;
pub mod overlay;
pub mod recorder;
pub mod screenshot;
pub mod system;
pub mod webcam;
