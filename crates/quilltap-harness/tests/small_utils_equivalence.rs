//! Tier-1 differential test #12 (Wave 1 / B4): small pure leaf utilities.
//!
//! Covers chat-type / participant-status predicates, semver parse + compare
//! (parseable pairs only — the localeCompare fallback is deferred), the
//! pronoun→gender hints, the tag-style merge, and the char-count colour class.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/small-utils.ts \
//!     > /tmp/oracle-small-utils.ndjson
//! Run:
//!   QT_ORACLE_SMALL_UTILS=/tmp/oracle-small-utils.ndjson cargo test -p quilltap-harness

use quilltap_core::char_count::char_count_class;
use quilltap_core::chat_predicates::{
    can_receive_whisper, is_help_like_chat_type, is_moderation_exempt_chat_type,
    is_participant_present, migrate_is_active_to_status, ParticipantStatus,
};
use quilltap_core::pronoun_gender::{
    gender_from_pronouns, gender_noun_from_pronouns, gender_prefix_from_pronouns,
};
use quilltap_core::semver::{compare_versions, parse_version};
use quilltap_core::tag_style::{merge_with_default_tag_style, PartialTagVisualStyle};
use serde::Deserialize;

fn status_from_str(s: &str) -> ParticipantStatus {
    match s {
        "active" => ParticipantStatus::Active,
        "silent" => ParticipantStatus::Silent,
        "absent" => ParticipantStatus::Absent,
        "removed" => ParticipantStatus::Removed,
        other => panic!("unknown status {other}"),
    }
}

#[derive(Deserialize)]
struct WireParsed {
    major: i64,
    minor: i64,
    patch: i64,
}

#[derive(Deserialize)]
struct WireGender {
    gender: Option<String>,
    noun: Option<String>,
    prefix: String,
}

#[derive(Deserialize)]
struct WireStyle {
    emoji: Option<String>,
    #[serde(rename = "foregroundColor")]
    foreground_color: String,
    #[serde(rename = "backgroundColor")]
    background_color: String,
    #[serde(rename = "emojiOnly")]
    emoji_only: bool,
    bold: bool,
    italic: bool,
    strikethrough: bool,
}

#[derive(Deserialize, Default)]
struct WirePartialStyle {
    #[serde(default)]
    emoji: Option<String>,
    #[serde(default, rename = "foregroundColor")]
    foreground_color: Option<String>,
    #[serde(default, rename = "backgroundColor")]
    background_color: Option<String>,
    #[serde(default, rename = "emojiOnly")]
    emoji_only: Option<bool>,
    #[serde(default)]
    bold: Option<bool>,
    #[serde(default)]
    italic: Option<bool>,
    #[serde(default)]
    strikethrough: Option<bool>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "chatPred")]
    ChatPred {
        id: String,
        #[serde(rename = "fn")]
        func: String,
        #[serde(rename = "chatType")]
        chat_type: Option<String>,
        out: bool,
    },
    #[serde(rename = "statusPred")]
    StatusPred {
        id: String,
        #[serde(rename = "fn")]
        func: String,
        status: String,
        out: bool,
    },
    #[serde(rename = "migrate")]
    Migrate {
        id: String,
        #[serde(rename = "isActive")]
        is_active: bool,
        #[serde(rename = "removedAt")]
        removed_at: Option<String>,
        out: String,
    },
    #[serde(rename = "parseVer")]
    ParseVer {
        id: String,
        version: String,
        out: Option<WireParsed>,
    },
    #[serde(rename = "compareVer")]
    CompareVer {
        id: String,
        a: String,
        b: String,
        out: i32,
    },
    #[serde(rename = "gender")]
    Gender {
        id: String,
        subject: Option<String>,
        out: WireGender,
    },
    #[serde(rename = "tagStyle")]
    TagStyle {
        id: String,
        style: Option<WirePartialStyle>,
        out: WireStyle,
    },
    #[serde(rename = "charCount")]
    CharCount {
        id: String,
        current: i64,
        max: i64,
        out: String,
    },
}

#[test]
fn small_utils_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_SMALL_UTILS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_SMALL_UTILS to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts = [0usize; 8];
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<OracleRow>(line).unwrap() {
            OracleRow::ChatPred {
                id,
                func,
                chat_type,
                out,
            } => {
                let got = match func.as_str() {
                    "help" => is_help_like_chat_type(chat_type.as_deref()),
                    "moderation" => is_moderation_exempt_chat_type(chat_type.as_deref()),
                    other => panic!("unknown chatPred fn {other}"),
                };
                assert_eq!(got, out, "chatPred '{id}'");
                counts[0] += 1;
            }
            OracleRow::StatusPred {
                id,
                func,
                status,
                out,
            } => {
                let s = status_from_str(&status);
                let got = match func.as_str() {
                    "present" => is_participant_present(s),
                    "whisper" => can_receive_whisper(s),
                    other => panic!("unknown statusPred fn {other}"),
                };
                assert_eq!(got, out, "statusPred '{id}'");
                counts[1] += 1;
            }
            OracleRow::Migrate {
                id,
                is_active,
                removed_at,
                out,
            } => {
                let got = migrate_is_active_to_status(is_active, removed_at.as_deref());
                assert_eq!(got.as_str(), out, "migrate '{id}'");
                counts[2] += 1;
            }
            OracleRow::ParseVer { id, version, out } => {
                let got = parse_version(&version);
                match (got, out) {
                    (None, None) => {}
                    (Some(g), Some(o)) => {
                        assert_eq!(g.major, o.major, "parseVer '{id}' major");
                        assert_eq!(g.minor, o.minor, "parseVer '{id}' minor");
                        assert_eq!(g.patch, o.patch, "parseVer '{id}' patch");
                    }
                    (g, o) => panic!(
                        "parseVer '{id}': presence mismatch rust={} oracle={}",
                        g.is_some(),
                        o.is_some()
                    ),
                }
                counts[3] += 1;
            }
            OracleRow::CompareVer { id, a, b, out } => {
                assert_eq!(compare_versions(&a, &b), out, "compareVer '{id}'");
                counts[4] += 1;
            }
            OracleRow::Gender { id, subject, out } => {
                let subj = subject.as_deref();
                assert_eq!(
                    gender_from_pronouns(subj),
                    out.gender.as_deref(),
                    "gender '{id}' gender"
                );
                assert_eq!(
                    gender_noun_from_pronouns(subj),
                    out.noun.as_deref(),
                    "gender '{id}' noun"
                );
                assert_eq!(
                    gender_prefix_from_pronouns(subj),
                    out.prefix,
                    "gender '{id}' prefix"
                );
                counts[5] += 1;
            }
            OracleRow::TagStyle { id, style, out } => {
                let partial = style.map(|s| PartialTagVisualStyle {
                    emoji: s.emoji,
                    foreground_color: s.foreground_color,
                    background_color: s.background_color,
                    emoji_only: s.emoji_only,
                    bold: s.bold,
                    italic: s.italic,
                    strikethrough: s.strikethrough,
                });
                let got = merge_with_default_tag_style(partial.as_ref());
                assert_eq!(
                    got.emoji.as_deref(),
                    out.emoji.as_deref(),
                    "tagStyle '{id}' emoji"
                );
                assert_eq!(
                    got.foreground_color, out.foreground_color,
                    "tagStyle '{id}' fg"
                );
                assert_eq!(
                    got.background_color, out.background_color,
                    "tagStyle '{id}' bg"
                );
                assert_eq!(got.emoji_only, out.emoji_only, "tagStyle '{id}' emojiOnly");
                assert_eq!(got.bold, out.bold, "tagStyle '{id}' bold");
                assert_eq!(got.italic, out.italic, "tagStyle '{id}' italic");
                assert_eq!(
                    got.strikethrough, out.strikethrough,
                    "tagStyle '{id}' strike"
                );
                counts[6] += 1;
            }
            OracleRow::CharCount {
                id,
                current,
                max,
                out,
            } => {
                assert_eq!(char_count_class(current, max), out, "charCount '{id}'");
                counts[7] += 1;
            }
        }
    }

    assert!(
        counts.iter().all(|&c| c > 0),
        "oracle file looks empty/partial: {counts:?}"
    );
    eprintln!("OK: small-utils matched oracle (counts {counts:?}).");
}
