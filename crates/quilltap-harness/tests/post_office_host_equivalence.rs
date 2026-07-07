//! Tier-1 differential for the W4.6b Host-notifications PURE content /
//! opaqueContent builders — the byte-exact steampunk / Wodehouse strings, diffed
//! against v4's REAL exports (`harness/oracle/cases/post-office-host.ts`).
//!
//! Covers every EXPORTED builder branch: `add` (description / empty branches +
//! `{{char}}`/`{{user}}` templating — the identity branch is DB-backed, pinned by
//! a `services::host_notifications` self-test), `remove`, `status-change` (known
//! phrases + unknown fallback), `scenario`, `user-character` (desc/empty), the
//! multi-character roster (alone fallback + company section), silent-mode
//! entry/exit, `join-scenario`, and `timestamp`. The PRIVATE continuation /
//! merge / no-user-character builders are not exported (not driven here); they
//! are covered by transcription + self-tests and the parent's central tier-3
//! differential (persisted `content` column). The POST functions' row shape is
//! also verified by that parent tier-3.
//!
//! Generate (Node 24, from the v4 checkout; `$V5` = the quilltap-v5 checkout —
//! the repo root post-merge, or this worktree in flight):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5/harness/oracle/cases/post-office-host.ts > /tmp/oracle-po-host.ndjson
//! Run:
//!   QT_ORACLE_PO_HOST=/tmp/oracle-po-host.ndjson \
//!     cargo test -p quilltap-harness --test post_office_host_equivalence

use serde_json::Value;

use quilltap_core::message_formatter::{OtherParticipant, ParticipantPronouns};
use quilltap_core::services::host_notifications as hn;

/// Recompute the Rust value for one `(kind, id)` oracle row.
fn rust_value(kind: &str, id: &str) -> Value {
    let s = match kind {
        "add_content" | "add_opaque" => {
            let (name, desc, user) = add_case(id);
            if kind == "add_content" {
                hn::build_add_content(name, None, desc, user)
            } else {
                hn::build_add_opaque_content(name, None, desc, user)
            }
        }
        "remove_content" => hn::build_remove_content(remove_case(id)),
        "remove_opaque" => hn::build_remove_opaque_content(remove_case(id)),
        "status_content" => {
            let (n, f, t) = status_case(id);
            hn::build_status_change_content(n, f, t)
        }
        "status_opaque" => {
            let (n, f, t) = status_case(id);
            hn::build_status_change_opaque_content(n, f, t)
        }
        "scenario_content" => hn::build_scenario_content(scenario_case(id)),
        "scenario_opaque" => hn::build_scenario_opaque_content(scenario_case(id)),
        "user_char_content" => {
            let (n, d) = user_char_case(id);
            hn::build_user_character_content(n, d)
        }
        "user_char_opaque" => {
            let (n, d) = user_char_case(id);
            hn::build_user_character_opaque_content(n, d)
        }
        "roster_content" => {
            let (r, others) = roster_case(id);
            hn::build_multi_character_roster_content(r, &others)
        }
        "roster_opaque" => {
            let (r, others) = roster_case(id);
            hn::build_multi_character_roster_opaque_content(r, &others)
        }
        "silent_entry_content" => hn::build_silent_mode_entry_content(silent_case(id)),
        "silent_entry_opaque" => hn::build_silent_mode_entry_opaque_content(silent_case(id)),
        "silent_exit_content" => hn::build_silent_mode_exit_content(silent_case(id)),
        "silent_exit_opaque" => hn::build_silent_mode_exit_opaque_content(silent_case(id)),
        "join_content" => {
            let (n, sc, u) = join_case(id);
            hn::build_join_scenario_content(n, sc, u)
        }
        "join_opaque" => {
            let (n, sc, u) = join_case(id);
            hn::build_join_scenario_opaque_content(n, sc, u)
        }
        "timestamp_content" => hn::build_timestamp_content(timestamp_case(id)),
        "timestamp_opaque" => hn::build_timestamp_opaque_content(timestamp_case(id)),
        other => panic!("unknown oracle kind {other}"),
    };
    Value::String(s)
}

// ---- case data (mirrors the oracle corpus) ----

fn add_case(id: &str) -> (&'static str, Option<&'static str>, Option<&'static str>) {
    // (name, description, user)
    match id {
        "desc-plain" => ("Ada", Some("A brilliant analyst."), Some("Charlie")),
        "desc-template" => (
            "Ada",
            Some("Known by {{user}}, called {{char}}."),
            Some("Charlie"),
        ),
        "desc-user-null" => ("Cid", Some("A plain description."), None),
        "no-desc" => ("Bea", Some(""), None),
        "desc-only-whitespace" => ("Dot", Some("   "), Some("Eve")),
        other => panic!("unknown add case {other}"),
    }
}

fn remove_case(id: &str) -> &'static str {
    match id {
        "plain" => "Ada",
        "unicode" => "Zoë",
        other => panic!("unknown remove case {other}"),
    }
}

fn status_case(id: &str) -> (&'static str, &'static str, &'static str) {
    match id {
        "active-to-silent" => ("Ada", "active", "silent"),
        "silent-to-active" => ("Ada", "silent", "active"),
        "absent-to-removed" => ("Bea", "absent", "removed"),
        "unknown-from" => ("Cid", "lurking", "active"),
        "unknown-to" => ("Cid", "active", "lurking"),
        other => panic!("unknown status case {other}"),
    }
}

fn scenario_case(id: &str) -> &'static str {
    match id {
        "plain" => "The parlor at dusk.",
        "trailing-ws" => "  The parlor at dusk.  ",
        other => panic!("unknown scenario case {other}"),
    }
}

fn user_char_case(id: &str) -> (&'static str, Option<&'static str>) {
    match id {
        "with-desc-template" => ("Cid", Some("The voice of {{user}}.")),
        "empty-desc" => ("Cid", Some("")),
        "null-desc" => ("Cid", None),
        other => panic!("unknown user-char case {other}"),
    }
}

fn roster_case(id: &str) -> (&'static str, Vec<OtherParticipant>) {
    match id {
        "alone" => ("Ada", vec![]),
        "company" => (
            "Ada",
            vec![
                OtherParticipant {
                    name: "Bea".into(),
                    aliases: Some(vec!["B".into()]),
                    pronouns: Some(ParticipantPronouns {
                        subject: "she".into(),
                        object: "her".into(),
                        possessive: "hers".into(),
                    }),
                    description: Some("A beekeeper.".into()),
                    status: Some("silent".into()),
                },
                OtherParticipant {
                    name: "User Cid".into(),
                    aliases: None,
                    pronouns: None,
                    description: None,
                    status: Some("active".into()),
                },
            ],
        ),
        other => panic!("unknown roster case {other}"),
    }
}

fn silent_case(id: &str) -> &'static str {
    match id {
        "ada" => "Ada",
        other => panic!("unknown silent case {other}"),
    }
}

fn join_case(id: &str) -> (&'static str, &'static str, Option<&'static str>) {
    match id {
        "template-user" => (
            "Ada",
            "You, {{char}}, arrived to find {{user}} waiting.  ",
            Some("Charlie"),
        ),
        "no-user" => ("Ada", "You simply appeared.", None),
        other => panic!("unknown join case {other}"),
    }
}

fn timestamp_case(id: &str) -> &'static str {
    match id {
        "plain" => "Monday, the 3rd of never",
        other => panic!("unknown timestamp case {other}"),
    }
}

#[test]
fn post_office_host_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PO_HOST") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("QT_ORACLE_PO_HOST unset; skipping");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).expect("read oracle ndjson");
    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle row");
        let kind = row["kind"].as_str().unwrap();
        let id = row["id"].as_str().unwrap();
        let want = &row["value"];
        let got = rust_value(kind, id);
        assert_eq!(&got, want, "host-notification builder {kind}/{id} diverges");
        count += 1;
    }
    assert!(count > 0, "oracle had no rows");
    eprintln!("post-office-host: {count} rows matched");
}
