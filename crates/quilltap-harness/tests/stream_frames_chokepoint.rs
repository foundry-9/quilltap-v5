//! P4.D366 — the stream-frames CHOKEPOINT census (modelled on v4
//! `__tests__/unit/lib/chat/stream-frames-chokepoint.test.ts`): ONE renderer
//! (`quilltap_core::services::stream_frames`) is the only producer of chat
//! frame bytes.
//!
//! Scans the production zones (every `#[cfg(test)] mod … { … }` block and
//! every `tests.rs` / `tests/` file excluded; `stream_frames.rs` itself
//! excluded) of `crates/quilltap-core/src/{services,storyteller}`,
//! `crates/quilltap-core/src/yield_signal.rs` and `crates/quilltap-host/src`,
//! and asserts:
//!
//! 1. `ChatEvent`'s derive line carries no `Serialize` (ruling L2 — the
//!    compiler enforces the chokepoint; this pins the ruling itself).
//! 2. ZERO `TaggingSink` / `TaggedEvent` / `sse_tag` / `tag_frame` (the
//!    pre-4.12 seat retired; `stream_frames` renders the seat).
//! 3. ZERO hand-built frame literals: `json!({ "done"`, `json!({ "kind":
//!    "<frame kind>"` (each of the 20 kinds), `"swipeDone"`,
//!    `json!({ "error": "Failed to generate`.
//! 4. Production callers of the LEGACY conveniences, by NUMBER, by file:
//!    `Event::chat(` = 0, and `Event::chat_error(` = exactly the TWO named
//!    legacy shells (Brahma, Help) in `quilltap-host/src/spine.rs`.
//! 5. The scanned-file count clears a floor, so a moved tree cannot pass
//!    vacuously (v4's `toBeGreaterThan(30)` idiom).
//!
//! Mutation proofs (each recorded in the P4.D366 lane record): deriving
//! `Serialize` back onto `ChatEvent`, re-adding a `TaggingSink` mention, a
//! `json!({ "done": true })` in `regenerate_swipe.rs`, an `Event::chat(` in
//! `spine.rs`, and pointing the scan at an empty directory each turn this red.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs")
            && p.file_name()
                .is_some_and(|n| n != "tests.rs" && n != "stream_frames.rs")
        {
            out.push(p);
        }
    }
}

/// The file minus every `#[cfg(test)]`-gated `mod` block (brace-matched).
fn production_zone(src: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        if t == "#[cfg(test)]" {
            // The gated item: skip to the end of its braces (or its `;`).
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim().starts_with("#[") {
                j += 1;
            }
            if j < lines.len() && lines[j].trim_start().starts_with("mod ") {
                let mut depth: i64 = 0;
                let mut seen = false;
                while j < lines.len() {
                    for c in lines[j].chars() {
                        if c == '{' {
                            depth += 1;
                            seen = true;
                        } else if c == '}' {
                            depth -= 1;
                        }
                    }
                    if (seen && depth == 0) || (!seen && lines[j].trim_end().ends_with(';')) {
                        break;
                    }
                    j += 1;
                }
                i = j + 1;
                continue;
            }
        }
        out.push_str(lines[i]);
        out.push('\n');
        i += 1;
    }
    out
}

fn scanned() -> Vec<(PathBuf, String)> {
    let r = root();
    let mut files = Vec::new();
    rs_files(&r.join("crates/quilltap-core/src/services"), &mut files);
    rs_files(&r.join("crates/quilltap-core/src/storyteller"), &mut files);
    files.push(r.join("crates/quilltap-core/src/yield_signal.rs"));
    rs_files(&r.join("crates/quilltap-host/src"), &mut files);
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let src = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p:?}: {e}"));
            (p, production_zone(&src))
        })
        .collect()
}

#[test]
fn chat_event_derives_no_serialize() {
    let p = root().join("crates/quilltap-core/src/services/chat_events.rs");
    let src = std::fs::read_to_string(&p).unwrap();
    let lines: Vec<&str> = src.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.starts_with("pub enum ChatEvent"))
        .expect("ChatEvent is declared in chat_events.rs");
    let derive = lines[..at]
        .iter()
        .rev()
        .find(|l| l.starts_with("#[derive("))
        .expect("ChatEvent has a derive line");
    assert!(
        !derive.contains("Serialize"),
        "ChatEvent must not derive Serialize (P4.D366 L2): {derive}"
    );
    assert!(
        !lines[at.saturating_sub(4)..at]
            .iter()
            .any(|l| l.contains("#[serde(")),
        "ChatEvent carries a serde attribute again"
    );
}

#[test]
fn the_retired_seat_and_hand_built_frames_are_gone() {
    let files = scanned();
    assert!(
        files.len() > 30,
        "the census scanned only {} files — has the tree moved?",
        files.len()
    );
    // `json!({ "kind": "<a frame kind>"` — a `kind` key alone is no frame (the
    // announcer's `{kind: "character"}` sender), so the frame kinds are named.
    let frame_kinds = [
        "content",
        "reasoning",
        "status",
        "fileProcessing",
        "toolsDetected",
        "toolResult",
        "turnStart",
        "done",
        "turnComplete",
        "chainComplete",
        "error",
        "pendingExternalTurn",
        "posted",
        "storytellerRound",
        "storytellerBid",
        "roundOpen",
        "handAsked",
        "handAnswered",
        "confirmationResult",
        "swipeDone",
    ];
    let kind_literals: Vec<String> = frame_kinds
        .iter()
        .map(|k| format!("json!({{ \"kind\": \"{k}\""))
        .collect();
    let mut forbidden: Vec<&str> = vec![
        "TaggingSink",
        "TaggedEvent",
        "sse_tag",
        "tag_frame",
        "json!({ \"done\"",
        "\"swipeDone\"",
        "json!({ \"error\": \"Failed to generate",
    ];
    forbidden.extend(kind_literals.iter().map(String::as_str));
    let mut hits = Vec::new();
    for (p, zone) in &files {
        for f in &forbidden {
            let n = zone.matches(f).count();
            if n > 0 {
                hits.push(format!("{} × {:?} in {}", n, f, p.display()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "frame bytes built outside stream_frames:\n{}",
        hits.join("\n")
    );
}

#[test]
fn the_legacy_conveniences_have_only_the_two_named_callers() {
    let files = scanned();
    let mut chat = 0usize;
    let mut chat_error: Vec<(String, usize)> = Vec::new();
    for (p, zone) in &files {
        chat += zone.matches("Event::chat(").count();
        let n = zone.matches("Event::chat_error(").count();
        if n > 0 {
            chat_error.push((p.file_name().unwrap().to_string_lossy().into_owned(), n));
        }
    }
    assert_eq!(chat, 0, "a production caller of the legacy Event::chat(");
    assert_eq!(
        chat_error,
        vec![("spine.rs".to_string(), 2)],
        "Event::chat_error( must have exactly the Brahma + Help shells"
    );
    // And they ARE those two shells.
    let spine = files
        .iter()
        .find(|(p, _)| p.ends_with("quilltap-host/src/spine.rs"))
        .map(|(_, z)| z.clone())
        .expect("spine.rs scanned");
    for shell in ["fn run_brahma_send", "fn run_help_send"] {
        let at = spine
            .find(shell)
            .unwrap_or_else(|| panic!("{shell} exists"));
        let next_fn = spine[at + shell.len()..]
            .find("\n    async fn ")
            .map_or(spine.len(), |o| at + shell.len() + o);
        assert_eq!(
            spine[at..next_fn].matches("Event::chat_error(").count(),
            1,
            "{shell} keeps its one legacy error shell"
        );
    }
}
