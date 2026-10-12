//! Tier-1 differential (P4.D279, the wardrobe keystone — v4 `4a38222e6`): the
//! wardrobe folder rules.
//!
//! Diffs [`quilltap_core::wardrobe_folders`] against v4's REAL
//! `lib/wardrobe/folders.ts` (and `WardrobeFolderPathSchema`'s refine message
//! from `lib/schemas/wardrobe.types.ts`), string-exact, over a corpus that
//! carries v4's own `folders.test.ts` case-for-case plus the shapes it leaves
//! implicit (hand-made spellings under `resolveWardrobeFolder`, a case-only
//! match, the depth cap across an existing ancestor, the reserved `images`
//! segment at the first level vs nested, `../x`, backslash paths, whitespace-
//! only input, the item-picture folder under `folderOfWardrobePath`, non-ASCII
//! and astral segments, the 100-code-unit sanitize cap, a case-different
//! ancestor under `rebaseFolder`). A throwing v4 call is an `Err` here, its
//! message compared exactly.
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/wardrobe-folders.ts \
//!     > /tmp/oracle-wardrobe-folders.ndjson
//! Run:
//!   QT_ORACLE_WARDROBE_FOLDERS=/tmp/oracle-wardrobe-folders.ndjson \
//!     cargo test -p quilltap-harness --test wardrobe_folders_equivalence -- --nocapture

use quilltap_core::wardrobe_folders::{
    conflict_default_message, folder_crumb, folder_name, folder_of_wardrobe_path, folder_segments,
    is_safe_wardrobe_folder_path, is_same_folder, is_under_folder, is_valid_wardrobe_folder_path,
    join_folder, normalize_wardrobe_folder_name, normalize_wardrobe_folder_path,
    normalize_wardrobe_folder_path_or_root, not_found_message, not_in_location_message,
    parent_folder, rebase_folder, resolve_wardrobe_folder, wardrobe_folder_store_path,
    WardrobeFolderPathError, FOLDER_SCHEMA_MESSAGE, WARDROBE_FOLDER_MAX_DEPTH,
    WARDROBE_RESERVED_FOLDER_SEGMENTS,
};
use serde::Deserialize;

/// The corpus's own marker — a stale NDJSON from before this lane cannot pass
/// quietly (`oracle-regen-silent-stale-pass`).
const BASELINE: &str = "p4.d279-wardrobe-folders";

/// v4's `WardrobeFolderPathError` class name — the only error the pure rules
/// throw; any other name on a row is a corpus defect, not a port diff.
const PATH_ERROR_NAME: &str = "WardrobeFolderPathError";

/// v4's `{ ok: true, value } | { ok: false, name, message }`.
#[derive(Deserialize)]
struct Outcome {
    ok: bool,
    value: Option<String>,
    name: Option<String>,
    message: Option<String>,
}

fn assert_outcome(got: &Result<String, WardrobeFolderPathError>, want: &Outcome, label: &str) {
    match (got, want.ok) {
        (Ok(value), true) => {
            assert_eq!(Some(value), want.value.as_ref(), "{label}: value");
        }
        (Err(err), false) => {
            assert_eq!(
                want.name.as_deref(),
                Some(PATH_ERROR_NAME),
                "{label}: v4 threw a different error class"
            );
            assert_eq!(Some(&err.0), want.message.as_ref(), "{label}: message");
        }
        (Ok(value), false) => panic!(
            "{label}: v5 returned {value:?} where v4 threw {:?}",
            want.message
        ),
        (Err(err), true) => panic!(
            "{label}: v5 errored {:?} where v4 returned {:?}",
            err.0, want.value
        ),
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "meta")]
    Meta {
        baseline: String,
        #[serde(rename = "maxDepth")]
        max_depth: usize,
        reserved: Vec<String>,
    },
    #[serde(rename = "messages")]
    Messages {
        #[serde(rename = "notFound")]
        not_found: String,
        #[serde(rename = "notFoundRoot")]
        not_found_root: String,
        conflict: String,
        #[serde(rename = "conflictCustom")]
        conflict_custom: String,
        #[serde(rename = "notInLocation")]
        not_in_location: String,
        #[serde(rename = "notInLocationOne")]
        not_in_location_one: String,
        #[serde(rename = "notInLocationNone")]
        not_in_location_none: String,
        schema: String,
        #[serde(rename = "schemaColon")]
        schema_colon: String,
    },
    #[serde(rename = "normalize")]
    Normalize {
        id: String,
        input: Option<String>,
        out: Outcome,
    },
    #[serde(rename = "normalize_or_root")]
    NormalizeOrRoot {
        id: String,
        input: Option<String>,
        out: String,
    },
    #[serde(rename = "normalize_name")]
    NormalizeName {
        id: String,
        input: Option<String>,
        out: Outcome,
    },
    #[serde(rename = "safe")]
    Safe { id: String, path: String, out: bool },
    #[serde(rename = "valid")]
    Valid { id: String, path: String, out: bool },
    #[serde(rename = "resolve")]
    Resolve {
        id: String,
        existing: Vec<String>,
        requested: Option<String>,
        out: Outcome,
    },
    #[serde(rename = "segments")]
    Segments {
        id: String,
        folder: String,
        out: Vec<String>,
    },
    #[serde(rename = "folder_of")]
    FolderOf {
        id: String,
        path: String,
        out: String,
    },
    #[serde(rename = "folder_views")]
    FolderViews {
        id: String,
        folder: String,
        #[serde(rename = "storePath")]
        store_path: String,
        parent: String,
        name: String,
        crumb: String,
    },
    #[serde(rename = "join")]
    Join {
        id: String,
        parts: Vec<String>,
        out: String,
    },
    #[serde(rename = "same")]
    Same {
        id: String,
        a: Option<String>,
        b: Option<String>,
        out: bool,
    },
    #[serde(rename = "under")]
    Under {
        id: String,
        folder: String,
        ancestor: String,
        out: bool,
    },
    #[serde(rename = "rebase")]
    Rebase {
        id: String,
        folder: String,
        from: String,
        to: String,
        out: String,
    },
}

#[test]
fn wardrobe_folders_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_WARDROBE_FOLDERS") else {
        eprintln!("SKIP: set QT_ORACLE_WARDROBE_FOLDERS to the oracle NDJSON (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut saw_meta, mut saw_messages) = (false, false);
    let (mut normalizes, mut or_roots, mut names, mut safes, mut valids, mut resolves) =
        (0, 0, 0, 0, 0, 0);
    let (mut segments, mut folder_ofs, mut views, mut joins, mut sames, mut unders, mut rebases) =
        (0, 0, 0, 0, 0, 0, 0);
    // Both outcomes of every throwing function must be present — a corpus that
    // only returns (or only throws) is blind to half the contract.
    let (mut normalize_errs, mut name_errs, mut resolve_errs) = (0, 0, 0);
    let mut non_ascii_rows = 0;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).expect("oracle row") {
            Row::Meta {
                baseline,
                max_depth,
                reserved,
            } => {
                assert_eq!(
                    baseline, BASELINE,
                    "the oracle NDJSON predates this lane — regenerate it"
                );
                assert_eq!(
                    max_depth, WARDROBE_FOLDER_MAX_DEPTH,
                    "WARDROBE_FOLDER_MAX_DEPTH"
                );
                assert_eq!(
                    reserved, WARDROBE_RESERVED_FOLDER_SEGMENTS,
                    "WARDROBE_RESERVED_FOLDER_SEGMENTS"
                );
                saw_meta = true;
            }

            Row::Messages {
                not_found,
                not_found_root,
                conflict,
                conflict_custom,
                not_in_location,
                not_in_location_one,
                not_in_location_none,
                schema,
                schema_colon,
            } => {
                assert_eq!(not_found_message("Formal/Evening"), not_found);
                assert_eq!(not_found_message(""), not_found_root);
                assert_eq!(conflict_default_message("Formal"), conflict);
                // The custom message replaces the default whole (v4 `message ??`).
                assert_eq!(conflict_custom, "Holds files that are not items");
                let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
                assert_eq!(
                    not_in_location_message(&ids(&["i-1", "i-2"])),
                    not_in_location
                );
                assert_eq!(not_in_location_message(&ids(&["i-1"])), not_in_location_one);
                assert_eq!(not_in_location_message(&[]), not_in_location_none);
                assert_eq!(
                    FOLDER_SCHEMA_MESSAGE, schema,
                    "WardrobeFolderPathSchema refine"
                );
                assert_eq!(
                    FOLDER_SCHEMA_MESSAGE, schema_colon,
                    "WardrobeFolderPathSchema refine"
                );
                saw_messages = true;
            }

            Row::Normalize { id, input, out } => {
                let got = normalize_wardrobe_folder_path(input.as_deref());
                assert_outcome(&got, &out, &format!("normalize '{id}'"));
                if !out.ok {
                    normalize_errs += 1;
                }
                if input.as_deref().is_some_and(|s| !s.is_ascii()) {
                    non_ascii_rows += 1;
                }
                normalizes += 1;
            }

            Row::NormalizeOrRoot { id, input, out } => {
                assert_eq!(
                    normalize_wardrobe_folder_path_or_root(input.as_deref()),
                    out,
                    "normalize_or_root '{id}'"
                );
                or_roots += 1;
            }

            Row::NormalizeName { id, input, out } => {
                let got = normalize_wardrobe_folder_name(input.as_deref());
                assert_outcome(&got, &out, &format!("normalize_name '{id}'"));
                if !out.ok {
                    name_errs += 1;
                }
                names += 1;
            }

            Row::Safe { id, path, out } => {
                assert_eq!(is_safe_wardrobe_folder_path(&path), out, "safe '{id}'");
                if !path.is_ascii() {
                    non_ascii_rows += 1;
                }
                safes += 1;
            }

            Row::Valid { id, path, out } => {
                assert_eq!(is_valid_wardrobe_folder_path(&path), out, "valid '{id}'");
                valids += 1;
            }

            Row::Resolve {
                id,
                existing,
                requested,
                out,
            } => {
                let got = resolve_wardrobe_folder(&existing, requested.as_deref());
                assert_outcome(&got, &out, &format!("resolve '{id}'"));
                if !out.ok {
                    resolve_errs += 1;
                }
                resolves += 1;
            }

            Row::Segments { id, folder, out } => {
                assert_eq!(folder_segments(&folder), out, "segments '{id}'");
                segments += 1;
            }

            Row::FolderOf { id, path, out } => {
                assert_eq!(folder_of_wardrobe_path(&path), out, "folder_of '{id}'");
                if !path.is_ascii() {
                    non_ascii_rows += 1;
                }
                folder_ofs += 1;
            }

            Row::FolderViews {
                id,
                folder,
                store_path,
                parent,
                name,
                crumb,
            } => {
                assert_eq!(
                    wardrobe_folder_store_path(&folder),
                    store_path,
                    "store_path '{id}'"
                );
                assert_eq!(parent_folder(&folder), parent, "parent '{id}'");
                assert_eq!(folder_name(&folder), name, "name '{id}'");
                assert_eq!(folder_crumb(&folder), crumb, "crumb '{id}'");
                views += 1;
            }

            Row::Join { id, parts, out } => {
                let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
                assert_eq!(join_folder(&parts), out, "join '{id}'");
                joins += 1;
            }

            Row::Same { id, a, b, out } => {
                assert_eq!(
                    is_same_folder(a.as_deref(), b.as_deref()),
                    out,
                    "same '{id}'"
                );
                sames += 1;
            }

            Row::Under {
                id,
                folder,
                ancestor,
                out,
            } => {
                assert_eq!(is_under_folder(&folder, &ancestor), out, "under '{id}'");
                unders += 1;
            }

            Row::Rebase {
                id,
                folder,
                from,
                to,
                out,
            } => {
                assert_eq!(rebase_folder(&folder, &from, &to), out, "rebase '{id}'");
                if !folder.is_ascii() {
                    non_ascii_rows += 1;
                }
                rebases += 1;
            }
        }
    }

    assert!(saw_meta, "oracle file carries no meta row: {path}");
    assert!(saw_messages, "oracle file carries no messages row: {path}");
    // Shape assertions, not hand counts (`harness-corpus-shape-constants-rot`):
    // every arm exercised, both outcomes of every throwing function present,
    // and the non-ASCII shapes (the UTF-16 `.length`/`.slice` seam) covered.
    assert!(normalizes >= 30, "normalize cases: {normalizes}");
    assert!(
        normalize_errs >= 8,
        "normalize throw arms: {normalize_errs}"
    );
    assert!(or_roots >= 5, "normalize_or_root cases: {or_roots}");
    assert!(names >= 9, "normalize_name cases: {names}");
    assert!(name_errs >= 5, "normalize_name throw arms: {name_errs}");
    assert!(safes >= 15, "safe cases: {safes}");
    assert!(valids >= 10, "valid cases: {valids}");
    assert!(resolves >= 25, "resolve cases: {resolves}");
    assert!(resolve_errs >= 6, "resolve throw arms: {resolve_errs}");
    assert!(segments >= 5, "segments cases: {segments}");
    assert!(folder_ofs >= 12, "folder_of cases: {folder_ofs}");
    assert!(views >= 6, "folder_views cases: {views}");
    assert!(joins >= 7, "join cases: {joins}");
    assert!(sames >= 7, "same cases: {sames}");
    assert!(unders >= 8, "under cases: {unders}");
    assert!(rebases >= 10, "rebase cases: {rebases}");
    assert!(non_ascii_rows >= 8, "non-ASCII rows: {non_ascii_rows}");
    eprintln!(
        "OK: wardrobe-folders matched oracle ({normalizes} normalize / {or_roots} or_root / \
         {names} name / {safes} safe / {valids} valid / {resolves} resolve / {segments} segments / \
         {folder_ofs} folder_of / {views} views / {joins} join / {sames} same / {unders} under / \
         {rebases} rebase cases)."
    );
}
