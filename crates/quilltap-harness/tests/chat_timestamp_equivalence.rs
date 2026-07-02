//! Tier-1 differential test: chat timestamp utilities (chat-orchestration wave 1.2, Part A).
//!
//! Covers resolveTimezone, calculateCurrentTimestamp, shouldInjectTimestamp,
//! formatTimestampForSystemPrompt, and initializeFictionalTime. The injected
//! clock (`now_ms`) and host local offset (`localOffsetMin`) are pinned by the
//! oracle so every row is fully deterministic; exact equality on every field.
//!
//! Generate the oracle output (TZ=UTC is REQUIRED — the undefined-timezone path
//! reads the host TZ):
//!   cd ~/source/quilltap-server
//!   TZ=UTC npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-timestamp.ts \
//!     > /tmp/oracle-chat-timestamp.ndjson
//! Run:
//!   QT_ORACLE_CHAT_TIMESTAMP=/tmp/oracle-chat-timestamp.ndjson cargo test -p quilltap-harness

use quilltap_core::chat_timestamp::{
    calculate_current_timestamp, format_timestamp_for_system_prompt, initialize_fictional_time,
    resolve_timezone, should_inject_timestamp, CalculatedTimestamp, TimestampConfig,
    TimestampFormat, TimestampMode,
};
use serde::Deserialize;

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
    #[serde(rename = "format")]
    Format {
        id: String,
        formatted: String,
        #[serde(rename = "isoValue")]
        iso_value: String,
        #[serde(rename = "isFictional")]
        is_fictional: bool,
        #[serde(rename = "autoPrepend")]
        auto_prepend: bool,
        out: String,
    },
    #[serde(rename = "init")]
    Init {
        id: String,
        base: WireConfig,
        fictional: String,
        #[serde(rename = "nowMs")]
        now_ms: i64,
        out: WireConfig,
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
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
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
            Row::Format {
                id,
                formatted,
                iso_value,
                is_fictional,
                auto_prepend,
                out,
            } => {
                let ts = CalculatedTimestamp {
                    formatted,
                    iso_value,
                    is_fictional,
                };
                let got = format_timestamp_for_system_prompt(&ts, auto_prepend);
                assert_eq!(got, out, "format '{id}'");
            }
            Row::Init {
                id,
                base,
                fictional,
                now_ms,
                out,
            } => {
                let base_cfg: TimestampConfig = base.into();
                let want: TimestampConfig = out.into();
                let got = initialize_fictional_time(&base_cfg, &fictional, now_ms);
                assert_eq!(got, want, "init '{id}'");
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: chat-timestamp matched oracle ({count} rows).");
}
