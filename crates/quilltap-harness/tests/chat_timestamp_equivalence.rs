//! Tier-1 differential test: chat timestamp utilities (chat-orchestration wave 1.2, Part A).
//!
//! Covers resolveTimezone, parseTimestampInTimezone, calculateCurrentTimestamp,
//! shouldInjectTimestamp, and
//! ensureFictionalBaseRealTime. The injected clock (`now_ms`) and host local
//! offset (`localOffsetMin`) are pinned by the oracle so every row is fully
//! deterministic; exact equality on every field.
//!
//! P4.d18 (the `e3a9654f` fictional-story-clock drift) widened this family with
//! the parse-in-timezone rows, the ZONE-LESS `datetime-local` fictional bases in
//! the calc family, and the ensure-anchor rows. The zone-less calc rows are the
//! load-bearing addition: their absence is what let the port's
//! `parse_date_ms → 0` bug (every real fictional clock reading 1970-adjacent
//! nonsense) survive a green differential.
//!
//! P4.d28 (the `b3ee00f1` Markdown-transcript drift) added the `calcAt` family:
//! the extracted `calculateTimestampAt` over arbitrary historical instants with
//! an explicit fallback anchor — the shape the transcript renderer calls. Those
//! oracle rows are recorded with the wall clock pinned to a 1969 sentinel, so a
//! port that reached for a clock instead of the passed instant cannot pass.
//!
//! Generate the oracle output (TZ=UTC is REQUIRED — the undefined-timezone path
//! reads the host TZ):
//!   cd ~/source/quilltap-server
//!   TZ=UTC npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-timestamp.ts \
//!     > /tmp/oracle-chat-timestamp.ndjson
//! Run:
//!   QT_ORACLE_CHAT_TIMESTAMP=/tmp/oracle-chat-timestamp.ndjson \
//!     cargo test -p quilltap-harness --test chat_timestamp_equivalence

use quilltap_core::chat_timestamp::{
    calculate_current_timestamp, calculate_timestamp_at, ensure_fictional_base_real_time,
    parse_timestamp_in_timezone, resolve_timezone, should_inject_timestamp, CalculatedTimestamp,
    TimestampConfig, TimestampFormat, TimestampMode,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct WireConfig {
    mode: String,
    format: String,
    #[serde(rename = "customFormat", default)]
    custom_format: Option<String>,
    #[serde(rename = "useFictionalTime")]
    use_fictional_time: bool,
    #[serde(rename = "fictionalBaseTimestamp", default)]
    fictional_base_timestamp: Option<String>,
    #[serde(rename = "fictionalBaseRealTime", default)]
    fictional_base_real_time: Option<String>,
    #[serde(rename = "autoPrepend")]
    auto_prepend: bool,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(rename = "intervalMinutes")]
    interval_minutes: i64,
}

fn parse_mode(s: &str) -> TimestampMode {
    match s {
        "NONE" => TimestampMode::None,
        "START_ONLY" => TimestampMode::StartOnly,
        "EVERY_MESSAGE" => TimestampMode::EveryMessage,
        "EVERY_N_MINUTES" => TimestampMode::EveryNMinutes,
        other => panic!("unknown mode: {other}"),
    }
}

fn parse_format(s: &str) -> TimestampFormat {
    match s {
        "ISO8601" => TimestampFormat::Iso8601,
        "FRIENDLY" => TimestampFormat::Friendly,
        "DATE_ONLY" => TimestampFormat::DateOnly,
        "TIME_ONLY" => TimestampFormat::TimeOnly,
        "CUSTOM" => TimestampFormat::Custom,
        other => panic!("unknown format: {other}"),
    }
}

impl From<WireConfig> for TimestampConfig {
    fn from(w: WireConfig) -> Self {
        TimestampConfig {
            mode: parse_mode(&w.mode),
            format: parse_format(&w.format),
            custom_format: w.custom_format,
            use_fictional_time: w.use_fictional_time,
            fictional_base_timestamp: w.fictional_base_timestamp,
            fictional_base_real_time: w.fictional_base_real_time,
            auto_prepend: w.auto_prepend,
            timezone: w.timezone,
            interval_minutes: w.interval_minutes,
        }
    }
}

#[derive(Deserialize)]
struct WireCalcOut {
    formatted: String,
    #[serde(rename = "isoValue")]
    iso_value: String,
    #[serde(rename = "isFictional")]
    is_fictional: bool,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "resolve")]
    Resolve {
        id: String,
        config: Option<String>,
        settings: Option<String>,
        out: Option<String>,
    },
    #[serde(rename = "calc")]
    Calc {
        id: String,
        config: WireConfig,
        timezone: Option<String>,
        #[serde(rename = "nowMs")]
        now_ms: i64,
        #[serde(rename = "localOffsetMin")]
        local_offset_min: i64,
        #[serde(default)]
        out: Option<WireCalcOut>,
        #[serde(default)]
        threw: bool,
    },
    #[serde(rename = "calcAt")]
    CalcAt {
        id: String,
        config: WireConfig,
        timezone: Option<String>,
        #[serde(rename = "realInstantMs")]
        real_instant_ms: i64,
        #[serde(rename = "fallbackAnchorMs", default)]
        fallback_anchor_ms: Option<i64>,
        #[serde(rename = "localOffsetMin")]
        local_offset_min: i64,
        #[serde(default)]
        out: Option<WireCalcOut>,
        #[serde(default)]
        threw: bool,
    },
    #[serde(rename = "inject")]
    Inject {
        id: String,
        config: Option<WireConfig>,
        #[serde(rename = "isInitial")]
        is_initial: bool,
        #[serde(rename = "minutesSince", default)]
        minutes_since: Option<i64>,
        out: bool,
    },
    #[serde(rename = "parseTz")]
    ParseTz {
        id: String,
        value: String,
        timezone: Option<String>,
        #[serde(rename = "localOffsetMin")]
        local_offset_min: i64,
        out: i64,
    },
    #[serde(rename = "ensure")]
    Ensure {
        id: String,
        config: Value,
        #[serde(rename = "anchorMs", default)]
        anchor_ms: Option<i64>,
        #[serde(rename = "nowMs")]
        now_ms: i64,
        out: Value,
    },
}

#[test]
fn chat_timestamp_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_CHAT_TIMESTAMP") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHAT_TIMESTAMP to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
    let mut naive_calc_rows = 0usize;
    let mut anchored_calc_at_rows = 0usize;
    let mut fictional_anchored_calc_at_rows = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        {
            let raw: Value = serde_json::from_str(line).unwrap();
            *kinds
                .entry(raw["kind"].as_str().unwrap().to_string())
                .or_default() += 1;
            if raw["kind"] == "calc" && raw["id"].as_str().unwrap().starts_with("naive-") {
                naive_calc_rows += 1;
            }
            if raw["kind"] == "calcAt" && raw["fallbackAnchorMs"].is_i64() {
                anchored_calc_at_rows += 1;
                if raw["config"]["useFictionalTime"] == Value::Bool(true)
                    && raw["config"]["fictionalBaseRealTime"].is_null()
                {
                    fictional_anchored_calc_at_rows += 1;
                }
            }
        }
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::Resolve {
                id,
                config,
                settings,
                out,
            } => {
                let got = resolve_timezone(config.as_deref(), settings.as_deref());
                assert_eq!(got, out, "resolve '{id}'");
            }
            Row::Calc {
                id,
                config,
                timezone,
                now_ms,
                local_offset_min,
                out,
                threw,
            } => {
                let cfg: TimestampConfig = config.into();
                let got = calculate_current_timestamp(
                    &cfg,
                    timezone.as_deref(),
                    now_ms,
                    local_offset_min,
                );
                if threw {
                    assert!(got.is_err(), "calc '{id}' expected error, got {got:?}");
                } else {
                    let want = out.expect("non-threw row must carry out");
                    let got = got.unwrap_or_else(|e| panic!("calc '{id}' unexpected error: {e}"));
                    assert_eq!(
                        got,
                        CalculatedTimestamp {
                            formatted: want.formatted,
                            iso_value: want.iso_value,
                            is_fictional: want.is_fictional,
                        },
                        "calc '{id}'"
                    );
                }
            }
            Row::CalcAt {
                id,
                config,
                timezone,
                real_instant_ms,
                fallback_anchor_ms,
                local_offset_min,
                out,
                threw,
            } => {
                let cfg: TimestampConfig = config.into();
                let got = calculate_timestamp_at(
                    real_instant_ms,
                    &cfg,
                    timezone.as_deref(),
                    fallback_anchor_ms,
                    local_offset_min,
                );
                if threw {
                    assert!(got.is_err(), "calcAt '{id}' expected error, got {got:?}");
                } else {
                    let want = out.expect("non-threw row must carry out");
                    let got = got.unwrap_or_else(|e| panic!("calcAt '{id}' unexpected error: {e}"));
                    assert_eq!(
                        got,
                        CalculatedTimestamp {
                            formatted: want.formatted,
                            iso_value: want.iso_value,
                            is_fictional: want.is_fictional,
                        },
                        "calcAt '{id}'"
                    );
                }
            }
            Row::Inject {
                id,
                config,
                is_initial,
                minutes_since,
                out,
            } => {
                let cfg: Option<TimestampConfig> = config.map(Into::into);
                let got = should_inject_timestamp(cfg.as_ref(), is_initial, minutes_since);
                assert_eq!(got, out, "inject '{id}'");
            }
            Row::ParseTz {
                id,
                value,
                timezone,
                local_offset_min,
                out,
            } => {
                let got =
                    parse_timestamp_in_timezone(&value, timezone.as_deref(), local_offset_min)
                        .unwrap_or_else(|e| panic!("parseTz '{id}' unexpected error: {e}"));
                assert_eq!(got, out, "parseTz '{id}' (value {value:?})");
            }
            Row::Ensure {
                id,
                config,
                anchor_ms,
                now_ms,
                out,
            } => {
                // v4's `anchor ?? new Date()` — the Rust signature takes the
                // already-resolved instant, so the default is applied here.
                let got = ensure_fictional_base_real_time(&config, anchor_ms.unwrap_or(now_ms));
                // Compare the SERIALIZED forms: `Value`'s own PartialEq is
                // order-independent, and key order is load-bearing (this object
                // is written straight into the `chats.timestampConfig` column).
                assert_eq!(
                    serde_json::to_string(&got).unwrap(),
                    serde_json::to_string(&out).unwrap(),
                    "ensure '{id}'"
                );
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");

    // Shape assertions, not row-count constants: a regenerated oracle that
    // silently dropped a family would otherwise still pass. The `naive-` calc
    // floor is the one that matters — those zone-less `datetime-local` bases
    // are the case class P4.d18 added, and losing them re-opens the blind spot
    // that hid `parse_date_ms → 0`.
    for family in ["resolve", "calc", "calcAt", "inject", "parseTz", "ensure"] {
        assert!(
            kinds.get(family).copied().unwrap_or(0) > 0,
            "oracle carries no '{family}' rows — regeneration lost a family (kinds: {kinds:?})"
        );
    }
    assert!(
        naive_calc_rows >= 20,
        "expected the zone-less datetime-local fictional-base calc rows (>= 20), got {naive_calc_rows}"
    );
    // P4.d28: the `calcAt` rows that carry an explicit fallback anchor are the
    // extraction's whole point (the transcript passes the chat's createdAt), and
    // the anchored-fictional subset is what proves the anchor is CONSULTED
    // rather than ignored. A regeneration that dropped either would leave the
    // family nominally present and the extraction unproven.
    assert!(
        anchored_calc_at_rows >= 20,
        "expected the fallback-anchor calcAt rows (>= 20), got {anchored_calc_at_rows}"
    );
    assert!(
        fictional_anchored_calc_at_rows >= 10,
        "expected the anchored-FICTIONAL calcAt rows (>= 10), got {fictional_anchored_calc_at_rows}"
    );

    eprintln!("OK: chat-timestamp matched oracle ({count} rows, kinds: {kinds:?}).");
}
