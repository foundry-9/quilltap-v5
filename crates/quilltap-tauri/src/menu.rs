//! The window menu — Tauri's default menu plus the browser affordances the
//! shell must supply itself (the finding-#13 class: v4 is browser-hosted,
//! so these come free from the browser chrome; under the shell they exist
//! only if the shell wires them).
//!
//! Reload (Cmd+R): macOS routes key equivalents through the menu, so with
//! no menu item carrying the accelerator the keystroke is silently dropped
//! — the §3 deep-link index fallback was unreachable by keyboard (found on
//! the M5 human walk, 2026-07-18). The item reloads the main webview in
//! place; the qtap protocol answers the current route with the embedded
//! dist's index fallback, so a deep link lands back in the SPA.

use tauri::menu::{Menu, MenuEvent, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, Runtime};

/// The reload item's id — [`handle_menu_event`] matches on it.
pub const RELOAD_MENU_ID: &str = "reload";

/// The browser reload keystroke (Cmd+R on macOS, Ctrl+R elsewhere).
pub const RELOAD_ACCELERATOR: &str = "CmdOrCtrl+R";

/// Tauri's default menu with `View → Reload` prepended: the default View
/// submenu (fullscreen) gains Reload + a separator at the top; a platform
/// whose default menu carries no View submenu gets one appended holding
/// only Reload. Everything else (app/File/Edit/Window/Help) stays the
/// default — losing it would lose Cmd+Q, copy/paste, etc.
pub fn build_app_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(app)?;
    let reload = MenuItem::with_id(
        app,
        RELOAD_MENU_ID,
        "Reload",
        true,
        Some(RELOAD_ACCELERATOR),
    )?;
    let view = menu.items()?.into_iter().find_map(|item| match item {
        MenuItemKind::Submenu(s) => s.text().map(|t| t == "View").unwrap_or(false).then_some(s),
        _ => None,
    });
    match view {
        Some(view) => view.prepend_items(&[&reload, &PredefinedMenuItem::separator(app)?])?,
        None => menu.append(&Submenu::with_items(app, "View", true, &[&reload])?)?,
    }
    Ok(menu)
}

/// React to the Reload item: reload the main webview in place. No window
/// (e.g. during teardown) is a no-op, and a reload failure has nothing
/// actionable — the user just presses again.
pub fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, event: &MenuEvent) {
    if event.id() == RELOAD_MENU_ID {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.reload();
        }
    }
}
