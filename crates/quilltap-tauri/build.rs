fn main() {
    // `frontendDist` points at the Angular build output
    // (../../apps/web/dist/quilltap/browser — the `@angular/build:application`
    // default for the `quilltap` project). A plain `cargo build`/`cargo test`
    // must stay green on a checkout with no SPA build (the dist tree is
    // gitignored), and tauri-build validates the path — so materialize it
    // empty when absent. A real `ng build` fills the same directory.
    let dist = std::path::Path::new("../../apps/web/dist/quilltap/browser");
    if !dist.exists() {
        std::fs::create_dir_all(dist).expect("materialize an empty frontendDist");
    }
    tauri_build::build()
}
