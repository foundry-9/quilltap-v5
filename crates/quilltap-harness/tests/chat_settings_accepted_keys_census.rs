//! The chat-settings PUT allow-list census (P4.D356 item 4, v4 `3b8b6f42b`).
//!
//! v4's `settings/chat/route.ts` refuses any top-level PUT key outside its
//! 34-key `ACCEPTED_KEYS` with a 400 (ported as
//! `quilltap_core::api::settings::ACCEPTED_KEYS`). That turns every v5 SPA
//! caller that sends a key v4 never accepted — or echoes a GET row back — into
//! a broken save. This census is the ownership proof the drift ledger asked
//! for ("every v5 SPA `chatSettingsUpdate` caller sends only accepted keys"):
//!
//! - (a) the Rust list equals v4's, IN ORDER — read from
//!   `$QT_V4_CHECKOUT/app/api/v1/settings/chat/route.ts` (skips with a reason
//!   when `QT_V4_CHECKOUT` is unset — the guards' convention);
//! - (b) every registered caller — the `ChatSettingsCard` subclasses' `this.save({…})`
//!   keys (found by scanning, then pinned against [`CARD_KEYS`]) and the
//!   direct `chatSettingsUpdate` dispatchers ([`DIRECT`]) — sends only
//!   accepted keys, and each named key literal is in its file;
//! - (c) the NEGATIVE sweep: every non-spec `.ts` under `apps/web/src/app`
//!   that dispatches `chatSettingsUpdate` or calls `updateChatSettings(` is
//!   registered — a new caller must be added here;
//! - and no caller sends `defaultTimestampConfig` or `timezone` (both refused
//!   at the pin).
//!
//! No SPA file is changed by this census; it only READS `apps/web/src`.
//!
//! Run:
//!   QT_V4_CHECKOUT=~/source/quilltap-server \
//!     cargo test -p quilltap-harness --test chat_settings_accepted_keys_census -- --nocapture

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use quilltap_core::api::settings::ACCEPTED_KEYS;

fn app_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/src/app")
}

/// The `ChatSettingsCard` subclasses and the top-level keys their
/// `this.save({…})` calls send (the base class passes the bag through
/// `updateChatSettings` in `chat-settings.api.ts`). Measured 2026-11-04 at the
/// lane's base; a card that adds a key must register it here.
const CARD_KEYS: &[(&str, &[&str])] = &[
    (
        "screens/settings/chat/agent-mode-settings.ts",
        &["agentModeSettings"],
    ),
    (
        "screens/settings/chat/answer-confirmation-settings.ts",
        &["answerConfirmationSettings"],
    ),
    (
        "screens/settings/chat/auto-scroll-settings.ts",
        &["autoScrollOnResponseComplete"],
    ),
    (
        "screens/settings/chat/automation-settings.ts",
        &["autoDetectRng", "customToolTriggers"],
    ),
    (
        "screens/settings/chat/composer-emoji-settings.ts",
        &["composerEmoji"],
    ),
    (
        "screens/settings/chat/composer-slash-commands-settings.ts",
        &["composerSlashCommands"],
    ),
    (
        "screens/settings/chat/composer-spellcheck-settings.ts",
        &["composerSpellcheck"],
    ),
    (
        "screens/settings/chat/composer-unicode-settings.ts",
        &["composerUnicode"],
    ),
    (
        "screens/settings/chat/context-compression-settings.ts",
        &["contextCompressionSettings"],
    ),
    (
        "screens/settings/chat/custom-tools-settings.ts",
        &["customTools"],
    ),
    (
        "screens/settings/chat/image-description-settings.ts",
        &["imageDescriptionProfileId"],
    ),
    (
        "screens/settings/chat/impersonation-voice-settings.ts",
        &["impersonationVoiceMode"],
    ),
    (
        "screens/settings/chat/memory-cascade-settings.ts",
        &["memoryCascadePreferences"],
    ),
    (
        "screens/settings/chat/smart-typography-settings.ts",
        &["smartTypographySettings"],
    ),
    (
        "screens/settings/chat/thinking-display-settings.ts",
        &["thinkingDisplay"],
    ),
    (
        "screens/settings/chat/token-display-settings.ts",
        &["tokenDisplaySettings"],
    ),
    (
        "screens/settings/concierge/concierge-settings.api.ts",
        &["conciergeSettings"],
    ),
    ("screens/settings/concierge/concierge-tab.ts", &[]),
    (
        "screens/settings/images/story-backgrounds-card.ts",
        &["storyBackgroundsSettings"],
    ),
    (
        "screens/settings/images/wardrobe-images-card.ts",
        &["wardrobeImageSettings"],
    ),
    (
        "screens/settings/system/auto-lock-settings-card.ts",
        &["autoLockSettings"],
    ),
    (
        "screens/settings/system/llm-logging-settings-card.ts",
        &["llmLoggingSettings"],
    ),
];

/// The files that dispatch `chatSettingsUpdate` (or call an
/// `updateChatSettings(` wrapper) themselves, with the top-level keys the
/// enclosing code can emit — READ from each body (a dynamic `patch` /
/// `merged` lists what its builder writes).
const DIRECT: &[(&str, &[&str])] = &[
    (
        "autonomous/autonomous-room-defaults.ts",
        &["autonomousRoomSettings"],
    ),
    (
        "screens/salon/salon-conversation.ts",
        &["memoryCascadePreferences"],
    ),
    // `updateSettings(patch)` — its two callers pass these.
    (
        "screens/settings/appearance/appearance-tab.ts",
        &["avatarDisplayMode", "avatarDisplayStyle"],
    ),
    (
        "screens/settings/chat/composition-mode-settings.ts",
        &["compositionModeDefault"],
    ),
    (
        "screens/settings/chat/text-replacement-settings.ts",
        &["textReplacementsEnabled"],
    ),
    (
        "screens/settings/providers/cheap-llm-card.ts",
        &["cheapLLMSettings"],
    ),
    (
        "screens/settings/templates/roleplay-templates-card.ts",
        &["defaultRoleplayTemplateId"],
    ),
    (
        "screens/settings/wizard/wizard-api.ts",
        &["cheapLLMSettings"],
    ),
    ("theme/theme.service.ts", &["themePreference"]),
];

/// Callers whose keys are another file's: the base card's pass-through
/// (`ChatSettingsCard.save` → `updateChatSettings`), the wizard step that
/// calls `wizard-api.ts`'s wrapper (which owns the key), and the two spec
/// harnesses (stubs that RECORD dispatches, never send one).
const PASS_THROUGH: &[&str] = &[
    "core/core-contract.ts",
    "screens/settings/chat/chat-settings.api.ts",
    "screens/settings/wizard/steps/test-confirm-step.ts",
    "screens/settings/chat/chat-settings.spec-harness.ts",
    "screens/settings/concierge/concierge.spec-harness.ts",
];

fn read(rel: &str) -> String {
    std::fs::read_to_string(app_root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "ts")
            && !p.to_string_lossy().ends_with(".spec.ts")
        {
            out.push(p);
        }
    }
}

/// The `this.save({ KEY …` keys a card file sends.
fn save_keys(text: &str) -> BTreeSet<String> {
    let re = regex::Regex::new(r"this\.save\(\s*\{\s*(\w+)\s*[:,}]").unwrap();
    re.captures_iter(text).map(|c| c[1].to_string()).collect()
}

#[test]
fn accepted_keys_match_v4s_route_in_order() {
    let Ok(v4) = std::env::var("QT_V4_CHECKOUT") else {
        println!("SKIP: set QT_V4_CHECKOUT to compare ACCEPTED_KEYS with v4's route.ts");
        return;
    };
    let route = std::fs::read_to_string(Path::new(&v4).join("app/api/v1/settings/chat/route.ts"))
        .expect("v4's settings/chat route");
    let start = route
        .find("const ACCEPTED_KEYS = new Set([")
        .expect("v4's ACCEPTED_KEYS");
    let body = &route[start..start + route[start..].find("])").unwrap()];
    let re = regex::Regex::new(r"'([A-Za-z]+)'").unwrap();
    let v4_keys: Vec<String> = re.captures_iter(body).map(|c| c[1].to_string()).collect();
    let v5_keys: Vec<String> = ACCEPTED_KEYS.iter().map(|k| k.to_string()).collect();
    assert_eq!(v5_keys, v4_keys, "ACCEPTED_KEYS drifted from v4's route.ts");
    println!("ACCEPTED_KEYS: {} keys, v4's order", v4_keys.len());
}

#[test]
fn every_spa_caller_sends_only_accepted_keys() {
    let accepted: BTreeSet<&str> = ACCEPTED_KEYS.iter().copied().collect();
    let mut every_key: BTreeSet<String> = BTreeSet::new();

    // (b) the cards — scanned AND pinned.
    let mut found_cards: Vec<String> = Vec::new();
    let mut files = Vec::new();
    walk(&app_root(), &mut files);
    for p in &files {
        let text = std::fs::read_to_string(p).unwrap();
        if text.contains("extends ChatSettingsCard") {
            let rel = p
                .strip_prefix(app_root())
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            found_cards.push(rel.clone());
            let want = CARD_KEYS
                .iter()
                .find(|(f, _)| *f == rel)
                .unwrap_or_else(|| {
                    panic!("{rel} extends ChatSettingsCard but is not registered in CARD_KEYS")
                });
            let got = save_keys(&text);
            let want_set: BTreeSet<String> = want.1.iter().map(|k| k.to_string()).collect();
            assert_eq!(got, want_set, "{rel}: the keys its this.save calls send");
            every_key.extend(got);
        }
    }
    found_cards.sort();
    let mut registered: Vec<String> = CARD_KEYS.iter().map(|(f, _)| f.to_string()).collect();
    registered.sort();
    assert_eq!(
        found_cards, registered,
        "a registered card no longer extends ChatSettingsCard"
    );

    // (b) the direct dispatchers.
    for (rel, keys) in DIRECT {
        let text = read(rel);
        assert!(
            text.contains("chatSettingsUpdate") || text.contains("updateChatSettings("),
            "{rel}: no chatSettingsUpdate dispatch any more"
        );
        for k in *keys {
            assert!(text.contains(k), "{rel}: the key literal `{k}` is gone");
            every_key.insert((*k).to_string());
        }
    }

    for k in &every_key {
        assert!(
            accepted.contains(k.as_str()),
            "the SPA sends `{k}`, which v4's ACCEPTED_KEYS refuses with a 400"
        );
    }
    // Refused at the pin; no caller may send either (Tier 2 item 20).
    for refused in [
        "defaultTimestampConfig",
        "timezone",
        "id",
        "userId",
        "createdAt",
        "updatedAt",
    ] {
        assert!(!every_key.contains(refused), "a caller sends `{refused}`");
    }

    // (c) the negative sweep: every dispatcher is registered.
    for p in &files {
        let text = std::fs::read_to_string(p).unwrap();
        if !(text.contains("chatSettingsUpdate") || text.contains("updateChatSettings(")) {
            continue;
        }
        let rel = p
            .strip_prefix(app_root())
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let known = DIRECT.iter().any(|(f, _)| *f == rel)
            || PASS_THROUGH.contains(&rel.as_str())
            || CARD_KEYS.iter().any(|(f, _)| *f == rel);
        assert!(
            known,
            "{rel} dispatches chatSettingsUpdate but is not in the census"
        );
    }
    println!(
        "census: {} cards, {} direct dispatchers, {} keys sent — all accepted",
        CARD_KEYS.len(),
        DIRECT.len(),
        every_key.len()
    );
}
