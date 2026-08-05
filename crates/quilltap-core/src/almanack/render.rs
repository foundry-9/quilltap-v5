//! The Almanack — markdown renderer (v4 `lib/tools/almanack/render.ts`).
//!
//! A pure function of [`AlmanackReportData`]: no I/O, no clock, no globals.
//! That is what makes the whole rendered volume byte-diffable from a fixture,
//! and it keeps "what we gathered" separate from "how we say it".
//!
//! ## Locale
//!
//! v4 calls `toLocaleDateString()` / `toLocaleString()` with **no arguments**,
//! i.e. the ambient locale and zone. v4's own tests pin `TZ=UTC` in the jest
//! config (a `beforeAll` pin is too late — ICU caches the zone), the oracle
//! recipes here pin it explicitly, and this port computes en-US/UTC parts to
//! match. Number grouping is `Number.prototype.toLocaleString()`
//! ([`to_locale_string`]).

use crate::format_time::{locale_date_time_us, locale_date_us};
use crate::jsnum::to_fixed;

use super::phases::ALMANACK_TITLE;
use super::types::{AlmanackReportData, CacheRow, ProfileWindowRow};
use crate::format_bytes::format_bytes;

/// JS `String(number)` for the finite domain. Rust's `{}` for `f64` is the
/// shortest round-trip decimal without an exponent, which coincides with V8's
/// `Number::toString` across every value the report carries (integer counts and
/// small fractional dials).
fn js_num(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if n == 0.0 {
        // JS `String(-0)` is `"0"` (only `toLocaleString` keeps the sign);
        // Rust's Display would emit `"-0"`. `Math.round(-0.5)` reaches here.
        return "0".to_string();
    }
    format!("{n}")
}

/// `Number.prototype.toLocaleString()` under en-US: `,` thousands grouping and
/// at most three fraction digits, rounded **half-expand on the shortest decimal
/// representation** (probed against Node 24: `12345.6785 → "12,345.679"`, where
/// `toFixed(3)` gives `"12345.678"` because it rounds the exact binary value).
///
/// The report's `toLocaleString` domain is integer counts in practice; the
/// fractional arm is ported so a mean or ratio that reaches one of these fields
/// renders as v4 renders it rather than being silently truncated.
pub fn to_locale_string(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 { "∞" } else { "-∞" }.to_string();
    }
    let raw = format!("{n}");
    let (neg, digits) = match raw.strip_prefix('-') {
        Some(rest) => (true, rest.to_string()),
        None => (false, raw),
    };
    let (int_part, frac_part) = match digits.split_once('.') {
        Some((i, f)) => (i.to_string(), f.to_string()),
        None => (digits, String::new()),
    };
    let (int_part, frac_part) = round_decimal_half_up(&int_part, &frac_part, 3);
    let frac_trimmed = frac_part.trim_end_matches('0');

    let mut grouped = String::new();
    let len = int_part.len();
    for (i, ch) in int_part.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    out.push_str(&grouped);
    if !frac_trimmed.is_empty() {
        out.push('.');
        out.push_str(frac_trimmed);
    }
    out
}

/// Round a non-negative decimal split into (integer digits, fraction digits) to
/// `places` fraction digits, half away from zero. Pure string arithmetic — the
/// magnitudes involved never approach `u128`'s range as a whole, but the digit
/// strings can, so the carry is done digit-wise.
fn round_decimal_half_up(int_part: &str, frac_part: &str, places: usize) -> (String, String) {
    if frac_part.len() <= places {
        let mut frac = frac_part.to_string();
        while frac.len() < places {
            frac.push('0');
        }
        return (int_part.to_string(), frac);
    }
    let kept: Vec<u8> = frac_part[..places].bytes().map(|b| b - b'0').collect();
    let round_up = frac_part.as_bytes()[places] >= b'5';
    let mut digits: Vec<u8> = int_part.bytes().map(|b| b - b'0').collect();
    let mut frac_digits = kept;
    if round_up {
        let mut carry = 1u8;
        for d in frac_digits.iter_mut().rev() {
            let v = *d + carry;
            *d = v % 10;
            carry = v / 10;
            if carry == 0 {
                break;
            }
        }
        if carry > 0 {
            for d in digits.iter_mut().rev() {
                let v = *d + carry;
                *d = v % 10;
                carry = v / 10;
                if carry == 0 {
                    break;
                }
            }
            if carry > 0 {
                digits.insert(0, carry);
            }
        }
    }
    let int_out: String = digits.iter().map(|d| (d + b'0') as char).collect();
    let frac_out: String = frac_digits.iter().map(|d| (d + b'0') as char).collect();
    (int_out, frac_out)
}

/// v4 `formatUSD` — `$` + `toFixed(4)`.
fn format_usd(amount: f64) -> String {
    format!("${}", to_fixed(amount, 4))
}

/// v4 `formatDate` — `'N/A'` for a null/unparseable stamp.
fn format_date(iso: Option<&str>) -> String {
    match iso.filter(|s| !s.is_empty()) {
        None => "N/A".to_string(),
        Some(s) => locale_date_us(s).unwrap_or_else(|| "N/A".to_string()),
    }
}

/// v4 `formatDateTime` — `'N/A'` for a null/unparseable stamp.
fn format_date_time(iso: Option<&str>) -> String {
    match iso.filter(|s| !s.is_empty()) {
        None => "N/A".to_string(),
        Some(s) => locale_date_time_us(s).unwrap_or_else(|| "N/A".to_string()),
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "Yes"
    } else {
        "No"
    }
}

/// v4 `formatMs` — `'—'` for null/non-finite, whole ms under a second, else
/// seconds to two places. `Math.round` is JS's (half toward +∞), not Rust's
/// (half away from zero) — they differ on `-0.5`.
fn format_ms(ms: Option<f64>) -> String {
    let Some(ms) = ms.filter(|m| m.is_finite()) else {
        return "—".to_string();
    };
    if ms < 1000.0 {
        return format!("{} ms", js_num((ms + 0.5).floor()));
    }
    format!("{} s", to_fixed(ms / 1000.0, 2))
}

fn percent(value: f64) -> String {
    format!("{}%", to_fixed(value * 100.0, 1))
}

/// Escape a cell so a stray pipe can't shear a markdown table in half.
fn cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

/// v4's very common "one row per named count" shape.
fn count_list(rows: &[(String, f64)], empty: &str) -> Vec<String> {
    if rows.is_empty() {
        return vec![format!("*{empty}*")];
    }
    rows.iter()
        .map(|(label, count)| format!("- **{label}**: {}", to_locale_string(*count)))
        .collect()
}

/// v4 `${value}` over a `string | undefined` — an absent optional stringifies
/// as `"undefined"`. Reached only if a designated-profile row carried a
/// provider without a model, which the collector never produces; ported for
/// faithfulness rather than left to panic.
fn opt_str(value: Option<&String>) -> &str {
    value.map(String::as_str).unwrap_or("undefined")
}

/// v4 `renderAlmanackMarkdown(data)` — the whole volume, byte-for-byte.
pub fn render_almanack_markdown(data: &AlmanackReportData) -> String {
    let mut lines: Vec<String> = Vec::new();
    macro_rules! push {
        ($($v:expr),+ $(,)?) => {{ $(lines.push(String::from($v));)+ }};
    }

    push!(format!("# {ALMANACK_TITLE}"), "");
    push!(
        format!(
            "Generated: {}",
            format_date_time(Some(data.generated_at.as_str()))
        ),
        ""
    );
    push!(
        "A compendium of the establishment as it stands: what is installed, what is configured,",
        "and what has accumulated. Safe to share — see the privacy note in the help page.",
        "",
    );

    // ========================================================================
    // Phase 1 — Taking the measure of the premises
    // ========================================================================

    push!("## The Premises", "");
    push!("### System Information", "");
    push!(format!("- **Version**: {}", data.version));
    push!(format!("- **Node Environment**: {}", data.node_env));
    let re = &data.runtime_environment;
    push!(format!("- **Node Version**: {}", re.node_version));
    push!(format!("- **Platform**: {} ({})", re.platform, re.arch));
    push!(format!("- **OS**: {} {}", re.os_type, re.os_release));
    push!(format!("- **Runtime Type**: {}", re.runtime_type));
    if let Some(shell) = re.electron_shell_version.as_ref().filter(|s| !s.is_empty()) {
        push!(format!("- **Electron Shell**: {shell}"));
    }
    if !re.shell_capabilities.is_empty() {
        push!(format!(
            "- **Shell Capabilities**: {}",
            re.shell_capabilities.join(", ")
        ));
    }
    push!(format!(
        "- **Total Memory**: {}",
        format_bytes(re.total_memory_bytes)
    ));
    push!(format!(
        "- **Free Memory**: {}",
        format_bytes(re.free_memory_bytes)
    ));
    push!(format!(
        "- **Uptime**: {}h {}m",
        js_num((re.uptime_seconds / 3600.0).floor()),
        js_num(((re.uptime_seconds % 3600.0) / 60.0).floor())
    ));
    push!(format!("- **Timezone**: {}", re.timezone));
    push!(format!("- **Data Directory**: {}", re.data_directory), "");

    push!("### Databases & Security", "");
    push!(format!(
        "- **Passphrase Protected**: {}",
        yes_no(data.database_security.passphrase_protected)
    ));
    if let Some(v) = data
        .database_security
        .highest_app_version
        .as_ref()
        .filter(|s| !s.is_empty())
    {
        push!(format!("- **Highest App Version Seen**: {v}"));
    }
    push!("");
    push!("| Database | Size | Present |");
    push!("|----------|------|---------|");
    for db in &data.database_security.databases {
        push!(format!(
            "| {} | {} | {} |",
            cell(&db.label),
            format_bytes(db.size_bytes),
            yes_no(db.present)
        ));
    }
    push!("");

    push!("### Backup Status", "");
    push!("| Database | Backups | Newest | Oldest | Total Size |");
    push!("|----------|---------|--------|--------|------------|");
    for backup in &data.backup_status {
        push!(format!(
            "| {} | {} | {} | {} | {} |",
            cell(&backup.label),
            js_num(backup.count),
            format_date(backup.newest_date.as_deref()),
            format_date(backup.oldest_date.as_deref()),
            format_bytes(backup.total_size_bytes)
        ));
    }
    push!("");

    push!("### Migration State", "");
    push!(format!(
        "- **Migrations Applied**: {}",
        js_num(data.migration_state.applied_count)
    ));
    push!(format!(
        "- **Last Migration**: {}",
        data.migration_state
            .last_migration_id
            .clone()
            .unwrap_or_else(|| "None recorded".to_string())
    ));
    push!(format!(
        "- **Applied At**: {}",
        format_date_time(data.migration_state.last_migration_at.as_deref())
    ));
    push!(
        format!(
            "- **Applied By Version**: {}",
            data.migration_state
                .last_migration_version
                .clone()
                .unwrap_or_else(|| "N/A".to_string())
        ),
        ""
    );

    // ========================================================================
    // Phase 2 — Cataloguing the machinery
    // ========================================================================

    push!("## The Machinery", "");

    push!("### Plugins", "");
    push!(format!("- **Enabled**: {}", data.plugins.enabled.len()));
    push!(format!("- **Disabled**: {}", data.plugins.disabled.len()));
    push!(format!(
        "- **Installed from npm**: {}",
        js_num(data.plugins.npm_installed)
    ));
    push!(format!(
        "- **Bundled with the app**: {}",
        js_num(data.plugins.bundled)
    ));
    push!(format!(
        "- **Plugin config rows**: {}",
        js_num(data.plugins.plugin_config_rows)
    ));
    push!(
        format!(
            "- **Per-character plugin data rows**: {}",
            js_num(data.plugins.character_plugin_data_rows)
        ),
        ""
    );

    if !data.plugins.by_capability.is_empty() {
        push!("| Capability | Plugins |");
        push!("|------------|---------|");
        for row in &data.plugins.by_capability {
            push!(format!(
                "| {} | {} |",
                cell(&row.capability),
                js_num(row.count)
            ));
        }
        push!("");
    }

    for (heading, list) in [
        ("Enabled Plugins", &data.plugins.enabled),
        ("Disabled Plugins", &data.plugins.disabled),
    ] {
        push!(format!("#### {heading}"), "");
        if list.is_empty() {
            push!("*None*", "");
            continue;
        }
        push!("| Name | Version | Source | Capabilities |");
        push!("|------|---------|--------|--------------|");
        for plugin in list.iter() {
            push!(format!(
                "| {} | {} | {} | {} |",
                cell(&plugin.title),
                cell(&plugin.version),
                if plugin.installed_from_npm {
                    "npm"
                } else {
                    "bundled"
                },
                cell(&plugin.capabilities.join(", "))
            ));
        }
        push!("");
    }

    push!("### LLM Providers", "");
    push!("| Provider | Configured | Capabilities |");
    push!("|----------|------------|--------------|");
    for provider in &data.providers {
        let mut caps: Vec<&str> = Vec::new();
        if provider.capabilities.chat {
            caps.push("Chat");
        }
        if provider.capabilities.image_generation {
            caps.push("Images");
        }
        if provider.capabilities.embeddings {
            caps.push("Embeddings");
        }
        if provider.capabilities.web_search {
            caps.push("Web Search");
        }
        push!(format!(
            "| {} | {} | {} |",
            cell(&provider.display_name),
            if provider.configured { "✓" } else { "✗" },
            cell(&caps.join(", "))
        ));
    }
    push!("");

    push!("### Models by Provider", "");
    if data.models_by_provider.is_empty() {
        push!("*No configured providers with models*", "");
    }
    for pm in &data.models_by_provider {
        push!(format!("#### {}", pm.provider), "");
        match pm.error.as_ref().filter(|e| !e.is_empty()) {
            Some(err) => push!(format!("*Error fetching models: {err}*")),
            None => {
                if !pm.models.is_empty() {
                    for model in &pm.models {
                        push!(format!("- {model}"));
                    }
                } else {
                    push!("*No models available*");
                }
            }
        }
        push!("");
    }

    push!("### Model Discovery Cache", "");
    push!(
        "How fresh the cached model lists are. A cache that has not moved in months is how a",
        "silently broken discovery endpoint presents itself.",
        "",
    );
    if data.provider_model_cache.is_empty() {
        push!("*Cache is empty — model lists are being fetched live*", "");
    } else {
        push!("| Provider | Total | Chat | Image | Embedding | Last Refreshed |");
        push!("|----------|-------|------|-------|-----------|----------------|");
        for row in &data.provider_model_cache {
            push!(format!(
                "| {} | {} | {} | {} | {} | {} |",
                cell(&row.provider),
                js_num(row.total),
                js_num(row.chat),
                js_num(row.image),
                js_num(row.embedding),
                format_date_time(row.newest_updated_at.as_deref())
            ));
        }
        push!("");
    }

    push!("### API Keys", "");
    if data.api_key_usage.is_empty() {
        push!("*No API keys stored*", "");
    } else {
        push!("| Provider | Keys | Active | Never Used | Last Used |");
        push!("|----------|------|--------|------------|-----------|");
        for row in &data.api_key_usage {
            push!(format!(
                "| {} | {} | {} | {} | {} |",
                cell(&row.provider),
                js_num(row.total),
                js_num(row.active),
                js_num(row.never_used),
                format_date(row.last_used.as_deref())
            ));
        }
        push!("");
    }
    if !data.api_key_types.is_empty() {
        push!("Key types the registered providers can accept:", "");
        for t in &data.api_key_types {
            push!(format!("- {t}"));
        }
        push!("");
    }

    push!("### Designated Workers", "");
    push!(match &data.cheap_llm.provider {
        Some(p) => format!(
            "- **Cheap LLM**: {p} / {} ({})",
            opt_str(data.cheap_llm.model.as_ref()),
            opt_str(data.cheap_llm.profile_name.as_ref())
        ),
        None => "- **Cheap LLM**: *Not configured*".to_string(),
    });
    if let Some(p) = &data.image_prompt_llm.provider {
        push!(format!(
            "- **Image Prompt LLM**: {p} / {} ({})",
            opt_str(data.image_prompt_llm.model.as_ref()),
            opt_str(data.image_prompt_llm.profile_name.as_ref())
        ));
    }
    push!(match &data.embedding_provider.provider {
        Some(p) => format!(
            "- **Embedding Provider**: {p} / {} ({})",
            opt_str(data.embedding_provider.model.as_ref()),
            opt_str(data.embedding_provider.profile_name.as_ref())
        ),
        None => "- **Embedding Provider**: *Not configured*".to_string(),
    });
    push!("");

    push!("### Image Providers", "");
    if !data.image_providers.is_empty() {
        push!("| Provider | Models |");
        push!("|----------|--------|");
        for provider in &data.image_providers {
            let joined = provider.models.join(", ");
            push!(format!(
                "| {} | {} |",
                cell(&provider.display_name),
                cell(if joined.is_empty() {
                    "*No models listed*"
                } else {
                    &joined
                })
            ));
        }
    } else {
        push!("*No image providers available*");
    }
    push!("");

    push!("### Embedding Providers", "");
    if !data.embedding_providers.is_empty() {
        push!("| Provider | Models |");
        push!("|----------|--------|");
        for provider in &data.embedding_providers {
            let joined = provider.models.join(", ");
            push!(format!(
                "| {} | {} |",
                cell(&provider.display_name),
                cell(if joined.is_empty() {
                    "*No models listed*"
                } else {
                    &joined
                })
            ));
        }
    } else {
        push!("*No embedding providers available*");
    }
    push!("");

    push!("### MCP Servers", "");
    if data.mcp_servers.configured > 0.0 {
        push!(format!(
            "- **Configured**: {}",
            js_num(data.mcp_servers.configured)
        ));
        push!(format!(
            "- **Enabled**: {}",
            js_num(data.mcp_servers.enabled)
        ));
        push!(format!(
            "- **Auto-Reconnect**: {}",
            yes_no(data.mcp_servers.auto_reconnect)
        ));
        push!(
            format!(
                "- **Max Reconnect Attempts**: {}",
                js_num(data.mcp_servers.max_reconnect_attempts)
            ),
            ""
        );
        push!(
            "Server names (URLs and credentials are deliberately omitted):",
            ""
        );
        for name in &data.mcp_servers.server_names {
            push!(format!("- {name}"));
        }
    } else {
        push!("*No MCP servers configured*");
    }
    push!("");

    let ti = &data.theme_info;
    push!("### Calliope (Themes)", "");
    push!(format!(
        "- **Active Theme**: {}",
        ti.active_theme_id
            .clone()
            .unwrap_or_else(|| "Default".to_string())
    ));
    push!(format!("- **Colour Mode**: {}", ti.color_mode));
    push!(format!("- **Total Themes**: {}", js_num(ti.stats.total)));
    push!(format!(
        "- **With Dark Mode**: {}",
        js_num(ti.stats.with_dark_mode)
    ));
    push!(format!(
        "- **With CSS Overrides**: {}",
        js_num(ti.stats.with_css_overrides)
    ));
    push!(format!(
        "- **Shipping Icon Overrides**: {}",
        js_num(ti.themes_with_icons)
    ));
    push!(format!(
        "- **Total Icon Overrides**: {}",
        js_num(ti.total_icon_overrides)
    ));
    if ti.total_unknown_icon_overrides > 0.0 {
        push!(format!(
            "- **Overrides naming an unknown icon**: {} (these take effect on nothing — likely typos in a theme manifest)",
            js_num(ti.total_unknown_icon_overrides)
        ));
    }
    if ti.stats.errors > 0.0 {
        push!(format!("- **Load Errors**: {}", js_num(ti.stats.errors)));
    }
    push!("");
    if !ti.themes.is_empty() {
        push!("| Theme | Version | Source | Icon Overrides | Unknown |");
        push!("|-------|---------|--------|----------------|---------|");
        for theme in &ti.themes {
            push!(format!(
                "| {} | {} | {} | {} | {} |",
                cell(&theme.name),
                cell(&theme.version),
                cell(&theme.source),
                js_num(theme.icon_overrides),
                js_num(theme.unknown_icon_overrides)
            ));
        }
        push!("");
    }

    // ========================================================================
    // Phase 3 — Auditing the ledgers
    // ========================================================================

    push!("## The Ledgers", "");

    let ds = &data.database_stats;
    push!("### Census", "");
    push!("| Collection | Count |");
    push!("|------------|-------|");
    push!(format!("| Characters | {} |", js_num(ds.characters)));
    push!(format!(
        "| Favourite Characters | {} |",
        js_num(ds.favorite_characters)
    ));
    push!(format!("| Chats | {} |", js_num(ds.chats)));
    push!(format!("| Memories | {} |", to_locale_string(ds.memories)));
    push!(format!("| Tags | {} |", js_num(ds.tags)));
    push!(format!("| Projects | {} |", js_num(ds.projects)));
    push!(format!("| Groups | {} |", js_num(ds.groups)));
    push!(format!(
        "| Connection Profiles | {} ({} web search, {} tool use, {} dangerous) |",
        js_num(ds.connection_profiles.total),
        js_num(ds.connection_profiles.web_search_enabled),
        js_num(ds.connection_profiles.tool_use_enabled),
        js_num(ds.connection_profiles.dangerous_compatible)
    ));
    push!(format!(
        "| Image Profiles | {} |",
        js_num(ds.image_profiles)
    ));
    push!(format!(
        "| Embedding Profiles | {} |",
        js_num(ds.embedding_profiles)
    ));
    push!(format!(
        "| Prompt Templates | {} ({} built-in, {} custom) |",
        js_num(ds.prompt_templates.total),
        js_num(ds.prompt_templates.built_in),
        js_num(ds.prompt_templates.custom)
    ));
    push!(format!(
        "| Roleplay Templates | {} ({} built-in, {} custom) |",
        js_num(ds.roleplay_templates.total),
        js_num(ds.roleplay_templates.built_in),
        js_num(ds.roleplay_templates.custom)
    ));
    push!("");

    let cs = &data.chat_stats;
    push!("### Chat Statistics", "");
    push!(format!(
        "- **Total Messages**: {}",
        to_locale_string(cs.total_messages)
    ));
    push!(format!(
        "- **Total Prompt Tokens**: {}",
        to_locale_string(cs.total_prompt_tokens)
    ));
    push!(format!(
        "- **Total Completion Tokens**: {}",
        to_locale_string(cs.total_completion_tokens)
    ));
    push!(format!(
        "- **Estimated Total Cost**: {}",
        format_usd(cs.total_estimated_cost_usd)
    ));
    push!(format!(
        "- **Agent Mode Chats**: {}",
        js_num(cs.agent_mode_chats)
    ));
    push!(
        format!("- **Dangerous Chats**: {}", js_num(cs.dangerous_chats)),
        ""
    );

    let cb = &data.chat_breakdown;
    push!("### The Shape of the Chats", "");
    for line in count_list(
        &cb.by_type
            .iter()
            .map(|r| (r.chat_type.clone(), r.count))
            .collect::<Vec<_>>(),
        "No chats",
    ) {
        push!(line);
    }
    push!("");
    push!(format!("- **Paused**: {}", js_num(cb.paused_chats)));
    push!(format!(
        "- **In Document Mode**: {}",
        js_num(cb.document_mode_chats)
    ));
    push!(format!(
        "- **Open document rows**: {}",
        js_num(cb.chat_document_rows)
    ));
    push!(format!(
        "- **Chats with several documents open**: {}",
        js_num(cb.multi_document_chats)
    ));
    push!(format!(
        "- **With an equipped outfit**: {}",
        js_num(cb.chats_with_equipped_outfit)
    ));
    push!(format!(
        "- **With pending outfit notifications**: {}",
        js_num(cb.pending_outfit_notification_chats)
    ));
    push!(format!(
        "- **On the narrative clock**: {}",
        js_num(cb.narrative_timeline_chats)
    ));
    push!(
        format!(
            "- **Carrying chat state**: {}",
            js_num(cb.chats_with_non_empty_state)
        ),
        ""
    );

    if !cb.participant_histogram.is_empty() {
        push!("Cast sizes:", "");
        push!("| Participants | Chats |");
        push!("|--------------|-------|");
        for row in &cb.participant_histogram {
            push!(format!(
                "| {} | {} |",
                js_num(row.participants),
                js_num(row.chats)
            ));
        }
        push!("");
    }

    let ar = &data.autonomous_rooms;
    push!("### Autonomous Rooms", "");
    if ar.total == 0.0 {
        push!("*No autonomous rooms*", "");
    } else {
        push!(format!("- **Total**: {}", js_num(ar.total)));
        push!(format!("- **Scheduled**: {}", js_num(ar.scheduled)));
        push!(format!(
            "- **Overdue for their next run**: {}",
            js_num(ar.overdue)
        ));
        push!(format!(
            "- **With a turn budget**: {}",
            js_num(ar.with_turn_budget)
        ));
        push!(format!(
            "- **With a token budget**: {}",
            js_num(ar.with_token_budget)
        ));
        push!(format!(
            "- **With a wall-clock budget**: {}",
            js_num(ar.with_wall_clock_budget)
        ));
        push!(format!(
            "- **With a spend cap**: {}",
            js_num(ar.with_spend_cap)
        ));
        push!(
            format!(
                "- **Permitting destructive tools**: {}",
                js_num(ar.destructive_tools_allowed)
            ),
            ""
        );
        push!("| Run State | Rooms |");
        push!("|-----------|-------|");
        for row in &ar.by_run_state {
            push!(format!(
                "| {} | {} |",
                cell(&row.run_state),
                js_num(row.count)
            ));
        }
        push!("");
        push!("| Visibility | Rooms |");
        push!("|------------|-------|");
        for row in &ar.by_visibility {
            push!(format!(
                "| {} | {} |",
                cell(&row.visibility),
                js_num(row.count)
            ));
        }
        push!("");
    }

    let mb = &data.memory_breakdown;
    push!("### The Commonplace Book", "");
    push!(format!(
        "- **Total memories**: {}",
        to_locale_string(mb.total)
    ));
    push!(format!(
        "- **Characters holding memories**: {}",
        js_num(mb.characters_with_memories)
    ));
    push!(format!(
        "- **With an event time (`occurredAt`)**: {}",
        to_locale_string(mb.with_occurred_at)
    ));
    push!(format!(
        "- **With a narrative time**: {}",
        to_locale_string(mb.with_narrative_time)
    ));
    push!(format!(
        "- **With extracted entities**: {}",
        to_locale_string(mb.with_entities)
    ));
    push!(format!(
        "- **Embedded**: {}",
        to_locale_string(mb.with_embedding)
    ));
    push!(format!(
        "- **Total reinforcements**: {}",
        to_locale_string(mb.reinforced_total)
    ));
    push!(
        format!(
            "- **Most-reinforced memory**: {} times",
            js_num(mb.max_reinforcement_count)
        ),
        ""
    );
    let memory_groups: [(&str, Vec<(String, f64)>); 3] = [
        (
            "By kind",
            mb.by_kind
                .iter()
                .map(|r| (r.kind.clone(), r.count))
                .collect(),
        ),
        (
            "By source",
            mb.by_source
                .iter()
                .map(|r| (r.source.clone(), r.count))
                .collect(),
        ),
        (
            "By witnessed context",
            mb.by_witnessed_context
                .iter()
                .map(|r| (r.witnessed_context.clone(), r.count))
                .collect(),
        ),
    ];
    for (heading, rows) in memory_groups {
        push!(format!("**{heading}**"), "");
        for line in count_list(&rows, "None") {
            push!(line);
        }
        push!("");
    }

    let chb = &data.character_breakdown;
    push!("### Characters", "");
    push!(format!("- **Total**: {}", js_num(chb.total)));
    push!(format!("- **Vault-linked**: {}", js_num(chb.vault_linked)));
    if chb.vaultless > 0.0 {
        push!(format!("- **Without a vault**: {}", js_num(chb.vaultless)));
    }
    push!(format!("- **NPCs**: {}", js_num(chb.npcs)));
    push!(format!(
        "- **User-controlled personas**: {}",
        js_num(chb.user_controlled)
    ));
    push!(format!(
        "- **Carina answerers**: {}",
        js_num(chb.carina_answerers)
    ));
    push!(format!(
        "- **Permitted to see the Staff**: {}",
        js_num(chb.system_transparent)
    ));
    push!(format!(
        "- **May dress themselves**: {}",
        js_num(chb.can_dress_themselves)
    ));
    push!(format!(
        "- **May create outfits**: {}",
        js_num(chb.can_create_outfits)
    ));
    push!(
        format!(
            "- **With a Core-whisper override**: {}",
            js_num(chb.core_whisper_overrides)
        ),
        ""
    );

    push!("### Feature Configuration", "");
    let fc = &data.feature_config;
    push!("#### The Concierge (Dangerous Content)", "");
    push!(format!("- **Mode**: {}", fc.dangerous_content.mode));
    push!(format!(
        "- **Threshold**: {}",
        js_num(fc.dangerous_content.threshold)
    ));
    push!(format!(
        "- **Scan Text Chat**: {}",
        yes_no(fc.dangerous_content.scan_text_chat)
    ));
    push!(format!(
        "- **Scan Image Prompts**: {}",
        yes_no(fc.dangerous_content.scan_image_prompts)
    ));
    push!(
        format!(
            "- **Scan Image Generation**: {}",
            yes_no(fc.dangerous_content.scan_image_generation)
        ),
        ""
    );

    push!("#### Context Compression", "");
    push!(format!(
        "- **Enabled**: {}",
        yes_no(fc.context_compression.enabled)
    ));
    push!(format!(
        "- **Window Size**: {}",
        js_num(fc.context_compression.window_size)
    ));
    push!(
        format!(
            "- **Compression Target Tokens**: {}",
            js_num(fc.context_compression.compression_target_tokens)
        ),
        ""
    );

    push!("#### Prospero (Agent Mode)", "");
    push!(format!(
        "- **Max Turns**: {}",
        js_num(fc.agent_mode.max_turns)
    ));
    push!(
        format!(
            "- **Default Enabled**: {}",
            yes_no(fc.agent_mode.default_enabled)
        ),
        ""
    );

    push!("#### The Lantern (Story Backgrounds)", "");
    push!(format!(
        "- **Enabled**: {}",
        yes_no(fc.story_backgrounds.enabled)
    ));
    push!(
        format!(
            "- **Has Default Image Profile**: {}",
            yes_no(fc.story_backgrounds.has_default_image_profile)
        ),
        ""
    );

    push!("#### Aurora (Core Whisper)", "");
    push!(format!(
        "- **Enabled**: {}",
        yes_no(fc.core_whisper.enabled)
    ));
    push!(format!(
        "- **Interval**: {}",
        js_num(fc.core_whisper.interval)
    ));
    push!(format!(
        "- **Silence Threshold**: {}",
        js_num(fc.core_whisper.silence_threshold)
    ));
    push!(format!(
        "- **Packet Token Budget**: {}",
        js_num(fc.core_whisper.packet_token_budget)
    ));
    push!(
        format!(
            "- **Fires on Context Transition**: {}",
            yes_no(fc.core_whisper.fire_on_context_transition)
        ),
        ""
    );

    push!("#### Pascal the Croupier (RNG)", "");
    push!(
        format!("- **Auto-Detect RNG**: {}", yes_no(fc.auto_detect_rng)),
        ""
    );

    push!("#### Pascal's Workbench (Custom Tools)", "");
    push!(format!(
        "- **Custom tools offered to models**: {}",
        yes_no(fc.custom_tools)
    ));
    push!(
        "- *(the inventory of definitions lives in the Scriptorium section — they are documents, not settings)*",
        "",
    );

    push!("#### Timestamps", "");
    push!(format!("- **Mode**: {}", fc.timestamps.mode));
    push!(format!("- **Format**: {}", fc.timestamps.format), "");

    push!("#### Saquel Ytzama (Auto-Lock)", "");
    push!(format!("- **Enabled**: {}", yes_no(fc.auto_lock.enabled)));
    push!(
        format!("- **Idle Minutes**: {}", js_num(fc.auto_lock.idle_minutes)),
        ""
    );

    push!("#### Memory Cascade & Housekeeping", "");
    push!(format!(
        "- **On Message Delete**: {}",
        fc.memory_cascade.on_message_delete
    ));
    push!(format!(
        "- **On Swipe/Regenerate**: {}",
        fc.memory_cascade.on_swipe_regenerate
    ));
    push!(format!(
        "- **Auto Housekeeping**: {}",
        yes_no(fc.auto_housekeeping.enabled)
    ));
    push!(format!(
        "- **Per-Character Cap**: {}",
        js_num(fc.auto_housekeeping.per_character_cap)
    ));
    push!(format!(
        "- **Merge Similar**: {}",
        yes_no(fc.auto_housekeeping.merge_similar)
    ));
    push!(
        format!(
            "- **Merge Threshold**: {}",
            js_num(fc.auto_housekeeping.auto_merge_similar_threshold)
        ),
        ""
    );

    push!("#### Salon Behaviour", "");
    push!(format!(
        "- **Answer Confirmation**: {}",
        yes_no(fc.answer_confirmation.enabled)
    ));
    push!(format!(
        "- **Thinking Visible by Default**: {}",
        yes_no(fc.thinking_display.default_visible)
    ));
    push!(format!(
        "- **Thinking Collapsed by Default**: {}",
        yes_no(fc.thinking_display.default_collapsed)
    ));
    push!(format!(
        "- **Composer Spellcheck**: {}",
        yes_no(fc.composer_spellcheck)
    ));
    push!(format!(
        "- **Auto-Scroll on Response Complete**: {}",
        yes_no(fc.auto_scroll_on_response_complete)
    ));
    push!(format!(
        "- **Text Replacements**: {} ({} of {} rules enabled)",
        yes_no(fc.text_replacements.enabled),
        js_num(fc.text_replacements.enabled_rules),
        js_num(fc.text_replacements.rules)
    ));
    push!(
        format!(
            "- **Avatar Display**: {} / {}",
            fc.avatar_display.mode, fc.avatar_display.style
        ),
        ""
    );

    push!("#### Image Description", "");
    push!(format!(
        "- **Primary profile configured**: {}",
        yes_no(fc.image_description_profile_configured)
    ));
    push!(
        format!(
            "- **Uncensored fallback configured**: {}",
            yes_no(fc.uncensored_image_description_profile_configured)
        ),
        "",
    );

    let is = &data.instance_settings;
    push!("### Instance Settings", "");
    push!(format!(
        "- **Stale-chat retention**: {} days",
        js_num(is.stale_chat_days)
    ));
    push!(format!(
        "- **Chats eligible for the next sweep**: {}",
        js_num(is.chats_eligible_for_next_sweep)
    ));
    push!(format!(
        "- **Max concurrent background jobs**: {}",
        js_num(is.max_concurrent_jobs)
    ));
    push!(format!(
        "- **Memory recall**: scope policy `{}`, related-memory expansion {}",
        is.memory_recall.scope_policy,
        yes_no(is.memory_recall.expand_related)
    ));
    push!(format!(
        "- **Memory extraction limits**: {} (max {}/hour, soft start {}, floor {})",
        yes_no(is.memory_extraction_limits.enabled),
        js_num(is.memory_extraction_limits.max_per_hour),
        js_num(is.memory_extraction_limits.soft_start_fraction),
        js_num(is.memory_extraction_limits.soft_floor)
    ));
    push!(
        format!(
            "- **Last maintenance sweep**: {}",
            format_date_time(is.last_maintenance_sweep_at.as_deref())
        ),
        "",
    );

    let bj = &data.background_jobs;
    push!("### Background Jobs", "");
    push!("**By status**", "");
    for line in count_list(
        &bj.by_status
            .iter()
            .map(|r| (r.status.clone(), r.count))
            .collect::<Vec<_>>(),
        "No jobs",
    ) {
        push!(line);
    }
    push!("");
    push!("**By type**", "");
    for line in count_list(
        &bj.by_type
            .iter()
            .map(|r| (r.job_type.clone(), r.count))
            .collect::<Vec<_>>(),
        "No jobs",
    ) {
        push!(line);
    }
    push!("");
    push!(format!(
        "- **Attempts exhausted**: {}",
        js_num(bj.attempts_exhausted)
    ));
    push!(
        format!(
            "- **Oldest pending job scheduled**: {}",
            format_date_time(bj.oldest_pending_scheduled_at.as_deref())
        ),
        "",
    );
    if !bj.failed.is_empty() {
        push!("| Failed Job Type | Count | Most Recent Error |");
        push!("|-----------------|-------|-------------------|");
        for row in &bj.failed {
            push!(format!(
                "| {} | {} | {} |",
                cell(&row.job_type),
                js_num(row.count),
                cell(row.last_error.as_deref().unwrap_or("—"))
            ));
        }
        push!("");
    }

    let ep = &data.embedding_pipeline;
    push!("### Embedding Pipeline", "");
    push!(format!(
        "- **Conversation chunks**: {} ({} unembedded)",
        to_locale_string(ep.conversation_chunks.total),
        to_locale_string(ep.conversation_chunks.unembedded)
    ));
    push!(format!(
        "- **Help docs**: {} ({} unembedded)",
        js_num(ep.help_docs.total),
        js_num(ep.help_docs.unembedded)
    ));
    push!(format!(
        "- **Permanently failed rows**: {}",
        js_num(ep.permanently_failed)
    ));
    push!(format!(
        "- **Active profile dimensions**: {}",
        match ep.active_profile_dimensions {
            Some(d) => js_num(d),
            None => "Not configured".to_string(),
        }
    ));
    if ep.dimension_mismatch {
        push!(
            "- **⚠ Dimension mismatch**: stored vectors do not all match the active embedding profile. Recall against the mismatched vectors will be wrong until they are reindexed."
        );
    }
    push!("");
    if !ep.stored_dimensions.is_empty() {
        push!("| Table | Dimensions | Vectors |");
        push!("|-------|------------|---------|");
        for row in &ep.stored_dimensions {
            push!(format!(
                "| {} | {} | {} |",
                cell(&row.table),
                js_num(row.dimensions),
                to_locale_string(row.vectors)
            ));
        }
        push!("");
    }
    if !ep.status_by_entity_type.is_empty() {
        push!("| Entity Type | Status | Rows |");
        push!("|-------------|--------|------|");
        for row in &ep.status_by_entity_type {
            push!(format!(
                "| {} | {} | {} |",
                cell(&row.entity_type),
                cell(&row.status),
                to_locale_string(row.count)
            ));
        }
        push!("");
    }

    let term = &data.terminal;
    push!("### Ariel (Terminal)", "");
    push!(format!(
        "- **Total sessions**: {}",
        js_num(term.total_sessions)
    ));
    push!(format!("- **Still live**: {}", js_num(term.live_sessions)));
    push!(format!(
        "- **Exited non-zero**: {}",
        js_num(term.non_zero_exits)
    ));
    push!(
        format!(
            "- **Shells seen**: {}",
            if !term.distinct_shells.is_empty() {
                term.distinct_shells.join(", ")
            } else {
                "None".to_string()
            }
        ),
        "",
    );

    let ss = &data.storage_stats;
    push!("### Legacy File Ledger", "");
    push!(
        "The `files` table. Since the document-store cutovers the bulk of stored bytes lives in the",
        "Scriptorium (below); what remains here is mostly generated-image bookkeeping and residue.",
        "",
    );
    push!(format!(
        "- **Total Files**: {}",
        to_locale_string(ss.total_files)
    ));
    push!(format!("- **Total Size**: {}", format_bytes(ss.total_size)));
    push!(
        format!("- **Files not marked `ok`**: {}", js_num(ss.not_ok_files)),
        ""
    );
    if !ss.folders.is_empty() {
        push!("| Folder | Files | Size |");
        push!("|--------|-------|------|");
        for folder in &ss.folders {
            push!(format!(
                "| {} | {} | {} |",
                cell(&folder.path),
                js_num(folder.file_count),
                format_bytes(folder.total_size)
            ));
        }
        push!("");
    }
    if !ss.generated_images_by_model.is_empty() {
        push!("Generated images by model:", "");
        push!("| Model | Images | Size |");
        push!("|-------|--------|------|");
        for row in &ss.generated_images_by_model {
            push!(format!(
                "| {} | {} | {} |",
                cell(&row.generation_model),
                js_num(row.count),
                format_bytes(row.bytes)
            ));
        }
        push!("");
    }

    // ========================================================================
    // Phase 4 — Touring the Scriptorium
    // ========================================================================

    push!("## The Scriptorium", "");
    let s = &data.scriptorium;
    if !s.available {
        push!(
            "*The mount-index database could not be opened, so this whole section is unavailable.",
            "That is itself worth reporting: character content, wardrobe, photos, mail and every",
            "document byte live there.*",
            "",
        );
    } else {
        push!("### Document Stores", "");
        push!(format!(
            "- **Total stores**: {} ({} enabled)",
            js_num(s.mount_points.total),
            js_num(s.mount_points.enabled)
        ));
        push!(format!(
            "- **Stores with a scan error**: {}",
            js_num(s.mount_points.scan_errors)
        ));
        push!(
            format!(
                "- **Stores stuck mid-conversion**: {}",
                js_num(s.mount_points.conversion_errors)
            ),
            ""
        );

        if !s.mount_points.by_kind.is_empty() {
            push!("| Mount Type | Store Type | Stores | Enabled | Files | Chunks | Size |");
            push!("|------------|------------|--------|---------|-------|--------|------|");
            for row in &s.mount_points.by_kind {
                push!(format!(
                    "| {} | {} | {} | {} | {} | {} | {} |",
                    cell(&row.mount_type),
                    cell(&row.store_type),
                    js_num(row.count),
                    js_num(row.enabled),
                    to_locale_string(row.file_count),
                    to_locale_string(row.chunk_count),
                    format_bytes(row.total_size_bytes)
                ));
            }
            push!("");
        }

        push!("**Scan status**", "");
        for line in count_list(
            &s.mount_points
                .scan_statuses
                .iter()
                .map(|r| (r.status.clone(), r.count))
                .collect::<Vec<_>>(),
            "No stores",
        ) {
            push!(line);
        }
        push!("");
        push!("**Conversion status**", "");
        for line in count_list(
            &s.mount_points
                .conversion_statuses
                .iter()
                .map(|r| (r.status.clone(), r.count))
                .collect::<Vec<_>>(),
            "No stores",
        ) {
            push!(line);
        }
        push!("");

        if !s.mount_points.well_known.is_empty() {
            push!("The three global stores:", "");
            push!("| Store | Setting Key | Resolves |");
            push!("|-------|-------------|----------|");
            for row in &s.mount_points.well_known {
                let name = if row.resolved {
                    row.name.clone().unwrap_or_else(|| "(unnamed)".to_string())
                } else if row.mount_point_id.as_ref().is_some_and(|s| !s.is_empty()) {
                    "Missing row".to_string()
                } else {
                    "Not provisioned".to_string()
                };
                push!(format!(
                    "| {} | `{}` | {} |",
                    cell(&row.label),
                    cell(&row.setting_key),
                    cell(&name)
                ));
            }
            push!("");
        }

        push!("### Contents", "");
        push!(format!(
            "- **Content rows** (`doc_mount_files`): {}",
            to_locale_string(s.content.file_rows)
        ));
        push!(format!(
            "- **Link rows** (visible locations): {}",
            to_locale_string(s.content.link_rows)
        ));
        push!(format!(
            "- **Text documents**: {} ({})",
            to_locale_string(s.content.document_rows),
            format_bytes(s.content.document_text_bytes)
        ));
        push!(format!(
            "- **Binary blobs**: {} ({})",
            to_locale_string(s.content.blob_rows),
            format_bytes(s.content.blob_bytes)
        ));
        push!(format!(
            "- **Chunks**: {} ({} unembedded, {} tokens)",
            to_locale_string(s.content.chunk_rows),
            to_locale_string(s.content.unembedded_chunks),
            to_locale_string(s.content.chunk_tokens)
        ));
        push!("");
        if !s.content.blobs_by_mime_type.is_empty() {
            push!("| Stored MIME Type | Blobs | Size |");
            push!("|------------------|-------|------|");
            for row in &s.content.blobs_by_mime_type {
                push!(format!(
                    "| {} | {} | {} |",
                    cell(&row.stored_mime_type),
                    to_locale_string(row.count),
                    format_bytes(row.bytes)
                ));
            }
            push!("");
        }

        push!("### Links & Policy", "");
        push!(format!(
            "- **Links per content row**: {}",
            to_fixed(s.links.dedup_ratio, 2)
        ));
        push!(format!(
            "- **Hard-link groups**: {} ({} links belong to one)",
            js_num(s.links.hard_link_groups),
            js_num(s.links.hard_linked_links)
        ));
        push!(format!(
            "- **Extraction errors**: {}",
            js_num(s.links.extraction_errors)
        ));
        push!(format!(
            "- **Conversion errors**: {}",
            js_num(s.links.conversion_errors)
        ));
        push!(format!(
            "- **Documents withheld from embedding**: {}",
            js_num(s.links.policy_embed_denied)
        ));
        push!(format!(
            "- **Documents hidden from characters**: {}",
            js_num(s.links.policy_character_read_denied)
        ));
        push!(
            format!(
                "- **Documents characters may not write**: {}",
                js_num(s.links.policy_character_write_denied)
            ),
            ""
        );

        push!("### Character Vaults", "");
        push!(format!(
            "- **Vault-linked characters**: {}",
            js_num(s.character_vaults.total)
        ));
        push!(format!(
            "- **With the `properties.json` keystone**: {}",
            js_num(s.character_vaults.with_keystone)
        ));
        if s.character_vaults.missing_keystone > 0.0 {
            push!(format!(
                "- **⚠ Missing the keystone**: {}. Reading one of these characters raises `CharacterVaultUnavailableError` — a hard failure for that character, not a silently empty one.",
                js_num(s.character_vaults.missing_keystone)
            ));
        }
        push!(
            format!(
                "- **With a `metadata.json`**: {}",
                js_num(s.character_vaults.with_metadata)
            ),
            ""
        );

        push!("### Wardrobe", "");
        if s.wardrobe.iter().all(|w| w.items == 0.0) {
            push!("*No wardrobe items*", "");
        } else {
            push!("| Tier | Items | Archived |");
            push!("|------|-------|----------|");
            for row in &s.wardrobe {
                push!(format!(
                    "| {} | {} | {} |",
                    cell(&row.tier),
                    js_num(row.items),
                    js_num(row.archived)
                ));
            }
            push!("");
        }

        let ct = &s.custom_tools;
        push!("### Pascal's Workbench (Custom Tools)", "");
        push!(format!("- **Definitions found**: {}", js_num(ct.total)));
        push!(format!(
            "- **With saved presets**: {} ({} preset files)",
            js_num(ct.with_presets),
            js_num(ct.preset_files)
        ));
        push!(format!(
            "- **Consulting an LLM**: {}",
            js_num(ct.with_llm_consult)
        ));
        push!(format!(
            "- **With side effects**: {}",
            js_num(ct.with_effects)
        ));
        push!(format!(
            "- **Gated on invoker metadata**: {}",
            js_num(ct.metadata_gated)
        ));
        push!(
            format!("- **Failed to parse**: {}", js_num(ct.parse_failures)),
            ""
        );
        if !ct.by_store.is_empty() {
            push!("| Store | Definitions |");
            push!("|-------|-------------|");
            for row in &ct.by_store {
                push!(format!("| {} | {} |", cell(&row.store), js_num(row.count)));
            }
            push!("");
        }
        if !ct.parse_failure_detail.is_empty() {
            push!("Definitions that would not load:", "");
            push!("| Store | Path | Reason |");
            push!("|-------|------|--------|");
            for row in &ct.parse_failure_detail {
                push!(format!(
                    "| {} | {} | {} |",
                    cell(&row.store),
                    cell(&row.path),
                    cell(&row.reason)
                ));
            }
            push!("");
        }

        push!("### Suparṇā's Post Office", "");
        push!(format!(
            "- **Letters delivered**: {}",
            js_num(s.post_office.letters)
        ));
        push!(format!(
            "- **Not yet announced**: {}",
            js_num(s.post_office.unannounced)
        ));
        push!(
            format!(
                "- **Characters with a mailbox**: {}",
                js_num(s.post_office.mailboxes)
            ),
            ""
        );

        push!("### Photographs", "");
        push!(format!(
            "- **In character vaults**: {} ({})",
            js_num(s.photos.character_vault_photos),
            format_bytes(s.photos.character_vault_bytes)
        ));
        push!(format!(
            "- **In your gallery**: {} ({})",
            js_num(s.photos.user_gallery_photos),
            format_bytes(s.photos.user_gallery_bytes)
        ));
        push!(
            "- *Counts and bytes only — never titles, captions or filenames.*",
            ""
        );

        push!("### Scenarios & State", "");
        if s.scenarios.iter().all(|row| row.count == 0.0) {
            push!("*No scenarios*", "");
        } else {
            push!("| Tier | Scenarios |");
            push!("|------|-----------|");
            for row in &s.scenarios {
                push!(format!("| {} | {} |", cell(&row.tier), js_num(row.count)));
            }
            push!("");
        }
        push!(
            "State cascade coverage (chat → project → group → general):",
            ""
        );
        push!(format!(
            "- **Chats carrying state**: {}",
            js_num(s.state_cascade.chats_with_state)
        ));
        push!(format!(
            "- **Projects carrying state**: {}",
            js_num(s.state_cascade.projects_with_state)
        ));
        push!(format!(
            "- **Groups carrying state**: {}",
            js_num(s.state_cascade.groups_with_state)
        ));
        push!(
            format!(
                "- **General state present**: {}",
                yes_no(s.state_cascade.general_state_present)
            ),
            ""
        );
    }

    // ========================================================================
    // Phase 5 — Assembling the dramatis personae
    // ========================================================================

    push!("## Dramatis Personae", "");

    push!("### Ten Busiest Characters", "");
    if data.personae.top_characters.is_empty() {
        push!("*No characters*", "");
    } else {
        push!("| Character | Chats | Memories | Vault Size |");
        push!("|-----------|-------|----------|------------|");
        for row in &data.personae.top_characters {
            push!(format!(
                "| {} | {} | {} | {} |",
                cell(&row.name),
                js_num(row.chats),
                to_locale_string(row.memories),
                format_bytes(row.storage_bytes)
            ));
        }
        push!("");
        push!(
            format!(
                "Ranked by chats, ties broken by memories. Help chats and Brahma Console sessions are excluded. Participants marked `removed` {} — a character written out of a scene was still in it. Vault size is the store's cached total as of its last scan; because content is addressed by hash and hard-linkable across vaults, bytes shared between two characters appear in both rows.",
                if data.personae.counts_removed_participants {
                    "still count"
                } else {
                    "are not counted"
                }
            ),
            "",
        );
    }

    push!("### Projects", "");
    if data.personae.projects.is_empty() {
        push!("*No projects*", "");
    } else {
        push!("| Project | Linked Stores | Chats | Files | Documents | State |");
        push!("|---------|---------------|-------|-------|-----------|-------|");
        for row in &data.personae.projects {
            push!(format!(
                "| {} | {} | {} | {} | {} | {} |",
                cell(&row.name),
                js_num(row.linked_stores),
                js_num(row.chats),
                js_num(row.files),
                js_num(row.documents),
                if row.has_state { "✓" } else { "—" }
            ));
        }
        push!("");
    }

    push!("### Groups", "");
    if data.personae.groups.is_empty() {
        push!("*No groups*", "");
    } else {
        push!("| Group | Members | Linked Stores | Official Store |");
        push!("|-------|---------|---------------|----------------|");
        for row in &data.personae.groups {
            let members = row.members.join(", ");
            push!(format!(
                "| {} | {} | {} | {} |",
                cell(&row.name),
                cell(if members.is_empty() {
                    "*none*"
                } else {
                    &members
                }),
                js_num(row.linked_stores),
                if row.has_official_store {
                    "✓"
                } else {
                    "⚠ missing"
                }
            ));
        }
        push!("");
        if data.personae.groups.iter().any(|g| !g.has_official_store) {
            push!(
                "A group without an official store never had one provisioned — its description, instructions,",
                "state and scenarios have nowhere to live.",
                "",
            );
        }
    }

    // ========================================================================
    // Phase 6 — Reading the wire records
    // ========================================================================

    let wr = &data.wire_records;
    push!("## The Wire Records", "");
    push!(
        format!(
            "> These figures are drawn from the LLM logs. Logging is currently **{}**",
            if wr.logging_enabled { "on" } else { "off" }
        ),
        format!(
            "> with a retention window of **{}**.",
            if wr.retention_days == 0.0 {
                "forever".to_string()
            } else {
                format!("{} days", js_num(wr.retention_days))
            }
        ),
        "> Enable LLM logging and lengthen the retention window (Settings → Chat → LLM Logging) and the Almanack's",
        "> arithmetic grows correspondingly richer.",
        "",
    );
    if !wr.exact_profile_attribution {
        push!(
            "> **Attribution is approximate.** This database predates the per-row profile columns, so requests are",
            "> grouped by provider and model. Two profiles sharing a provider and model appear as one line.",
            "",
        );
    }

    push!("### Totals", "");
    push!(format!(
        "- **Log entries**: {}",
        to_locale_string(wr.total_entries)
    ));
    push!(format!(
        "- **Prompt tokens**: {}",
        to_locale_string(wr.token_usage.prompt_tokens)
    ));
    push!(format!(
        "- **Completion tokens**: {}",
        to_locale_string(wr.token_usage.completion_tokens)
    ));
    push!(format!(
        "- **Total tokens**: {}",
        to_locale_string(wr.token_usage.total_tokens)
    ));
    push!(
        format!("- **Verbose mode**: {}", yes_no(wr.verbose_mode)),
        ""
    );

    push!("### Requests by Type", "");
    if wr.by_type.is_empty() {
        push!("*No logged requests*", "");
    } else {
        push!(
            "| Type | Requests | Prompt | Completion | Total | Avg Latency | Measured | Errors |"
        );
        push!(
            "|------|----------|--------|------------|-------|-------------|----------|--------|"
        );
        for row in &wr.by_type {
            push!(format!(
                "| {} | {} | {} | {} | {} | {} | {}/{} | {} |",
                cell(&row.log_type),
                to_locale_string(row.requests),
                to_locale_string(row.prompt_tokens),
                to_locale_string(row.completion_tokens),
                to_locale_string(row.total_tokens),
                format_ms(row.avg_duration_ms),
                js_num(row.measured_requests),
                js_num(row.requests),
                js_num(row.failures)
            ));
        }
        push!("");
        push!(
            "\"Measured\" is the denominator behind the average — requests carrying a usable `durationMs`.",
            "Rows written before the timing gaps were closed carry none and are excluded from the average.",
            "",
        );
    }

    push!("### Connection Profiles — Lifetime Counters", "");
    if wr.connection_profile_lifetime.is_empty() {
        push!("*No connection profiles*", "");
    } else {
        push!("| Profile | Provider / Model | Messages | Prompt | Completion | Total | Last Touched |");
        push!("|---------|------------------|----------|--------|------------|-------|--------------|");
        for row in &wr.connection_profile_lifetime {
            push!(format!(
                "| {} | {} | {} | {} | {} | {} | {} |",
                cell(&row.name),
                cell(&format!("{} / {}", row.provider, row.model_name)),
                to_locale_string(row.message_count),
                to_locale_string(row.total_prompt_tokens),
                to_locale_string(row.total_completion_tokens),
                to_locale_string(row.total_tokens),
                format_date(row.last_touched_at.as_deref())
            ));
        }
        push!("");
        push!(
            "These counters live on the profile rows and cover the profile's whole life, not the log retention",
            "window. There is no last-used column, so \"Last Touched\" is the row's `updatedAt` — a proxy.",
            "",
        );
    }

    render_window(
        &mut lines,
        "Connection Profiles — Within the Retention Window",
        &wr.connection_profile_window,
        "No logged requests",
    );

    render_window(
        &mut lines,
        "Image Profiles — Within the Retention Window",
        &wr.image_profile_window,
        "No logged image generations",
    );
    if !wr.image_profile_window.is_empty() {
        push!(
            "A Concierge reroute logs a second row under the fallback profile, so a rerouted generation appears",
            "once under the profile that refused it and once under the profile that produced it. Naive totals",
            "therefore double-count rerouted work.",
            "",
        );
    }

    render_cache(
        &mut lines,
        "Prompt Cache by Provider",
        &wr.cache_by_provider,
        "No cache data recorded",
    );
    render_cache(
        &mut lines,
        "Prompt Cache by Profile",
        &wr.cache_by_profile,
        "No per-profile cache data recorded",
    );
    push!(
        "Cache figures come from the `cacheUsage` payload, which in practice only chat messages carry — the",
        "streaming path is its sole writer. There is no \"was this a hit\" flag; a hit is a row reporting more",
        "than zero cache-read tokens. Provider plugins strip cache reads out of the reported prompt tokens, so",
        "\"Uncached Prompt\" and \"Cache Read\" together make the whole prompt.",
        "",
    );

    lines.join("\n")
}

/// v4's `renderWindow` closure.
fn render_window(lines: &mut Vec<String>, heading: &str, rows: &[ProfileWindowRow], empty: &str) {
    lines.push(format!("### {heading}"));
    lines.push(String::new());
    if rows.is_empty() {
        lines.push(format!("*{empty}*"));
        lines.push(String::new());
        return;
    }
    lines.push(
        "| Profile | Provider / Model | Requests | Tokens | Avg | Median | Measured | Errors |"
            .to_string(),
    );
    lines.push(
        "|---------|------------------|----------|--------|-----|--------|----------|--------|"
            .to_string(),
    );
    for row in rows {
        lines.push(format!(
            "| {} | {} | {} | {} | {} | {} | {}/{} | {} |",
            cell(&row.label),
            cell(&format!("{} / {}", row.provider, row.model_name)),
            to_locale_string(row.requests),
            to_locale_string(row.total_tokens),
            format_ms(row.avg_duration_ms),
            format_ms(row.median_duration_ms),
            js_num(row.measured_requests),
            js_num(row.requests),
            js_num(row.failures)
        ));
    }
    lines.push(String::new());
}

/// v4's `renderCache` closure.
fn render_cache(lines: &mut Vec<String>, heading: &str, rows: &[CacheRow], empty: &str) {
    lines.push(format!("### {heading}"));
    lines.push(String::new());
    if rows.is_empty() {
        lines.push(format!("*{empty}*"));
        lines.push(String::new());
        return;
    }
    lines.push("| Key | Requests w/ Cache Data | Hits | Hit Rate | Cache Read | Cache Write | Uncached Prompt | Token Hit Rate |".to_string());
    lines.push("|-----|------------------------|------|----------|------------|-------------|-----------------|----------------|".to_string());
    for row in rows {
        lines.push(format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            cell(&row.label),
            to_locale_string(row.rows_with_cache_usage),
            to_locale_string(row.rows_with_cache_read),
            percent(row.request_hit_ratio),
            to_locale_string(row.cache_read_tokens),
            to_locale_string(row.cache_creation_tokens),
            to_locale_string(row.uncached_prompt_tokens),
            percent(row.token_hit_ratio)
        ));
    }
    lines.push(String::new());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_string_matches_node() {
        // Probed against Node 24 `Number.prototype.toLocaleString()`.
        for (n, want) in [
            (0.0, "0"),
            (999.0, "999"),
            (1000.0, "1,000"),
            (1_234_567.0, "1,234,567"),
            (-4321.0, "-4,321"),
            (1.5, "1.5"),
            (0.0005, "0.001"),
            (1234.5678, "1,234.568"),
            (1234.0005, "1,234.001"),
            (0.30000000000000004, "0.3"),
            (0.3333333333333333, "0.333"),
            (1_234_567.891, "1,234,567.891"),
            (100.25, "100.25"),
            (12345.6785, "12,345.679"),
            (1e21, "1,000,000,000,000,000,000,000"),
            (-0.0004, "-0"),
        ] {
            assert_eq!(to_locale_string(n), want, "toLocaleString({n})");
        }
    }

    #[test]
    fn format_helpers_match_v4() {
        assert_eq!(format_usd(1.5), "$1.5000");
        assert_eq!(format_usd(0.0), "$0.0000");
        assert_eq!(format_ms(None), "—");
        assert_eq!(format_ms(Some(f64::NAN)), "—");
        assert_eq!(format_ms(Some(999.4)), "999 ms");
        // JS `Math.round` is half toward +∞ — 0.5 rounds up, −0.5 to −0.
        assert_eq!(format_ms(Some(0.5)), "1 ms");
        assert_eq!(format_ms(Some(1000.0)), "1.00 s");
        assert_eq!(format_ms(Some(18200.0)), "18.20 s");
        assert_eq!(percent(0.8181818181818182), "81.8%");
        assert_eq!(percent(0.0), "0.0%");
        assert_eq!(cell("a|b\nc"), "a\\|b c");
        assert_eq!(yes_no(true), "Yes");
        assert_eq!(format_date(None), "N/A");
        assert_eq!(format_date(Some("nope")), "N/A");
        assert_eq!(format_date(Some("2026-08-04T00:00:00.000Z")), "8/4/2026");
        assert_eq!(
            format_date_time(Some("2026-08-05T12:00:00.000Z")),
            "8/5/2026, 12:00:00 PM"
        );
        assert_eq!(count_list(&[], "None"), vec!["*None*".to_string()]);
        assert_eq!(
            count_list(&[("semantic".into(), 1200.0)], "None"),
            vec!["- **semantic**: 1,200".to_string()]
        );
    }

    /// Rust's `f64` Display is the shortest round-trip form without an exponent
    /// — the property `js_num` leans on.
    #[test]
    fn js_num_matches_js_string() {
        assert_eq!(js_num(3.0), "3");
        assert_eq!(js_num(0.7), "0.7");
        assert_eq!(js_num(0.9), "0.9");
        assert_eq!(js_num(-0.0), "0");
        assert_eq!(js_num(4096.0), "4096");
    }
}
