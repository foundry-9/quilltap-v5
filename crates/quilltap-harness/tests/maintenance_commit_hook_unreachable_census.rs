//! R-L's census (P4.D297): v4's maintenance COMMIT HOOK is unreachable at the
//! pin, so v5 does not port it.
//!
//! v4's dispatcher runs `maybeEnqueueMaintenanceAfterCommit(jobType, writes)`
//! (`lib/memory/maintenance/triggers.ts`) after every job batch commits: it
//! reads the characters the batch CREATED memories for
//! (`characterIdsFromMemoryWrites` — `memories.create` writes only) and runs
//! the watermark for them. v5 has no buffered write list; P4.D271 R-B carried
//! the hook as a mark log + a `WATERMARK_ROWID_WINDOW` table. This census
//! proves no reachable job batch yields a character, so the hook, the mark
//! log and the window table all retire (and the ONE live watermark caller is
//! admission's after-insert site — the unifier's `HANDOFF:`, §R.10(h)):
//!
//! 1. v4 calls `memories.create(` from EXACTLY four non-test sites —
//!    admission's `insertRow` (`lib/memory/admission/apply.ts`), the
//!    consolidation plan's digest create (`lib/memory/consolidation.ts`), the
//!    backup restore and the `.qtap` import.
//! 2. `insertRow` is called only from `admit.ts`'s `applyAction`, which only
//!    `admitPrepared` calls — and `admitPrepared` THROWS in the job child
//!    ("admitPrepared runs in the parent"): a job admits over host-RPC, so
//!    its batch never carries the create.
//! 3. The digest create sits in `executePlan`, run only by the maintenance
//!    job — and the hook's own type gate (`if (jobType ===
//!    'MEMORY_MAINTENANCE') return`) excludes that batch.
//! 4. No job handler reaches the restore or the import (nothing under
//!    `lib/background-jobs/` imports either).
//!
//! A future v4 buffered create (a fifth caller, a lifted gate, a job that
//! restores) reddens this census — the convergence tripwire: re-measure, then
//! port the hook if it has become reachable.
//!
//! Reads the v4 checkout at test time through `common::v4_root`
//! (`QT_V4_CHECKOUT`, then `QT_V4_ROOT`, then `$HOME/source/quilltap-server`)
//! — point it at the round's PIN (`QT_V4_ROOT=/tmp/qt-v4-pin-…`); an absent
//! checkout prints a loud `SKIP:` line. No recipe stage: nothing to regenerate.
//!
//! Run standalone (export `QT_V4_ROOT` at the round's pin first):
//!   cargo test -p quilltap-harness --test maintenance_commit_hook_unreachable_census

use std::path::{Path, PathBuf};

mod common;

/// The four `memories.create(` callers at the `83334b9a5` pin.
const CREATE_CALLERS: [&str; 4] = [
    "lib/backup/restore/restore.ts",
    "lib/import/quilltap-import/import-entities.ts",
    "lib/memory/admission/apply.ts",
    "lib/memory/consolidation.ts",
];

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if matches!(
                name.as_str(),
                "node_modules" | ".next" | "__tests__" | "dist"
            ) {
                continue;
            }
            walk(&path, out);
        } else if (name.ends_with(".ts") || name.ends_with(".tsx"))
            && !name.contains(".test.")
            && !name.ends_with(".d.ts")
        {
            out.push(path);
        }
    }
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// The body of `fn_name` (from its declaration to the next top-level `}`).
fn function_body<'a>(src: &'a str, decl: &str) -> &'a str {
    let start = src
        .find(decl)
        .unwrap_or_else(|| panic!("declaration {decl:?} not found"));
    let rest = &src[start..];
    let end = rest.find("\n}\n").map_or(rest.len(), |e| e + 3);
    &rest[..end]
}

#[test]
fn maintenance_commit_hook_is_unreachable_at_the_pin() {
    let Some(root) = common::v4_root().filter(|r| r.join("lib").is_dir()) else {
        eprintln!(
            "SKIP: no v4 checkout (set QT_V4_ROOT to the round's pin) — the commit-hook census cannot run."
        );
        return;
    };
    let mut failures: Vec<String> = Vec::new();

    // ── 1. exactly the four `memories.create(` callers ─────────────────────
    let mut files = Vec::new();
    for top in ["lib", "app", "packages", "plugins"] {
        walk(&root.join(top), &mut files);
    }
    let mut callers: Vec<(String, usize)> = files
        .iter()
        .filter_map(|p| {
            let src = std::fs::read_to_string(p).ok()?;
            let n = src.matches("memories.create(").count();
            (n > 0).then(|| {
                (
                    p.strip_prefix(&root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    n,
                )
            })
        })
        .collect();
    callers.sort();
    let names: Vec<&str> = callers.iter().map(|(p, _)| p.as_str()).collect();
    if names != CREATE_CALLERS {
        failures.push(format!(
            "memories.create( callers moved: {callers:?} (expected {CREATE_CALLERS:?})"
        ));
    }
    for (path, n) in &callers {
        if *n != 1 {
            failures.push(format!("{path}: {n} memories.create( calls (expected 1)"));
        }
    }

    // ── 2. admission's insert runs in the parent only ──────────────────────
    let apply = read(&root, "lib/memory/admission/apply.ts");
    if !function_body(&apply, "export async function insertRow(").contains("memories.create(") {
        failures.push("apply.ts: the create is no longer inside insertRow".into());
    }
    let insert_callers: Vec<String> = files
        .iter()
        .filter(|p| {
            std::fs::read_to_string(p)
                .map(|s| s.contains("insertRow("))
                .unwrap_or(false)
        })
        .map(|p| p.strip_prefix(&root).unwrap().to_string_lossy().to_string())
        .filter(|p| !p.ends_with("admission/apply.ts"))
        .collect();
    if insert_callers != ["lib/memory/admission/admit.ts"] {
        failures.push(format!("insertRow callers moved: {insert_callers:?}"));
    }
    let admit = read(&root, "lib/memory/admission/admit.ts");
    let prepared = function_body(&admit, "export async function admitPrepared(");
    if !prepared.contains("if (isJobChild()) {")
        || !prepared.contains("throw new Error('admitPrepared runs in the parent")
    {
        failures.push("admit.ts: admitPrepared no longer throws in the job child".into());
    }
    // `insertRow` runs from `applyAction` (its insert arms), which only
    // `admitPrepared` calls.
    let apply_action = function_body(&admit, "async function applyAction(");
    if admit.matches("insertRow(").count() != apply_action.matches("insertRow(").count() {
        failures.push("admit.ts: insertRow is called outside applyAction".into());
    }
    if admit.matches("applyAction(").count() != prepared.matches("applyAction(").count() + 1 {
        failures.push("admit.ts: applyAction is called outside admitPrepared".into());
    }

    // ── 3. the digest create is the maintenance job's, which the hook skips ─
    let consolidation = read(&root, "lib/memory/consolidation.ts");
    if !function_body(&consolidation, "async function executePlan(").contains("memories.create(") {
        failures.push("consolidation.ts: the create is no longer inside executePlan".into());
    }
    let triggers = read(&root, "lib/memory/maintenance/triggers.ts");
    let hook = function_body(
        &triggers,
        "export async function maybeEnqueueMaintenanceAfterCommit(",
    );
    if !hook.contains("if (jobType === 'MEMORY_MAINTENANCE') return") {
        failures.push("triggers.ts: the commit hook's MEMORY_MAINTENANCE gate moved".into());
    }

    // ── 4. no job reaches the restore or the import ────────────────────────
    let mut jobs = Vec::new();
    walk(&root.join("lib/background-jobs"), &mut jobs);
    for p in &jobs {
        let src = std::fs::read_to_string(p).unwrap_or_default();
        if src.contains("@/lib/backup/restore") || src.contains("quilltap-import") {
            failures.push(format!(
                "{} reaches the restore / the import from a job",
                p.strip_prefix(&root).unwrap().display()
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "the maintenance commit hook may have become reachable — re-measure R-L, port \
         maybeEnqueueMaintenanceAfterCommit if so:\n{}",
        failures.join("\n")
    );
    eprintln!(
        "OK: the maintenance commit hook is unreachable at {} (four memories.create( callers, \
         admission parent-only, the digest create gated, no job restores or imports; {} files read).",
        root.display(),
        files.len()
    );
}
