/**
 * Oracle case (P4.59): v4's site-plugins gate — `lib/plugins/site-plugins.ts`.
 *
 * Drives v4's REAL `isSitePluginEnabled` over a permutation corpus of
 * `SITE_PLUGINS_ENABLED` / `SITE_PLUGINS_DISABLED` values (unset, empty,
 * whitespace-only, the literal `all` in three casings, comma lists with stray
 * spaces and empty segments, and the disabled-wins overlap). This is the gate
 * v4's manifest loader applies BEFORE `enabledByDefault` is read, so it decides
 * whether the bundled Serper search plugin registers at all; the Rust port
 * (`provider_manifest::search::is_site_plugin_enabled`) takes the env values as
 * arguments because the core reads no environment.
 *
 * Emits one NDJSON row per case: { label, plugin, enabled, disabled, result }.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/site-plugins.ts \
 *     > /tmp/oracle-site-plugins.ndjson
 */

import { isSitePluginEnabled } from '@/lib/plugins/site-plugins';

const SERPER = 'qtap-plugin-search-serper';
const OTHER = 'qtap-plugin-anthropic';

interface Case {
  label: string;
  plugin: string;
  enabled?: string;
  disabled?: string;
}

const CASES: Case[] = [
  { label: 'both_unset', plugin: SERPER },
  { label: 'enabled_empty', plugin: SERPER, enabled: '' },
  { label: 'enabled_whitespace', plugin: SERPER, enabled: '   ' },
  { label: 'enabled_all', plugin: SERPER, enabled: 'all' },
  { label: 'enabled_all_upper', plugin: SERPER, enabled: 'ALL' },
  { label: 'enabled_all_padded', plugin: SERPER, enabled: '  All  ' },
  { label: 'enabled_other_only', plugin: SERPER, enabled: OTHER },
  { label: 'enabled_list_includes', plugin: SERPER, enabled: `${OTHER},${SERPER}` },
  { label: 'enabled_list_padded', plugin: SERPER, enabled: ` ${OTHER} ,  ${SERPER}  ` },
  { label: 'enabled_list_empty_segments', plugin: SERPER, enabled: `${OTHER},,` },
  { label: 'enabled_commas_only', plugin: SERPER, enabled: ',,,' },
  { label: 'disabled_serper', plugin: SERPER, disabled: SERPER },
  { label: 'disabled_all_folds_to_empty', plugin: SERPER, disabled: 'all' },
  { label: 'disabled_other', plugin: SERPER, disabled: OTHER },
  { label: 'disabled_wins_over_enabled', plugin: SERPER, enabled: SERPER, disabled: SERPER },
  { label: 'disabled_whitespace', plugin: SERPER, disabled: '  ' },
  { label: 'disabled_list_padded', plugin: SERPER, disabled: ` ${OTHER} , ${SERPER} ` },
  { label: 'other_plugin_default', plugin: OTHER },
  { label: 'other_plugin_enabled_serper_only', plugin: OTHER, enabled: SERPER },
];

function main(): void {
  const lines: string[] = [];
  for (const c of CASES) {
    if (c.enabled === undefined) delete process.env.SITE_PLUGINS_ENABLED;
    else process.env.SITE_PLUGINS_ENABLED = c.enabled;
    if (c.disabled === undefined) delete process.env.SITE_PLUGINS_DISABLED;
    else process.env.SITE_PLUGINS_DISABLED = c.disabled;

    const result = isSitePluginEnabled(c.plugin);
    lines.push(
      JSON.stringify({
        label: c.label,
        plugin: c.plugin,
        enabled: c.enabled ?? null,
        disabled: c.disabled ?? null,
        result,
      }),
    );
  }
  process.stdout.write(lines.join('\n') + '\n');
}

main();
