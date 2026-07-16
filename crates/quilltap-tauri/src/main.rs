//! The Quilltap desktop shell binary (P4.7a) — see the lib for the wiring.

// No extra console window on Windows in release (the Tauri convention).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    quilltap_tauri::run()
}
