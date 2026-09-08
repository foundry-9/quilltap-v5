//! The vendored built-in prompt-template catalogue guard (P4.83).
//!
//! `crates/quilltap-core/src/services/builtin_prompt_templates.json` is a
//! VENDORED v5 artifact: v5 has no plugin system, so those 21 rows ARE the
//! "Sample Prompts" a v5 instance seeds. Their source is v4's
//! `plugins/dist/qtap-plugin-default-system-prompts/prompts/*.md`, read through
//! the plugin's `loadPrompts()` and the system-prompt registry's display-name
//! rule. This test re-derives the whole table from the checkout's `.md` files
//! and holds it byte-identical — so a v4 commit that edits, adds or removes a
//! prompt fails the gate and names the re-vendor obligation, instead of leaving
//! v5 quietly shipping last month's text (the `help_tree_embed_guard` shape).
//!
//! The two rules it re-derives, both transcribed from v4:
//!
//!   - `parsePromptFilename` (`plugins/dist/qtap-plugin-default-system-prompts/
//!     index.ts:26-34`): strip `.md`, split on `_`; the LAST part is the
//!     `category` and the rest rejoined by `_` is the `modelHint`; fewer than
//!     two parts → `{modelHint: baseName, category: 'GENERAL'}`.
//!   - the registry display name (`lib/plugins/system-prompt-registry.ts:186`):
//!     `` `${modelHint} ${category[0] + category.slice(1).toLowerCase()}` ``.
//!     ⚠ THIS, not the filename, is the seeded row's `name`.
//!
//! The file ORDER is `readdirSync(...).filter(.md).sort()` — JS's default sort,
//! UTF-16 code-unit order. Every name here is ASCII, so Rust's byte sort agrees;
//! the assertion is on the resulting list, so a name that ever broke that
//! agreement would show up as an order mismatch rather than pass silently.
//!
//! The locator is `QT_V4_CHECKOUT` (default `$HOME/source/quilltap-server`, the
//! convention every recipe header uses). An absent checkout prints a loud
//! `SKIP:` — a machine without the v4 tree must not fail here — and never a
//! silent pass. A checkout whose prompts have MOVED is a FAIL, never a skip.
//!
//! Regenerate the vendored table (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/provision/dump-prompt-templates.ts
//!   cd $V5W
//!   cargo test -p quilltap-harness --test builtin_prompt_templates_guard

use std::path::PathBuf;

use quilltap_core::services::builtin_prompt_templates::catalogue;

const PLUGIN_ID: &str = "default-system-prompts";

fn v4_checkout() -> PathBuf {
    match std::env::var("QT_V4_CHECKOUT") {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => {
            let home = std::env::var("HOME").expect("HOME must be set to locate the v4 checkout");
            PathBuf::from(home).join("source/quilltap-server")
        }
    }
}

/// v4 `parsePromptFilename` (the plugin's own helper), verbatim.
fn parse_prompt_filename(base_name: &str) -> (String, String) {
    let parts: Vec<&str> = base_name.split('_').collect();
    if parts.len() < 2 {
        return (base_name.to_string(), "GENERAL".to_string());
    }
    let category = parts[parts.len() - 1].to_string();
    let model_hint = parts[..parts.len() - 1].join("_");
    (model_hint, category)
}

/// v4 `loadPromptsFromPlugin`'s `displayName` — `${modelHint} ${category
/// .charAt(0) + category.slice(1).toLowerCase()}`. JS `charAt(0)`/`slice(1)`
/// are UTF-16 units; every shipped category is ASCII, so `char_indices` is
/// equivalent here (a non-ASCII category would be caught as a mismatch).
fn display_name(model_hint: &str, category: &str) -> String {
    let mut chars = category.chars();
    let head: String = chars.next().map(String::from).unwrap_or_default();
    let tail: String = chars.as_str().to_lowercase();
    format!("{model_hint} {head}{tail}")
}

#[test]
fn the_vendored_catalogue_equals_v4s_shipped_prompts() {
    let checkout = v4_checkout();
    let prompts_dir = checkout.join(format!("plugins/dist/qtap-plugin-{PLUGIN_ID}/prompts"));
    if !prompts_dir.is_dir() {
        println!(
            "SKIP: no v4 prompts directory at {} (set QT_V4_CHECKOUT) — cannot verify the \
             vendored built-in prompt-template catalogue on this machine",
            prompts_dir.display()
        );
        return;
    }

    // `readdirSync(...).filter(f => f.endsWith('.md')).sort()`. Rust's read_dir
    // is UNORDERED (the `node-readdir-is-sorted-rust-read-dir-is-not` trap), so
    // the sort here is not decoration.
    let mut files: Vec<String> = std::fs::read_dir(&prompts_dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", prompts_dir.display()))
        .map(|e| {
            e.expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| n.ends_with(".md"))
        .collect();
    files.sort();

    let expected: Vec<(String, String, String, String, String)> = files
        .iter()
        .map(|file| {
            let base = file.strip_suffix(".md").expect("filtered on .md");
            let (model_hint, category) = parse_prompt_filename(base);
            let content = std::fs::read_to_string(prompts_dir.join(file))
                .unwrap_or_else(|e| panic!("read {file}: {e}"));
            (
                format!("{PLUGIN_ID}/{base}"),
                display_name(&model_hint, &category),
                content,
                model_hint,
                category,
            )
        })
        .collect();

    let vendored = catalogue().expect("the vendored catalogue parses");

    assert_eq!(
        vendored.len(),
        expected.len(),
        "the vendored catalogue holds {} prompts, v4's plugin ships {} — RE-VENDOR: run \
         harness/oracle/provision/dump-prompt-templates.ts from the v4 checkout (its header \
         carries the invocation) and commit the regenerated \
         crates/quilltap-core/src/services/builtin_prompt_templates.json",
        vendored.len(),
        expected.len()
    );

    for (got, (prompt_id, name, content, model_hint, category)) in
        vendored.iter().zip(expected.iter())
    {
        assert_eq!(
            &got.prompt_id, prompt_id,
            "registry id (order or membership) moved"
        );
        assert_eq!(&got.name, name, "display name for {prompt_id} moved");
        assert_eq!(
            &got.model_hint, model_hint,
            "modelHint for {prompt_id} moved"
        );
        assert_eq!(&got.category, category, "category for {prompt_id} moved");
        assert!(
            &got.content == content,
            "the vendored content for {prompt_id} differs from the checkout's .md — RE-VENDOR \
             (see harness/oracle/provision/dump-prompt-templates.ts)"
        );
    }
}

#[test]
fn the_filename_rules_are_v4s() {
    // The shipped shapes.
    assert_eq!(
        parse_prompt_filename("CLAUDE_COMPANION"),
        ("CLAUDE".to_string(), "COMPANION".to_string())
    );
    // Multi-part model hints rejoin on '_'.
    assert_eq!(
        parse_prompt_filename("GPT_4O_ROMANTIC"),
        ("GPT_4O".to_string(), "ROMANTIC".to_string())
    );
    // Fewer than two parts.
    assert_eq!(
        parse_prompt_filename("SOLO"),
        ("SOLO".to_string(), "GENERAL".to_string())
    );
    assert_eq!(display_name("MODERN", "GENERAL"), "MODERN General");
    assert_eq!(display_name("GPT4O", "COMPANION"), "GPT4O Companion");
}
