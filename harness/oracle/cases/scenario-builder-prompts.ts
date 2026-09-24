/**
 * Oracle case: the Scenario Builder's two prompt builders (v4 `d1c06cd9d`,
 * `lib/scenario-builder/system-prompt.ts`) —
 * `buildScenarioBuilderSystemPrompt({ mode, webAvailable, toolInstructions,
 * now })` and `buildScenarioBuilderUserMessage(input)` — plus the module's
 * private `formatIsoWithOffset`, reached through the system prompt's `## Now`
 * section.
 *
 * Drives v4's REAL builders. `now` is a fixed instant and the zone is set PER
 * ROW through `process.env.TZ` (Node re-reads it on assignment), so the
 * offset's sign (Chicago − / Tokyo +), the `% 60` remainder (Kolkata +05:30,
 * St John's −02:30 / −03:30, Kathmandu +05:45) and UTC's `+00:00` (never `Z`)
 * are all asked. v5's twin takes a `jiff::Zoned` built from the row's
 * `(epochMs, tz)` — so the Rust side resolves the IANA zone itself and the
 * whole `Date`-in-a-zone path is compared, not just the formatter.
 *
 * The user-message corpus asks `details` empty / blank / padded, a
 * whitespace-PADDED `currentScenario` (v4 tests the TRIMMED value for
 * truthiness but interpolates the UNTRIMMED one — §R.4(d), reproduced, never
 * fixed), a padded `contextSummary` (interpolated TRIMMED), the revision pair
 * both / one / neither, and `null` vs absent optionals.
 *
 * ⚠ PIN REQUIRED at the TARGET `d1c06cd9d`: the module does not exist at the
 * `00c290c9a` baseline, so a baseline-pinned run fails to IMPORT — that failure
 * is the pin verification.
 *
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   rm -f /tmp/oracle-scenario-builder-prompts.ndjson
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/scenario-builder-prompts.ts \
 *     > /tmp/oracle-scenario-builder-prompts.ndjson
 */

import {
  SCENARIO_TARGET_TOKENS,
  buildScenarioBuilderSystemPrompt,
  buildScenarioBuilderUserMessage,
  type ScenarioBuilderUserMessageInput,
} from '@/lib/scenario-builder/system-prompt';

const out = (row: unknown) => process.stdout.write(JSON.stringify(row) + '\n');

out({ kind: 'const', id: 'target-tokens', value: SCENARIO_TARGET_TOKENS });

// --- the system prompt -------------------------------------------------------
// 2026-09-23T19:05:09.000Z — a DST-season instant; 2026-01-15T06:30:00Z a
// standard-time one (St John's moves −02:30 → −03:30 across them).
const INSTANTS = [Date.UTC(2026, 8, 23, 19, 5, 9), Date.UTC(2026, 0, 15, 6, 30, 0), Date.UTC(1999, 11, 31, 23, 59, 59)];
const ZONES = ['America/Chicago', 'Asia/Tokyo', 'Asia/Kolkata', 'America/St_Johns', 'Asia/Kathmandu', 'UTC'];
const TOOL_BLOCK = '## Tools\n\nCall `search` with `{"query": "…"}`.\n\nThen call `submit_final_response`.';

let n = 0;
for (const tz of ZONES) {
  for (const epochMs of INSTANTS) {
    process.env.TZ = tz;
    for (const mode of ['real', 'in-world'] as const) {
      for (const webAvailable of [true, false]) {
        for (const toolInstructions of ['', TOOL_BLOCK]) {
          // Keep the corpus bounded: the zone × instant grid rides one mode/web
          // pairing; the full mode × web × tools grid rides the first zone.
          const full = tz === ZONES[0] && epochMs === INSTANTS[0];
          if (!full && !(mode === 'in-world' && webAvailable && toolInstructions === '')) continue;
          out({
            kind: 'system',
            id: `system-${n++}`,
            tz,
            epochMs,
            mode,
            webAvailable,
            toolInstructions,
            out: buildScenarioBuilderSystemPrompt({ mode, webAvailable, toolInstructions, now: new Date(epochMs) }),
          });
        }
      }
    }
  }
}

// --- the user message --------------------------------------------------------
const base: ScenarioBuilderUserMessageInput = {
  mode: 'in-world',
  location: 'The Lantern',
  time: 'dusk',
  details: '',
};
const users: Array<[string, ScenarioBuilderUserMessageInput]> = [
  ['minimal', base],
  ['real-mode', { ...base, mode: 'real', location: 'Chicago, the Loop', time: '1927, November' }],
  ['details-blank', { ...base, details: '   ' }],
  ['details-padded', { ...base, details: '  jazz, rain  ' }],
  ['details-multiline', { ...base, details: 'line one\nline two' }],
  ['location-untrimmed-verbatim', { ...base, location: '  spaced  ', time: ' t ' }],
  ['current-scenario', { ...base, currentScenario: 'The old scene.' }],
  ['current-scenario-padded', { ...base, currentScenario: '  \n The old scene. \n  ' }],
  ['current-scenario-blank', { ...base, currentScenario: '   ' }],
  ['current-scenario-null', { ...base, currentScenario: null }],
  ['context-summary', { ...base, contextSummary: 'They argued.' }],
  ['context-summary-padded', { ...base, contextSummary: '  They argued.  ' }],
  ['context-summary-blank', { ...base, contextSummary: '\n' }],
  ['both-in-chat', { ...base, currentScenario: ' Old. ', contextSummary: ' Summary. ' }],
  ['revision-pair', { ...base, priorDraft: 'The draft.', revision: 'More rain.' }],
  ['revision-pair-padded', { ...base, priorDraft: '  The draft.  ', revision: '  More rain.  ' }],
  ['revision-empty-strings', { ...base, priorDraft: '', revision: '' }],
  ['revision-draft-only', { ...base, priorDraft: 'The draft.' }],
  ['revision-instruction-only', { ...base, revision: 'More rain.' }],
  ['revision-draft-null', { ...base, priorDraft: null, revision: 'More rain.' }],
  [
    'everything',
    {
      mode: 'real',
      location: 'Paris',
      time: '1925',
      details: ' a café ',
      currentScenario: ' Old. ',
      contextSummary: ' Summary. ',
      priorDraft: 'Draft.',
      revision: 'Shorter.',
    },
  ],
];
for (const [id, input] of users) {
  out({ kind: 'user', id, input, out: buildScenarioBuilderUserMessage(input) });
}
