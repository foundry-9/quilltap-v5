//! The shell-menu guard (dogfood finding #13): the app menu must carry a
//! View → Reload item under the reload id — without it macOS drops Cmd+R
//! (key equivalents route through the menu) and the §3 deep-link reload
//! path is unreachable by keyboard. Built against the mock runtime; the
//! visual half (the webview actually reloading) is the human walk's.
//!
//! Harness-free (`harness = false` in Cargo.toml): muda refuses menu
//! construction off the platform main thread on macOS, and libtest runs
//! `#[test]` fns on worker threads — this `main()` IS the main thread. A
//! panic anywhere fails the binary, which fails `cargo test`.

use quilltap_tauri::menu::{build_app_menu, RELOAD_ACCELERATOR, RELOAD_MENU_ID};
use tauri::menu::MenuItemKind;

fn main() {
    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    let menu = build_app_menu(app.handle()).expect("menu builds");

    let view = menu
        .items()
        .expect("menu items")
        .into_iter()
        .find_map(|item| match item {
            MenuItemKind::Submenu(s) if s.text().unwrap() == "View" => Some(s),
            _ => None,
        })
        .expect("a View submenu exists");
    let first = view
        .items()
        .expect("view items")
        .into_iter()
        .next()
        .expect("the View submenu is non-empty");
    match first {
        MenuItemKind::MenuItem(item) => {
            assert_eq!(item.id(), RELOAD_MENU_ID);
            assert_eq!(item.text().unwrap(), "Reload");
        }
        other => panic!("first View item is not the Reload item: {:?}", other.id()),
    }

    // The keystroke is fixed at item construction; the constant is what
    // build_app_menu hands to MenuItem::with_id (no accelerator getter
    // exists to read it back off the built item).
    assert_eq!(RELOAD_ACCELERATOR, "CmdOrCtrl+R");

    // The default menu survived the patch — Cmd+Q and copy/paste live there.
    let submenus: Vec<String> = menu
        .items()
        .unwrap()
        .iter()
        .filter_map(|i| match i {
            MenuItemKind::Submenu(s) => s.text().ok(),
            _ => None,
        })
        .collect();
    assert!(
        submenus.iter().any(|t| t == "Edit"),
        "Edit kept: {submenus:?}"
    );
    assert!(
        submenus.iter().any(|t| t == "Window"),
        "Window kept: {submenus:?}"
    );

    println!("menu_contract: app_menu_carries_view_reload ... ok");
}
